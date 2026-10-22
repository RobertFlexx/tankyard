use crate::{NetError, UdpServer, unix_time};
use tankyard_protocol::{ChatMessage, ServerCommand, ServerControl, ServerKind, ServerRole};
impl UdpServer {
    pub(crate) fn match_settings(&self) -> tankyard_protocol::MatchSettings {
        tankyard_protocol::MatchSettings {
            party: self
                .settings
                .party
                .as_ref()
                .map(|party| tankyard_protocol::PartyMatchOptions {
                    width: self.settings.world.width as u16,
                    height: self.settings.world.height as u16,
                    dynamic_arena: self.settings.world.dynamic_arena,
                    starting_level: party.starting_level,
                }),
            mode: self.session.world.config.mode,
            map: self.session.world.config.map.clone(),
            difficulty: self.settings.difficulty.name().into(),
            bots: self
                .settings
                .party
                .as_ref()
                .map_or(self.settings.bot_count, |p| p.ally_slots),
        }
    }
    pub(crate) fn broadcast_match_settings(&mut self) {
        self.broadcast_control(&ServerControl::MatchSettings {
            settings: self.match_settings(),
        });
    }
    pub(crate) fn configure_match(
        &mut self,
        mode: tankyard_core::Mode,
        map: &str,
        difficulty: tankyard_ai::Difficulty,
    ) -> Result<(), NetError> {
        let candidate =
            self.settings
                .configured_match(&self.session.bundle.maps, mode, map, difficulty)?;
        self.settings = candidate;
        self.restart_round();
        Ok(())
    }
    pub(crate) fn send_rooms(&mut self, client: u64) {
        let role = self.access.grant(client, unix_time().as_secs()).role;
        self.send_control(
            client,
            &ServerControl::Rooms {
                rooms: self.chat.list(role),
            },
        );
    }
    pub(crate) fn join_room(&mut self, client: u64, room: &str) -> Result<(), NetError> {
        let role = self.access.grant(client, unix_time().as_secs()).role;
        let history = self.chat.join(client, room, role).map_err(NetError)?;
        for message in history {
            self.send_control(client, &ServerControl::Chat { message });
        }
        Ok(())
    }
    pub(crate) fn player_chat(
        &mut self,
        client: u64,
        room: String,
        text: String,
    ) -> Result<(), NetError> {
        let now = unix_time().as_secs();
        if self.access.muted(client, now) {
            return Err(NetError("You are muted on this server.".into()));
        }
        let player = self
            .player_id(client)
            .ok_or_else(|| NetError("Player is not connected.".into()))?;
        let tank = self
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == player)
            .ok_or_else(|| NetError("Player is not connected.".into()))?;
        let message = ChatMessage {
            id: 0,
            room,
            player,
            name: tank.name.clone(),
            game_creator: tank.game_creator,
            server_badge: tank.server_badge,
            text: text.trim().replace('\u{2014}', "-"),
            time: now,
        };
        let (message, recipients) = self
            .chat
            .publish(
                client,
                self.access.grant(client, now).role,
                message.clone(),
                now,
            )
            .map_err(NetError)?;
        for recipient in recipients {
            if !self.chat.can_read(
                recipient,
                &message.room,
                self.access.grant(recipient, now).role,
            ) {
                continue;
            }
            self.send_control(
                recipient,
                &ServerControl::Chat {
                    message: message.clone(),
                },
            );
        }
        Ok(())
    }
    pub(crate) fn server_command(
        &mut self,
        actor: u64,
        command: ServerCommand,
    ) -> Result<(), NetError> {
        let now = unix_time().as_secs();
        if self.settings.kind != ServerKind::Community && !self.settings.central_owner_enabled() {
            return Err(NetError(
                "Server permissions apply only to community servers.".into(),
            ));
        }
        self.access
            .authorize(actor, &command, now)
            .map_err(NetError)?;
        if !self.access.take_admin_token(actor, now) {
            return Err(NetError(
                "Wait before sending another server command.".into(),
            ));
        }
        let client_for = |player: u64| {
            self.peers
                .iter()
                .find_map(|(client, p)| (p.entity == Some(player)).then_some(*client))
                .ok_or_else(|| NetError("Player was not found.".into()))
        };
        let target = match &command {
            ServerCommand::Kick { player, .. }
            | ServerCommand::Ban { player, .. }
            | ServerCommand::Mute { player, .. }
            | ServerCommand::Warn { player, .. }
            | ServerCommand::Nominate { player, .. } => Some(client_for(*player)?),
            _ => None,
        };
        if let Some(target) = target
            && (!self.access.can_affect(actor, target, now)
                || self.peers.get(&target).is_some_and(|peer| peer.creator))
        {
            return Err(NetError(
                "You cannot moderate a player with equal or higher server authority.".into(),
            ));
        }
        crate::access::audit(
            self.settings.creator_audit_file(),
            &crate::access::AuditRecord {
                time: now,
                actor: &actor.to_string(),
                action: "server_moderation",
                target: &target.map_or_else(|| "server".into(), |t| t.to_string()),
                reason: &format!("{command:?}"),
            },
        )
        .map_err(NetError)?;
        match command {
            ServerCommand::Warn { reason, .. } => self.warn_client(target.unwrap(), reason),
            ServerCommand::Announce { text } => self.announce(text.trim().to_owned()),
            ServerCommand::Kick { reason, .. } => self.reject(
                target.unwrap(),
                if reason.is_empty() {
                    "Removed by a server moderator."
                } else {
                    &reason
                },
            ),
            ServerCommand::Ban {
                minutes, reason, ..
            } => {
                self.ban_client(target.unwrap(), u64::from(minutes), &reason)?;
            }
            ServerCommand::Mute { minutes, .. } => {
                self.access
                    .mute(
                        target.unwrap(),
                        if minutes == 0 {
                            0
                        } else {
                            now.saturating_add(u64::from(minutes) * 60)
                        },
                    )
                    .map_err(NetError)?;
                self.send_voice_status(target.unwrap());
            }
            ServerCommand::Nominate {
                role, permissions, ..
            } => {
                self.access
                    .nominate(actor, target.unwrap(), role, &permissions, now)
                    .map_err(NetError)?;
                self.send_access(target.unwrap());
                self.chat.refresh_role(target.unwrap(), role);
                self.send_rooms(target.unwrap());
            }
            ServerCommand::CreateRoom {
                id,
                name,
                minimum_role,
            } => {
                if minimum_role > self.access.grant(actor, now).role {
                    return Err(NetError("Room role exceeds your server authority.".into()));
                }
                self.chat.create(id, name, minimum_role).map_err(NetError)?;
                for client in self.peers.keys().copied().collect::<Vec<_>>() {
                    self.send_rooms(client);
                }
            }
            ServerCommand::DeleteRoom { id } => {
                self.chat.delete(&id).map_err(NetError)?;
                for client in self.peers.keys().copied().collect::<Vec<_>>() {
                    self.send_rooms(client);
                }
            }
            ServerCommand::SetBots { count } => {
                let result = self.admin_command(&format!("bots {count}"));
                if result.message.starts_with("Error:") {
                    return Err(NetError(result.message));
                }
            }
            ServerCommand::ReloadMods => self.reload_mods()?,
            ServerCommand::ConfigureMatch {
                mode,
                map,
                difficulty,
            } => {
                let difficulty = crate::difficulty_from_name(&difficulty).map_err(NetError)?;
                self.configure_match(mode, &map, difficulty)?;
            }
            ServerCommand::ConfigureParty { settings } => {
                if self.settings.party.is_none() || !settings.valid() {
                    return Err(NetError(
                        "Only an active party leader can configure this party.".into(),
                    ));
                }
                let options = settings
                    .party
                    .ok_or_else(|| NetError("Missing party settings.".into()))?;
                let mut candidate = self.settings.configured_match(
                    &self.session.bundle.maps,
                    settings.mode,
                    &settings.map,
                    crate::difficulty_from_name(&settings.difficulty).map_err(NetError)?,
                )?;
                candidate.world.width = f32::from(options.width);
                candidate.world.height = f32::from(options.height);
                candidate.world.dynamic_arena = options.dynamic_arena;
                candidate.world.modded |= options.starting_level > 1;
                let party = candidate.party.as_mut().expect("active party");
                party.ally_slots = settings.bots;
                party.starting_level = options.starting_level;
                candidate.validate()?;
                self.settings = candidate;
                self.restart_round();
            }
            ServerCommand::RestartRound => self.restart_round(),
            ServerCommand::ReadAudit => {
                if self.settings.audit_file.is_empty() {
                    return Err(NetError("Audit logging is disabled by the host.".into()));
                }
                let lines =
                    crate::access::read_audit(&self.settings.audit_file).map_err(NetError)?;
                self.send_control(actor, &ServerControl::Audit { lines });
            }
        }
        Ok(())
    }
    pub(crate) fn local_role(&mut self, client: u64, role: ServerRole) -> Result<(), NetError> {
        if self.settings.kind != ServerKind::Community && !self.settings.central_owner_enabled() {
            return Err(NetError(
                "Server roles are limited to community servers.".into(),
            ));
        }
        crate::access::audit(
            self.settings.creator_audit_file(),
            &crate::access::AuditRecord {
                time: unix_time().as_secs(),
                actor: "host",
                action: "server_role",
                target: &client.to_string(),
                reason: &format!("{role:?}"),
            },
        )
        .map_err(NetError)?;
        self.access
            .local_grant(client, crate::access::Grant::for_role(role))
            .map_err(NetError)?;
        self.send_access(client);
        self.chat.refresh_role(client, role);
        self.send_rooms(client);
        Ok(())
    }
    pub(crate) fn community_result(&mut self, client: u64, result: Result<(), NetError>) {
        self.send_control(
            client,
            &ServerControl::Action {
                accepted: result.is_ok(),
                reason: result.err().map_or_else(String::new, |e| e.to_string()),
            },
        );
    }
}
