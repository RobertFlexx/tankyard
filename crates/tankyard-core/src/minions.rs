//! Little tanks, real bullets, and no sneaky client-side damage bullshit.
use crate::physics::sweep_rect;
use crate::{DT, EventKind, MinionGun, Projectile, V2, World, stat_scale};
use tankyard_content::ProjectileKind;

fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

impl World {
    pub(crate) fn minion_target(&self, pos: V2, team: u8, owner: u64, range: f32) -> Option<V2> {
        self.minion_target_with_id(pos, team, owner, range, 0)
            .map(|(_, pos)| pos)
    }

    pub(crate) fn minion_target_with_id(
        &self,
        pos: V2,
        team: u8,
        owner: u64,
        range: f32,
        preferred: u64,
    ) -> Option<(u64, V2)> {
        let pick = |candidates: &mut dyn Iterator<Item = (u64, V2)>| {
            candidates
                .filter_map(|(id, target)| {
                    let distance = (target - pos).length_squared();
                    if distance > range * range
                        || self
                            .walls
                            .iter()
                            .any(|&wall| sweep_rect(pos, target, 0.5, wall).is_some())
                    {
                        return None;
                    }
                    // A small distance advantage must not flip the squad's aim
                    // every tick as two opponents cross each other's path.
                    Some((
                        distance * if id == preferred { 0.72 } else { 1. },
                        id,
                        target,
                    ))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, id, target)| (id, target))
        };
        let tanks = self
            .tanks
            .iter()
            .filter(|tank| {
                tank.alive
                    && tank.id != owner
                    && tank.protection <= 0.
                    && self.hostile(team, tank.team)
            })
            .map(|tank| (tank.id, tank.pos));
        let bosses = self
            .bosses
            .iter()
            .filter(|boss| boss.health > 0. && self.hostile(team, boss.team))
            .map(|boss| (boss.id, boss.pos));
        if let Some(enemy) = pick(&mut tanks.chain(bosses)) {
            return Some(enemy);
        }
        pick(
            &mut self
                .targets
                .iter()
                .filter(|target| target.health > 0.)
                .map(|target| (target.id, target.pos)),
        )
    }

    pub(crate) fn fire_minion(&mut self, index: usize, queued: usize) -> Option<Projectile> {
        let p = &self.projectiles[index];
        if p.lifetime <= 0.0 || p.penetration <= 0.0 {
            return None;
        }
        let tank = self
            .tanks
            .iter()
            .find(|tank| tank.id == p.owner && tank.alive)?;
        let class = self.content.class(&tank.class)?;
        if !class
            .barrels
            .iter()
            .any(|barrel| barrel.projectile == ProjectileKind::Minion)
        {
            return None;
        }
        let speed = 420.0 * stat_scale(tank.stats[3]) * class.multipliers[3];
        let range = (speed * 1.25).min(650.);
        let (goal, locked) = if tank.input.alt_fire {
            (None, 0)
        } else if tank.input.fire {
            let cursor = tank.pos + V2::from_angle(tank.aim) * tank.input.aim_distance.min(1600.0);
            // If the pointer is sitting on this minion, fire along the owner's
            // aim instead of spinning the gun at a zero-length vector.
            (
                Some(if cursor.distance(p.pos) < p.radius * 1.5 {
                    let aim = p.gun.map_or(tank.aim, |gun| gun.aim);
                    p.pos + V2::from_angle(aim) * 200.0
                } else {
                    cursor
                }),
                0,
            )
        } else {
            let target = self.minion_target_with_id(p.pos, p.team, p.owner, range, p.homing_target);
            (target.map(|(_, pos)| pos), target.map_or(0, |(id, _)| id))
        };
        let reload = stat_scale(tank.stats[6]) * class.multipliers[6];
        let damage = 6.0
            * stat_scale(tank.stats[5])
            * class.multipliers[5]
            * self.config.rules.damage_multiplier;
        let penetration = 1.5 * stat_scale(tank.stats[4]) * class.multipliers[4];
        let full = self.projectiles.len() + queued >= self.config.max_projectiles;
        let p = &mut self.projectiles[index];
        p.homing_target = locked;
        let gun = p.gun.get_or_insert(MinionGun {
            aim: p.vel.angle(),
            cooldown: 0.35,
            shots: 0,
        });
        let previous = gun.cooldown;
        gun.cooldown = (previous - DT).max(0.0);
        let goal = goal?;
        if p.pos.distance(goal) > range + p.radius * MinionGun::BARREL_LENGTH {
            return None;
        }
        let desired = (goal - p.pos).angle();
        gun.aim = wrap(gun.aim + wrap(desired - gun.aim) * (1.0 - (-12.0 * DT).exp()));
        if gun.cooldown > 0.0 || p.age < 0.3 || wrap(desired - gun.aim).abs() > 0.25 || full {
            return None;
        }
        let direction = V2::from_angle(gun.aim);
        let origin = p.pos + direction * (p.radius * MinionGun::BARREL_LENGTH);
        let radius = (p.radius * 0.34).clamp(2.5, 5.0);
        // Muzzle clipping through solid walls is bullshit. No magic bullets.
        if !self.config.bounds().contains_circle(origin, radius)
            || self.walls.iter().any(|&wall| {
                sweep_rect(p.pos, origin, radius, wall).is_some()
                    || sweep_rect(p.pos, goal, 0.5, wall).is_some()
            })
        {
            return None;
        }
        gun.cooldown = (0.7 / reload
            + if previous > 0.0 {
                (previous - DT).min(0.0)
            } else {
                0.0
            })
        .max(0.0);
        gun.shots = gun.shots.wrapping_add(1);
        p.vel -= direction * 2.0;
        let (owner, team, id) = (p.owner, p.team, p.id);
        let shot = self.simple_projectile(
            owner,
            team,
            origin,
            direction * speed,
            ProjectileKind::Bullet,
            damage,
            radius,
            1.25,
            penetration,
        );
        self.event(
            EventKind::Shot,
            origin,
            id,
            ProjectileKind::Bullet as u32,
            team,
        );
        Some(shot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mode, PlayerInput, Rect, Target, TargetKind, WorldConfig};
    use tankyard_content::Content;

    fn factory() -> (World, u64) {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                spawn_protection: 0.0,
                ..WorldConfig::default()
            },
            Content::builtin(),
        );
        let owner = world.add_player("Factory", false);
        assert!(world.sandbox_class(owner, "foundry"));
        world.tanks[0].pos = V2::new(500.0, 500.0);
        (world, owner)
    }

    fn minion(world: &mut World, owner: u64) {
        let mut p = world.simple_projectile(
            owner,
            0,
            V2::new(600.0, 500.0),
            V2::new(175.0, 0.0),
            ProjectileKind::Minion,
            12.0,
            11.0,
            60.0,
            9.0,
        );
        p.age = 1.0;
        p.gun = Some(MinionGun {
            aim: 0.0,
            cooldown: 0.0,
            shots: 0,
        });
        world.projectiles.push(p);
    }

    #[test]
    fn factory_minion_shoots_a_real_owner_attributed_bullet() {
        let (mut world, owner) = factory();
        minion(&mut world, owner);
        world.tanks[0].input = PlayerInput {
            fire: true,
            aim: 0.0,
            aim_distance: 360.0,
            ..PlayerInput::default()
        };
        let shot = world.fire_minion(0, 0).expect("minion should fire");
        assert_eq!(shot.kind, ProjectileKind::Bullet);
        assert_eq!(shot.owner, owner);
        assert!(shot.vel.x > 300.0);
        assert!(shot.vel.y.abs() < 0.001);
        assert!(shot.pos.x > world.projectiles[0].pos.x);
        assert_eq!(world.projectiles[0].gun.unwrap().shots, 1);
        assert!(world.fire_minion(0, 0).is_none(), "reload actually matters");
    }

    #[test]
    fn idle_factory_minions_pick_an_unblocked_target() {
        let (mut world, owner) = factory();
        minion(&mut world, owner);
        world.walls.clear();
        world.walls.push(Rect {
            x: 680.0,
            y: 470.0,
            w: 12.0,
            h: 65.0,
        });
        for (id, pos) in [(1000, V2::new(740.0, 500.0)), (1001, V2::new(780.0, 620.0))] {
            world.targets.push(Target {
                id,
                pos,
                vel: V2::ZERO,
                radius: 14.0,
                health: 100.0,
                max_health: 100.0,
                kind: TargetKind::Square,
                angle: 0.0,
                xp: 10,
            });
        }
        assert_eq!(
            world.minion_target(V2::new(600.0, 500.0), 0, owner, 600.0),
            Some(V2::new(780.0, 620.0))
        );
        let shot = world.fire_minion(0, 0);
        // The gun will turn toward the target first, not instant-snap through a wall.
        assert!(shot.is_none());
        for _ in 0..50 {
            if world.fire_minion(0, 0).is_some() {
                return;
            }
        }
        panic!("minion never aimed at the visible shape");
    }

    #[test]
    fn recall_disables_minion_guns_without_deleting_the_squad() {
        let (mut world, owner) = factory();
        minion(&mut world, owner);
        world.tanks[0].input = PlayerInput {
            fire: true,
            alt_fire: true,
            ..PlayerInput::default()
        };
        assert!(world.fire_minion(0, 0).is_none());
        assert_eq!(world.projectiles.len(), 1);
        world.tanks[0].input.alt_fire = false;
        assert!(world.fire_minion(0, 0).is_some());
    }

    #[test]
    fn minion_spawner_has_a_real_six_unit_limit_and_refills_after_losses() {
        let (mut world, owner) = factory();
        world.tanks[0].input.fire = false;
        for _ in 0..600 {
            world.step();
            assert!(
                world
                    .projectiles
                    .iter()
                    .filter(|p| p.owner == owner && p.kind.is_drone())
                    .count()
                    <= 6,
                "no seventh damn drone"
            );
        }
        assert_eq!(world.tanks[0].deployed_drones, 6);
        assert_eq!(
            world
                .projectiles
                .iter()
                .filter(|p| p.kind == ProjectileKind::Minion)
                .count(),
            6
        );
        world
            .projectiles
            .iter_mut()
            .find(|p| p.kind == ProjectileKind::Minion)
            .unwrap()
            .lifetime = 0.0;
        world.tanks[0].cooldowns.fill(0.0);
        world.step();
        assert_eq!(world.tanks[0].deployed_drones, 6);
    }
    #[test]
    fn mixed_drone_and_minion_barrels_share_the_same_owner_cap() {
        let (mut world, owner) = factory();
        let ordinary_drone = world.content.class("hive").unwrap().barrels[0].clone();
        world
            .content
            .classes
            .iter_mut()
            .find(|class| class.id == "foundry")
            .unwrap()
            .barrels
            .push(ordinary_drone);
        for n in 0..5 {
            let p = world.simple_projectile(
                owner,
                0,
                V2::new(900.0 + n as f32 * 40.0, 600.0),
                V2::ZERO,
                if n % 2 == 0 {
                    ProjectileKind::Minion
                } else {
                    ProjectileKind::Drone
                },
                12.0,
                11.0,
                60.0,
                9.0,
            );
            world.projectiles.push(p);
        }
        world.tanks[0].input.fire = true;
        world.fire_weapons();
        assert_eq!(
            world
                .projectiles
                .iter()
                .filter(|p| p.owner == owner && p.kind.is_drone())
                .count(),
            6,
            "the damn cap belongs to the owner, not the individual barrel"
        );
    }
}
