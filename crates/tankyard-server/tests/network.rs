use std::{
    net::UdpSocket,
    thread,
    time::{Duration, Instant},
};
use tankyard_content::Content;
use tankyard_core::{Mode, PlayerInput, V2, WorldConfig};
use tankyard_server::{NetworkClient, ServerSettings, UdpServer};

const STEP: Duration = Duration::from_nanos(16_666_667);
fn settings() -> ServerSettings {
    ServerSettings {
        bind: "127.0.0.1:0".into(),
        bot_count: 0,
        max_players: 8,
        allow_sandbox: true,
        ban_file: String::new(),
        world: WorldConfig {
            mode: Mode::Sandbox,
            target_count: 0,
            spawn_protection: 0.,
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
        if clients.iter().all(|client| client.is_ready()) {
            return;
        }
    }
    panic!("localhost clients failed to become ready");
}

#[test]
fn delayed_reliable_controls_are_deferred_without_disconnects_or_lost_requests() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Burst", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    // More than the entire control burst allowance arrives before a server tick.
    // Alternating levels makes the last request observably different.
    for i in 0..48 {
        assert!(client.sandbox_level(if i == 47 { 55 } else { 3 }));
    }
    pump(&mut server, &mut [&mut client], 240);
    assert!(client.is_ready());
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .find(|t| Some(t.id) == client.player_id)
            .unwrap()
            .level,
        55
    );
}

#[test]
fn failed_ban_storage_leaves_the_target_connected_and_no_hidden_ban() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Player", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    let file = std::env::temp_dir().join(format!("tankyard-ban-parent-{}", client.client_id()));
    std::fs::write(&file, b"not a directory").unwrap();
    server.settings.ban_file = file.join("bans.json").to_string_lossy().into();
    let result = server.admin_command(&format!("ban {} 5", client.player_id.unwrap()));
    assert!(result.message.starts_with("Error:"));
    assert!(server.admin_command("bans").message.is_empty());
    pump(&mut server, &mut [&mut client], 12);
    assert!(client.is_ready());
    std::fs::remove_file(file).unwrap();
}

#[test]
fn banning_one_loopback_relay_client_keeps_other_and_new_players_connected() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let address = server.address.to_string();
    let mut offender = NetworkClient::connect(&address, "Offender", "").unwrap();
    let mut bystander = NetworkClient::connect(&address, "Bystander", "").unwrap();
    ready(&mut server, &mut [&mut offender, &mut bystander]);
    let result = server.admin_command(&format!("ban {} 5", offender.player_id.unwrap()));
    assert!(
        result.message.contains("shared relay address preserved"),
        "{}",
        result.message
    );
    pump(&mut server, &mut [&mut bystander], 8);
    assert!(bystander.is_ready());
    let mut newcomer = NetworkClient::connect(&address, "Newcomer", "").unwrap();
    ready(&mut server, &mut [&mut bystander, &mut newcomer]);
    assert!(newcomer.is_ready());
    assert!(bystander.is_ready());
}

fn voice_frame(client: &NetworkClient, sequence: u32) -> tankyard_protocol::VoiceFrame {
    tankyard_protocol::VoiceFrame {
        round: client.player_round.unwrap(),
        sequence,
        // Transport treats codec payloads as opaque bounded packets.
        samples: vec![0xf8; 240],
    }
}

#[test]
fn proximity_voice_routes_only_to_nearby_living_players_and_rejects_replays() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let address = server.address.to_string();
    let mut speaker = NetworkClient::connect(&address, "Speaker", "").unwrap();
    let mut near = NetworkClient::connect(&address, "Nearby", "").unwrap();
    let mut far = NetworkClient::connect(&address, "Distant", "").unwrap();
    let mut dead = NetworkClient::connect(&address, "Spectator", "").unwrap();
    ready(
        &mut server,
        &mut [&mut speaker, &mut near, &mut far, &mut dead],
    );
    for (client, pos, alive) in [
        (&speaker, V2::new(500., 600.), true),
        (&near, V2::new(750., 600.), true),
        (&far, V2::new(2400., 600.), true),
        (&dead, V2::new(520., 600.), false),
    ] {
        let tank = server
            .session
            .world
            .tanks
            .iter_mut()
            .find(|t| Some(t.id) == client.player_id)
            .unwrap();
        tank.pos = pos;
        tank.alive = alive;
    }
    assert!(speaker.voice_enabled);
    assert!(speaker.send_voice(voice_frame(&speaker, u32::MAX)));
    pump(
        &mut server,
        &mut [&mut speaker, &mut near, &mut far, &mut dead],
        8,
    );
    let packet = near
        .voice_packets
        .pop_front()
        .expect("nearby player must hear the speaker");
    assert_eq!(packet.player, speaker.player_id.unwrap());
    assert!(packet.volume > 0 && packet.volume < 255);
    assert!(
        speaker.voice_packets.is_empty()
            && far.voice_packets.is_empty()
            && dead.voice_packets.is_empty()
    );
    assert!(speaker.send_voice(voice_frame(&speaker, u32::MAX)));
    pump(&mut server, &mut [&mut speaker, &mut near], 4);
    assert!(
        near.voice_packets.is_empty(),
        "duplicate frames must be discarded"
    );
    assert!(speaker.send_voice(voice_frame(&speaker, 0)));
    pump(&mut server, &mut [&mut speaker, &mut near], 4);
    assert_eq!(near.voice_packets.pop_front().unwrap().frame.sequence, 0);
    server
        .session
        .world
        .tanks
        .iter_mut()
        .find(|t| Some(t.id) == speaker.player_id)
        .unwrap()
        .alive = false;
    assert!(speaker.send_voice(voice_frame(&speaker, 1)));
    pump(&mut server, &mut [&mut speaker, &mut near], 4);
    assert!(
        near.voice_packets.is_empty(),
        "spectators must not transmit to live players"
    );
}

#[test]
fn server_mute_and_voice_disable_are_enforced_even_if_a_client_bypasses_its_ui() {
    use tankyard_protocol::ServerCommand;
    let mut server = UdpServer::bind(settings()).unwrap();
    let address = server.address.to_string();
    let mut owner = NetworkClient::connect(&address, "Owner", "").unwrap();
    let mut speaker = NetworkClient::connect(&address, "Speaker", "").unwrap();
    ready(&mut server, &mut [&mut owner, &mut speaker]);
    for (index, tank) in server.session.world.tanks.iter_mut().enumerate() {
        tank.pos = V2::new(500. + index as f32 * 90., 500.);
    }
    let reply = server.admin_command(&format!("owner-client {}", owner.client_id()));
    assert!(!reply.message.starts_with("Error:"));
    pump(&mut server, &mut [&mut owner, &mut speaker], 8);
    assert!(owner.admin(ServerCommand::Mute {
        player: speaker.player_id.unwrap(),
        minutes: 1
    }));
    pump(&mut server, &mut [&mut owner, &mut speaker], 8);
    assert!(speaker.voice_muted);
    assert!(!speaker.send_voice(voice_frame(&speaker, 1)));
    speaker.voice_muted = false;
    assert!(speaker.send_voice(voice_frame(&speaker, 1)));
    pump(&mut server, &mut [&mut owner, &mut speaker], 4);
    assert!(owner.voice_packets.is_empty());
    assert!(owner.admin(ServerCommand::Mute {
        player: speaker.player_id.unwrap(),
        minutes: 0
    }));
    pump(&mut server, &mut [&mut owner, &mut speaker], 8);
    assert!(!speaker.voice_muted);
    assert!(speaker.send_voice(voice_frame(&speaker, 2)));
    pump(&mut server, &mut [&mut owner, &mut speaker], 4);
    assert_eq!(owner.voice_packets.pop_front().unwrap().frame.sequence, 2);
    server.settings.voice_enabled = false;
    assert!(speaker.send_voice(voice_frame(&speaker, 3)));
    pump(&mut server, &mut [&mut owner, &mut speaker], 4);
    assert!(owner.voice_packets.is_empty());
}

#[test]
fn spectator_interest_follows_the_watched_player_and_stops_on_respawn() {
    let mut config = settings();
    config.interest_radius = 400.;
    let mut server = UdpServer::bind(config).unwrap();
    let address = server.address.to_string();
    let mut viewer = NetworkClient::connect(&address, "Viewer", "").unwrap();
    let mut first = NetworkClient::connect(&address, "First", "").unwrap();
    let mut second = NetworkClient::connect(&address, "Second", "").unwrap();
    ready(&mut server, &mut [&mut viewer, &mut first, &mut second]);
    let (viewer_id, first_id, second_id) = (
        viewer.player_id.unwrap(),
        first.player_id.unwrap(),
        second.player_id.unwrap(),
    );
    for (id, pos) in [
        (viewer_id, V2::new(400., 400.)),
        (first_id, V2::new(1600., 1600.)),
        (second_id, V2::new(3000., 2100.)),
    ] {
        server
            .session
            .world
            .tanks
            .iter_mut()
            .find(|t| t.id == id)
            .unwrap()
            .pos = pos;
    }
    let dead = server
        .session
        .world
        .tanks
        .iter_mut()
        .find(|t| t.id == viewer_id)
        .unwrap();
    dead.alive = false;
    dead.last_attacker = first_id;
    assert!(server.session.world.sandbox_class(first_id, "anchor"));
    assert!(server.session.world.sandbox_class(second_id, "anchor"));
    assert!(first.send_input(PlayerInput {
        sequence: 1,
        fire: true,
        ..Default::default()
    }));
    assert!(second.send_input(PlayerInput {
        sequence: 1,
        fire: true,
        ..Default::default()
    }));
    assert!(viewer.spectate(Some(second_id)));
    pump(&mut server, &mut [&mut viewer, &mut first, &mut second], 12);
    let snapshot = viewer.snapshot.as_ref().unwrap();
    assert!(snapshot.projectiles.iter().any(|p| p.owner == second_id));
    assert!(snapshot.projectiles.iter().all(|p| p.owner != first_id));
    // Removing the selected target must immediately restore an eligible view.
    server
        .session
        .world
        .tanks
        .iter_mut()
        .find(|t| t.id == second_id)
        .unwrap()
        .alive = false;
    pump(&mut server, &mut [&mut viewer, &mut first, &mut second], 8);
    assert!(
        viewer
            .snapshot
            .as_ref()
            .unwrap()
            .projectiles
            .iter()
            .any(|p| p.owner == first_id)
    );
    let tank = server
        .session
        .world
        .tanks
        .iter_mut()
        .find(|t| t.id == viewer_id)
        .unwrap();
    tank.alive = true;
    pump(&mut server, &mut [&mut viewer, &mut first, &mut second], 8);
    assert!(
        viewer.snapshot.as_ref().unwrap().projectiles.is_empty(),
        "living players must receive their own camera interest again"
    );
    assert!(viewer.spectate(Some(first_id)));
    pump(&mut server, &mut [&mut viewer, &mut first], 8);
    assert!(viewer.snapshot.as_ref().unwrap().projectiles.is_empty());
}

#[test]
fn websocket_disconnect_reaches_the_game_server_before_the_bridge_closes() {
    use std::net::TcpListener;
    let mut server = UdpServer::bind(settings()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let relay = listener.local_addr().unwrap();
    drop(listener);
    let address = server.address;
    thread::spawn(move || tankyard_server::websocket::run_relay(&relay.to_string(), address, 4));
    let start = Instant::now();
    let mut client = loop {
        match NetworkClient::connect(&format!("ws://{relay}"), "Pilot", "") {
            Ok(client) => break client,
            Err(_) if start.elapsed() < Duration::from_secs(3) => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => panic!("{error}"),
        }
    };
    ready(&mut server, &mut [&mut client]);
    assert_eq!(server.player_count(), 1);
    drop(client);
    for _ in 0..120 {
        pump(&mut server, &mut [], 1);
        if server.player_count() == 0 {
            return;
        }
    }
    panic!("bridge discarded the final disconnect packet");
}

#[test]
fn empty_online_arenas_sleep_and_resume_when_a_player_joins() {
    let mut config = settings();
    config.bot_count = 12;
    let mut server = UdpServer::bind(config).unwrap();
    let before = tankyard_protocol::encode_snapshot(&server.session.world.snapshot()).unwrap();
    pump(&mut server, &mut [], 120);
    assert_eq!(
        tankyard_protocol::encode_snapshot(&server.session.world.snapshot()).unwrap(),
        before
    );
    let mut client = NetworkClient::connect(&server.address.to_string(), "Pilot", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    pump(&mut server, &mut [&mut client], 30);
    assert!(server.session.world.tick > 0);
    client.disconnect();
    pump(&mut server, &mut [], 30);
    assert_eq!(server.player_count(), 0);
    let tick = server.session.world.tick;
    pump(&mut server, &mut [], 120);
    assert_eq!(server.session.world.tick, tick);
}

#[test]
fn delayed_input_bursts_are_coalesced_without_kicking_the_player() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Burst", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    for sequence in 1..=200 {
        assert!(client.send_input(PlayerInput {
            sequence,
            movement: V2::new(1.0, 0.0),
            ..Default::default()
        }));
    }
    client.flush().unwrap();
    pump(&mut server, &mut [&mut client], 12);
    assert!(client.is_ready());
    assert_eq!(server.player_count(), 1);
    let player = server
        .session
        .world
        .tanks
        .iter()
        .find(|tank| Some(tank.id) == client.player_id)
        .unwrap();
    assert_eq!(player.last_input, 200);
    // Even excess traffic is bounded and dropped rather than kicking a player.
    let mut queued = 0;
    for sequence in 201..=600 {
        queued += usize::from(client.send_input(PlayerInput {
            sequence,
            movement: V2::new(1.0, 0.0),
            ..Default::default()
        }));
    }
    assert!(
        queued >= 128,
        "a delayed burst should reach the server rate limit"
    );
    assert!(
        queued < 400,
        "the client queue must stay bounded under overload"
    );
    client.flush().unwrap();
    pump(&mut server, &mut [&mut client], 12);
    assert!(client.is_ready());
    assert!(server.metrics.rejected_packets > 0);
    assert!(client.send_input(PlayerInput {
        sequence: 601,
        ..Default::default()
    }));
    pump(&mut server, &mut [&mut client], 8);
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .find(|tank| Some(tank.id) == client.player_id)
            .unwrap()
            .input
            .movement,
        V2::ZERO
    );
}

#[test]
fn delayed_click_and_ability_taps_survive_input_coalescing() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Tap", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    server
        .session
        .world
        .sandbox_class(client.player_id.unwrap(), "riposte");
    assert!(client.send_input(PlayerInput {
        sequence: 1,
        fire: true,
        steer_return: true,
        ability: true,
        ..Default::default()
    }));
    assert!(client.send_input(PlayerInput {
        sequence: 2,
        ..Default::default()
    }));
    client.flush().unwrap();
    pump(&mut server, &mut [&mut client], 1);
    let tank = server
        .session
        .world
        .tanks
        .iter()
        .find(|tank| Some(tank.id) == client.player_id)
        .unwrap();
    assert_eq!(tank.last_input, 2);
    assert!(
        !tank.input.steer_return,
        "drone steering must follow the newest held-button state"
    );
    assert!(
        tank.shot_count > 0,
        "a queued release must not swallow a short click"
    );
    assert!(
        tank.ability_cooldown > 0.0,
        "a queued release must not swallow an ability tap"
    );
}

#[test]
fn online_bots_scale_with_join_leave_restart_and_admin_limits() {
    let mut config = settings();
    config.bot_count = 12;
    let mut server = UdpServer::bind(config).unwrap();
    let bots = |server: &UdpServer| {
        server
            .session
            .world
            .tanks
            .iter()
            .filter(|tank| tank.bot)
            .count()
    };
    assert_eq!(bots(&server), 6);
    let mut clients = Vec::new();
    for humans in 1..=7 {
        clients.push(
            NetworkClient::connect(&server.address.to_string(), &format!("Pilot {humans}"), "")
                .unwrap(),
        );
        let mut refs: Vec<_> = clients.iter_mut().collect();
        ready(&mut server, &mut refs);
        assert_eq!(server.player_count(), humans);
        assert_eq!(bots(&server), 6usize.saturating_sub(humans));
    }
    assert!(
        !server
            .admin_command("restart")
            .message
            .starts_with("Error:")
    );
    assert_eq!(bots(&server), 0);
    for _ in 0..3 {
        clients.pop().unwrap().disconnect();
        let mut refs: Vec<_> = clients.iter_mut().collect();
        pump(&mut server, &mut refs, 12);
    }
    assert_eq!(server.player_count(), 4);
    assert_eq!(bots(&server), 2);
    let reply = server.admin_command("bots 1");
    assert!(!reply.message.starts_with("Error:"), "{}", reply.message);
    assert_eq!(bots(&server), 1);
    server.admin_command("bots 0");
    assert_eq!(bots(&server), 0);
}

#[test]
fn fixed_bot_policy_is_available_and_offline_population_is_unchanged() {
    let mut config = settings();
    config.bot_count = 12;
    config.adaptive_bots = false;
    let mut server = UdpServer::bind(config).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Pilot", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .filter(|tank| tank.bot)
            .count(),
        12
    );
    let offline = tankyard_server::LocalSession::new(WorldConfig::default(), 12);
    assert_eq!(
        offline.world.tanks.iter().filter(|tank| tank.bot).count(),
        12
    );
}

#[test]
fn connection_and_controls_recover_under_delay_jitter_and_packet_loss() {
    use tankyard_server::netem::{EmulationConfig, NetworkEmulator};
    let mut server = UdpServer::bind(settings()).unwrap();
    let relay = NetworkEmulator::start(
        server.address,
        EmulationConfig {
            latency_ms: 80,
            jitter_ms: 15,
            loss_percent: 10.0,
            seed: 42,
        },
    )
    .unwrap();
    let mut client = NetworkClient::connect(&relay.address.to_string(), "Loss test", "").unwrap();
    let start = Instant::now();
    let mut previous = start;
    let mut sequence = 0;
    let mut received_ticks = 0;
    let mut last_tick = 0;
    let mut stopped = false;
    while start.elapsed() < Duration::from_secs(5) {
        let now = Instant::now();
        let elapsed = now.duration_since(previous);
        previous = now;
        client.update(elapsed).unwrap();
        if client.is_ready() {
            sequence += 1;
            assert!(client.send_input(PlayerInput {
                sequence,
                movement: if start.elapsed() < Duration::from_secs(3) {
                    V2::new(1.0, 0.0)
                } else {
                    V2::ZERO
                },
                ..Default::default()
            }));
            client.flush().unwrap();
        }
        server.update(elapsed).unwrap();
        if let Some(snapshot) = client.player_snapshot() {
            if snapshot.tick > last_tick {
                received_ticks += 1;
                last_tick = snapshot.tick;
            }
            if start.elapsed() > Duration::from_secs(4) {
                stopped = snapshot.tanks.iter().any(|tank| {
                    Some(tank.id) == client.player_id && tank.input.movement == V2::ZERO
                });
            }
        }
        thread::sleep(STEP);
    }
    assert!(client.is_ready());
    assert!(
        received_ticks > 25,
        "only {received_ticks} snapshots arrived"
    );
    assert!(
        stopped,
        "released controls must reach the server despite lost packets"
    );
    assert!(relay.stats().dropped > 0);
    assert_eq!(server.metrics.dropped_catchup_ticks, 0);
}

#[test]
fn coop_wave_snapshots_and_bot_settings_keep_raiders_separate_from_teammates() {
    let mut config = settings();
    config.world.mode = Mode::CoopPve;
    config.world.spawn_protection = 15.0;
    config.bot_count = 2;
    let mut server = UdpServer::bind(config).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Pilot", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    let player = client.player_id.unwrap();
    assert!(
        client
            .player_snapshot()
            .unwrap()
            .tanks
            .iter()
            .find(|tank| tank.id == player)
            .unwrap()
            .spawn_immunity
    );
    assert!(client.send_input(PlayerInput {
        sequence: 1,
        movement: V2::new(1.0, 0.0),
        ..Default::default()
    }));
    pump(&mut server, &mut [&mut client], 8);
    assert!(
        !server
            .session
            .world
            .tanks
            .iter()
            .find(|tank| tank.id == player)
            .unwrap()
            .spawn_immunity
    );
    assert!(client.send_input(PlayerInput {
        sequence: 2,
        ..Default::default()
    }));
    pump(&mut server, &mut [&mut client], 480);
    let snapshot = client.player_snapshot().unwrap();
    assert_eq!(snapshot.config.mode, Mode::CoopPve);
    assert_eq!(snapshot.match_state.arena.wave, 1);
    assert!(!snapshot.match_state.arena.intermission);
    assert!(snapshot.match_state.arena.enemies > 0);
    let raiders: Vec<_> = server
        .session
        .world
        .tanks
        .iter()
        .filter(|tank| tank.team == 2)
        .map(|tank| tank.id)
        .collect();
    assert!(!raiders.is_empty());
    let reply = server.admin_command("bots 0");
    assert!(!reply.message.starts_with("Error:"), "{}", reply.message);
    assert_eq!(server.settings.bot_count, 0);
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .filter(|tank| tank.bot && tank.team == 1)
            .count(),
        0
    );
    assert!(
        raiders
            .iter()
            .all(|id| server.session.world.tanks.iter().any(|tank| tank.id == *id))
    );
}

#[test]
fn round_restarts_keep_each_client_bound_to_its_own_human_tank() {
    let mut config = settings();
    config.bot_count = 3;
    let mut server = UdpServer::bind(config).unwrap();
    let address = server.address.to_string();
    let mut a = NetworkClient::connect(&address, "Alpha", "").unwrap();
    let mut b = NetworkClient::connect(&address, "Beta", "").unwrap();
    ready(&mut server, &mut [&mut a, &mut b]);
    for mode in ["ffa", "deathmatch", "random-events", "coop-pve", "sandbox"] {
        let reply = server.admin_command(&format!("mode {mode}"));
        assert!(!reply.message.starts_with("Error:"), "{}", reply.message);
        let round = server.session.world.match_state.round;
        for _ in 0..240 {
            pump(&mut server, &mut [&mut a, &mut b], 1);
            for (client, name) in [(&a, "Alpha"), (&b, "Beta")] {
                if let Some(snapshot) = client.player_snapshot() {
                    assert_eq!(client.player_round, Some(snapshot.match_state.round));
                    let tank = snapshot
                        .tanks
                        .iter()
                        .find(|t| Some(t.id) == client.player_id)
                        .unwrap();
                    assert!(!tank.bot);
                    assert_eq!(tank.name, name);
                }
            }
            if [&a, &b]
                .iter()
                .all(|client| client.is_ready() && client.player_round == Some(round))
            {
                break;
            }
        }
        assert!(
            [&a, &b]
                .iter()
                .all(|client| client.is_ready() && client.player_round == Some(round))
        );
    }
}

#[test]
fn two_real_udp_clients_move_evolve_damage_and_disconnect() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let address = server.address.to_string();
    let mut a = NetworkClient::connect(&address, "Alpha", "").unwrap();
    let mut b = NetworkClient::connect(&address, "Beta", "").unwrap();
    ready(&mut server, &mut [&mut a, &mut b]);
    assert_eq!(server.player_count(), 2);
    let alpha = a.player_id.unwrap();
    let beta = b.player_id.unwrap();
    for tank in &mut server.session.world.tanks {
        tank.pos = if tank.id == alpha {
            V2::new(600., 600.)
        } else {
            V2::new(1100., 600.)
        };
        tank.vel = V2::ZERO;
    }
    assert!(
        a.snapshot
            .as_ref()
            .unwrap()
            .tanks
            .iter()
            .any(|t| t.id == beta)
    );
    let initial = server
        .session
        .world
        .tanks
        .iter()
        .find(|t| t.id == alpha)
        .unwrap()
        .pos;
    for sequence in 1..31 {
        assert!(a.send_input(PlayerInput {
            sequence,
            movement: V2::new(1., 0.),
            ..Default::default()
        }));
        pump(&mut server, &mut [&mut a, &mut b], 1);
    }
    assert!(
        server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == alpha)
            .unwrap()
            .pos
            .x
            > initial.x + 20.
    );
    assert!(
        a.snapshot
            .as_ref()
            .unwrap()
            .tanks
            .iter()
            .find(|t| t.id == alpha)
            .unwrap()
            .last_input
            > 0
    );
    assert!(a.sandbox_level(45));
    assert!(a.upgrade_stat(5));
    let branch = Content::builtin().class("vector").unwrap().upgrades[0].clone();
    assert!(a.evolve(&branch));
    pump(&mut server, &mut [&mut a, &mut b], 12);
    let tank = a
        .snapshot
        .as_ref()
        .unwrap()
        .tanks
        .iter()
        .find(|t| t.id == alpha)
        .unwrap();
    assert_eq!(tank.class, branch);
    assert_eq!(tank.stats[5], 1);
    assert_eq!(tank.level, 45);
    assert!(a.sandbox_class("vector"));
    pump(&mut server, &mut [&mut a, &mut b], 6);
    for tank in &mut server.session.world.tanks {
        tank.protection = 0.;
        tank.vel = V2::ZERO;
        tank.input = PlayerInput::default();
        tank.pos = if tank.id == alpha {
            V2::new(600., 600.)
        } else {
            V2::new(850., 600.)
        };
    }
    let mut hit = false;
    for sequence in 31..151 {
        a.send_input(PlayerInput {
            sequence,
            aim: 0.,
            fire: true,
            ..Default::default()
        });
        pump(&mut server, &mut [&mut a, &mut b], 1);
        let defender = server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == beta)
            .unwrap();
        if defender.health < defender.max_health || !defender.alive {
            hit = true;
            break;
        }
    }
    assert!(
        hit,
        "authoritative bullets did not damage the second client"
    );
    pump(&mut server, &mut [&mut a, &mut b], 6);
    assert!(
        b.snapshot
            .as_ref()
            .unwrap()
            .tanks
            .iter()
            .find(|t| t.id == beta)
            .is_some_and(|t| t.health < t.max_health || !t.alive || t.deaths > 0)
    );
    b.disconnect();
    pump(&mut server, &mut [&mut a], 12);
    assert_eq!(server.player_count(), 1);
    assert!(
        !a.snapshot
            .as_ref()
            .unwrap()
            .tanks
            .iter()
            .any(|t| t.id == beta)
    );
}

#[test]
fn multiplayer_sandbox_is_server_permission_controlled() {
    let mut config = settings();
    config.allow_sandbox = false;
    let mut server = UdpServer::bind(config).unwrap();
    let mut client = NetworkClient::connect(&server.address.to_string(), "Pilot", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    assert!(!client.sandbox_allowed);
    let id = client.player_id.unwrap();
    client.sandbox_allowed = true; // An untrusted client can alter its own UI.
    assert!(client.sandbox_level(45));
    pump(&mut server, &mut [&mut client], 12);
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .level,
        30
    );
    assert!(client.last_action.is_some());
}

#[test]
fn bastion_accepts_resumed_sequence_gaps_without_advancing_movement_or_weapon_time() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let mut client =
        NetworkClient::connect(&server.address.to_string(), "Resumed pilot", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    let id = client.player_id.unwrap();
    let tank = server
        .session
        .world
        .tanks
        .iter_mut()
        .find(|t| t.id == id)
        .unwrap();
    tank.pos = V2::new(1800., 1300.);
    tank.vel = V2::ZERO;
    for sequence in [1, 90_000, 90_001, 1_000_000] {
        let before = server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .pos;
        let tick = server.session.world.tick;
        assert!(client.send_input(PlayerInput {
            sequence,
            movement: V2::new(1., 1.),
            fire: true,
            ..Default::default()
        }));
        pump(&mut server, &mut [&mut client], 2);
        let tank = server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == id)
            .unwrap();
        assert_eq!(tank.last_input, sequence);
        assert!(
            tank.pos.distance(before) < 20.,
            "client sequence advanced movement"
        );
        assert!(server.session.world.tick - tick <= 3);
        assert!(client.is_ready());
    }
    assert_eq!(server.bastion.counters().invalid_inputs, 0);
    assert!(
        server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .shot_count
            <= 2
    );
}

#[test]
fn input_packets_cannot_speed_up_weapons_and_stale_input_becomes_neutral() {
    let mut server = UdpServer::bind(settings()).unwrap();
    let mut client =
        NetworkClient::connect(&server.address.to_string(), "Cooldown Pilot", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    let id = client.player_id.unwrap();
    let cooldown = Content::builtin().class("vector").unwrap().barrels[0].cooldown;
    let before = server
        .session
        .world
        .tanks
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .shot_count;
    for tick in 0..60 {
        // Two legal input packets per simulation tick must still produce only
        // the weapon's authoritative cooldown cadence.
        for packet in 0..2 {
            assert!(client.send_input(PlayerInput {
                sequence: 1 + tick * 2 + packet,
                movement: V2::new(1., 0.),
                fire: true,
                ..Default::default()
            }));
        }
        pump(&mut server, &mut [&mut client], 1);
    }
    let shots = server
        .session
        .world
        .tanks
        .iter()
        .find(|t| t.id == id)
        .unwrap()
        .shot_count
        - before;
    assert!(shots > 0);
    assert!(
        shots <= (1.1 / cooldown).ceil() as u32 + 2,
        "extra packets accelerated weapon firing"
    );
    pump(&mut server, &mut [&mut client], 45);
    let tank = server
        .session
        .world
        .tanks
        .iter()
        .find(|t| t.id == id)
        .unwrap();
    assert_eq!(tank.input.movement, V2::ZERO);
    assert!(!tank.input.fire);
    let stopped_shots = tank.shot_count;
    pump(&mut server, &mut [&mut client], 30);
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .shot_count,
        stopped_shots
    );
    assert!(
        client.is_ready(),
        "neutral input must not terminate an otherwise live connection"
    );
}

#[test]
fn password_rejection_and_arbitrary_udp_do_not_break_server() {
    let mut config = settings();
    config.password = "correct".into();
    let mut server = UdpServer::bind(config).unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    for length in [0, 1, 20, 128, 1500, 4096] {
        socket.send_to(&vec![255; length], server.address).unwrap();
    }
    let mut bad =
        NetworkClient::connect(&server.address.to_string(), "Pilot", "incorrect").unwrap();
    for _ in 0..180 {
        if bad.update(STEP).is_err() {
            break;
        }
        server.update(STEP).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    assert!(bad.error.as_ref().is_some_and(|s| s.contains("password")));
    assert_eq!(server.player_count(), 0);
    let mut good = NetworkClient::connect(&server.address.to_string(), "Pilot", "correct").unwrap();
    ready(&mut server, &mut [&mut good]);
    assert_eq!(server.player_count(), 1);
}

#[test]
fn dedicated_server_process_accepts_independent_client_sockets() {
    let free = UdpSocket::bind("127.0.0.1:0").unwrap();
    let address = free.local_addr().unwrap();
    drop(free);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_tankyard-server"))
        .args([
            "--bind",
            &address.to_string(),
            "--bots",
            "0",
            "--no-console",
            "--seconds",
            "6",
        ])
        .current_dir(std::env::temp_dir())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut a = NetworkClient::connect(&address.to_string(), "Process Alpha", "").unwrap();
    let mut b = NetworkClient::connect(&address.to_string(), "Process Beta", "").unwrap();
    let started = Instant::now();
    let mut previous = started;
    let mut sequence = 0;
    while started.elapsed() < Duration::from_secs(4) {
        let now = Instant::now();
        let elapsed = now.duration_since(previous);
        previous = now;
        let _ = a.update(elapsed);
        let _ = b.update(elapsed);
        if a.is_ready() && b.is_ready() {
            sequence += 1;
            a.send_input(PlayerInput {
                sequence,
                movement: V2::new(1., 0.),
                fire: true,
                ..Default::default()
            });
            if a.snapshot
                .as_ref()
                .unwrap()
                .tanks
                .iter()
                .find(|t| Some(t.id) == a.player_id)
                .is_some_and(|t| t.last_input > 10)
            {
                break;
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    let success = a.is_ready()
        && b.is_ready()
        && a.snapshot.as_ref().is_some_and(|s| {
            s.tanks
                .iter()
                .find(|t| Some(t.id) == a.player_id)
                .is_some_and(|t| t.last_input > 10)
        });
    a.disconnect();
    b.disconnect();
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        success,
        "separate headless process did not replicate sequenced client inputs"
    );
}
