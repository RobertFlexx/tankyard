use crate::*;
use tankyard_content::{Content, ProjectileKind};
fn empty(mode: Mode) -> World {
    World::new(
        WorldConfig {
            mode,
            target_count: 0,
            spawn_protection: 0.,
            ..WorldConfig::default()
        },
        Content::builtin(),
    )
}

fn firing_trace(class: &str, seed: u64, reload: u8, ticks: u64) -> Vec<Vec<u64>> {
    let mut world = World::new(
        WorldConfig {
            seed,
            target_count: 0,
            mode: Mode::Sandbox,
            ..Default::default()
        },
        Content::builtin(),
    );
    let player = world.add_player("Cadence", false);
    world.sandbox_class(player, class);
    for _ in 0..reload {
        assert!(world.upgrade_stat(player, 6));
    }
    for _ in 0..600 {
        world.step();
    }
    let count = world.tanks[0].barrel_shots.len();
    let mut trace = vec![Vec::new(); count];
    world.tanks[0].input.fire = true;
    for tick in 0..ticks {
        let before = world.tanks[0].barrel_shots.clone();
        world.step();
        for (index, previous) in before.iter().enumerate() {
            if world.tanks[0].barrel_shots[index] != *previous {
                trace[index].push(tick);
            }
        }
        world.projectiles.clear();
    }
    trace
}

#[test]
fn multi_barrel_cadence_drifts_between_staggered_shots_and_occasional_volleys() {
    for class in [
        "fork",
        "gunner",
        "triplet",
        "triple_twin",
        "streamliner",
        "echo",
    ] {
        for reload in [0, 7] {
            let trace = firing_trace(class, 42, reload, 9000);
            let mut singles = 0;
            let mut volleys = 0;
            for tick in 120..9000 {
                let count = trace
                    .iter()
                    .filter(|shots| shots.binary_search(&tick).is_ok())
                    .count();
                singles += usize::from(count == 1);
                volleys += usize::from(count >= 2);
            }
            assert!(
                singles > 20,
                "{class} at reload {reload} stays locked together"
            );
            assert!(
                volleys > 0,
                "{class} at reload {reload} never overlaps its rhythm"
            );
            let content = Content::builtin();
            for (barrel, shots) in content.class(class).unwrap().barrels.iter().zip(&trace) {
                let interval = if barrel.pattern == tankyard_content::FirePattern::Burst {
                    (barrel.cooldown + 0.17) / 3.0
                } else {
                    barrel.cooldown
                };
                let expected = 150.0 * stat_scale(reload) / interval;
                assert!(
                    (shots.len() as f32 - expected).abs() < 4.0,
                    "{class} reload {reload}: {} shots, expected {expected}",
                    shots.len()
                );
                if barrel.pattern == tankyard_content::FirePattern::Burst {
                    let short = 0.085 / stat_scale(reload) / DT;
                    for (index, pair) in shots.windows(2).enumerate() {
                        if index % 3 != 2 {
                            assert!(
                                ((pair[1] - pair[0]) as f32 - short).abs() <= 1.01,
                                "a burst must keep its three-shot spacing"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cadence_is_reproducible_for_a_seed_and_varies_between_arenas() {
    let trace = firing_trace("fork", 42, 0, 2400);
    assert_eq!(trace, firing_trace("fork", 42, 0, 2400));
    assert_ne!(trace, firing_trace("fork", 71, 0, 2400));
}

#[test]
fn fractional_reload_and_idle_time_cannot_create_a_shot_backlog() {
    let mut world = empty(Mode::Sandbox);
    let player = world.add_player("Pilot", false);
    world
        .content
        .classes
        .iter_mut()
        .find(|class| class.id == "vector")
        .unwrap()
        .barrels[0]
        .cooldown = 0.071;
    for _ in 0..600 {
        world.step();
    }
    assert_eq!(world.tanks[0].cooldowns, [0.0]);
    world.tanks[0].input.fire = true;
    for _ in 0..6000 {
        world.step();
        world.projectiles.clear();
    }
    let expected = 100.0 / 0.071;
    assert!((world.tanks[0].barrel_shots[0] as f32 - expected).abs() < 2.0);
    world.tanks[0].input.fire = false;
    for _ in 0..600 {
        world.step();
    }
    assert_eq!(world.tanks[0].cooldowns, [0.0]);
    let before = world.tanks[0].barrel_shots[0];
    world.tanks[0].input.fire = true;
    world.step();
    assert_eq!(world.tanks[0].id, player);
    assert_eq!(world.tanks[0].barrel_shots[0], before + 1);
}

#[test]
fn front_and_rear_mounts_stay_collinear_when_the_tank_turns() {
    for class in ["echo", "overtrapper", "hybrid", "relay"] {
        for aim in [0.0, 0.73, 1.57, -2.4] {
            let mut world = empty(Mode::Sandbox);
            let player = world.add_player("Aligned", false);
            world.sandbox_class(player, class);
            world.tanks[0].pos = V2::new(800., 800.);
            world.tanks[0].aim = aim;
            world.tanks[0].input.fire = true;
            world.fire_weapons();
            world.projectiles.clear();
            world.tanks[0].cooldowns.fill(0.);
            world.fire_weapons();
            let axis = V2::from_angle(aim);
            let normal = V2::new(-axis.y, axis.x);
            let origin = world.tanks[0].pos;
            assert!(
                world
                    .projectiles
                    .iter()
                    .any(|shot| (shot.pos - origin).dot(axis) < 0.)
            );
            for shot in &world.projectiles {
                assert!(
                    (shot.pos - origin).dot(normal).abs() < 0.001,
                    "{class} muzzle leaves its centerline at {aim}"
                );
            }
        }
    }
}
#[test]
fn zero_damage_cannot_reset_regeneration_or_replace_damage_credit() {
    let mut world = empty(Mode::Ffa);
    let player = world.add_player("Victim", false);
    let attacker = world.add_player("Attacker", false);
    let bystander = world.add_player("Bystander", false);
    world.tick = 300;
    world.damage_tank(0, 10., attacker, V2::ZERO);
    world.tick += 100;
    world.damage_tank(0, 0., bystander, V2::new(5., 0.));
    world.record_damage(player, bystander, 0.);
    assert_eq!(world.tanks[0].last_attacker, attacker);
    assert_eq!(world.tanks[0].damaged_at, 300);
    assert_eq!(world.tanks[0].health, 90.);
    assert_eq!(world.tanks[0].vel.x, 5.);
    world.reward_contributors(player, attacker, 100, 100);
    assert_eq!(world.tanks[1].score, 100);
    assert_eq!(world.tanks[2].score, 0);
}

#[test]
fn death_clears_temporary_combat_effects_before_respawning() {
    let mut world = empty(Mode::Ffa);
    let player = world.add_player("Pilot", false);
    world.tanks[0].boost = 4.;
    world.tanks[0].shield = 6.;
    world.tanks[0].ability_cooldown = 20.;
    world.tanks[0].carrying = true;
    world.tanks[0].last_attacker = 999;
    world.tanks[0].health = 0.;
    world.step();
    let tank = &world.tanks[0];
    assert!(!tank.alive && !tank.carrying);
    assert_eq!(
        (
            tank.boost,
            tank.shield,
            tank.ability_cooldown,
            tank.last_attacker
        ),
        (0., 0., 0., 0)
    );
    for sequence in 1..=200 {
        world.set_input(
            player,
            PlayerInput {
                sequence,
                ability: true,
                ..Default::default()
            },
        );
        world.step();
    }
    assert!(world.tanks[0].alive);
    assert_eq!((world.tanks[0].boost, world.tanks[0].shield), (0., 0.));
}

#[test]
fn drones_arrive_at_the_cursor_and_recover_full_speed_for_a_distant_command() {
    let mut world = empty(Mode::Ffa);
    let player = world.add_player("Pilot", false);
    world.tanks[0].pos = V2::new(1000., 1000.);
    let drone = world.simple_projectile(
        player,
        0,
        V2::new(1300., 1000.),
        V2::new(0., 300.),
        ProjectileKind::Drone,
        10.,
        7.,
        20.,
        10.,
    );
    let drone_id = drone.id;
    world.projectiles.push(drone);
    world.set_input(
        player,
        PlayerInput {
            fire: true,
            aim_distance: 80.,
            ..Default::default()
        },
    );
    for _ in 0..180 {
        world.step();
    }
    let drone = world.projectiles.iter().find(|p| p.id == drone_id).unwrap();
    assert!(
        drone.pos.distance(V2::new(1080., 1000.)) < 25.,
        "drone did not arrive at its cursor"
    );
    let before = drone.pos;
    world.set_input(
        player,
        PlayerInput {
            sequence: 1,
            fire: true,
            aim_distance: 1000.,
            ..Default::default()
        },
    );
    for _ in 0..60 {
        world.step();
    }
    let drone = world.projectiles.iter().find(|p| p.id == drone_id).unwrap();
    assert!(
        drone.vel.length() > 280.,
        "drone never recovered its launch speed"
    );
    assert!(drone.pos.x > before.x + 200.);
}

#[test]
fn drone_guard_slots_do_not_jump_when_another_projectile_is_removed() {
    fn arena() -> (World, EntityId) {
        let mut world = empty(Mode::Ffa);
        let player = world.add_player("Pilot", false);
        world.tanks[0].pos = V2::new(1000., 1000.);
        let other = world.simple_projectile(
            player,
            0,
            V2::new(100., 100.),
            V2::ZERO,
            ProjectileKind::Bullet,
            1.,
            4.,
            10.,
            1.,
        );
        world.projectiles.push(other);
        let drone = world.simple_projectile(
            player,
            0,
            V2::new(1100., 1000.),
            V2::new(0., 300.),
            ProjectileKind::Drone,
            10.,
            7.,
            20.,
            10.,
        );
        let id = drone.id;
        world.projectiles.push(drone);
        world.set_input(
            player,
            PlayerInput {
                alt_fire: true,
                ..Default::default()
            },
        );
        (world, id)
    }
    let (mut a, drone) = arena();
    let (mut b, _) = arena();
    b.projectiles.remove(0);
    for _ in 0..30 {
        a.step();
        b.step();
    }
    let a = a.projectiles.iter().find(|p| p.id == drone).unwrap();
    let b = b.projectiles.iter().find(|p| p.id == drone).unwrap();
    assert!(a.pos.distance(b.pos) < 0.001);
    assert!(a.vel.distance(b.vel) < 0.001);
}
#[test]
fn movement_ability_prediction_matches_the_server_in_open_space_and_near_walls() {
    use tankyard_content::Ability;
    for ability in [
        Ability::Dash,
        Ability::Shield,
        Ability::Recall,
        Ability::Cloak,
        Ability::Overdrive,
        Ability::Blink,
        Ability::Fortify,
        Ability::Parry,
    ] {
        for obstacle in [false, true] {
            let mut world = empty(Mode::Sandbox);
            let class = world
                .content
                .classes
                .iter()
                .find(|c| c.ability == ability)
                .unwrap()
                .id
                .clone();
            let player = world.add_player("Prediction", false);
            assert!(world.sandbox_class(player, &class));
            world.tanks[0].pos = V2::new(1000., 1000.);
            if obstacle {
                world.walls.push(Rect {
                    x: 1150.,
                    y: 700.,
                    w: 40.,
                    h: 600.,
                });
            }
            let mut predicted = world.tanks[0].clone();
            for sequence in 1..=120 {
                let input = PlayerInput {
                    sequence,
                    movement: V2::new(0.4, 0.3),
                    aim: 0.1,
                    ability: true,
                    ..Default::default()
                };
                world.set_input(player, input);
                predicted.input = input;
                physics::predict_tank(
                    &mut predicted,
                    &world.content,
                    &world.config,
                    &world.walls,
                    world.tick + 1,
                    DT,
                );
                world.step();
                let actual = &world.tanks[0];
                assert!(
                    predicted.pos.distance(actual.pos) < 0.001,
                    "{ability:?} position diverged with obstacle={obstacle} on tick {sequence}"
                );
                assert!(
                    predicted.vel.distance(actual.vel) < 0.001,
                    "{ability:?} velocity diverged with obstacle={obstacle} on tick {sequence}"
                );
                assert_eq!(predicted.ability_cooldown, actual.ability_cooldown);
                assert_eq!(predicted.shield, actual.shield);
            }
        }
    }
}

#[test]
fn every_class_moves_fires_and_uses_abilities() {
    let content = Content::builtin();
    for class in &content.classes {
        let mut world = empty(Mode::Sandbox);
        let id = world.add_player("Pilot", false);
        world.add_player("Enemy", false);
        assert!(world.sandbox_class(id, &class.id));
        world.tanks[0].pos = V2::new(900., 900.);
        world.tanks[1].pos = V2::new(1350., 900.);
        let before = world.tanks[0].pos;
        let mut shots = 0;
        let mut abilities = 0;
        for seq in 1..=150 {
            world.set_input(
                id,
                PlayerInput {
                    sequence: seq,
                    movement: V2::new(0., 0.25),
                    aim: 0.,
                    fire: true,
                    ability: seq == 1,
                    ..Default::default()
                },
            );
            world.step();
            shots += world
                .events
                .iter()
                .filter(|e| e.kind as u8 == EventKind::Shot as u8 && e.subject == id)
                .count();
            abilities += world
                .events
                .iter()
                .filter(|e| e.kind as u8 == EventKind::Ability as u8 && e.subject == id)
                .count();
        }
        assert!(
            world.tanks[0].pos.distance(before) > 5.,
            "{} must move",
            class.id
        );
        if !class.barrels.is_empty() {
            assert!(shots > 0, "{} must fire", class.id);
        } else {
            assert!(
                class.multipliers[2] >= 2.,
                "unarmed class must have body specialization"
            );
        }
        if class.ability != tankyard_content::Ability::None {
            assert_eq!(abilities, 1, "{} ability cooldown", class.id);
        }
        assert!(
            world
                .projectiles
                .iter()
                .all(|p| p.pos.finite() && p.vel.finite())
        );
    }
}
#[test]
fn upgrade_budget_and_graph_are_enforced() {
    let mut w = empty(Mode::Sandbox);
    let id = w.add_player("Pilot", false);
    assert!(w.sandbox_level(id, 45));
    for _ in 0..20 {
        w.upgrade_stat(id, 0);
    }
    assert_eq!(w.tanks[0].stats[0], 7);
    assert!(!w.upgrade_stat(id, 8));
    for i in 1..8 {
        for _ in 0..7 {
            w.upgrade_stat(id, i);
        }
    }
    assert_eq!(w.tanks[0].stats.iter().copied().sum::<u8>(), 33);
    assert_eq!(w.tanks[0].points, 0);
    assert!(!w.upgrade_stat(id, 7));
    assert!(!w.evolve(id, "invalid"));
    let tier3 = w
        .content
        .classes
        .iter()
        .find(|c| c.tier == 3)
        .unwrap()
        .id
        .clone();
    assert!(
        !w.evolve(id, &tier3),
        "must traverse parents even at level45"
    );
    let tier1 = w.content.class("vector").unwrap().upgrades[0].clone();
    assert!(w.evolve(id, &tier1));
    assert!(!w.evolve(id, "vector"));
}
#[test]
fn authoritative_input_rejects_invalid_and_stale() {
    let mut w = empty(Mode::Ffa);
    let id = w.add_player("Pilot", false);
    assert!(!w.set_input(
        id,
        PlayerInput {
            movement: V2::new(f32::NAN, 1.),
            ..Default::default()
        }
    ));
    assert!(!w.set_input(
        id,
        PlayerInput {
            aim: f32::INFINITY,
            ..Default::default()
        }
    ));
    assert!(!w.set_input(
        id,
        PlayerInput {
            movement: V2::new(5., 0.),
            ..Default::default()
        }
    ));
    assert!(w.set_input(
        id,
        PlayerInput {
            sequence: 10,
            ..Default::default()
        }
    ));
    assert!(!w.set_input(
        id,
        PlayerInput {
            sequence: 9,
            ..Default::default()
        }
    ));
}
#[test]
fn swept_combat_damage_score_and_respawn() {
    let mut w = empty(Mode::Ffa);
    let a = w.add_player("A", false);
    let b = w.add_player("B", false);
    w.tanks[0].pos = V2::new(500., 500.);
    w.tanks[1].pos = V2::new(800., 500.);
    let shot = w.simple_projectile(
        a,
        0,
        V2::new(600., 500.),
        V2::new(24000., 0.),
        ProjectileKind::Bullet,
        200.,
        3.,
        1.,
        1.,
    );
    w.projectiles.push(shot);
    w.step();
    assert!(!w.tanks.iter().find(|t| t.id == b).unwrap().alive);
    assert_eq!(w.tanks[0].kills, 1);
    assert!(w.tanks[0].score > 0);
    for _ in 0..185 {
        w.step();
    }
    assert!(!w.tanks[1].alive, "player must explicitly request respawn");
    assert!(w.set_input(
        b,
        PlayerInput {
            ability: true,
            sequence: 1,
            ..Default::default()
        }
    ));
    w.step();
    assert!(w.tanks[1].alive);
    assert!(w.tanks[1].health > 0.);
}
#[test]
fn one_hit_budget_stops_before_second_target() {
    let mut w = empty(Mode::Ffa);
    let id = w.add_player("A", false);
    for (n, x) in [(1000, 600.), (1001, 700.)] {
        w.targets.push(Target {
            id: n,
            pos: V2::new(x, 500.),
            vel: V2::ZERO,
            radius: 20.,
            health: 100.,
            max_health: 100.,
            kind: TargetKind::Square,
            angle: 0.,
            xp: 10,
        });
    }
    let shot = w.simple_projectile(
        id,
        0,
        V2::new(500., 500.),
        V2::new(24000., 0.),
        ProjectileKind::Bullet,
        20.,
        3.,
        1.,
        1.,
    );
    w.projectiles.push(shot);
    w.step();
    assert_eq!(w.targets[0].health, 80.);
    assert_eq!(w.targets[1].health, 100.);
}
#[test]
fn team_damage_and_spawn_protection() {
    let mut w = empty(Mode::TwoTeams);
    let a = w.add_player("A", false);
    let b = w.add_player("B", false);
    w.tanks[1].team = 1;
    w.tanks[0].pos = V2::new(500., 500.);
    w.tanks[1].pos = V2::new(800., 500.);
    let shot = w.simple_projectile(
        a,
        1,
        V2::new(700., 500.),
        V2::new(12000., 0.),
        ProjectileKind::Bullet,
        80.,
        3.,
        1.,
        1.,
    );
    w.projectiles.push(shot);
    w.step();
    assert_eq!(w.tanks[1].health, 100.);
    w.tanks[1].team = 2;
    w.tanks[1].protection = 2.;
    let shot = w.simple_projectile(
        a,
        1,
        V2::new(700., 500.),
        V2::new(12000., 0.),
        ProjectileKind::Bullet,
        80.,
        3.,
        1.,
        1.,
    );
    w.projectiles.push(shot);
    w.step();
    assert_eq!(w.tanks.iter().find(|t| t.id == b).unwrap().health, 100.);
}
#[test]
fn every_boss_attacks_and_never_hits_itself() {
    for kind in [
        BossKind::Sentinel,
        BossKind::Prism,
        BossKind::Foundry,
        BossKind::Tempest,
        BossKind::Bastion,
        BossKind::Eclipse,
    ] {
        let mut w = empty(Mode::Sandbox);
        w.add_player("A", false);
        w.tanks[0].pos = V2::new(2300., 1300.);
        let boss = w.spawn_boss(kind);
        let starting = w.bosses[0].health;
        let mut shots = 0;
        for _ in 0..220 {
            w.step();
            shots += w
                .events
                .iter()
                .filter(|e| e.subject == boss && e.kind as u8 == EventKind::Shot as u8)
                .count();
        }
        assert!(shots > 0, "{kind:?} must attack");
        assert_eq!(
            w.bosses[0].health, starting,
            "{kind:?} cannot damage itself"
        );
        if kind == BossKind::Foundry {
            assert!(
                w.projectiles
                    .iter()
                    .any(|p| p.owner == boss && p.kind == ProjectileKind::Drone),
                "boss drones persist"
            );
        }
    }
}
#[test]
fn bounded_projectiles_and_long_running_finite_state() {
    let mut w = empty(Mode::Sandbox);
    w.config.max_projectiles = 150;
    let id = w.add_player("A", false);
    let class = w
        .content
        .classes
        .iter()
        .max_by_key(|c| c.barrels.len() * c.barrels.iter().map(|b| b.count as usize).sum::<usize>())
        .unwrap()
        .id
        .clone();
    w.sandbox_class(id, &class);
    w.sandbox_invulnerability(id, true);
    for sequence in 1..=600 {
        w.set_input(
            id,
            PlayerInput {
                sequence,
                aim: sequence as f32 * 0.05,
                fire: true,
                movement: V2::new(1., 1.),
                ability: sequence % 120 == 0,
                ..Default::default()
            },
        );
        w.step();
        assert!(w.projectiles.len() <= 150);
        assert!(w.tanks[0].pos.finite());
        assert!(
            w.projectiles
                .iter()
                .all(|p| p.pos.finite() && p.vel.finite())
        );
    }
}
#[test]
fn same_seed_and_inputs_produce_identical_simulation() {
    let config = WorldConfig {
        target_count: 50,
        ..Default::default()
    };
    let mut a = World::new(config.clone(), Content::builtin());
    let mut b = World::new(config, Content::builtin());
    let id1 = a.add_player("A", false);
    let id2 = b.add_player("A", false);
    assert_eq!(id1, id2);
    for sequence in 1..=500 {
        let input = PlayerInput {
            sequence,
            movement: V2::from_angle(sequence as f32 * 0.01),
            aim: sequence as f32 * 0.02,
            fire: true,
            ..Default::default()
        };
        a.set_input(id1, input);
        b.set_input(id2, input);
        a.step();
        b.step();
        assert_eq!(a.tanks[0].pos, b.tanks[0].pos);
        assert_eq!(a.targets.len(), b.targets.len());
        assert_eq!(a.projectiles.len(), b.projectiles.len());
    }
}
#[test]
fn mothership_and_boss_rush_complete() {
    let mut w = empty(Mode::Mothership);
    w.bosses[1].health = 0.;
    w.step();
    assert_eq!(w.match_state.winner, Some(1));
    let mut w = empty(Mode::BossRush);
    w.add_player("A", false);
    for _ in 0..6 {
        for b in &mut w.bosses {
            b.health = 0.;
        }
        for _ in 0..250 {
            w.step();
        }
    }
    assert!(w.match_state.finished);
    assert_eq!(w.match_state.boss_wave, 6);
    assert_eq!(w.match_state.round, 1);
}

#[test]
fn later_boss_rush_rounds_still_have_six_waves() {
    for round in [2, 7, 42] {
        let mut world = empty(Mode::BossRush);
        world.match_state.round = round;
        world.add_player("Pilot", false);
        assert_eq!(world.match_state.boss_wave, 1);
        for wave in 1..=6 {
            assert_eq!(world.match_state.boss_wave, wave);
            let boss = world
                .bosses
                .iter_mut()
                .find(|boss| boss.health > 0.0)
                .unwrap();
            assert_eq!(boss.max_health, 1800.0 + f32::from(wave) * 350.0);
            boss.health = 0.0;
            for _ in 0..250 {
                world.step();
            }
            assert_eq!(world.match_state.finished, wave == 6);
        }
        assert_eq!(world.match_state.round, round);
    }
}

#[test]
fn rim_turrets_respect_their_outward_targeting_arcs() {
    for aim in [0.0, 0.9, 2.6] {
        for port in 0..3 {
            let mut world = empty(Mode::Sandbox);
            let player = world.add_player("Pilot", false);
            world.add_player("Target", false);
            world.sandbox_class(player, "auto_three");
            world.tanks[0].pos = V2::new(700.0, 700.0);
            world.tanks[0].aim = aim;
            world.tanks[0].input.aim = aim;
            let angle = world.content.class("auto_three").unwrap().barrels[port].angle;
            world.tanks[1].pos = world.tanks[0].pos + V2::from_angle(aim + angle) * 300.0;
            world.tanks[1].protection = 0.0;
            world.fire_weapons();
            assert_eq!(
                world.projectiles.len(),
                1,
                "rim guns should not all aim inward at one enemy"
            );
            assert_eq!(world.tanks[0].barrel_shots[port], 1);
        }
    }
}

#[test]
fn sustained_direct_weapon_damage_stays_within_tier_budgets() {
    use tankyard_content::ProjectileKind::{Barrier, Drone, Mine, Trap, Turret};
    let content = Content::builtin();
    for class in &content.classes {
        let mut world = empty(Mode::Sandbox);
        let player = world.add_player("Pilot", false);
        world.sandbox_class(player, &class.id);
        world.tanks[0].input.fire = true;
        let mut damage = 0.0;
        // Measure steady fire after its initial volley has passed.
        for tick in 0..4200 {
            world.fire_weapons();
            if tick >= 600 {
                damage += world
                    .projectiles
                    .iter()
                    .filter(|p| !matches!(p.kind, Barrier | Drone | Mine | Trap | Turret))
                    .map(|p| p.damage)
                    .sum::<f32>();
            }
            world.projectiles.clear();
            world.events.clear();
            world.tick += 1;
        }
        let limit = [32.0, 48.0, 72.0, 95.0][class.tier as usize];
        assert!(
            damage / 60.0 <= limit + 4.0,
            "{} exceeds the tier {} damage budget: {}",
            class.id,
            class.tier,
            damage / 60.0
        );
    }
}

#[test]
fn opposing_projectiles_intercept_using_relative_sweeps() {
    let mut w = empty(Mode::Ffa);
    let a = w.add_player("A", false);
    let b = w.add_player("B", false);
    w.tanks[0].pos = V2::new(100., 100.);
    w.tanks[1].pos = V2::new(1200., 1200.);
    let first = w.simple_projectile(
        a,
        0,
        V2::new(350., 500.),
        V2::new(12000., 0.),
        ProjectileKind::Bullet,
        30.,
        5.,
        1.,
        1.,
    );
    let second = w.simple_projectile(
        b,
        0,
        V2::new(650., 500.),
        V2::new(-12000., 0.),
        ProjectileKind::Bullet,
        30.,
        5.,
        1.,
        1.,
    );
    w.projectiles.extend([first, second]);
    w.step();
    assert!(
        w.projectiles.is_empty(),
        "crossing bullets must intercept even when their endpoints do not overlap"
    );
}

#[test]
fn all_maps_spawn_clear_and_tick() {
    for map in tankyard_content::builtin_maps() {
        let mut world = World::new(
            WorldConfig {
                map: map.id.clone(),
                width: map.width,
                height: map.height,
                target_count: 20,
                mode: if map.id == "domination" {
                    Mode::Domination
                } else {
                    Mode::Ffa
                },
                ..Default::default()
            },
            Content::builtin(),
        );
        for _ in 0..12 {
            world.add_player("Pilot", true);
        }
        assert!(
            world
                .tanks
                .iter()
                .all(|t| world.position_clear(t.pos, t.radius)),
            "{} tank spawns",
            map.id
        );
        assert!(
            world
                .targets
                .iter()
                .all(|t| world.position_clear(t.pos, t.radius)),
            "{} target spawns",
            map.id
        );
        for _ in 0..30 {
            world.step();
        }
        assert!(world.tanks.iter().all(|t| t.pos.finite()));
    }
}

#[test]
fn objective_modes_never_score_kills_as_captures() {
    for mode in [Mode::Domination, Mode::CaptureCore] {
        let mut w = empty(mode);
        w.config.score_limit = 1;
        let killer = w.add_player("A", false);
        w.add_player("B", false);
        w.tanks[1].health = 0.;
        w.tanks[1].last_attacker = killer;
        w.step();
        assert_eq!(w.match_state.scores, [0; 4]);
        assert!(!w.match_state.finished);
    }
}
#[test]
fn automatic_weapons_break_protection_when_firing() {
    let mut w = empty(Mode::Sandbox);
    let id = w.add_player("A", false);
    let class = w
        .content
        .classes
        .iter()
        .find(|c| {
            c.barrels
                .iter()
                .any(|b| b.pattern == tankyard_content::FirePattern::Auto)
        })
        .unwrap()
        .id
        .clone();
    w.sandbox_class(id, &class);
    w.tanks[0].pos = V2::new(800., 800.);
    w.tanks[0].protection = 3.;
    w.targets.push(Target {
        id: 1000,
        pos: V2::new(1050., 800.),
        vel: V2::ZERO,
        radius: 20.,
        health: 500.,
        max_health: 500.,
        kind: TargetKind::Square,
        angle: 0.,
        xp: 10,
    });
    w.step();
    assert_eq!(w.tanks[0].protection, 0.);
    assert!(w.tanks[0].shot_count > 0);
}

#[test]
fn automatic_cannons_track_moving_targets_while_reloading() {
    let mut world = empty(Mode::Sandbox);
    let player = world.add_player("Auto", false);
    world.add_player("Enemy", false);
    assert!(world.sandbox_class(player, "triangulator"));
    world.tanks[0].pos = V2::new(800.0, 800.0);
    world.tanks[1].pos = V2::new(1000.0, 800.0);
    world.fire_weapons();
    assert!(!world.projectiles.is_empty());
    world.projectiles.clear();
    let fired = world.tanks[0].shot_count;

    world.tanks[1].pos = V2::new(800.0, 1100.0);
    for tick in 1..=6 {
        world.tick = tick;
        world.fire_weapons();
    }
    assert!((world.tanks[0].barrel_aims[0] - std::f32::consts::FRAC_PI_2).abs() < 0.001);
    assert_eq!(world.tanks[0].shot_count, fired);
    assert!(world.projectiles.is_empty(), "tracking must respect reload");
}

#[test]
fn automatic_cannons_choose_visible_enemies() {
    let mut world = empty(Mode::Sandbox);
    let player = world.add_player("Auto", false);
    for name in ["Hidden", "Visible", "Ally", "Protected"] {
        world.add_player(name, false);
    }
    assert!(world.sandbox_class(player, "triangulator"));
    world.tanks[0].pos = V2::new(800.0, 800.0);
    world.tanks[0].team = 1;
    world.tanks[1].pos = V2::new(950.0, 800.0);
    world.tanks[1].team = 2;
    world.tanks[2].pos = V2::new(800.0, 1100.0);
    world.tanks[2].team = 2;
    world.tanks[3].pos = V2::new(850.0, 800.0);
    world.tanks[3].team = 1;
    world.tanks[4].pos = V2::new(860.0, 800.0);
    world.tanks[4].team = 2;
    world.tanks[4].protection = 3.0;
    world.walls.push(Rect {
        x: 900.0,
        y: 750.0,
        w: 10.0,
        h: 100.0,
    });

    world.fire_weapons();
    let shots: Vec<_> = world
        .projectiles
        .iter()
        .filter(|p| p.owner == player)
        .collect();
    assert_eq!(shots.len(), 3);
    assert!(
        shots
            .iter()
            .all(|shot| shot.vel.y > 0.0 && shot.vel.x.abs() < 0.01)
    );

    world.projectiles.clear();
    world.tanks[2].alive = false;
    world.tanks[0].cooldowns.fill(0.0);
    world.fire_weapons();
    assert!(world.projectiles.iter().all(|p| p.owner != player));
}
#[test]
fn boss_contacts_separate_and_allow_ramming_damage() {
    let mut w = empty(Mode::Sandbox);
    w.add_player("Ram", false);
    w.spawn_boss(BossKind::Sentinel);
    w.tanks[0].pos = w.bosses[0].pos + V2::new(30., 0.);
    let health = w.bosses[0].health;
    w.step();
    assert!(
        w.tanks[0].pos.distance(w.bosses[0].pos) >= w.tanks[0].radius + w.bosses[0].radius - 0.1
    );
    assert!(w.bosses[0].health < health);
    assert!(w.tanks[0].health < w.tanks[0].max_health);
}

#[test]
fn respawn_request_cannot_bypass_cooldown() {
    let mut world = empty(Mode::Ffa);
    let player = world.add_player("Retry", false);
    world.tanks[0].alive = false;
    world.tanks[0].respawn_timer = 2.0;
    assert!(world.set_input(
        player,
        PlayerInput {
            sequence: 1,
            ability: true,
            ..Default::default()
        }
    ));
    world.step();
    assert!(!world.tanks[0].alive);
    for _ in 0..122 {
        world.step();
    }
    assert!(world.tanks[0].alive);
}

#[test]
fn respawn_without_player_consent_does_not_happen() {
    let mut world = empty(Mode::Ffa);
    world.add_player("Observer", false);
    world.tanks[0].alive = false;
    world.tanks[0].respawn_timer = 0.0;
    for _ in 0..200 {
        world.step();
    }
    assert!(!world.tanks[0].alive);
}

#[test]
fn player_death_reduces_both_score_and_level_and_restores_earned_points() {
    let mut world = empty(Mode::Ffa);
    let player = world.add_player("Veteran", false);
    assert!(
        !world.sandbox_level(player, 45),
        "FFA cannot use sandbox cheats"
    );
    world.award(player, xp_for_level(45), xp_for_level(45));
    assert_eq!(world.tanks[0].level, 45);
    assert_eq!(world.tanks[0].points, 33);
    assert!(world.upgrade_stat(player, 3));
    assert!(world.upgrade_stat(player, 3));
    assert_eq!(world.tanks[0].stats[3], 2);
    world.tanks[0].health = 0.0;
    world.step();
    let tank = &world.tanks[0];
    assert!(!tank.alive);
    assert_eq!(tank.level, 22);
    assert_eq!(tank.xp, xp_for_level(22));
    assert_eq!(tank.score, xp_for_level(22));
    assert_eq!(tank.points, points_for_level(22));
    assert_eq!(tank.stats, [0; 8]);
    assert_eq!(tank.class, "vector");
    for _ in 0..185 {
        world.step();
    }
    assert!(!world.tanks[0].alive);
    assert!(world.set_input(
        player,
        PlayerInput {
            ability: true,
            sequence: 1,
            ..Default::default()
        }
    ));
    world.step();
    assert!(world.tanks[0].alive);
    assert_eq!(world.tanks[0].level, 22);
    assert_eq!(world.tanks[0].points, 21);
}

#[test]
fn surviving_projectiles_exchange_damage_only_once() {
    let mut world = empty(Mode::Ffa);
    let first = world.simple_projectile(
        1000,
        0,
        V2::new(350., 500.),
        V2::new(12000., 0.),
        ProjectileKind::Bullet,
        20.,
        5.,
        1.,
        3.,
    );
    let second = world.simple_projectile(
        1001,
        0,
        V2::new(650., 500.),
        V2::new(-12000., 0.),
        ProjectileKind::Bullet,
        20.,
        5.,
        1.,
        3.,
    );
    world.projectiles.extend([first, second]);
    world.step();
    assert_eq!(world.projectiles.len(), 2);
    for projectile in &world.projectiles {
        assert_eq!(projectile.penetration, 2.0);
        assert_eq!(projectile.hit.len(), 1);
    }
}

#[test]
fn fast_explosives_blast_at_contact_instead_of_the_sweep_endpoint() {
    let mut world = empty(Mode::Ffa);
    let owner = world.add_player("Owner", false);
    world.add_player("Hit", false);
    world.add_player("Nearby", false);
    world.tanks[0].pos = V2::new(100., 100.);
    world.tanks[1].pos = V2::new(700., 500.);
    world.tanks[2].pos = V2::new(700., 550.);
    let shot = world.simple_projectile(
        owner,
        0,
        V2::new(500., 500.),
        V2::new(36000., 0.),
        ProjectileKind::Explosive,
        80.,
        3.,
        1.,
        1.,
    );
    world.projectiles.push(shot);
    world.step();
    assert!(
        world.tanks[2].health < 100.,
        "nearby tank must receive splash at the impact"
    );
    assert!(world.tanks[2].alive);
}

#[test]
fn cannon_muzzles_cannot_spawn_bullets_through_a_thin_wall() {
    let mut world = empty(Mode::Sandbox);
    let player = world.add_player("Wall", false);
    world.tanks[0].pos = V2::new(500., 500.);
    world.walls.push(Rect {
        x: 524.,
        y: 400.,
        w: 5.,
        h: 200.,
    });
    world.set_input(
        player,
        PlayerInput {
            sequence: 1,
            fire: true,
            ..Default::default()
        },
    );
    world.step();
    assert!(
        world.projectiles.is_empty(),
        "shot must stop at the wall inside its barrel"
    );
}

#[test]
fn triple_twin_fires_three_balanced_parallel_pairs_at_any_heading() {
    for aim in [0.0, 0.73, 1.57, -2.4] {
        let mut world = empty(Mode::Sandbox);
        let player = world.add_player("Twins", false);
        assert!(world.sandbox_class(player, "triple_twin"));
        world.tanks[0].pos = V2::new(800.0, 800.0);
        world.tanks[0].aim = aim;
        world.tanks[0].input.fire = true;
        world.fire_weapons();
        world.projectiles.clear();
        world.tanks[0].vel = V2::ZERO;
        world.tanks[0].cooldowns.fill(0.0);
        world.fire_weapons();
        assert_eq!(world.projectiles.len(), 6);
        for (index, pair) in world.projectiles.as_chunks::<2>().0.iter().enumerate() {
            let direction = V2::from_angle(aim + index as f32 * std::f32::consts::TAU / 3.0);
            let perpendicular = V2::new(-direction.y, direction.x);
            let delta = pair[1].pos - pair[0].pos;
            assert!(
                delta.dot(direction).abs() < 0.001,
                "paired muzzles must end together"
            );
            assert!(
                (delta.dot(perpendicular).abs() - 20.0).abs() < 0.001,
                "paired guns need equal lateral spacing"
            );
            let midpoint = (pair[0].pos + pair[1].pos) * 0.5 - world.tanks[0].pos;
            assert!(
                midpoint.dot(perpendicular).abs() < 0.001,
                "each pair must be centered on its firing axis"
            );
        }
    }
}

#[test]
fn battleship_launches_from_four_opposing_side_ports() {
    let mut world = empty(Mode::Sandbox);
    let player = world.add_player("Ports", false);
    world.sandbox_class(player, "battleship");
    world.tanks[0].pos = V2::new(800.0, 800.0);
    world.tanks[0].aim = 0.73;
    world.tanks[0].input.fire = true;
    world.fire_weapons();
    world.projectiles.clear();
    world.tanks[0].cooldowns.fill(0.0);
    world.fire_weapons();
    assert_eq!(
        world.projectiles.len(),
        16,
        "the total owner limit stays the same"
    );
    let forward = V2::from_angle(world.tanks[0].aim);
    let mut ports: Vec<(V2, V2)> = Vec::new();
    for projectile in &world.projectiles {
        assert_eq!(projectile.kind, ProjectileKind::Drone);
        if let Some(port) = ports
            .iter_mut()
            .find(|port| port.0.distance(projectile.pos) < 0.001)
        {
            port.1 += projectile.vel;
        } else {
            ports.push((projectile.pos, projectile.vel));
        }
    }
    assert_eq!(ports.len(), 4);
    let mut center = V2::ZERO;
    for (pos, velocity) in &ports {
        assert!(velocity.normalized().dot(forward).abs() < 0.001);
        center += *pos;
    }
    assert!((center / 4.0).distance(world.tanks[0].pos) < 0.001);
    assert!(ports[0].1.normalized().dot(ports[2].1.normalized()) < -0.999);
}

#[test]
fn multi_drone_volley_obeys_the_owner_limit() {
    let mut world = empty(Mode::Sandbox);
    let player = world.add_player("Drones", false);
    world.sandbox_class(player, "flock");
    let limit = usize::from(world.content.class("flock").unwrap().drone_limit);
    for n in 0..limit - 1 {
        let drone = world.simple_projectile(
            player,
            0,
            V2::new(1200. + n as f32 * 20., 1000.),
            V2::ZERO,
            ProjectileKind::Drone,
            10.,
            7.,
            10.,
            3.,
        );
        world.projectiles.push(drone);
    }
    world.tanks[0].input.fire = true;
    world.fire_weapons();
    assert_eq!(
        world.projectiles.len(),
        limit,
        "pending pellets count toward the limit"
    );
}

#[test]
fn contact_resolution_keeps_hulls_inside_the_arena() {
    let mut world = empty(Mode::Ffa);
    world.add_player("Edge", false);
    world.add_player("Contact", false);
    world.tanks[0].pos = V2::new(22., 500.);
    world.tanks[1].pos = V2::new(30., 500.);
    world.tanks[0].vel = V2::new(-100., 0.);
    world.step();
    for tank in &world.tanks {
        assert!(tank.pos.x >= tank.radius && tank.pos.x <= world.config.width - tank.radius);
    }
    assert!(world.tanks[0].vel.x >= 0.0);
}

#[test]
fn shape_knockback_slows_down_after_a_hit() {
    let mut world = empty(Mode::Ffa);
    world.targets.push(Target {
        id: 1000,
        pos: V2::new(800., 800.),
        vel: V2::new(90., 0.),
        radius: 14.,
        health: 35.,
        max_health: 35.,
        kind: TargetKind::Square,
        angle: 0.,
        xp: 22,
    });
    for _ in 0..120 {
        world.step();
    }
    assert!(world.targets[0].vel.length() < 1.0);
    assert!(world.targets[0].pos.x < 840.0);
}

#[test]
fn reused_snapshots_refresh_entities_and_remove_old_ones() {
    let mut world = empty(Mode::Ffa);
    let first = world.add_player("First", false);
    let mut display = world.snapshot();
    let second = world.add_player("Second", false);
    world.remove_player(first);
    world.tanks[0].input.movement = V2::new(1., 0.);
    world.step();
    world.snapshot_into(&mut display);
    assert_eq!(display.tick, world.tick);
    assert_eq!(display.tanks.len(), 1);
    assert_eq!(display.tanks[0].id, second);
    assert_eq!(display.tanks[0].pos, world.tanks[0].pos);
    assert_eq!(display.tanks[0].name, "Second");
}

#[test]
fn manual_recoil_prediction_matches_the_authoritative_simulation() {
    for class in [
        "vector",
        "fork",
        "lance",
        "spray",
        "hammer",
        "gyro",
        "triplet",
        "needlefan",
        "streamliner",
        "echo",
    ] {
        let mut world = empty(Mode::Sandbox);
        let player = world.add_player("Prediction", false);
        world.sandbox_class(player, class);
        world.tanks[0].pos = V2::new(1700., 1300.);
        let mut predicted = world.tanks[0].clone();
        for sequence in 1..=180 {
            let input = PlayerInput {
                sequence,
                movement: V2::new(0.4, 0.3),
                aim: sequence as f32 * 0.013,
                fire: sequence % 90 < 65,
                ..Default::default()
            };
            world.set_input(player, input);
            predicted.input = input;
            physics::predict_tank(
                &mut predicted,
                &world.content,
                &world.config,
                &world.walls,
                world.tick + 1,
                DT,
            );
            world.step();
            assert!(
                predicted.pos.distance(world.tanks[0].pos) < 0.001,
                "{class} position diverged on tick {sequence}"
            );
            assert!(
                predicted.vel.distance(world.tanks[0].vel) < 0.001,
                "{class} recoil diverged on tick {sequence}"
            );
        }
    }
}

#[test]
fn spawn_protection_prevents_outgoing_body_damage() {
    let mut world = empty(Mode::Ffa);
    world.add_player("Protected", false);
    world.add_player("Opponent", false);
    world.tanks[0].pos = V2::new(500., 500.);
    world.tanks[1].pos = V2::new(530., 500.);
    world.tanks[0].protection = 3.0;
    world.step();
    assert_eq!(world.tanks[0].health, 100.0);
    assert_eq!(
        world.tanks[1].health, 100.0,
        "protected tanks cannot deal free contact damage"
    );
}
