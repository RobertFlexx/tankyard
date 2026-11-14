//! A private office maze without arena combat or objectives. Its generated
//! walls keep movement predictable; the host alone applies the healing.
use crate::{Rect, Tank, WorldConfig};

pub const POCKET_SECONDS: f32 = 10.;
/// Forty percent of maximum hull health over a complete visit.
pub const POCKET_HEAL_RATE: f32 = 0.04;

/// Every pocket uses the same deterministic, open-ended office maze. Its
/// geometry is derived on each side, so it requires no map download or packets.
pub fn walls(config: &WorldConfig) -> Vec<Rect> {
    let bounds = config.bounds();
    let cell = 400.;
    let columns = (bounds.w / cell).ceil().min(64.) as i32;
    let rows = (bounds.h / cell).ceil().min(64.) as i32;
    let mut walls = Vec::with_capacity((columns * rows * 2) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let x = bounds.x + column as f32 * cell;
            let y = bounds.y + row as f32 * cell;
            let mut key = config.seed ^ (column as u64 * 0x517cc1b7) ^ (row as u64 * 0x6eed0e9d);
            key = (key ^ (key >> 17)).wrapping_mul(0x9e3779b97f4a7c15);
            // Wide gaps join every room. Alternate the ends rather than making
            // corridors with a wall at every crossing or sealed rooms.
            if column + 1 < columns {
                walls.push(Rect {
                    x: x + 376.,
                    y: y + if key & 1 == 0 { 0. } else { 150. },
                    w: 48.,
                    h: 250.,
                });
            }
            if row > 0 && key & 6 != 0 {
                walls.push(Rect {
                    x: x + if key & 8 == 0 { 0. } else { 150. },
                    y: y - 24.,
                    w: 250.,
                    h: 48.,
                });
            }
        }
    }
    // A skirting wall marks the actual boundary instead of an invisible edge.
    walls.extend([
        Rect {
            x: bounds.x - 48.,
            y: bounds.y - 48.,
            w: bounds.w + 96.,
            h: 48.,
        },
        Rect {
            x: bounds.x - 48.,
            y: bounds.y + bounds.h,
            w: bounds.w + 96.,
            h: 48.,
        },
        Rect {
            x: bounds.x - 48.,
            y: bounds.y,
            w: 48.,
            h: bounds.h,
        },
        Rect {
            x: bounds.x + bounds.w,
            y: bounds.y,
            w: 48.,
            h: bounds.h,
        },
    ]);
    walls
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
    }
}
