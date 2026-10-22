use crate::{DT, EntityId, V2, World};

pub(crate) const MAX_FLIGHT_SECONDS: f32 = 2.2;
pub(crate) const MAX_FLIGHT_DISTANCE: f32 = 850.0;
const ACQUISITION_RANGE: f32 = 600.0;
const TURN_RADIANS_PER_SECOND: f32 = 1.1;
// A 120-degree forward cone. Shots that pass a target keep travelling away.
const CONE_COSINE: f32 = 0.5;

pub(crate) fn flight_lifetime(lifetime: f32, speed: f32) -> f32 {
    lifetime
        .min(MAX_FLIGHT_SECONDS)
        .min(MAX_FLIGHT_DISTANCE / speed.max(1.0))
}

impl World {
    pub(crate) fn steer_homing(&mut self, index: usize) {
        let shot = &self.projectiles[index];
        let forward = shot.vel.normalized();
        let pos = shot.pos;
        let in_cone = |goal: V2| {
            let offset = goal - pos;
            let distance = offset.length_squared();
            let ahead = offset.dot(forward);
            distance < ACQUISITION_RANGE * ACQUISITION_RANGE
                && ahead > 0.0
                && ahead * ahead >= distance * CONE_COSINE * CONE_COSINE
        };
        let goal = self
            .homing_position(shot.homing_target, shot.team, shot.owner)
            .filter(|&goal| in_cone(goal))
            .or_else(|| self.nearest_homing_position(pos, shot.team, shot.owner, in_cone));
        if let Some(goal) = goal {
            let heading = shot.vel.angle();
            let difference = ((goal - pos).angle() - heading + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let turn =
                difference.clamp(-TURN_RADIANS_PER_SECOND * DT, TURN_RADIANS_PER_SECOND * DT);
            // Turning cannot slow a round into a hovering, repeatedly circling threat.
            self.projectiles[index].vel = V2::from_angle(heading + turn) * shot.cruise_speed;
        }
    }

    fn nearest_homing_position(
        &self,
        pos: V2,
        team: u8,
        owner: EntityId,
        in_cone: impl Fn(V2) -> bool,
    ) -> Option<V2> {
        nearest(
            self.tanks
                .iter()
                .filter(|tank| {
                    tank.alive
                        && tank.health > 0.0
                        && tank.id != owner
                        && tank.protection <= 0.0
                        && !self.config.base_protected(tank.team, tank.pos, tank.radius)
                        && self.hostile(team, tank.team)
                })
                .map(|tank| tank.pos)
                .chain(
                    self.bosses
                        .iter()
                        .filter(|boss| boss.health > 0.0 && self.hostile(team, boss.team))
                        .map(|boss| boss.pos),
                ),
            pos,
            &in_cone,
        )
        .or_else(|| {
            nearest(
                self.targets
                    .iter()
                    .filter(|target| target.health > 0.0)
                    .map(|target| target.pos),
                pos,
                in_cone,
            )
        })
    }
}

fn nearest(
    positions: impl Iterator<Item = V2>,
    pos: V2,
    in_cone: impl Fn(V2) -> bool,
) -> Option<V2> {
    positions.filter(|&goal| in_cone(goal)).min_by(|a, b| {
        (*a - pos)
            .length_squared()
            .total_cmp(&(*b - pos).length_squared())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldConfig;
    use tankyard_content::{Content, ProjectileKind};

    fn arena(target: V2, speed: f32, lifetime: f32) -> World {
        let mut world = World::new(
            WorldConfig {
                width: 8000.0,
                height: 8000.0,
                target_count: 0,
                spawn_protection: 0.0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let shooter = world.add_player("Shooter", false);
        world.add_player("Target", false);
        world.tanks[0].pos = V2::new(1400.0, 1500.0);
        world.tanks[1].pos = target;
        let shot = world.simple_projectile(
            shooter,
            0,
            V2::new(1500.0, 1500.0),
            V2::new(speed, 0.0),
            ProjectileKind::Homing,
            15.0,
            6.0,
            lifetime,
            1.0,
        );
        world.projectiles.push(shot);
        world
    }

    #[test]
    fn homing_turns_gradually_at_full_speed_and_cannot_lock_behind_it() {
        let mut world = arena(V2::new(1800.0, 1700.0), 380.0, 3.0);
        world.steer_homing(0);
        let velocity = world.projectiles[0].vel;
        assert!(
            velocity.angle() > 0.0 && velocity.angle() <= TURN_RADIANS_PER_SECOND * DT + 0.00001
        );
        assert!((velocity.length() - 380.0).abs() < 0.001);
        world.tanks[1].pos = V2::new(1450.0, 1500.0);
        world.projectiles[0].vel = V2::new(380.0, 0.0);
        world.projectiles[0].homing_target = world.tanks[1].id;
        for _ in 0..120 {
            world.steer_homing(0);
        }
        assert_eq!(world.projectiles[0].vel, V2::new(380.0, 0.0));
        let ahead = world.add_player("Ahead", false);
        world.tanks[2].pos = V2::new(1750.0, 1600.0);
        world.steer_homing(0);
        assert!(
            world.projectiles[0].vel.y > 0.0,
            "a closer tank behind must not hide a valid forward target {ahead}"
        );
    }

    #[test]
    fn aiming_near_a_target_still_hits_but_a_late_sidestep_can_dodge() {
        let mut hit = arena(V2::new(1860.0, 1540.0), 280.0, 3.0);
        let health = hit.tanks[1].health;
        for _ in 0..140 {
            hit.step();
        }
        assert!(
            hit.tanks[1].health < health,
            "guidance must remain useful for an aimed shot"
        );
        let mut dodge = arena(V2::new(1860.0, 1500.0), 280.0, 3.0);
        let health = dodge.tanks[1].health;
        for tick in 0..140 {
            if tick >= 39 {
                dodge.tanks[1].pos.y += 230.0 * DT;
            }
            dodge.step();
        }
        assert_eq!(
            dodge.tanks[1].health, health,
            "a close sidestep should beat the turn limit"
        );
        assert!(dodge.projectiles.is_empty());
    }

    #[test]
    fn every_homing_barrel_has_finite_flight_even_with_speed_upgrades() {
        let content = Content::builtin();
        let mut barrels = 0;
        for class in &content.classes {
            for barrel in class
                .barrels
                .iter()
                .filter(|barrel| barrel.projectile == ProjectileKind::Homing)
            {
                barrels += 1;
                assert!(barrel.lifetime <= MAX_FLIGHT_SECONDS);
                for speed in [
                    barrel.speed,
                    barrel.speed * crate::stat_scale(7) * class.multipliers[3],
                ] {
                    let mut world = arena(V2::new(7000.0, 7000.0), speed, barrel.lifetime);
                    let shot = &world.projectiles[0];
                    assert!(shot.lifetime * speed <= MAX_FLIGHT_DISTANCE + 0.001);
                    for _ in 0..135 {
                        world.step();
                    }
                    assert!(
                        world.projectiles.is_empty(),
                        "{} rounds failed to expire",
                        class.id
                    );
                }
            }
        }
        assert_eq!(barrels, 18);
        let short = arena(V2::new(7000.0, 7000.0), 280.0, 0.4);
        assert_eq!(short.projectiles[0].lifetime, 0.4);
    }

    #[test]
    fn aim_outside_the_forward_cone_or_range_gets_no_free_correction() {
        for target in [V2::new(1510.0, 1800.0), V2::new(2200.0, 1600.0)] {
            let mut world = arena(target, 380.0, 3.0);
            world.steer_homing(0);
            assert_eq!(world.projectiles[0].vel, V2::new(380.0, 0.0));
        }
    }

    #[test]
    fn fast_rounds_cannot_hit_past_their_travel_budget_on_the_last_tick() {
        for (distance, should_hit) in [(842.0, true), (882.0, false)] {
            let mut world = arena(V2::new(1500.0 + distance, 1500.0), 1800.0, 30.0);
            let health = world.tanks[1].health;
            for _ in 0..135 {
                world.step();
            }
            assert_eq!(world.tanks[1].health < health, should_hit);
            assert!(world.projectiles.is_empty());
        }
    }
}
