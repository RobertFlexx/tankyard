use std::{sync::Arc, thread, time::Duration};
use tankyard_core::{Mode, WorldConfig};
use tankyard_protocol::{
    ServerCommand, ServerKind, ServerPermission as Permission, ServerRole as Role,
};
use tankyard_server::{
    NetworkClient, ServerSettings, UdpServer,
    access::{AccountContext, AccountVerifier, VerifiedAccount},
};
const STEP: Duration = Duration::from_nanos(16_666_667);
fn settings() -> ServerSettings {
    ServerSettings {
        bind: "127.0.0.1:0".into(),
        server_id: "test-server".into(),
        bot_count: 0,
        ban_file: String::new(),
        world: WorldConfig {
            mode: Mode::Sandbox,
            target_count: 0,
            ..Default::default()
        },
        ..Default::default()
    }
}
fn pump(server: &mut UdpServer, clients: &mut [&mut NetworkClient], ticks: usize) {
    for _ in 0..ticks {
        for client in clients.iter_mut() {
            client.update(STEP).unwrap();
        }
        server.update(STEP).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
}
fn ready(server: &mut UdpServer, clients: &mut [&mut NetworkClient]) {
    for _ in 0..240 {
        pump(server, clients, 1);
        if clients.iter().all(|c| c.is_ready()) {
            return;
        }
    }
    panic!("handshake did not finish");
}
#[test]
fn names_roles_and_forged_proofs_cannot_enable_private_creator_tools() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let mut player =
        NetworkClient::connect(&server.address.to_string(), "Game Creator", "").unwrap();
    ready(&mut server, &mut [&mut player]);
    pump(&mut server, &mut [&mut player], 12);
    assert!(player.creator_challenge.is_some());
    assert!(!player.owner_extension_available);
    let id = player.player_id.unwrap();
    server.admin_command(&format!("role {id} owner"));
    pump(&mut server, &mut [&mut player], 8);
    assert_eq!(player.role, Role::Owner);
    assert!(player.submit_creator_proof(vec![0; 64]));
    assert!(player.owner_extension(br#"{"action":"god","enabled":true}"#.to_vec(), vec![0; 64]));
    pump(&mut server, &mut [&mut player], 12);
    assert!(
        !server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .game_creator
    );
    assert!(
        player
            .last_action
            .as_deref()
            .is_some_and(|message| message.contains("Verified creator"))
    );
    assert!(!server.session.world.config.modded);
}
#[test]
fn match_controls_preserve_connections_roles_and_report_authoritative_settings() {
    let mut config = settings();
    config.world.mode = Mode::Ffa;
    config.bot_count = 2;
    config.difficulty = tankyard_ai::Difficulty::Expert;
    let mut server = UdpServer::bind(config).unwrap();
    let address = server.address.to_string();
    let mut owner = NetworkClient::connect(&address, "Owner", "").unwrap();
    let mut player = NetworkClient::connect(&address, "Player", "").unwrap();
    ready(&mut server, &mut [&mut owner, &mut player]);
    pump(&mut server, &mut [&mut owner, &mut player], 8);
    assert_eq!(owner.match_settings.as_ref().unwrap().difficulty, "Expert");
    assert_eq!(owner.match_settings.as_ref().unwrap().bots, 2);
    let result = server.admin_command(&format!("role {} owner", owner.player_id.unwrap()));
    assert!(!result.message.starts_with("Error:"));
    pump(&mut server, &mut [&mut owner, &mut player], 8);
    let round = server.session.world.match_state.round;
    player.permissions.push(Permission::Configure);
    assert!(player.admin(ServerCommand::RestartRound));
    pump(&mut server, &mut [&mut owner, &mut player], 8);
    assert_eq!(server.session.world.match_state.round, round);
    assert!(player.last_action.as_ref().unwrap().contains("denied"));

    assert!(owner.admin(ServerCommand::ConfigureMatch {
        mode: Mode::Domination,
        map: "maze".into(),
        difficulty: "Advanced".into(),
    }));
    pump(&mut server, &mut [&mut owner, &mut player], 16);
    assert_eq!(server.session.world.match_state.round, round + 1);
    assert_eq!(server.session.world.config.mode, Mode::Domination);
    assert_eq!(server.session.world.map.id, "maze");
    assert_eq!(
        server.session.bots.difficulty,
        tankyard_ai::Difficulty::Advanced
    );
    assert_eq!(server.player_count(), 2);
    assert!(owner.is_ready() && player.is_ready());
    assert_eq!(owner.role, Role::Owner);
    assert_eq!(
        owner.match_settings.as_ref().unwrap().difficulty,
        "Advanced"
    );
    assert_eq!(
        owner.last_action.as_deref(),
        Some("Server command applied.")
    );

    assert!(owner.admin(ServerCommand::ConfigureMatch {
        mode: Mode::Ffa,
        map: "missing".into(),
        difficulty: "Normal".into(),
    }));
    pump(&mut server, &mut [&mut owner, &mut player], 8);
    assert_eq!(server.session.world.match_state.round, round + 1);
    assert_eq!(server.session.world.config.mode, Mode::Domination);
    assert!(
        owner
            .last_action
            .as_ref()
            .unwrap()
            .contains("not available")
    );
    assert!(owner.admin(ServerCommand::RestartRound));
    pump(&mut server, &mut [&mut owner, &mut player], 16);
    assert_eq!(server.session.world.match_state.round, round + 2);
    assert!(owner.is_ready() && player.is_ready());
    assert_eq!(owner.role, Role::Owner);
}
#[test]
fn community_delegation_chat_privacy_and_muting_are_authoritative() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let address = server.address.to_string();
    let mut owner = NetworkClient::connect(&address, "Owner", "").unwrap();
    let mut moderator = NetworkClient::connect(&address, "Moderator", "").unwrap();
    let mut player = NetworkClient::connect(&address, "Player", "").unwrap();
    ready(&mut server, &mut [&mut owner, &mut moderator, &mut player]);
    let owner_id = owner.player_id.unwrap();
    let moderator_id = moderator.player_id.unwrap();
    assert!(
        !server
            .admin_command(&format!("role {owner_id} owner"))
            .message
            .starts_with("Error:")
    );
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert_eq!(owner.role, Role::Owner);
    assert!(owner.chat_enabled);
    owner.snapshot.as_mut().unwrap().match_state.round = 0;
    assert!(owner.admin(ServerCommand::Kick {
        player: moderator_id,
        reason: String::new()
    }));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(
        owner
            .last_action
            .as_ref()
            .is_some_and(|s| s.contains("Match changed"))
    );
    assert_eq!(server.player_count(), 3);
    assert!(owner.admin(ServerCommand::Nominate {
        player: moderator_id,
        role: Role::Moderator,
        permissions: vec![Permission::Kick, Permission::Mute]
    }));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert_eq!(moderator.role, Role::Moderator);
    // A client can alter its own displayed permission list, but the server
    // must reject the resulting request before changing access.
    moderator.permissions.push(Permission::ManageRoles);
    assert!(moderator.admin(ServerCommand::Nominate {
        player: owner_id,
        role: Role::Player,
        permissions: vec![]
    }));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(
        moderator
            .last_action
            .as_ref()
            .is_some_and(|s| s.contains("denied"))
    );
    assert_eq!(owner.role, Role::Owner);
    assert!(owner.admin(ServerCommand::CreateRoom {
        id: "staff".into(),
        name: "Staff".into(),
        minimum_role: Role::Moderator
    }));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(moderator.rooms.iter().any(|r| r.id == "staff"));
    assert!(!player.rooms.iter().any(|r| r.id == "staff"));
    assert!(owner.join_room("staff"));
    assert!(moderator.join_room("staff"));
    assert!(player.join_room("staff"));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(moderator.send_chat("staff", "Hello"));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(owner.chat_messages.iter().any(|m| m.text == "Hello"));
    assert!(!player.chat_messages.iter().any(|m| m.text == "Hello"));
    assert!(owner.admin(ServerCommand::Mute {
        player: moderator_id,
        minutes: 10
    }));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(moderator.send_chat("staff", "Muted message"));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(
        !owner
            .chat_messages
            .iter()
            .any(|m| m.text == "Muted message")
    );
    assert!(owner.admin(ServerCommand::Nominate {
        player: moderator_id,
        role: Role::Player,
        permissions: vec![]
    }));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert_eq!(moderator.role, Role::Player);
    assert!(!moderator.rooms.iter().any(|r| r.id == "staff"));
    assert!(owner.send_chat("staff", "Private message"));
    pump(
        &mut server,
        &mut [&mut owner, &mut moderator, &mut player],
        8,
    );
    assert!(
        !moderator
            .chat_messages
            .iter()
            .any(|m| m.text == "Private message")
    );
}
struct Verifier;
impl AccountVerifier for Verifier {
    fn verify(
        &self,
        credential: &str,
        ctx: &AccountContext<'_>,
    ) -> Result<VerifiedAccount, String> {
        if credential != "test-proof"
            || ctx.server_id != "test-server"
            || ctx.client_id != 17
            || !ctx.secure_transport
        {
            return Err("denied".into());
        }
        Ok(VerifiedAccount {
            id: "test-account".into(),
            expires_at: ctx.now + 600,
        })
    }
}
#[test]
fn official_connections_use_server_signed_tokens_and_never_offer_chat() {
    let key = [7; 32];
    let mut config = settings();
    config.kind = ServerKind::Official;
    config.chat_enabled = true;
    config.require_accounts = true;
    config.netcode_private_key = Some("07".repeat(32));
    let mut server = UdpServer::bind(config).unwrap();
    server.set_account_verifier(Arc::new(Verifier)).unwrap();
    let bytes = tankyard_server::accounts::issue_connection_token(
        &key,
        17,
        vec![server.address],
        Duration::from_secs(60),
    )
    .unwrap();
    let token = tankyard_server::accounts::decode_connection_token(&bytes).unwrap();
    let mut client =
        NetworkClient::connect_authenticated(token, "Pilot", "", Some("test-proof".into()))
            .unwrap();
    ready(&mut server, &mut [&mut client]);
    assert_eq!(client.kind, ServerKind::Official);
    assert!(!client.chat_enabled);
    assert!(client.permissions.is_empty());
    assert!(client.rooms.is_empty());
    assert!(
        server
            .admin_command(&format!("role {} owner", client.player_id.unwrap()))
            .message
            .starts_with("Error:")
    );
    // Bypassing a local UI guard must not turn chat on at the server.
    client.chat_enabled = true;
    assert!(client.send_chat("lobby", "Hello"));
    pump(&mut server, &mut [&mut client], 8);
    assert!(client.chat_messages.is_empty());
    assert!(client.last_action.is_some());
}
#[test]
fn modded_catalog_rules_and_custom_maps_reach_an_unmodified_client() {
    let root = std::env::temp_dir().join(format!("tankyard-network-mod-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut content = tankyard_content::Content::builtin();
    let base = content
        .classes
        .iter_mut()
        .find(|c| c.id == "vector")
        .unwrap();
    base.upgrades.push("custom_tank".into());
    let base = base.clone();
    let mut custom = base.clone();
    custom.id = "custom_tank".into();
    custom.name = "Custom Tank".into();
    custom.parents = vec!["vector".into()];
    custom.upgrades.clear();
    custom.tier = 1;
    custom.min_level = 15;
    let mut map = tankyard_content::builtin_maps().remove(0);
    map.id = "custom_map".into();
    map.name = "Custom Map".into();
    let manifest = serde_json::json!({"api_version":1,"id":"custom","name":"Custom","version":"1","classes":[base,custom],"maps":[map],"rules":{"movement_multiplier":1.2}});
    std::fs::write(
        root.join("mod.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let mut config = settings();
    config.mods.push(root.clone());
    config.world.map = "custom_map".into();
    let mut server = UdpServer::bind(config).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Pilot", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    assert!(client.bundle.content.class("custom_tank").is_some());
    let snapshot = client.snapshot.as_ref().unwrap();
    assert!(snapshot.config.modded);
    assert_eq!(snapshot.config.map, "custom_map");
    assert_eq!(snapshot.config.rules.movement_multiplier, 1.2);
    assert!(client.evolve("custom_tank"));
    pump(&mut server, &mut [&mut client], 8);
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .find(|t| Some(t.id) == client.player_id)
            .unwrap()
            .class,
        "custom_tank"
    );
    std::fs::remove_dir_all(root).unwrap();
}
