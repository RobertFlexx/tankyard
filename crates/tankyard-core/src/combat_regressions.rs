use crate::*;
use tankyard_content::{Content, ProjectileKind};

fn arena(class: &str) -> (World, EntityId) {
    let mut world = World::new(
        WorldConfig {
            mode: Mode::Sandbox,
            target_count: 0,
            spawn_protection: 0.,
            ..Default::default()
        },
        Content::builtin(),
    );
    let owner = world.add_player("Pilot", false);
    assert!(world.sandbox_class(owner, class));
    world.tanks[0].pos = V2::new(1000., 1000.);
    (world, owner)
}

fn incoming(world: &mut World, owner: EntityId) {
    let shot = world.simple_projectile(
        owner,
        0,
        V2::new(970., 1000.),
        V2::new(400., 0.),
        ProjectileKind::Bullet,
        8.,
        5.,
        2.,
        1.,
    );
    world.projectiles.push(shot);
}

#[test]
fn late_upgrades_require_their_level_and_every_final_branch_can_reach_fifty_five() {
    let (mut world, id) = arena("riposte");
    assert!(world.sandbox_level(id, 44));
    assert!(!world.evolve(id, "counterguard"));
    assert!(world.sandbox_level(id, 45));
    assert!(world.evolve(id, "counterguard"));
    assert!(!world.evolve(id, "reprisal"));
    assert!(world.sandbox_level(id, 54));
    assert!(!world.evolve(id, "reprisal"));
    assert!(world.sandbox_level(id, 55));
    assert!(world.evolve(id, "reprisal"));
    assert_eq!(world.tanks[0].level, 55);
    assert_eq!(world.tanks[0].points, 38);
    let parents: Vec<_> = world
        .content
        .classes
        .iter()
        .filter(|c| c.tier == 3)
        .map(|c| c.id.clone())
        .collect();
    for parent in parents {
        assert!(world.sandbox_class(id, &parent));
        let children = world.content.class(&parent).unwrap().upgrades.clone();
        assert!(!children.is_empty(), "{parent} has no level 55 upgrade");
        for child in children {
            assert!(world.sandbox_class(id, &parent));
            assert!(
                world.evolve(id, &child),
                "{parent} cannot evolve into {child}"
            );
            assert_eq!(world.content.class(&child).unwrap().min_level, 55);
        }
    }
}

#[test]
fn perfect_parries_reward_timing_without_increasing_normal_cannon_damage() {
    for (class, shots, minimum) in [
        ("riposte", 1, 45.),
        ("counterguard", 1, 55.),
        ("reprisal", 3, 60.),
    ] {
        let (mut world, owner) = arena(class);
        let enemy = world.add_player("Opponent", false);
        world.tanks[1].pos = V2::new(800., 1000.);
        world.tanks[0].input.fire = true;
        world.fire_weapons();
        assert_eq!(
            world.projectiles[0].damage, 18.,
            "ordinary {class} cannon stays unchanged"
        );
        world.projectiles.clear();
        world.tanks[0].input.fire = false;
        world.tanks[0].health *= 0.5;
        let before = world.tanks[0].health;
        world.tanks[0].shield = world
            .content
            .class(class)
            .unwrap()
            .ability
            .parry_window()
            .unwrap();
        incoming(&mut world, enemy);
        assert!(world.parry_projectile(0, 0, V2::new(970., 1000.)));
        assert_eq!(world.projectiles.len(), shots);
        assert!(world.projectiles[0].damage >= minimum);
        assert!(world.tanks[0].health > before);
        assert!(
            world
                .projectiles
                .iter()
                .all(|p| p.kind == ProjectileKind::Homing
                    && p.owner == owner
                    && p.homing_target == enemy
                    && p.lifetime <= 2.2
                    && p.lifetime * p.cruise_speed <= 850.01)
        );
        world.tanks[0].input.fire = true;
        world.tanks[0].cooldowns.fill(0.);
        world.fire_weapons();
        assert!(
            world
                .projectiles
                .iter()
                .any(|p| p.kind == ProjectileKind::Bullet && p.damage == 18.)
        );
    }
}

#[test]
fn a_missed_parry_gets_no_reward_and_counter_echoes_obey_the_pool_limit() {
    let (mut world, _) = arena("reprisal");
    let enemy = world.add_player("Opponent", false);
    world.tanks[1].pos = V2::new(800., 1000.);
    world.tanks[0].input.ability = true;
    world.step();
    world.tanks[0].input.ability = false;
    for _ in 0..12 {
        world.step();
    }
    assert_eq!(world.tanks[0].shield, 0.);
    world.tanks[0].health *= 0.5;
    let health = world.tanks[0].health;
    incoming(&mut world, enemy);
    assert!(!world.parry_projectile(0, 0, V2::new(970., 1000.)));
    assert_eq!(world.tanks[0].health, health);
    assert_eq!(world.projectiles[0].owner, enemy);
    assert_eq!(world.projectiles[0].damage, 8.);
    world.tanks[0].shield = 0.1;
    world.config.max_projectiles = 1;
    assert!(world.parry_projectile(0, 0, V2::new(970., 1000.)));
    assert_eq!(
        world.projectiles.len(),
        1,
        "a full pool cannot create extra echo shots"
    );
}

#[test]
fn trap_range_upgrades_move_the_field_farther_without_extending_its_lifetime() {
    let mut result = Vec::new();
    for points in [0, 7] {
        let (mut world, owner) = arena("anchor");
        for _ in 0..points {
            assert!(world.upgrade_stat(owner, 3));
        }
        world.tanks[0].input.fire = true;
        world.step();
        let origin = world.projectiles[0].previous;
        world.tanks[0].input.fire = false;
        for _ in 0..180 {
            world.step();
        }
        let trap = &world.projectiles[0];
        result.push((trap.pos.distance(origin), trap.lifetime));
        assert_eq!(trap.kind, ProjectileKind::Trap);
    }
    assert!(
        result[0].0 > 80.,
        "base traps must clear a useful area around the tank"
    );
    assert!(result[1].0 > result[0].0 * 2.1);
    assert_eq!(result[0].1, result[1].1);
}

#[test]
fn reload_display_tracks_real_firing_and_charge_and_hides_without_a_manual_weapon() {
    let (mut world, _) = arena("vector");
    world.tanks[0].input.fire = true;
    world.step();
    let class = world.content.class("vector").unwrap();
    assert!(reload_progress(&world.tanks[0], class, &world.config).unwrap() < 0.04);
    world.tanks[0].input.fire = false;
    let mut previous = 0.;
    for _ in 0..40 {
        world.step();
        let progress = reload_progress(
            &world.tanks[0],
            world.content.class("vector").unwrap(),
            &world.config,
        )
        .unwrap();
        assert!(progress >= previous && (0. ..=1.).contains(&progress));
        previous = progress;
    }
    assert_eq!(previous, 1.);
    let (mut charged, _) = arena("lance");
    charged.tanks[0].input.fire = true;
    for _ in 0..20 {
        charged.step();
    }
    let progress = reload_progress(
        &charged.tanks[0],
        charged.content.class("lance").unwrap(),
        &charged.config,
    )
    .unwrap();
    // The lance is now a normal (non-charge) weapon: holding fire keeps it
    // shooting, so the reload display shows a fractional in-progress value.
    assert!(progress > 0.2 && progress < 0.35);
    for class in ["hive", "foundry", "auto_smasher"] {
        let (world, _) = arena(class);
        assert_eq!(
            reload_progress(
                &world.tanks[0],
                world.content.class(class).unwrap(),
                &world.config
            ),
            None
        );
    }
}

#[test]
fn minion_aim_stays_stable_when_the_cursor_is_on_its_body_and_distant_orders_do_not_waste_shots() {
    let (mut world, owner) = arena("foundry");
    let mut minion = world.simple_projectile(
        owner,
        0,
        V2::new(1100., 1000.),
        V2::new(175., 0.),
        ProjectileKind::Minion,
        12.,
        11.,
        60.,
        9.,
    );
    minion.age = 1.;
    minion.gun = Some(MinionGun {
        aim: 1.,
        cooldown: 0.,
        shots: 0,
    });
    world.projectiles.push(minion);
    world.tanks[0].input = PlayerInput {
        fire: true,
        aim_distance: 100.,
        ..Default::default()
    };
    let shot = world.fire_minion(0, 0).unwrap();
    assert!(
        (shot.vel.angle() - 1.).abs() < 0.001,
        "nearby cursor must not reverse a minion's gun"
    );
    world.tanks[0].input.aim_distance = 1500.;
    world.projectiles[0].gun.as_mut().unwrap().cooldown = 0.;
    assert!(
        world.fire_minion(0, 0).is_none(),
        "wait until a commanded target is in flight range"
    );
}

#[test]
fn minion_target_hysteresis_stops_crossing_targets_from_flipping_the_guns() {
    let (mut world, owner) = arena("foundry");
    let a = world.add_player("A", false);
    let b = world.add_player("B", false);
    world.tanks[1].pos = V2::new(1240., 1000.);
    world.tanks[2].pos = V2::new(1250., 1000.);
    let p = V2::new(1100., 1000.);
    assert_eq!(
        world.minion_target_with_id(p, 0, owner, 500., b).unwrap().0,
        b
    );
    world.tanks[1].pos.x = 1150.;
    assert_eq!(
        world.minion_target_with_id(p, 0, owner, 500., b).unwrap().0,
        a
    );
    world.walls.push(Rect {
        x: 1120.,
        y: 980.,
        w: 10.,
        h: 40.,
    });
    assert!(world.minion_target_with_id(p, 0, owner, 500., a).is_none());
}

#[test]
fn commanded_minions_settle_and_passive_replacements_do_not_kick_the_owner() {
    let (mut world, _) = arena("foundry");
    let start = world.tanks[0].pos;
    for _ in 0..600 {
        world.step();
    }
    assert_eq!(
        world.tanks[0].pos, start,
        "replacing minions must not move an idle owner"
    );
    world.tanks[0].input = PlayerInput {
        fire: true,
        aim_distance: 550.,
        ..Default::default()
    };
    for _ in 0..600 {
        world.step();
    }
    let cursor = start + V2::new(550., 0.);
    let minions: Vec<_> = world
        .projectiles
        .iter()
        .filter(|p| p.kind == ProjectileKind::Minion)
        .collect();
    assert_eq!(minions.len(), 6);
    assert!(
        minions
            .iter()
            .all(|p| p.pos.distance(cursor) < 125. && p.vel.length() < 30.)
    );
    world.tanks[0].input.alt_fire = true;
    for _ in 0..600 {
        world.step();
    }
    assert!(
        world
            .projectiles
            .iter()
            .filter(|p| p.kind == ProjectileKind::Minion)
            .all(|p| p.pos.distance(start) < 140.)
    );
}

#[test]
fn full_defense_pools_do_not_create_predicted_shots_or_recoil() {
    for class in ["anchor", "rook"] {
        let (mut world, owner) = arena(class);
        for (kind, limit) in [
            (ProjectileKind::Trap, 24),
            (ProjectileKind::Barrier, 8),
            (ProjectileKind::Turret, 6),
        ] {
            for i in 0..limit {
                let shot = world.simple_projectile(
                    owner,
                    0,
                    V2::new(1800. + i as f32 * 20., 1800.),
                    V2::ZERO,
                    kind,
                    1.,
                    3.,
                    60.,
                    5.,
                );
                world.projectiles.push(shot);
            }
        }
        world.step();
        assert_eq!(world.tanks[0].deployed_defenses, [24, 0, 8, 6]);
        let mut predicted = world.tanks[0].clone();
        world.tanks[0].input.fire = true;
        predicted.input.fire = true;
        for _ in 0..90 {
            physics::predict_tank(
                &mut predicted,
                &world.content,
                &world.config,
                &world.walls,
                world.tick + 1,
                DT,
            );
            world.step();
            assert_eq!(
                predicted.pos, world.tanks[0].pos,
                "{class} recoils despite a full pool"
            );
            assert_eq!(predicted.cooldowns, world.tanks[0].cooldowns);
            assert!(predicted.barrel_shots.iter().all(|&count| count == 0));
        }
    }
}

#[test]
fn single_click_and_held_fire_both_work_on_long_range_tanks() {
    for class in ["lance", "surveyor", "longbow", "shear", "rail"] {
        let (mut world, _) = arena(class);
        world.tanks[0].aim = 0.;
        world.tanks[0].input.fire = true;
        world.step();
        assert!(
            world.tanks[0].shot_count > 0,
            "{class}: a one-tick click did not fire"
        );
        let first_shots = world.tanks[0].shot_count;
        world.tanks[0].input.fire = false;
        for _ in 0..120 {
            world.step();
        }
        assert_eq!(
            world.tanks[0].shot_count, first_shots,
            "{class}: fired without input"
        );
        world.tanks[0].input.fire = true;
        for _ in 0..180 {
            world.step();
        }
        assert!(
            world.tanks[0].shot_count > first_shots + 1,
            "{class}: held fire stalled"
        );
    }
}

#[test]
fn kill_event_preserves_victim_name_without_leaking_it_from_a_respawn() {
    let mut world = World::new(
        WorldConfig {
            target_count: 0,
            spawn_protection: 0.,
            ..WorldConfig::default()
        },
        Content::builtin(),
    );
    let killer = world.add_player("Victor", false);
    let victim = world.add_player("Rival", false);
    world.tanks[1].health = 0.;
    world.tanks[1].last_attacker = killer;
    world.step();
    let event = world
        .events
        .iter()
        .find(|e| matches!(e.kind, EventKind::Kill))
        .unwrap();
    assert_eq!(event.subject, killer);
    let elimination = event.elimination.as_ref().unwrap();
    assert_eq!(elimination.name, "Rival");
    assert_eq!(elimination.victim, victim);
}

#[test]
fn modded_charge_weapon_releases_even_after_a_one_tick_tap() {
    let (mut world, _) = arena("lance");
    let class = world
        .content
        .classes
        .iter_mut()
        .find(|class| class.id == "lance")
        .unwrap();
    class.barrels[0].pattern = tankyard_content::FirePattern::Charge;
    world.tanks[0].input.fire = true;
    world.step();
    assert_eq!(world.tanks[0].shot_count, 0);
    world.tanks[0].input.fire = false;
    world.step();
    assert_eq!(world.tanks[0].shot_count, 1);
}
