use std::{thread, time::Duration};
use tankyard_core::{Mode, WorldConfig};
use tankyard_server::{NetworkClient, ServerSettings, UdpServer, party::Invite};
const STEP: Duration = Duration::from_nanos(16_666_667);

#[test]
fn roster_chat_and_menu_presence_survive_rounds_without_granting_host_authority() {
    let (mut server, invite) = arena(Mode::CoopPve);
    let mut leader = NetworkClient::connect(&invite.link(), "Leader", "").unwrap();
    let mut guest = NetworkClient::connect(&invite.link(), "Leader", "").unwrap();
    ready(&mut server, &mut [&mut leader, &mut guest]);
    let public: String = leader
        .admin_public_key()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert!(
        !server
            .admin_command(&format!("owner-client {} {public}", leader.client_id()))
            .message
            .starts_with("Error:")
    );
    pump(&mut server, &mut [&mut leader, &mut guest], 65);
    let leader_id = leader.client_id();
    let guest_id = guest.client_id();
    for client in [&leader, &guest] {
        assert_eq!(client.party_members.len(), 2);
        assert!(
            client
                .party_members
                .iter()
                .find(|member| member.client == leader_id)
                .unwrap()
                .leader
        );
        assert!(
            !client
                .party_members
                .iter()
                .find(|member| member.client == guest_id)
                .unwrap()
                .leader
        );
    }
    assert!(leader.party_presence(true));
    assert!(guest.send_chat("lobby", "Hello from the party 世界"));
    pump(&mut server, &mut [&mut leader, &mut guest], 10);
    assert!(
        guest
            .party_members
            .iter()
            .find(|member| member.client == leader_id)
            .unwrap()
            .in_menu
    );
    assert!(
        leader
            .chat_messages
            .iter()
            .any(|message| message.text == "Hello from the party 世界")
    );
    let before = leader.player_round.unwrap();
    assert!(
        !server
            .admin_command("restart")
            .message
            .starts_with("Error:")
    );
    pump(&mut server, &mut [&mut leader, &mut guest], 65);
    assert!(leader.is_ready() && guest.is_ready());
    assert_eq!(leader.player_round, Some(before + 1));
    assert_eq!(leader.client_id(), leader_id);
    assert!(
        guest
            .party_members
            .iter()
            .any(|member| member.client == leader_id && member.in_menu && member.leader)
    );
    assert!(
        leader
            .chat_messages
            .iter()
            .any(|message| message.text == "Hello from the party 世界")
    );
    assert!(leader.party_presence(false));
    pump(&mut server, &mut [&mut leader, &mut guest], 10);
    assert!(guest.party_members.iter().all(|member| !member.in_menu));
    guest.disconnect();
    pump(&mut server, &mut [&mut leader], 30);
    assert_eq!(leader.party_members.len(), 1);
    assert_eq!(leader.party_members[0].client, leader_id);
}

#[test]
fn roster_contains_every_party_member_independent_of_snapshot_interest() {
    let (mut server, invite) = arena(Mode::Ffa);
    server.settings.interest_radius = 600.;
    let mut first = NetworkClient::connect(&invite.link(), "Near", "").unwrap();
    let mut second = NetworkClient::connect(&invite.link(), "Far", "").unwrap();
    ready(&mut server, &mut [&mut first, &mut second]);
    let a = first.player_id.unwrap();
    let b = second.player_id.unwrap();
    for tank in &mut server.arenas[0].session.world.tanks {
        if tank.id == a {
            tank.pos = tankyard_core::V2::new(-1000., -1000.);
        }
        if tank.id == b {
            tank.pos = tankyard_core::V2::new(1000., 1000.);
        }
    }
    pump(&mut server, &mut [&mut first, &mut second], 5);
    let snapshot = first.snapshot.as_ref().unwrap();
    let near = snapshot.tanks.iter().find(|tank| tank.id == a).unwrap();
    let far = snapshot.tanks.iter().find(|tank| tank.id == b).unwrap();
    assert!(near.pos.distance(far.pos) > server.settings.interest_radius);
    assert_eq!(first.party_members.len(), 2);
    assert!(
        first
            .party_members
            .iter()
            .any(|member| member.client == second.client_id())
    );
}
fn voice(client: &NetworkClient, sequence: u32) -> tankyard_protocol::VoiceFrame {
    tankyard_protocol::VoiceFrame {
        round: client.player_round.unwrap(),
        sequence,
        // The transport treats codec payloads as opaque bounded packets.
        samples: vec![0xf8; 240],
    }
}

#[test]
fn a_party_hears_each_other_anywhere_down_or_alive_and_between_rounds() {
    let (mut server, invite) = arena(Mode::Ffa);
    let mut speaker = NetworkClient::connect(&invite.link(), "Speaker", "").unwrap();
    let mut far = NetworkClient::connect(&invite.link(), "Far", "").unwrap();
    let mut down = NetworkClient::connect(&invite.link(), "Down", "").unwrap();
    ready(&mut server, &mut [&mut speaker, &mut far, &mut down]);
    pump(&mut server, &mut [&mut speaker, &mut far, &mut down], 65);
    assert!(speaker.voice_enabled);
    assert_eq!(speaker.party_members.len(), 3);
    let (a, b, c) = (
        speaker.player_id.unwrap(),
        far.player_id.unwrap(),
        down.player_id.unwrap(),
    );
    for tank in &mut server.arenas[0].session.world.tanks {
        if tank.id == a {
            tank.pos = tankyard_core::V2::new(-1500., -1000.);
        } else if tank.id == b {
            tank.pos = tankyard_core::V2::new(1500., 1000.);
        } else if tank.id == c {
            tank.alive = false;
        }
    }
    // Far apart, and to someone who is down, at full volume.
    assert!(speaker.send_voice(voice(&speaker, 1)));
    pump(&mut server, &mut [&mut speaker, &mut far, &mut down], 6);
    for listener in [&mut far, &mut down] {
        let packet = listener.voice_packets.pop_front().expect("heard");
        assert_eq!((packet.player, packet.volume), (a, u8::MAX));
    }
    assert!(speaker.voice_packets.is_empty(), "never hears itself");

    // Someone who is down can still talk to the party.
    assert!(down.send_voice(voice(&down, 1)));
    pump(&mut server, &mut [&mut speaker, &mut far, &mut down], 6);
    assert_eq!(speaker.voice_packets.pop_front().unwrap().player, c);
    assert_eq!(far.voice_packets.pop_front().unwrap().player, c);

    // A finished round doesn't end the conversation.
    server.arenas[0].session.world.match_state.finished = true;
    assert!(speaker.send_voice(voice(&speaker, 2)));
    pump(&mut server, &mut [&mut speaker, &mut far, &mut down], 6);
    assert_eq!(far.voice_packets.pop_front().unwrap().frame.sequence, 2);
    far.voice_packets.clear();
    down.voice_packets.clear();

    // Nor does the next one, and replays are still dropped.
    assert!(
        !server
            .admin_command("restart")
            .message
            .starts_with("Error:")
    );
    pump(&mut server, &mut [&mut speaker, &mut far, &mut down], 65);
    far.voice_packets.clear();
    assert!(speaker.send_voice(voice(&speaker, 5)));
    pump(&mut server, &mut [&mut speaker, &mut far, &mut down], 6);
    assert_eq!(far.voice_packets.pop_front().unwrap().frame.sequence, 5);
    assert!(speaker.send_voice(voice(&speaker, 5)));
    pump(&mut server, &mut [&mut speaker, &mut far, &mut down], 6);
    assert!(far.voice_packets.is_empty(), "a replayed frame is dropped");
}

#[test]
fn a_server_with_voice_off_carries_no_party_voice() {
    let (mut server, invite) = arena(Mode::Ffa);
    let mut speaker = NetworkClient::connect(&invite.link(), "Speaker", "").unwrap();
    let mut listener = NetworkClient::connect(&invite.link(), "Listener", "").unwrap();
    ready(&mut server, &mut [&mut speaker, &mut listener]);
    pump(&mut server, &mut [&mut speaker, &mut listener], 65);
    server.settings.voice_enabled = false;
    assert!(speaker.send_voice(voice(&speaker, 1)));
    pump(&mut server, &mut [&mut speaker, &mut listener], 6);
    assert!(listener.voice_packets.is_empty());
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
    for _ in 0..300 {
        pump(server, clients, 1);
        if clients.iter().all(|client| client.is_ready()) {
            return;
        }
    }
    panic!("party clients never became ready");
}
fn arena(mode: Mode) -> (UdpServer, Invite) {
    let mut invite = Invite::create("127.0.0.1:5000".parse().unwrap());
    let server = UdpServer::bind(ServerSettings {
        bind: "127.0.0.1:0".into(),
        max_players: 8,
        bot_count: 0,
        ban_file: String::new(),
        map_rotation_seconds: 0,
        party: Some(invite.settings(4)),
        world: WorldConfig {
            mode,
            target_count: 0,
            spawn_protection: 15.,
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap();
    invite.address = server.address;
    (server, invite)
}
#[test]
fn friends_replace_only_allied_bots_stay_together_after_changes_and_never_claim_host() {
    for mode in [
        Mode::TwoTeams,
        Mode::CoopPve,
        Mode::BossRush,
        Mode::KingOfTheHill,
    ] {
        let (mut server, invite) = arena(mode);
        let mut first = NetworkClient::connect(&invite.link(), "Friend", "").unwrap();
        ready(&mut server, &mut [&mut first]);
        assert_eq!(
            server.arenas[0]
                .session
                .world
                .tanks
                .iter()
                .filter(|t| t.bot && t.team == 1)
                .count(),
            3
        );
        let mut second = NetworkClient::connect(&invite.link(), "Game Creator", "").unwrap();
        ready(&mut server, &mut [&mut first, &mut second]);
        assert_eq!(server.player_count(), 2);
        assert!(
            server.arenas[0]
                .session
                .world
                .tanks
                .iter()
                .filter(|t| !t.bot)
                .all(|t| t.team == 1 && !t.game_creator && !t.creator_boss)
        );
        assert_eq!(
            server.arenas[0]
                .session
                .world
                .tanks
                .iter()
                .filter(|t| t.bot && t.team == 1)
                .count(),
            2
        );
        assert_eq!(first.role, tankyard_protocol::ServerRole::Player);
        assert_eq!(second.role, tankyard_protocol::ServerRole::Player);
        assert!(!first.admin(tankyard_protocol::ServerCommand::RestartRound));
        if mode == Mode::TwoTeams {
            assert_eq!(
                server.arenas[0]
                    .session
                    .world
                    .tanks
                    .iter()
                    .filter(|t| t.team == 2)
                    .count(),
                4
            );
        }
        let round = first.player_round.unwrap();
        assert!(
            !server
                .admin_command("restart")
                .message
                .starts_with("Error:")
        );
        pump(&mut server, &mut [&mut first, &mut second], 20);
        assert!(first.is_ready() && second.is_ready());
        assert_eq!(first.player_round.unwrap(), round + 1);
        assert!(
            server.arenas[0]
                .session
                .world
                .tanks
                .iter()
                .filter(|t| !t.bot)
                .all(|t| t.team == 1)
        );
        second.disconnect();
        pump(&mut server, &mut [&mut first], 30);
        assert_eq!(server.player_count(), 1);
        assert_eq!(
            server.arenas[0]
                .session
                .world
                .tanks
                .iter()
                .filter(|t| t.bot && t.team == 1)
                .count(),
            3
        );
    }
}
#[test]
fn infection_party_preserves_conversion_and_late_joiners_do_not_replenish_survivors() {
    let (mut server, invite) = arena(Mode::Infection);
    let mut first = NetworkClient::connect(&invite.link(), "Leader", "").unwrap();
    ready(&mut server, &mut [&mut first]);
    pump(&mut server, &mut [&mut first], 510);
    assert_eq!(server.arenas[0].session.world.tanks.len(), 8);
    assert!(
        !server.arenas[0]
            .session
            .world
            .match_state
            .arena
            .intermission
    );
    let survivors = server.arenas[0]
        .session
        .world
        .tanks
        .iter()
        .filter(|tank| tank.team == 1)
        .count();
    let mut second = NetworkClient::connect(&invite.link(), "Late friend", "").unwrap();
    ready(&mut server, &mut [&mut first, &mut second]);
    let id = second.player_id.unwrap();
    assert_eq!(
        server.arenas[0]
            .session
            .world
            .tanks
            .iter()
            .find(|tank| tank.id == id)
            .unwrap()
            .team,
        2
    );
    assert!(
        server.arenas[0]
            .session
            .world
            .tanks
            .iter()
            .filter(|tank| tank.team == 1)
            .count()
            <= survivors
    );
    assert_eq!(server.arenas[0].session.world.tanks.len(), 8);
    for tank in &mut server.arenas[0].session.world.tanks {
        if tank.bot {
            tank.team = 2;
        }
    }
    pump(&mut server, &mut [&mut first, &mut second], 5);
    assert!(
        server.arenas[0]
            .session
            .world
            .tanks
            .iter()
            .filter(|tank| tank.bot)
            .all(|tank| tank.team == 2)
    );
}
#[test]
fn wrong_invite_and_plain_ip_do_not_admit_players() {
    for use_plain in [false, true] {
        let (mut server, invite) = arena(Mode::CoopPve);
        let destination = if use_plain {
            invite.address.to_string()
        } else {
            Invite::create(invite.address).link()
        };
        let mut client = NetworkClient::connect(&destination, "Intruder", "").unwrap();
        for _ in 0..300 {
            let result = client.update(STEP);
            server.update(STEP).unwrap();
            if result.is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(client.error.is_some());
        assert_eq!(server.player_count(), 0);
        assert!(!client.is_ready());
    }
}
#[test]
fn co_op_wave_enemies_are_never_removed_by_teammate_replacement() {
    let (mut server, invite) = arena(Mode::CoopPve);
    let mut client = NetworkClient::connect(&invite.link(), "Friend", "").unwrap();
    ready(&mut server, &mut [&mut client]);
    pump(&mut server, &mut [&mut client], 510);
    assert!(server.arenas[0].session.world.match_state.arena.wave > 0);
    let enemy_ids: Vec<_> = server.arenas[0]
        .session
        .world
        .tanks
        .iter()
        .filter(|t| t.team == 2)
        .map(|t| t.id)
        .collect();
    assert!(!enemy_ids.is_empty());
    let mut friend = NetworkClient::connect(&invite.link(), "Second", "").unwrap();
    ready(&mut server, &mut [&mut client, &mut friend]);
    for id in enemy_ids {
        assert!(
            server.arenas[0]
                .session
                .world
                .tanks
                .iter()
                .any(|t| t.id == id)
        );
    }
    assert_eq!(
        server.arenas[0]
            .session
            .world
            .tanks
            .iter()
            .filter(|t| t.bot && t.team == 1)
            .count(),
        2
    );
}

#[test]
fn leader_changes_modes_atomically_without_replacing_invites_members_or_chat() {
    use tankyard_protocol::{MatchSettings, PartyMatchOptions, ServerCommand};
    let (mut server, invite) = arena(Mode::CoopPve);
    server.settings.allow_sandbox = true;
    let original = invite.link();
    let mut leader = NetworkClient::connect(&original, "Leader", "").unwrap();
    let mut guest = NetworkClient::connect(&original, "Guest", "").unwrap();
    ready(&mut server, &mut [&mut leader, &mut guest]);
    let public: String = leader
        .admin_public_key()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert!(
        !server
            .admin_command(&format!("owner-client {} {public}", leader.client_id()))
            .message
            .starts_with("Error:")
    );
    pump(&mut server, &mut [&mut leader, &mut guest], 65);
    assert!(leader.party_presence(true));
    assert!(guest.send_chat("lobby", "Staying together"));
    pump(&mut server, &mut [&mut leader, &mut guest], 10);
    let ids = [leader.client_id(), guest.client_id()];
    // Even a custom client that bypasses its own permission checks cannot
    // configure the party without the server's process-local host grant.
    guest
        .permissions
        .push(tankyard_protocol::ServerPermission::Configure);
    let forged = MatchSettings {
        mode: Mode::Ffa,
        map: "open".into(),
        difficulty: "Normal".into(),
        bots: 3,
        party: Some(PartyMatchOptions {
            width: 2400,
            height: 1800,
            dynamic_arena: false,
            starting_level: 1,
        }),
    };
    assert!(guest.admin(ServerCommand::ConfigureParty { settings: forged }));
    pump(&mut server, &mut [&mut leader, &mut guest], 10);
    assert_eq!(server.arenas[0].session.world.config.mode, Mode::CoopPve);
    assert!(
        guest
            .last_action
            .as_ref()
            .is_some_and(|message| !message.is_empty())
    );
    guest.permissions.clear();
    for mode in [
        Mode::FourTeams,
        Mode::Sandbox,
        Mode::Infection,
        Mode::CoopPve,
    ] {
        let before = leader.player_round.unwrap();
        let settings = MatchSettings {
            mode,
            map: "open".into(),
            difficulty: "Expert".into(),
            bots: 3,
            party: Some(PartyMatchOptions {
                width: 2400,
                height: 1800,
                dynamic_arena: false,
                starting_level: 30,
            }),
        };
        assert!(!guest.admin(ServerCommand::ConfigureParty {
            settings: settings.clone()
        }));
        assert!(leader.admin(ServerCommand::ConfigureParty { settings }));
        pump(&mut server, &mut [&mut leader, &mut guest], 65);
        assert_eq!(leader.player_round, Some(before + 1));
        assert_eq!(guest.player_round, leader.player_round);
        assert_eq!([leader.client_id(), guest.client_id()], ids);
        assert_eq!(server.arenas[0].session.world.config.mode, mode);
        assert_eq!(server.settings.world.width, 2400.);
        assert_eq!(server.settings.party.as_ref().unwrap().ally_slots, 3);
        for client in [&leader, &guest] {
            assert!(client.is_ready());
            assert_eq!(client.party_members.len(), 2);
            assert!(
                client
                    .party_members
                    .iter()
                    .find(|member| member.client == ids[0])
                    .unwrap()
                    .in_menu
            );
            assert!(
                client
                    .chat_messages
                    .iter()
                    .any(|message| message.text == "Staying together")
            );
            assert_eq!(client.match_settings.as_ref().unwrap().mode, mode);
        }
        assert!(
            server.arenas[0]
                .session
                .world
                .tanks
                .iter()
                .filter(|tank| !tank.bot)
                .all(|tank| tank.level >= 30)
        );
    }
    assert_eq!(invite.link(), original);
    let mut late = NetworkClient::connect(&original, "Late guest", "").unwrap();
    ready(&mut server, &mut [&mut leader, &mut guest, &mut late]);
    assert_eq!(late.party_members.len(), 3);
}

#[test]
fn preparing_a_hosted_party_is_acknowledged_without_starting_a_round() {
    use tankyard_server::{HostedServer, PartyPlan};
    let (server, _) = arena(Mode::CoopPve);
    let mut settings = server.settings.clone();
    settings.bot_memory_file = Some(String::new());
    drop(server);
    let host = HostedServer::start(settings).unwrap();
    let prepared = host.plan_party_async(PartyPlan::default()).unwrap();
    prepared
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let status = host.command("status").unwrap();
    assert!(status.contains("round 1"), "{status}");
    assert!(status.contains(Mode::CoopPve.name()), "{status}");

    let missing = std::env::temp_dir().join(format!(
        "tankyard-missing-party-pack-{}",
        std::process::id()
    ));
    let refused = host
        .plan_party_async(PartyPlan {
            mods: vec![missing],
            boss_rush: Vec::new(),
        })
        .unwrap();
    assert!(
        refused
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_err()
    );
    assert!(host.command("status").unwrap().contains("round 1"));
}

#[test]
fn a_leader_brings_a_new_map_and_boss_lineup_to_a_running_party() {
    use tankyard_core::BossKind;
    use tankyard_protocol::{MatchSettings, PartyMatchOptions, ServerCommand};
    use tankyard_server::PartyPlan;
    let root = std::env::temp_dir().join(format!("tankyard-party-map-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut map = tankyard_content::builtin_maps().remove(0);
    map.id = "leaders_yard".into();
    map.name = "Leader's Yard".into();
    let manifest = serde_json::json!({
        "api_version": 1, "id": "leaders_pack", "name": "Leader's Pack",
        "version": "1", "maps": [map],
    });
    std::fs::write(
        root.join("mod.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();

    let (mut server, invite) = arena(Mode::CoopPve);
    let mut leader = NetworkClient::connect(&invite.link(), "Leader", "").unwrap();
    let mut guest = NetworkClient::connect(&invite.link(), "Guest", "").unwrap();
    ready(&mut server, &mut [&mut leader, &mut guest]);
    let public: String = leader
        .admin_public_key()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert!(
        !server
            .admin_command(&format!("owner-client {} {public}", leader.client_id()))
            .message
            .starts_with("Error:")
    );
    pump(&mut server, &mut [&mut leader, &mut guest], 65);
    let settings = |map: &str| MatchSettings {
        mode: Mode::BossRush,
        map: map.into(),
        difficulty: "Normal".into(),
        bots: 3,
        party: Some(PartyMatchOptions {
            width: 3600,
            height: 2600,
            dynamic_arena: false,
            starting_level: 1,
        }),
    };

    // Without the plan the party's server has never heard of this map, and
    // asking for it changes nothing.
    assert!(leader.admin(ServerCommand::ConfigureParty {
        settings: settings("leaders_yard")
    }));
    pump(&mut server, &mut [&mut leader, &mut guest], 10);
    assert_eq!(server.arenas[0].session.world.config.mode, Mode::CoopPve);
    assert!(
        leader
            .last_action
            .as_deref()
            .is_some_and(|m| m.contains("Map"))
    );

    // A pack that won't load is refused before it can touch the round.
    assert!(
        server
            .plan_party(PartyPlan {
                mods: vec![root.join("nothing_here")],
                boss_rush: Vec::new(),
            })
            .is_err()
    );
    assert_eq!(server.arenas[0].session.world.config.mode, Mode::CoopPve);

    // With it, both players land on the new map, facing the chosen bosses.
    let before = leader.player_round.unwrap();
    server
        .plan_party(PartyPlan {
            mods: vec![root.clone()],
            boss_rush: vec![BossKind::Trapper, BossKind::Prism],
        })
        .unwrap();
    assert_eq!(server.arenas[0].session.world.config.mode, Mode::CoopPve);
    // A guest cannot consume the leader's prepared map or change the round.
    assert!(!guest.admin(ServerCommand::ConfigureParty {
        settings: settings("leaders_yard")
    }));
    pump(&mut server, &mut [&mut leader, &mut guest], 10);
    assert_eq!(leader.player_round, Some(before));
    assert_eq!(server.arenas[0].session.world.config.mode, Mode::CoopPve);
    assert!(leader.admin(ServerCommand::ConfigureParty {
        settings: settings("leaders_yard")
    }));
    pump(&mut server, &mut [&mut leader, &mut guest], 80);
    assert_eq!(leader.player_round, Some(before + 1));
    assert_eq!(guest.player_round, leader.player_round);
    let world = &server.arenas[0].session.world;
    assert_eq!(world.config.map, "leaders_yard");
    assert_eq!(world.bosses[0].kind, BossKind::Trapper);
    assert!(
        world.match_state.announcement.contains("1 / 2"),
        "{}",
        world.match_state.announcement
    );
    for client in [&leader, &guest] {
        assert!(client.is_ready());
        assert!(client.bundle.maps.iter().any(|m| m.id == "leaders_yard"));
        assert_eq!(client.snapshot.as_ref().unwrap().config.map, "leaders_yard");
    }

    // The next round keeps the lineup, and a late friend gets the map too.
    assert!(
        !server
            .admin_command("restart")
            .message
            .starts_with("Error:")
    );
    pump(&mut server, &mut [&mut leader, &mut guest], 65);
    assert_eq!(
        server.arenas[0].session.world.bosses[0].kind,
        BossKind::Trapper
    );
    let mut late = NetworkClient::connect(&invite.link(), "Late", "").unwrap();
    ready(&mut server, &mut [&mut leader, &mut guest, &mut late]);
    assert!(late.bundle.maps.iter().any(|m| m.id == "leaders_yard"));

    // Going back to a built-in map works with the same packs still loaded.
    assert!(leader.admin(ServerCommand::ConfigureParty {
        settings: settings("open")
    }));
    pump(&mut server, &mut [&mut leader, &mut guest, &mut late], 80);
    assert_eq!(server.arenas[0].session.world.config.map, "open");
    assert!(leader.is_ready() && guest.is_ready() && late.is_ready());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn invite_authentication_survives_udp_address_translation_and_packet_loss() {
    use tankyard_server::netem::{EmulationConfig, NetworkEmulator};
    let (mut server, mut invite) = arena(Mode::TwoTeams);
    let proxy = NetworkEmulator::start(
        server.address,
        EmulationConfig {
            latency_ms: 60,
            jitter_ms: 10,
            loss_percent: 3.,
            ..Default::default()
        },
    )
    .unwrap();
    invite.address = proxy.address;
    let mut guest = NetworkClient::connect(&invite.link(), "Forwarded guest", "").unwrap();
    ready(&mut server, &mut [&mut guest]);
    assert!(guest.is_ready());
    assert!(guest.send_chat("lobby", "Translated UDP works"));
    pump(&mut server, &mut [&mut guest], 120);
    assert!(
        guest
            .chat_messages
            .iter()
            .any(|message| message.text == "Translated UDP works")
    );
}
