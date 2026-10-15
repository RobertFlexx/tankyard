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
fn round_restarts_keep_each_client_bound_to_its_own_human_tank() {
    let mut config = settings();
    config.bot_count = 3;
    let mut server = UdpServer::bind(config).unwrap();
    let address = server.address.to_string();
    let mut a = NetworkClient::connect(&address, "Alpha", "").unwrap();
    let mut b = NetworkClient::connect(&address, "Beta", "").unwrap();
    ready(&mut server, &mut [&mut a, &mut b]);
    for mode in ["ffa", "deathmatch", "sandbox"] {
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
