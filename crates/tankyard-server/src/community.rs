use crate::{NetError, UdpServer, unix_time};
use tankyard_protocol::{
    ChatMessage, OwnerTank, ServerCommand, ServerControl, ServerKind, ServerRole,
};

/// Staff can summon bosses up to this many at once, the same as Sandbox.
const MOST_SUMMONED_BOSSES: usize = 12;

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
            mode: self.arenas[0].session.world.config.mode,
            map: self.arenas[0].session.world.config.map.clone(),
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
        let candidate = self.settings.configured_match(
            &self.arenas[0].session.bundle.maps,
            mode,
            map,
            difficulty,
        )?;
        self.settings = candidate;
        self.restart_round();
        Ok(())
    }
    pub(crate) fn configure_rules(
        &mut self,
        rules: tankyard_mods::GameRules,
    ) -> Result<(), NetError> {
        rules.validate().map_err(NetError)?;
        for arena in &mut self.arenas {
            arena.session.world.set_rules(rules).map_err(NetError)?;
        }
        self.settings.world.rules = rules;
        self.settings.world.modded = true;
        self.settings.rules_override = Some(rules);
        self.announce("The host updated the arena rules.".into());
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
        let tank = self.arenas[self.arena_of(client)]
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
            text: crate::chat::clean_text(&text),
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
        let arena = self.arena_of(actor);
        let own_tank = if matches!(
            command,
            ServerCommand::ReadOwnTank | ServerCommand::SetOwnTank { .. }
        ) {
            let peer = self
                .peers
                .get(&actor)
                .filter(|peer| {
                    self.settings.central_owner_enabled()
                        && peer.central_owner
                        && peer.disconnect_at.is_none()
                        && self.access.grant(actor, now).role == ServerRole::Owner
                })
                .ok_or_else(|| NetError("Verified central server owner required.".into()))?;
            let world = &self.arenas[arena].session.world;
            let id = peer
                .entity
                .filter(|id| world.tanks.iter().any(|t| t.id == *id && !t.bot))
                .ok_or_else(|| NetError("Your tank is no longer in this arena.".into()))?;
            if let ServerCommand::SetOwnTank { tank } = &command
                && world.content.class(&tank.class).is_none()
            {
                return Err(NetError("That class is unavailable in this arena.".into()));
            }
            Some(id)
        } else {
            None
        };
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
        // The staff panel polls health while it's open, and reading the log
        // would only push real actions out of view. Neither changes anything.
        if !matches!(
            command,
            ServerCommand::ReadHealth | ServerCommand::ReadAudit | ServerCommand::ReadOwnTank
        ) {
            let named = |entity: u64| match self.tank_name(entity) {
                Some(name) => format!("{name} (#{entity})"),
                None => format!("#{entity}"),
            };
            let banned = |id: u64| {
                self.ban_rows()
                    .into_iter()
                    .find(|row| row.id == id)
                    .map_or_else(|| "an expired ban".to_owned(), |row| row.who)
            };
            let actor_name = self
                .peers
                .get(&actor)
                .and_then(|peer| peer.entity)
                .and_then(|entity| self.tank_name(entity))
                .unwrap_or_default();
            let target_name = target
                .and_then(|client| self.peers.get(&client))
                .and_then(|peer| peer.entity)
                .or(own_tank)
                .and_then(|entity| self.tank_name(entity))
                .unwrap_or_default();
            crate::access::audit(
                self.settings.creator_audit_file(),
                &crate::access::AuditRecord {
                    time: now,
                    actor: &actor.to_string(),
                    action: "server_moderation",
                    target: &target
                        .or(own_tank.map(|_| actor))
                        .map_or_else(|| "server".into(), |t| t.to_string()),
                    reason: &command.describe(named, banned),
                    actor_name: &actor_name,
                    target_name: &target_name,
                },
            )
            .map_err(NetError)?;
        }
        match command {
            ServerCommand::ReadOwnTank | ServerCommand::SetOwnTank { .. } => {
                // Both were checked before the token and the audit line.
                let Some(id) = own_tank else {
                    return Err(NetError("Verified central server owner required.".into()));
                };
                let world = &mut self.arenas[arena].session.world;
                if let ServerCommand::SetOwnTank { tank } = command
                    && !world.tune_owner_tank(id, &tank.class, tank.weapons, tank.invulnerable)
                {
                    return Err(NetError("Could not apply tank settings.".into()));
                }
                let Some(class) = world
                    .tanks
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.class.clone())
                else {
                    return Err(NetError("Your tank is no longer in this arena.".into()));
                };
                let tank = OwnerTank {
                    class,
                    weapons: world.owner_weapons(id),
                    invulnerable: world.owner_invulnerable(id),
                };
                self.send_control(actor, &ServerControl::OwnTank { tank });
            }
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
                // The map may come from a pack the leader has just added.
                let plan = self.party_plan.take();
                let maps = plan
                    .as_ref()
                    .map_or(&self.arenas[0].session.bundle.maps, |(_, loaded)| {
                        &loaded.bundle.maps
                    });
                let mut candidate = self.settings.configured_match(
                    maps,
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
                match plan {
                    // The map may only exist in the plan's packs.
                    Some(plan) => self.adopt_party_plan(&mut candidate, plan)?,
                    None => candidate.validate()?,
                }
                self.settings = candidate;
                self.restart_round();
            }
            ServerCommand::RestartRound => self.restart_round(),
            ServerCommand::ConfigureRules { rules } => self.configure_rules(rules)?,
            ServerCommand::ReadAudit => {
                if self.settings.creator_audit_file().is_empty() {
                    return Err(NetError("Audit logging is disabled by the host.".into()));
                }
                let lines = crate::access::read_audit(self.settings.creator_audit_file())
                    .map_err(NetError)?;
                self.send_control(actor, &ServerControl::Audit { lines });
            }
            ServerCommand::ReadBans => {
                let bans = self.ban_rows();
                self.send_control(actor, &ServerControl::Bans { bans });
            }
            ServerCommand::LiftBan { ban } => {
                self.lift_ban(ban)?;
                let bans = self.ban_rows();
                self.send_control(actor, &ServerControl::Bans { bans });
            }
            ServerCommand::ReadHealth => {
                let health = self.health();
                self.send_control(actor, &ServerControl::Health { health });
            }
            ServerCommand::SummonBoss { boss } => {
                let kind = tankyard_core::BossKind::from_id(&boss)
                    .ok_or_else(|| NetError("That boss doesn't exist.".into()))?;
                let arena = self.arena_of(actor);
                let world = &mut self.arenas[arena].session.world;
                if world.bosses.len() >= MOST_SUMMONED_BOSSES {
                    return Err(NetError(format!(
                        "This arena already has {MOST_SUMMONED_BOSSES} bosses."
                    )));
                }
                if world.spawn_boss(kind) == 0 {
                    return Err(NetError("No clear space for another boss.".into()));
                }
                world.config.modded = true;
                self.replicate_events(arena);
            }
            ServerCommand::DismissBosses => {
                let arena = self.arena_of(actor);
                let world = &mut self.arenas[arena].session.world;
                let bosses: std::collections::HashSet<_> =
                    world.bosses.iter().map(|boss| boss.id).collect();
                world.projectiles.retain(|p| !bosses.contains(&p.owner));
                world.bosses.clear();
                world.config.modded = true;
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
        let target_name = self
            .peers
            .get(&client)
            .and_then(|peer| peer.entity)
            .and_then(|entity| self.tank_name(entity))
            .unwrap_or_default();
        crate::access::audit(
            self.settings.creator_audit_file(),
            &crate::access::AuditRecord {
                time: unix_time().as_secs(),
                actor: "host",
                action: "server_role",
                target: &client.to_string(),
                reason: &format!("{role:?}"),
                target_name: &target_name,
                ..Default::default()
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
