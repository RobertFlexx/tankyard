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
        let visible = |goal| in_cone(goal) && self.can_see(pos, goal, 2.);
        let observed = self
            .homing_position(shot.homing_target, shot.team, shot.owner)
            .filter(|&goal| visible(goal))
            .map(|goal| (shot.homing_target, goal))
            .or_else(|| self.nearest_homing_position(pos, shot.team, shot.owner, visible));
        let sighting = observed.or_else(|| {
            shot.last_seen.filter(|&(id, at)| {
                in_cone(at)
                    && self
                        .guidance_target(id, shot.team, shot.owner)
                        .is_some_and(|now| !self.can_see(pos, now, 2.))
            })
        });
        if let Some((_, goal)) = sighting {
            let heading = shot.vel.angle();
            let difference = ((goal - pos).angle() - heading + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let turn =
                difference.clamp(-TURN_RADIANS_PER_SECOND * DT, TURN_RADIANS_PER_SECOND * DT);
            // Turning cannot slow a round into a hovering, repeatedly circling threat.
            self.projectiles[index].vel = V2::from_angle(heading + turn) * shot.cruise_speed;
        }
        self.projectiles[index].last_seen = sighting;
    }

    pub(crate) fn guidance_target(&self, id: EntityId, team: u8, owner: EntityId) -> Option<V2> {
        self.homing_position(id, team, owner).or_else(|| {
            self.targets
                .iter()
                .find(|t| t.id == id && t.health > 0.)
                .map(|t| t.pos)
        })
    }

    fn nearest_homing_position(
        &self,
        pos: V2,
        team: u8,
        owner: EntityId,
        in_cone: impl Fn(V2) -> bool,
    ) -> Option<(EntityId, V2)> {
        nearest(
            self.tanks
                .iter()
                .filter(|tank| tank.id != owner && self.can_hunt(team, tank))
                .map(|tank| (tank.id, tank.pos))
                .chain(
                    self.bosses
                        .iter()
                        .filter(|boss| boss.health > 0.0 && self.hostile(team, boss.team))
                        .map(|boss| (boss.id, boss.pos)),
                ),
            pos,
            &in_cone,
        )
        .or_else(|| {
            nearest(
                self.targets
                    .iter()
                    .filter(|target| target.health > 0.0)
                    .map(|target| (target.id, target.pos)),
                pos,
                in_cone,
            )
        })
    }
}

fn nearest(
    positions: impl Iterator<Item = (EntityId, V2)>,
    pos: V2,
    in_cone: impl Fn(V2) -> bool,
) -> Option<(EntityId, V2)> {
    positions.filter(|&(_, goal)| in_cone(goal)).min_by(|a, b| {
        (a.1 - pos)
            .length_squared()
            .total_cmp(&(b.1 - pos).length_squared())
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
    fn guidance_keeps_the_observed_point_when_a_tank_moves_behind_cover() {
        let at = V2::new(1800., 1540.);
        let mut world = arena(at, 400., 2.);
        world.steer_homing(0);
        assert_eq!(
            world.projectiles[0].last_seen,
            Some((world.tanks[1].id, at))
        );
        world.walls.push(crate::Rect {
            x: 1680.,
            y: 1515.,
            w: 50.,
            h: 80.,
        });
        world.tanks[1].pos = V2::new(1840., 1580.);
        world.steer_homing(0);
        assert_eq!(
            world.projectiles[0].last_seen,
            Some((world.tanks[1].id, at))
        );
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
