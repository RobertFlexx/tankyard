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
    for tank in &mut server.session.world.tanks {
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
            server
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
            server
                .session
                .world
                .tanks
                .iter()
                .filter(|t| !t.bot)
                .all(|t| t.team == 1 && !t.game_creator && !t.creator_boss)
        );
        assert_eq!(
            server
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
                server
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
            server
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
            server
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
    assert_eq!(server.session.world.tanks.len(), 8);
    assert!(!server.session.world.match_state.arena.intermission);
    let survivors = server
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
        server
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
        server
            .session
            .world
            .tanks
            .iter()
            .filter(|tank| tank.team == 1)
            .count()
            <= survivors
    );
    assert_eq!(server.session.world.tanks.len(), 8);
    for tank in &mut server.session.world.tanks {
        if tank.bot {
            tank.team = 2;
        }
    }
    pump(&mut server, &mut [&mut first, &mut second], 5);
    assert!(
        server
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
    assert!(server.session.world.match_state.arena.wave > 0);
    let enemy_ids: Vec<_> = server
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
        assert!(server.session.world.tanks.iter().any(|t| t.id == id));
    }
    assert_eq!(
        server
            .session
            .world
            .tanks
            .iter()
            .filter(|t| t.bot && t.team == 1)
            .count(),
        2
    );
}
