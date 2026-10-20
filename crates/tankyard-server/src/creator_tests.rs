use super::*;

// Only public signatures over fixed test messages are stored. Real sessions
// choose a fresh 256-bit nonce, so these fixtures cannot authorize a live peer.
const CLIENT: u64 = 41;
const NONCE: [u8; 32] = [73; 32];
const SERVER: &str = "creator-policy-test";

fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn fixtures() -> serde_json::Value {
    serde_json::from_str(include_str!("creator-fixtures.json")).unwrap()
}
struct Arena {
    server: UdpServer,
    player: EntityId,
    audit: std::path::PathBuf,
}
impl Drop for Arena {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.audit);
    }
}
impl Arena {
    fn new(enabled: bool, kind: ServerKind) -> Self {
        let suffix = u64::from_le_bytes(renet_netcode::generate_random_bytes());
        let audit = std::env::temp_dir().join(format!("tankyard-creator-{suffix:x}.jsonl"));
        let mut server = UdpServer::bind(ServerSettings {
            creator_admin: Some(enabled),
            kind,
            server_id: SERVER.into(),
            audit_file: audit.to_string_lossy().into(),
            bind: "127.0.0.1:0".into(),
            bot_count: 0,
            ban_file: String::new(),
            world: tankyard_core::WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        let player = server.session.world.add_player("Game Creator", false);
        server
            .access
            .add(CLIENT, None, unix_time().as_secs())
            .unwrap();
        server.peers.insert(
            CLIENT,
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
                admin_key: None,
                admin_nonce: None,
                last_admin_sequence: None,
                central_owner: false,
                central_proof_used: false,
                creator_nonce: Some(NONCE),
                creator: false,
                creator_session_nonce: None,
                last_owner_sequence: None,
                party_nonce: None,
            },
        );
        Self {
            server,
            player,
            audit,
        }
    }
    fn verify(&mut self) {
        self.server.handle_control(
            CLIENT,
            ClientControl::CreatorProof {
                signature: bytes(fixtures()["proof"].as_str().unwrap()),
            },
        );
        assert!(self.server.peers[&CLIENT].creator);
        assert!(self.server.session.world.tanks[0].game_creator);
    }
    fn command(&mut self, name: &str) -> Result<String, NetError> {
        let fixture = &fixtures()["commands"][name];
        self.server.apply_owner_extension(
            CLIENT,
            self.player,
            1,
            fixture["sequence"].as_u64().unwrap(),
            fixture["payload"].as_str().unwrap().as_bytes(),
            &bytes(fixture["signature"].as_str().unwrap()),
        )
    }
}

#[test]
fn valid_creator_identity_is_cosmetic_on_hosts_that_have_not_consented() {
    for kind in [ServerKind::Community, ServerKind::Official] {
        let mut arena = Arena::new(false, kind);
        arena.verify();
        assert!(!arena.server.creator_admin_available());
        assert!(
            arena
                .command("speed")
                .unwrap_err()
                .0
                .contains("disabled by this host")
        );
        assert_eq!(arena.server.session.world.tanks[0].speed_scale, 1.);
        assert!(!arena.server.session.world.config.modded);
        assert!(!arena.audit.exists());
    }
}

#[test]
fn official_default_authorizes_signed_tools_while_community_and_private_defaults_do_not() {
    for (kind, password, expected) in [
        (ServerKind::Official, "", true),
        (ServerKind::Community, "", false),
        (ServerKind::Official, "private", false),
        (ServerKind::Community, "private", false),
    ] {
        let mut arena = Arena::new(false, kind);
        arena.server.settings.creator_admin = None;
        arena.server.settings.password = password.into();
        arena.verify();
        assert_eq!(arena.server.creator_admin_available(), expected);
        assert_eq!(arena.command("speed").is_ok(), expected);
        assert_eq!(arena.audit.exists(), expected);
        if expected {
            assert_eq!(arena.server.session.world.tanks[0].speed_scale, 3.);
        }
    }
}

#[test]
fn consenting_official_and_community_hosts_execute_signed_bounded_commands_and_audit() {
    for kind in [ServerKind::Official, ServerKind::Community] {
        let mut arena = Arena::new(true, kind);
        arena.verify();
        assert!(arena.command("speed").is_ok());
        assert!(arena.command("god").is_ok());
        assert!(arena.command("bots").is_ok());
        assert!(arena.command("freeze").is_ok());
        assert_eq!(arena.server.session.world.tanks[0].speed_scale, 3.);
        assert!(arena.server.session.world.admin_modifiers[&arena.player].god);
        assert_eq!(arena.server.settings.bot_count, 4);
        assert!(!arena.server.settings.adaptive_bots);
        assert!(arena.server.session.world.bots_frozen);
        assert!(arena.server.session.world.config.modded);
        let audit = crate::access::read_audit(arena.audit.to_str().unwrap()).unwrap();
        assert_eq!(audit.len(), 8);
        assert!(audit[0].contains("creator_request"));
        assert!(audit[0].contains("speed"));
        assert!(audit[1].contains("creator_result"));
    }
}

#[test]
fn signatures_reject_tampering_replay_wrong_server_nonce_client_and_round() {
    let mut arena = Arena::new(true, ServerKind::Official);
    arena.verify();
    let fixture = &fixtures()["commands"]["speed"];
    let signature = bytes(fixture["signature"].as_str().unwrap());
    assert!(
        arena
            .server
            .apply_owner_extension(
                CLIENT,
                arena.player,
                1,
                0,
                br#"{"action":"speed","value":6.0}"#,
                &signature
            )
            .is_err()
    );
    assert!(
        arena
            .server
            .apply_owner_extension(
                CLIENT,
                arena.player,
                2,
                0,
                fixture["payload"].as_str().unwrap().as_bytes(),
                &signature
            )
            .is_err()
    );
    arena.server.settings.server_id = "another-server".into();
    assert!(arena.command("speed").is_err());
    arena.server.settings.server_id = SERVER.into();
    arena
        .server
        .peers
        .get_mut(&CLIENT)
        .unwrap()
        .creator_session_nonce = Some([74; 32]);
    assert!(arena.command("speed").is_err());
    arena
        .server
        .peers
        .get_mut(&CLIENT)
        .unwrap()
        .creator_session_nonce = Some(NONCE);
    assert!(
        arena
            .server
            .apply_owner_extension(
                CLIENT + 1,
                arena.player,
                1,
                0,
                fixture["payload"].as_str().unwrap().as_bytes(),
                &signature
            )
            .is_err()
    );
    assert!(arena.command("speed").is_ok());
    assert!(arena.command("speed").unwrap_err().0.contains("replayed"));
}

#[test]
fn rejected_values_and_unwritable_audits_cannot_mutate_the_arena() {
    let mut arena = Arena::new(true, ServerKind::Official);
    arena.verify();
    assert!(arena.command("invalid_speed").is_err());
    assert_eq!(arena.server.session.world.tanks[0].speed_scale, 1.);
    assert!(!arena.server.session.world.config.modded);
    let mut arena = Arena::new(true, ServerKind::Community);
    arena.verify();
    arena.server.settings.audit_file = std::env::temp_dir().to_string_lossy().into();
    assert!(arena.command("speed").is_err());
    assert!(!arena.server.session.world.config.modded);
    assert_eq!(arena.server.session.world.tanks[0].speed_scale, 1.);
}

#[test]
fn operator_revocation_is_immediate_and_old_commands_cannot_survive_reenable() {
    let mut arena = Arena::new(true, ServerKind::Official);
    arena.verify();
    arena.command("speed").unwrap();
    arena.command("god").unwrap();
    arena.command("freeze").unwrap();
    assert!(
        arena
            .server
            .admin_command("creator-admin off")
            .message
            .contains("revoked")
    );
    assert!(arena.server.session.world.admin_modifiers.is_empty());
    assert!(!arena.server.session.world.bots_frozen);
    assert_eq!(arena.server.session.world.tanks[0].speed_scale, 1.);
    assert!(arena.server.session.world.tanks[0].game_creator);
    assert!(arena.command("bots").is_err());
    assert!(
        arena
            .server
            .admin_command("creator-admin on")
            .message
            .contains("enabled")
    );
    assert!(
        arena
            .command("bots")
            .unwrap_err()
            .0
            .contains("verification")
    );
    assert_eq!(arena.server.settings.bot_count, 0);
    // A failed audit must never prevent an operator from turning powers off.
    arena.server.settings.audit_file = std::env::temp_dir().to_string_lossy().into();
    assert!(
        arena
            .server
            .admin_command("creator-admin off")
            .message
            .contains("revoked")
    );
    assert_eq!(arena.server.settings.creator_admin, Some(false));
}

#[test]
fn signed_commands_still_observe_rate_limits() {
    let mut arena = Arena::new(true, ServerKind::Community);
    arena.verify();
    let budget_time = unix_time().as_secs().saturating_add(30);
    for _ in 0..6 {
        assert!(arena.server.access.take_admin_token(CLIENT, budget_time));
    }
    assert!(arena.command("speed").unwrap_err().0.contains("Wait"));
    assert_eq!(arena.server.session.world.tanks[0].speed_scale, 1.);
    assert!(!arena.server.session.world.config.modded);
    assert!(!arena.audit.exists());
}

#[test]
fn trusted_handler_failures_are_contained_and_the_outcome_is_audited() {
    let mut arena = Arena::new(true, ServerKind::Community);
    arena.server.owner_extension = Some(|_, _, _| panic!("host extension failed"));
    arena.verify();
    assert!(
        arena
            .command("speed")
            .unwrap_err()
            .0
            .contains("extension failed")
    );
    let audit = crate::access::read_audit(arena.audit.to_str().unwrap()).unwrap();
    assert_eq!(audit.len(), 2);
    assert!(audit[1].contains("creator_result") && audit[1].contains("failed"));
    assert_eq!(arena.server.session.world.tanks[0].speed_scale, 1.);
}

#[test]
fn creator_moderation_is_host_local_and_preserves_regular_achievement_eligibility() {
    let mut arena = Arena::new(true, ServerKind::Official);
    arena.verify();
    let player = arena.server.session.world.add_player("Visitor", false);
    assert_eq!(player, 2);
    arena
        .server
        .access
        .add(42, None, unix_time().as_secs())
        .unwrap();
    let peer = Peer {
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
        admin_key: None,
        admin_nonce: None,
        last_admin_sequence: None,
        central_owner: false,
        central_proof_used: false,
        creator_nonce: None,
        creator: false,
        creator_session_nonce: None,
        last_owner_sequence: None,
        party_nonce: None,
    };
    arena.server.peers.insert(42, peer);
    arena.command("mute").unwrap();
    assert!(arena.server.access.muted(42, unix_time().as_secs()));
    assert!(!arena.server.session.world.config.modded);
    // Even a valid creator signature cannot turn a moderation request into a
    // host-code reload, role escalation, or policy change.
    let mut arena = Arena::new(true, ServerKind::Official);
    arena.verify();
    assert!(arena.command("bad_moderation").is_err());
    assert!(!arena.server.session.world.config.modded);
}
