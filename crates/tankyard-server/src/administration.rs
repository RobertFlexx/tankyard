//! Authenticated, server-local staff sessions. No game-wide moderation authority.
use crate::{NetError, UdpServer, unix_time};
use tankyard_core::ServerBadge;
use tankyard_protocol::*;

impl UdpServer {
    pub(crate) fn admin_challenge(&mut self, client: u64) {
        let nonce = renet_netcode::generate_random_bytes::<32>();
        if let Some(peer) = self.peers.get_mut(&client) {
            peer.admin_nonce = Some(nonce);
            peer.last_admin_sequence = None;
            peer.central_proof_used = false;
        }
        self.send_control(
            client,
            &ServerControl::AdminChallenge {
                nonce,
                server_id: self.settings.server_id.clone(),
                central_owner: self.settings.central_owner_enabled(),
            },
        );
    }
    pub(crate) fn central_owner_proof(
        &mut self,
        client: u64,
        signature: &[u8],
    ) -> Result<(), NetError> {
        if !self.settings.central_owner_enabled() {
            return Err(NetError(
                "Central owner access is disabled on this server.".into(),
            ));
        }
        let peer = self
            .peers
            .get_mut(&client)
            .ok_or_else(|| NetError("Connection closed.".into()))?;
        if peer.central_proof_used || peer.disconnect_at.is_some() {
            return Err(NetError(
                "A fresh central owner challenge is required.".into(),
            ));
        }
        peer.central_proof_used = true;
        let nonce = peer
            .admin_nonce
            .ok_or_else(|| NetError("Administration handshake required.".into()))?;
        let key = peer
            .admin_key
            .ok_or_else(|| NetError("Administration identity required.".into()))?;
        let message = central_owner_challenge(client, &nonce, &self.settings.server_id, &key);
        if !verify_admin_signature(&CENTRAL_OWNER_PUBLIC_KEY, &message, signature) {
            return Err(NetError("Invalid central owner proof.".into()));
        }
        crate::access::audit(
            self.settings.creator_audit_file(),
            &crate::access::AuditRecord {
                time: unix_time().as_secs(),
                actor: &client.to_string(),
                action: "central_owner_identity",
                target: &self.settings.server_id,
                reason: "verified",
            },
        )
        .map_err(NetError)?;
        // A connection grant is never written into account role files.
        self.access
            .session_grant(client, crate::access::Grant::for_role(ServerRole::Owner))
            .map_err(NetError)?;
        self.peers.get_mut(&client).unwrap().central_owner = true;
        self.send_access(client);
        Ok(())
    }
    pub(crate) fn signed_server_command(
        &mut self,
        client: u64,
        round: u32,
        sequence: u64,
        command: ServerCommand,
        signature: &[u8],
    ) -> Result<(), NetError> {
        if round != self.session.world.match_state.round || !command.valid() {
            return Err(NetError(
                "Match changed or command is invalid. Select the player again.".into(),
            ));
        }
        let peer = self
            .peers
            .get_mut(&client)
            .ok_or_else(|| NetError("Connection closed.".into()))?;
        let key = peer
            .admin_key
            .ok_or_else(|| NetError("Administration identity required.".into()))?;
        let nonce = peer
            .admin_nonce
            .ok_or_else(|| NetError("Administration handshake required.".into()))?;
        let payload = serde_json::to_vec(&command).map_err(|e| NetError(e.to_string()))?;
        if peer.disconnect_at.is_some()
            || peer
                .last_admin_sequence
                .is_some_and(|last| sequence <= last)
            || !verify_admin_signature(
                &key,
                &admin_command(
                    client,
                    &nonce,
                    &self.settings.server_id,
                    round,
                    sequence,
                    &payload,
                ),
                signature,
            )
        {
            return Err(NetError(
                "Invalid or replayed administration signature.".into(),
            ));
        }
        peer.last_admin_sequence = Some(sequence);
        self.server_command(client, command)
    }
    pub(crate) fn sync_player_badge(&mut self, client: u64) {
        let Some(peer) = self.peers.get(&client) else {
            return;
        };
        let badge = if peer.central_owner
            && self.settings.central_owner_enabled()
            && self.access.grant(client, unix_time().as_secs()).role == ServerRole::Owner
        {
            ServerBadge::CentralOwner
        } else {
            match self.access.grant(client, unix_time().as_secs()).role {
                ServerRole::Player => ServerBadge::Player,
                ServerRole::Moderator => ServerBadge::Moderator,
                ServerRole::Admin => ServerBadge::Admin,
                ServerRole::Owner => ServerBadge::Host,
            }
        };
        if let Some(tank) = self
            .session
            .world
            .tanks
            .iter_mut()
            .find(|tank| Some(tank.id) == peer.entity)
        {
            tank.server_badge = badge;
        }
    }
    pub(crate) fn warn_client(&mut self, client: u64, reason: String) {
        self.send_control(
            client,
            &ServerControl::Notice {
                notice: ServerNotice {
                    kind: NoticeKind::Warning,
                    text: reason,
                },
            },
        );
    }
    pub(crate) fn announce(&mut self, text: String) {
        self.broadcast_control(&ServerControl::Notice {
            notice: ServerNotice {
                kind: NoticeKind::Announcement,
                text,
            },
        });
    }
    pub(crate) fn set_central_owner_admin(&mut self, enabled: bool) -> Result<String, NetError> {
        let mut candidate = self.settings.clone();
        candidate.central_owner_admin = Some(enabled);
        candidate.validate()?;
        let audit = crate::access::audit(
            self.settings.creator_audit_file(),
            &crate::access::AuditRecord {
                time: unix_time().as_secs(),
                actor: "console",
                action: "central_owner_policy",
                target: &self.settings.server_id,
                reason: if enabled { "enabled" } else { "disabled" },
            },
        );
        if enabled {
            audit.map_err(NetError)?;
        } else if let Err(error) = audit {
            log::error!("Could not audit owner revocation: {error}");
        }
        self.settings.central_owner_admin = Some(enabled);
        for client in self.peers.keys().copied().collect::<Vec<_>>() {
            if let Some(peer) = self.peers.get_mut(&client)
                && peer.central_owner
            {
                peer.central_owner = false;
                self.access
                    .session_grant(client, crate::access::Grant::for_role(ServerRole::Player))
                    .map_err(NetError)?;
            }
            self.admin_challenge(client);
            self.send_access(client);
        }
        Ok(format!(
            "Central owner administration {}.",
            if enabled { "enabled" } else { "disabled" }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Peer;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::time::Duration;

    struct Arena {
        server: UdpServer,
        key: Ed25519KeyPair,
        audit: std::path::PathBuf,
    }
    impl Drop for Arena {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.audit);
        }
    }
    impl Arena {
        fn new() -> Self {
            let audit = std::env::temp_dir().join(format!(
                "tankyard-owner-audit-{:x}.jsonl",
                u64::from_le_bytes(renet_netcode::generate_random_bytes())
            ));
            let mut settings = crate::ServerSettings::central();
            settings.bind = "127.0.0.1:0".into();
            settings.bot_count = 0;
            settings.world.target_count = 0;
            settings.ban_file.clear();
            settings.audit_file = audit.to_string_lossy().into();
            let mut server = UdpServer::bind(settings).unwrap();
            let key = Ed25519KeyPair::from_seed_unchecked(&[17; 32]).unwrap();
            let player = server
                .session
                .world
                .add_player("Central Server Owner", false);
            server.access.add(7, None, unix_time().as_secs()).unwrap();
            server.peers.insert(
                7,
                Peer {
                    entity: Some(player),
                    connected_at: Duration::ZERO,
                    last_input_at: Duration::ZERO,
                    last_sequence: None,
                    control_tokens: 24.,
                    queued_control: None,
                    input_tokens: 256.,
                    voice_tokens: 8.,
                    last_voice_sequence: None,
                    voice_muted: false,
                    spectating: None,
                    disconnect_at: None,
                    creator_nonce: None,
                    creator: false,
                    creator_session_nonce: None,
                    last_owner_sequence: None,
                    party_nonce: None,
                    party_in_menu: false,
                    admin_key: Some(key.public_key().as_ref().try_into().unwrap()),
                    admin_nonce: Some([3; 32]),
                    last_admin_sequence: None,
                    central_owner: false,
                    central_proof_used: false,
                },
            );
            Self { server, key, audit }
        }
        fn proof(&self) -> Vec<u8> {
            // Only a signature over this fixed fixture nonce is public. Real
            // connections use unpredictable server-generated nonces.
            serde_json::from_str::<serde_json::Value>(include_str!("central-owner-fixture.json"))
                .unwrap()["proof"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect()
        }
        fn sign(&self, sequence: u64, round: u32, command: &ServerCommand) -> Vec<u8> {
            self.key
                .sign(&admin_command(
                    7,
                    &[3; 32],
                    "tankyard-central",
                    round,
                    sequence,
                    &serde_json::to_vec(command).unwrap(),
                ))
                .as_ref()
                .to_vec()
        }
    }
    #[test]
    fn central_identity_requires_its_key_bound_to_this_connection_and_host_policy() {
        let mut arena = Arena::new();
        assert!(arena.server.central_owner_proof(7, &[0; 64]).is_err());
        assert_eq!(
            arena.server.access.grant(7, unix_time().as_secs()).role,
            ServerRole::Player
        );
        assert!(
            arena.server.central_owner_proof(7, &arena.proof()).is_err(),
            "proof attempts are single use"
        );
        for change in 0..4 {
            let mut arena = Arena::new();
            match change {
                0 => arena.server.settings.central_owner_admin = Some(false),
                1 => arena.server.settings.kind = ServerKind::Community,
                2 => arena.server.peers.get_mut(&7).unwrap().admin_key = Some([0; 32]),
                _ => arena.server.peers.get_mut(&7).unwrap().admin_nonce = Some([4; 32]),
            }
            assert!(arena.server.central_owner_proof(7, &arena.proof()).is_err());
        }
    }
    #[test]
    fn signed_owner_has_scoped_authority_and_revocation_invalidates_prior_commands() {
        let mut arena = Arena::new();
        arena.server.central_owner_proof(7, &arena.proof()).unwrap();
        assert_eq!(
            arena.server.access.grant(7, unix_time().as_secs()).role,
            ServerRole::Owner
        );
        assert_eq!(
            arena.server.session.world.tanks[0].server_badge,
            ServerBadge::CentralOwner
        );
        assert!(!arena.server.session.world.tanks[0].game_creator);
        assert!(arena.server.session.world.admin_modifiers.is_empty());
        let round = arena.server.session.world.match_state.round;
        let command = ServerCommand::Announce {
            text: "Hello arena".into(),
        };
        let signature = arena.sign(0, round, &command);
        arena
            .server
            .signed_server_command(7, round, 0, command.clone(), &signature)
            .unwrap();
        assert!(
            arena
                .server
                .signed_server_command(7, round, 0, command.clone(), &signature)
                .is_err()
        );
        assert!(
            arena
                .server
                .signed_server_command(
                    7,
                    round,
                    1,
                    ServerCommand::Announce {
                        text: "Changed".into()
                    },
                    &signature
                )
                .is_err()
        );
        assert!(
            arena
                .server
                .signed_server_command(
                    7,
                    round + 1,
                    1,
                    command.clone(),
                    &arena.sign(1, round + 1, &command)
                )
                .is_err()
        );
        arena.server.set_central_owner_admin(false).unwrap();
        assert_eq!(
            arena.server.access.grant(7, unix_time().as_secs()).role,
            ServerRole::Player
        );
        assert_eq!(
            arena.server.session.world.tanks[0].server_badge,
            ServerBadge::Player
        );
        arena.server.set_central_owner_admin(true).unwrap();
        assert!(
            arena
                .server
                .signed_server_command(
                    7,
                    round,
                    1,
                    command.clone(),
                    &arena.sign(1, round, &command)
                )
                .is_err()
        );
        assert!(arena.server.central_owner_proof(7, &arena.proof()).is_err());
    }
    #[test]
    fn host_commands_require_the_registered_session_key_even_with_an_owner_grant() {
        let mut arena = Arena::new();
        arena.server.settings.kind = ServerKind::Community;
        arena.server.settings.central_owner_admin = Some(false);
        assert!(
            arena
                .server
                .admin_command(&format!("owner-client 7 {}", "00".repeat(32)))
                .message
                .starts_with("Error:")
        );
        assert_eq!(
            arena.server.access.grant(7, unix_time().as_secs()).role,
            ServerRole::Player
        );
        let public: String = arena
            .key
            .public_key()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert!(
            !arena
                .server
                .admin_command(&format!("owner-client 7 {public}"))
                .message
                .starts_with("Error:")
        );
        assert_eq!(
            arena.server.session.world.tanks[0].server_badge,
            ServerBadge::Host
        );
        let round = arena.server.session.world.match_state.round;
        let command = ServerCommand::SetBots { count: 0 };
        assert!(
            arena
                .server
                .signed_server_command(7, round, 0, command.clone(), &[0; 64])
                .is_err()
        );
        arena
            .server
            .signed_server_command(
                7,
                round,
                0,
                command.clone(),
                &arena.sign(0, round, &command),
            )
            .unwrap();
        arena.server.restart_round();
        assert_eq!(
            arena
                .server
                .session
                .world
                .tanks
                .iter()
                .find(|tank| !tank.bot)
                .unwrap()
                .server_badge,
            ServerBadge::Host
        );
    }
}
