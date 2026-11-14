//! A private office maze without arena combat or objectives. Its generated
//! walls keep movement predictable; the host alone applies the healing.
use crate::{PocketEcho, PocketLayout, Rect, Tank, V2, WorldConfig, physics};
use tankyard_content::Content;

pub const POCKET_SECONDS: f32 = 10.;
pub const POCKET_ECHO_LIMIT: usize = 3;
/// The longest still pose before a double gives chase, in seconds.
pub const POCKET_ECHO_WAKE: f32 = 3.;
/// Forty percent of maximum hull health over a complete visit.
pub const POCKET_HEAL_RATE: f32 = 0.04;

/// The collision plan is cached and shared with the renderer and pursuers.
pub fn walls(config: &WorldConfig) -> Vec<Rect> {
    crate::with_pocket_layout(config, |layout| layout.walls.clone())
}

pub(crate) fn advance(tank: &mut Tank, dt: f32, healing: bool) {
    if tank.pocket <= 0. {
        return;
    }
    let elapsed = dt.min(tank.pocket);
    if healing && tank.alive && tank.health > 0. {
        tank.health =
            (tank.health + tank.max_health * POCKET_HEAL_RATE * elapsed).min(tank.max_health);
    }
    tank.pocket = if tank.pocket <= dt + 0.0001 {
        0.
    } else {
        tank.pocket - dt
    };
    if tank.pocket == 0. {
        tank.pocket_echoes.clear();
    }
}

pub(crate) fn populate(
    tank: &mut Tank,
    config: &WorldConfig,
    classes: &[String],
    layout: &PocketLayout,
) {
    tank.pocket_echoes.clear();
    let phase = ((config.seed ^ tank.id.wrapping_mul(0x517cc1b7)) % 628) as f32 * 0.01;
    let mut used = Vec::with_capacity(POCKET_ECHO_LIMIT);
    for index in 0..POCKET_ECHO_LIMIT {
        let angle = phase + index as f32 * std::f32::consts::TAU / POCKET_ECHO_LIMIT as f32;
        let wanted = tank.pos + V2::from_angle(angle) * (420. + index as f32 * 80.);
        let Some(pos) = layout.spawn(tank.pos, wanted, &used) else {
            continue;
        };
        used.push(pos);
        tank.pocket_echoes.push(PocketEcho {
            pos,
            previous: pos,
            vel: V2::ZERO,
            aim: (tank.pos - pos).angle(),
            radius: (tank.radius * 0.9).clamp(4., 24.),
            class: classes
                .get(index % classes.len().max(1))
                .unwrap_or(&tank.class)
                .clone(),
            // A moment to take in the room before the first one moves.
            wake: 1.8 + index as f32 * 0.5,
        });
    }
}

/// Host-only pursuit. A double has no input, weapon state or arena entity id.
/// Contact ends the visit; it cannot inflict damage or leave the private layer.
pub(crate) fn pursue(
    tank: &mut Tank,
    before: V2,
    content: &Content,
    layout: &PocketLayout,
    dt: f32,
) -> bool {
    if tank.pocket <= 0. {
        return false;
    }
    // Slower than the visitor, so steady movement always opens a lead.
    let speed = (physics::tank_speed(tank, content) * 0.74).clamp(95., 195.);
    let target = tank.pos;
    let mut caught = false;
    for echo in &mut tank.pocket_echoes {
        echo.previous = echo.pos;
        echo.wake = (echo.wake - dt).max(0.);
        if echo.wake == 0. {
            let goal = layout
                .route(echo.pos, target, echo.radius)
                .unwrap_or(echo.pos);
            let delta = goal - echo.pos;
            let desired = delta.normalized() * speed.min(delta.length() / dt.max(0.001));
            echo.vel = echo.vel.lerp(desired, 1. - (-9. * dt).exp());
            echo.pos = layout.slide(echo.pos, echo.vel * dt, echo.radius);
            if echo.vel.length_squared() > 1. {
                echo.aim = echo.vel.angle();
            }
        }
        // Relative sweeps catch a crossing, even when both hulls move quickly.
        caught |= physics::sweep_circle(
            echo.previous - before,
            echo.pos - target,
            V2::ZERO,
            tank.radius + echo.radius,
        )
        .is_some();
    }
    if caught {
        tank.pocket = 0.;
        tank.pocket_echoes.clear();
        tank.charge = 0.;
        tank.input.fire = false;
    }
    caught
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DT, Mode, PlayerInput, V2, World, physics};
    use tankyard_content::{Content, ProjectileKind};

    fn world(mode: Mode) -> (World, u64) {
        let mut world = World::new(
            WorldConfig {
                mode,
                target_count: 0,
                spawn_protection: 0.,
                dynamic_arena: false,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Bacteria", false);
        world.tanks[0].team = 1;
        world.set_class(id, "bacteria");
        world.tanks[0].pos = V2::new(520., 520.);
        world.tanks[0].health = world.tanks[0].max_health * 0.2;
        (world, id)
    }

    fn enter(world: &mut World) {
        world.tanks[0].input = PlayerInput {
            ability: true,
            fire: true,
            ..Default::default()
        };
        world.step();
        assert_eq!(world.tanks[0].pocket, POCKET_SECONDS);
    }

    #[test]
    fn ten_seconds_isolated_healing_and_thirty_seconds_exposed() {
        let (mut world, id) = world(Mode::Sandbox);
        let shot = world.simple_projectile(
            id,
            1,
            V2::new(200., 200.),
            V2::ZERO,
            ProjectileKind::Trap,
            20.,
            5.,
            30.,
            1.,
        );
        world.projectiles.push(shot);
        enter(&mut world);
        assert!(!world.tanks[0].tangible());
        assert!(world.projectiles.iter().all(|p| p.owner != id));
        let velocity = world.tanks[0].vel;
        let health = world.tanks[0].health;
        // Exercise the timer and isolation without a catch interrupting it.
        // Pursuit and earned healing on a catch have their own regression below.
        world.tanks[0].pocket_echoes.clear();
        world.damage_tank(0, 10000., 0, V2::new(300., 0.));
        assert_eq!(world.tanks[0].health, health);
        assert_eq!(world.tanks[0].vel, velocity);
        for _ in 0..599 {
            world.step();
            assert!(world.tanks[0].pocket > 0.);
            assert!(world.projectiles.iter().all(|p| p.owner != id));
        }
        world.tanks[0].input = PlayerInput::default();
        world.step();
        let tank = &world.tanks[0];
        assert_eq!(tank.pocket, 0.);
        assert!(tank.tangible());
        assert!((tank.health / tank.max_health - 0.6).abs() < 0.001);
        assert!((tank.ability_cooldown - 30.).abs() < 0.01);
        assert!(world.position_clear(tank.pos, tank.radius));
    }

    #[test]
    fn pocket_cannot_carry_a_core_or_recapture_it() {
        let (mut world, id) = world(Mode::CaptureCore);
        let core = world.objectives.iter_mut().find(|o| o.team == 2).unwrap();
        core.carrier = Some(id);
        enter(&mut world);
        assert!(!world.tanks[0].carrying);
        assert!(world.objectives.iter().all(|o| o.carrier != Some(id)));
        world.tanks[0].input = PlayerInput::default();
        for _ in 0..30 {
            world.step();
        }
        assert!(world.objectives.iter().all(|o| o.carrier != Some(id)));
        assert_eq!(world.match_state.scores, [0; 4]);
    }

    #[test]
    fn pocket_does_not_contest_a_domination_point() {
        let (mut world, _) = world(Mode::Domination);
        enter(&mut world);
        world.tanks[0].input = PlayerInput::default();
        for _ in 0..120 {
            world.tanks[0].pos = world.objectives[0].pos;
            world.step();
        }
        assert_eq!(world.objectives[0].team, 0);
        assert_eq!(world.objectives[0].progress, 0.);
    }

    #[test]
    fn movement_prediction_matches_entry_maze_and_return() {
        let (mut world, _) = world(Mode::Sandbox);
        for kind in [
            tankyard_content::ZoneKind::Speed,
            tankyard_content::ZoneKind::Conveyor,
        ] {
            world.map.zones.push(tankyard_content::MapZone {
                kind,
                rect: [0., 0., 1., 1.],
                strength: 1.5,
                group: 0,
                angle: 40.,
                exit: [0., 0.],
            });
        }
        let mut predicted = world.tanks[0].clone();
        for tick in 0..650 {
            let input = PlayerInput {
                movement: V2::new(if tick < 350 { 1. } else { -1. }, 0.6),
                ability: tick == 0,
                ..Default::default()
            };
            world.tanks[0].input = input;
            predicted.input = input;
            // Match the client sampling pads before every reconciled step.
            predicted.terrain =
                crate::terrain_factor(&world.map, &world.config, 0, &world.poses, predicted.pos);
            predicted.push =
                crate::terrain_push(&world.map, &world.config, 0, &world.poses, predicted.pos);
            physics::predict_tank(
                &mut predicted,
                &world.content,
                &world.config,
                &world.walls,
                world.tick + 1,
                DT,
            );
            world.step();
            if tick == 0 {
                // Client prediction only moves the visitor. The host supplies
                // forced returns separately in authoritative snapshots.
                world.tanks[0].pocket_echoes.clear();
            }
            let actual = &world.tanks[0];
            assert!(
                actual.pos.distance(predicted.pos) < 0.02,
                "tick {tick}: {:?} != {:?}",
                actual.pos,
                predicted.pos
            );
            assert_eq!(actual.pocket, predicted.pocket);
        }
    }

    #[test]
    fn return_clears_walls_and_lethal_hooks_cannot_be_healed_back_to_life() {
        let (mut world, _) = world(Mode::Sandbox);
        enter(&mut world);
        world.tanks[0].input = PlayerInput::default();
        world.tanks[0].pocket = DT;
        let pos = world.tanks[0].pos;
        world.walls = vec![Rect {
            x: pos.x - 70.,
            y: pos.y - 70.,
            w: 140.,
            h: 140.,
        }];
        world.step();
        assert!(world.position_clear(world.tanks[0].pos, world.tanks[0].radius));
        world.tanks[0].pocket = 5.;
        world.tanks[0].health = 0.;
        world.step();
        assert!(!world.tanks[0].alive);
        assert_eq!(world.tanks[0].pocket, 0.);
        assert!(world.tanks[0].pocket_echoes.is_empty());
    }

    #[test]
    fn doubles_wake_then_catch_a_stationary_visitor_without_firing() {
        for seed in [1, 19, 203, 775] {
            let (mut world, _) = world(Mode::Sandbox);
            world.config.seed = seed;
            enter(&mut world);
            let initial: Vec<V2> = world.tanks[0].pocket_echoes.iter().map(|e| e.pos).collect();
            assert_eq!(initial.len(), POCKET_ECHO_LIMIT);
            for echo in &world.tanks[0].pocket_echoes {
                assert!(echo.pos.distance(world.tanks[0].pos) >= 340.);
            }
            let health = world.tanks[0].health;
            for _ in 0..60 {
                world.step();
            }
            assert!(
                world.tanks[0]
                    .pocket_echoes
                    .iter()
                    .zip(&initial)
                    .all(|(e, p)| e.pos == *p)
            );
            let mut steps = 60;
            while world.tanks[0].pocket > 0. && steps < 600 {
                world.step();
                steps += 1;
            }
            let tank = &world.tanks[0];
            assert!(
                steps < 600,
                "seed {seed}: a stationary visitor escaped every double"
            );
            assert_eq!(tank.pocket, 0.);
            assert!(tank.pocket_echoes.is_empty());
            let earned = tank.max_health * POCKET_HEAL_RATE * steps as f32 * DT;
            assert!((tank.health - health - earned).abs() < 0.01);
            assert!((tank.ability_cooldown - (40. - steps as f32 * DT)).abs() < 0.01);
            assert_eq!(tank.shot_count, 0);
            assert!(world.projectiles.is_empty());
            assert!(world.position_clear(tank.pos, tank.radius));
            assert_eq!(
                world
                    .events
                    .iter()
                    .filter(|e| matches!(e.kind, crate::EventKind::PocketEject))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn a_swept_contact_with_a_sleeping_double_returns_without_damage() {
        let (mut world, _) = world(Mode::Sandbox);
        enter(&mut world);
        let tank = &mut world.tanks[0];
        let layout = world.pocket_plan.as_deref().unwrap();
        tank.pos = layout.lights[0].pos;
        tank.pocket_echoes.truncate(1);
        let echo = &mut tank.pocket_echoes[0];
        echo.pos = tank.pos;
        echo.previous = echo.pos;
        echo.wake = 1.;
        let before = tank.pos - V2::new(90., 0.);
        tank.pos += V2::new(90., 0.);
        let health = tank.health;
        let cooldown = tank.ability_cooldown;
        assert!(pursue(tank, before, &world.content, layout, DT));
        assert_eq!(tank.pocket, 0.);
        assert!(tank.pocket_echoes.is_empty());
        assert_eq!(tank.health, health);
        assert_eq!(tank.ability_cooldown, cooldown);
        assert!(!tank.input.fire);
    }

    #[test]
    fn moving_away_can_outpace_a_double_and_natural_return_clears_it() {
        let (mut world, _) = world(Mode::Sandbox);
        world.tanks[0].pos = V2::new(200., 200.);
        enter(&mut world);
        world.tanks[0].pocket_echoes.truncate(1);
        let tank = &mut world.tanks[0];
        tank.pocket_echoes[0].pos = tank.pos - V2::new(120., 0.);
        tank.pocket_echoes[0].previous = tank.pocket_echoes[0].pos;
        tank.pocket_echoes[0].wake = 0.;
        tank.input = PlayerInput {
            movement: V2::new(1., 0.),
            ..Default::default()
        };
        for tick in 0..599 {
            world.step();
            assert!(world.tanks[0].pocket > 0., "caught at {tick}");
        }
        world.step();
        assert_eq!(world.tanks[0].pocket, 0.);
        assert!(world.tanks[0].pocket_echoes.is_empty());
        assert!(
            !world
                .events
                .iter()
                .any(|e| matches!(e.kind, crate::EventKind::PocketEject))
        );
    }

    #[test]
    fn echoes_use_living_player_classes_and_a_new_visit_starts_with_fresh_poses() {
        let (mut world, _) = world(Mode::Sandbox);
        let other = world.add_player("Other", false);
        world.set_class(other, "twin");
        enter(&mut world);
        assert_eq!(world.tanks[0].pocket_echoes[0].class, "bacteria");
        assert_eq!(world.tanks[0].pocket_echoes[1].class, "twin");
        world.tanks[0].pocket = DT;
        world.step();
        assert!(world.tanks[0].pocket_echoes.is_empty());
        world.tanks[0].ability_cooldown = 0.;
        enter(&mut world);
        assert!(
            world.tanks[0]
                .pocket_echoes
                .iter()
                .all(|e| e.wake > 1. && e.vel == V2::ZERO && e.pos == e.previous)
        );
    }
}
