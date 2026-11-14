//! Shared geometry for team homes, spawning, protection and rendering, plus
//! the guardian drones that keep raiders out of each base.
use crate::{DT, EntityId, Mode, Rect, V2, World, WorldConfig};
use tankyard_content::ProjectileKind;

/// Guardian drones belong to their base, not to a tank. Arenas allocate entity
/// IDs upward from small numbers, so this range never names a real owner.
pub const BASE_GUARD_OWNER: EntityId = u64::MAX - 8;
/// Raiders whose hull comes this close to a base wake its guardians.
pub const BASE_ALERT_DISTANCE: f32 = 170.;
const GUARD_RADIUS: f32 = 17.;
const GUARD_SPEED: f32 = 540.;
const GUARD_DAMAGE: f32 = 42.;
const GUARD_DURABILITY: f32 = 320.;
/// Long enough to be permanent; refreshed every tick while the base exists.
const GUARD_LIFETIME: f32 = 600.;
const GUARD_REPLACEMENT_SECONDS: f32 = 2.5;

pub fn base_guard_owner(team: u8) -> EntityId {
    BASE_GUARD_OWNER + u64::from(team.min(4))
}
pub fn base_guard_team(owner: EntityId) -> Option<u8> {
    (owner > BASE_GUARD_OWNER && owner <= BASE_GUARD_OWNER + 4)
        .then(|| (owner - BASE_GUARD_OWNER) as u8)
}
/// Objectives live inside the bases in these modes, so their bases protect
/// defenders but keep no guardians; raids must stay possible.
fn guard_count(mode: Mode) -> usize {
    match mode {
        Mode::CaptureCore | Mode::Mothership => 0,
        Mode::FourTeams => 4,
        _ => 6,
    }
}

impl WorldConfig {
    /// Team arenas have full-height side bases or four corner bases. Infection
    /// and cooperative modes deliberately have no permanent safe territory.
    pub fn team_base(&self, team: u8) -> Option<Rect> {
        if !matches!(
            self.mode,
            Mode::TwoTeams
                | Mode::FourTeams
                | Mode::Domination
                | Mode::Tag
                | Mode::Mothership
                | Mode::CaptureCore
                | Mode::Deathmatch
                | Mode::KingOfTheHill
        ) || team == 0
            || team > self.mode.teams()
        {
            return None;
        }
        let bounds = self.bounds();
        let (width, height) = if self.mode == Mode::FourTeams {
            (
                bounds.w * 0.18_f32.max(180. / self.width).min(0.3),
                bounds.h * 0.18_f32.max(180. / self.height).min(0.3),
            )
        } else {
            (
                bounds.w * 0.12_f32.max(180. / self.width).min(0.25),
                bounds.h,
            )
        };
        Some(Rect {
            x: bounds.x
                + if team == 2 || team == 3 {
                    bounds.w - width
                } else {
                    0.
                },
            y: bounds.y
                + if self.mode == Mode::FourTeams && (team == 2 || team == 4) {
                    bounds.h - height
                } else {
                    0.
                },
            w: width,
            h: height,
        })
    }

    /// A tank whose center is inside its own base takes no damage. Leaving the
    /// base removes protection immediately; this is independent of stationary
    /// spawn immunity. The radius is validated for callers' consistency.
    pub fn base_protected(&self, team: u8, position: V2, radius: f32) -> bool {
        radius.is_finite()
            && radius >= 0.
            && position.finite()
            && self
                .team_base(team)
                .is_some_and(|base| base.contains(position))
    }

    /// Whether a team's base keeps guardian drones in this mode.
    pub fn base_guarded(&self, team: u8) -> bool {
        guard_count(self.mode) > 0 && self.team_base(team).is_some()
    }

    /// How close a raider's hull may come before guardians engage. Small
    /// arenas use a proportionally smaller zone so central objectives stay clear.
    pub fn base_alert_distance(&self) -> f32 {
        let bounds = self.bounds();
        (bounds.w.min(bounds.h) * 0.065).min(BASE_ALERT_DISTANCE)
    }

    /// Whether a hull is close enough to a guarded base to be attacked.
    pub fn raiding_base(&self, base_team: u8, position: V2, radius: f32) -> bool {
        self.base_guarded(base_team)
            && self.team_base(base_team).is_some_and(|base| {
                position.finite()
                    && base.distance(position) <= self.base_alert_distance() + radius.max(0.)
            })
    }
}

impl World {
    /// Keep every base garrisoned. Losses are replaced one at a time, and
    /// survivors slowly repair; guardians never expire on their own.
    pub(crate) fn update_base_guards(&mut self) {
        let teams = self.config.mode.teams().min(4);
        for team in 1..=teams {
            let Some(base) = self.config.team_base(team) else {
                continue;
            };
            let owner = base_guard_owner(team);
            let mut alive = 0;
            for guard in self.projectiles.iter_mut().filter(|p| p.owner == owner) {
                if guard.lifetime > 0. && guard.penetration > 0. {
                    alive += 1;
                    guard.lifetime = GUARD_LIFETIME;
                    guard.penetration =
                        (guard.penetration + GUARD_DURABILITY * 0.03 * DT).min(GUARD_DURABILITY);
                }
            }
            let slot = usize::from(team - 1);
            self.base_guard_timers[slot] = (self.base_guard_timers[slot] - DT).max(0.);
            let opening = self.match_state.elapsed < 1.;
            if alive >= guard_count(self.config.mode)
                || (!opening && self.base_guard_timers[slot] > 0.)
                || self.projectiles.len() >= self.config.max_projectiles
            {
                continue;
            }
            self.base_guard_timers[slot] = GUARD_REPLACEMENT_SECONDS;
            let position = guard_slot(
                base,
                self.config.bounds(),
                alive,
                guard_count(self.config.mode),
                self.match_state.elapsed,
            );
            let mut guard = self.simple_projectile(
                owner,
                team,
                position,
                V2::ZERO,
                ProjectileKind::Drone,
                GUARD_DAMAGE * self.config.rules.damage_multiplier,
                GUARD_RADIUS,
                GUARD_LIFETIME,
                GUARD_DURABILITY,
            );
            guard.cruise_speed = GUARD_SPEED;
            self.projectiles.push(guard);
        }
    }

    /// Chase the raider nearest to this guardian, otherwise hold a patrol slot.
    pub(crate) fn base_guard_goal(&self, team: u8, id: EntityId, pos: V2) -> Option<V2> {
        let base = self.config.team_base(team)?;
        let mut nearest: Option<(f32, V2)> = None;
        for tank in &self.tanks {
            if !tank.tangible()
                || tank.health <= 0.
                || tank.team == team
                || !self.hostile(team, tank.team)
                || !self.config.raiding_base(team, tank.pos, tank.radius)
                // Contesting a point beside a base is not a raid.
                || self
                    .objectives
                    .iter()
                    .any(|o| o.pos.distance(tank.pos) < o.radius + tank.radius)
            {
                continue;
            }
            let distance = (tank.pos - pos).length_squared();
            if nearest.is_none_or(|(best, _)| distance < best) {
                // Lead the target slightly so a fleeing raider is cut off.
                nearest = Some((distance, tank.pos + tank.vel * 0.12));
            }
        }
        if let Some((_, target)) = nearest {
            return Some(target);
        }
        let owner = base_guard_owner(team);
        let mut rank = 0;
        let mut count = 0;
        for guard in self.projectiles.iter().filter(|p| p.owner == owner) {
            count += 1;
            if guard.id < id {
                rank += 1;
            }
        }
        Some(guard_slot(
            base,
            self.config.bounds(),
            rank,
            count.max(1),
            self.match_state.elapsed,
        ))
    }
}

/// Patrol positions: a line along the inner edge of a side base, or a slow
/// ring inside a corner base.
fn guard_slot(base: Rect, bounds: Rect, rank: usize, count: usize, time: f32) -> V2 {
    let fraction = (rank as f32 + 0.5) / count.max(1) as f32;
    let point = if base.h > base.w * 1.6 {
        let inner = if base.center().x < bounds.center().x {
            0.72
        } else {
            0.28
        };
        V2::new(
            base.x + base.w * inner + (time * 0.9 + rank as f32).sin() * base.w * 0.06,
            base.y + base.h * fraction,
        )
    } else {
        let angle = fraction * std::f32::consts::TAU + time * 0.35;
        base.center() + V2::from_angle(angle) * (base.w.min(base.h) * 0.3)
    };
    base.clamp_circle(point, GUARD_RADIUS + 4.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bases_follow_animated_bounds_and_protect_tanks_centered_inside() {
        for mode in [Mode::TwoTeams, Mode::FourTeams] {
            for scale in [0.5, 0.73, 1.] {
                let config = WorldConfig {
                    mode,
                    arena_scale: scale,
                    ..Default::default()
                };
                for team in 1..=mode.teams() {
                    let base = config.team_base(team).unwrap();
                    assert!(config.bounds().contains(base.center()));
                    assert!(config.base_protected(team, base.center(), 30.));
                    // Being in the base means the tank's center is inside it.
                    assert!(config.base_protected(
                        team,
                        V2::new(base.x + 1., base.center().y),
                        30.
                    ));
                    assert!(!config.base_protected(
                        team,
                        V2::new(base.x - 1., base.center().y),
                        30.
                    ));
                    assert!(!config.base_protected(team, V2::new(f32::NAN, 0.), 30.));
                    assert!(!config.base_protected(0, base.center(), 30.));
                    for other in 1..=mode.teams() {
                        if other != team {
                            assert!(!config.base_protected(other, base.center(), 30.));
                        }
                    }
                }
            }
        }
        for mode in [Mode::Ffa, Mode::Infection, Mode::CoopPve, Mode::BossRush] {
            assert!(
                WorldConfig {
                    mode,
                    ..Default::default()
                }
                .team_base(1)
                .is_none()
            );
        }
    }

    #[test]
    fn team_spawn_and_protection_stay_in_clear_bases_on_every_map_and_size() {
        use crate::{V2, World};
        for mode in [Mode::TwoTeams, Mode::FourTeams, Mode::CaptureCore] {
            for map in tankyard_content::builtin_maps() {
                for (width, height) in [(900., 700.), (2400., 1800.), (6000., 4000.)] {
                    for scale in [0.5, 1.] {
                        let mut world = World::new(
                            WorldConfig {
                                mode,
                                width,
                                height,
                                target_count: 0,
                                dynamic_arena: false,
                                spawn_protection: 0.,
                                map: map.id.clone(),
                                ..Default::default()
                            },
                            tankyard_content::Content::builtin(),
                        );
                        world.config.arena_scale = scale;
                        world.scale_map_walls();
                        for team in 1..=mode.teams() {
                            let id = world.add_player("Defender", false);
                            world.assign_team(id, team);
                            let index = world.tanks.iter().position(|tank| tank.id == id).unwrap();
                            let tank = &world.tanks[index];
                            assert!(
                                world.config.base_protected(team, tank.pos, tank.radius),
                                "{} {mode:?} {width} {scale} {team}",
                                map.id
                            );
                            assert!(world.walls.iter().all(|wall| {
                                crate::physics::circle_rect(tank.pos, tank.radius, *wall).is_none()
                            }));
                            let health = tank.health;
                            world.damage_tank(index, 9999., id + 1, V2::new(100., 0.));
                            assert_eq!(world.tanks[index].health, health);
                            assert_eq!(world.tanks[index].vel, V2::ZERO);
                            world.tanks[index].pos = world.config.bounds().center();
                            world.damage_tank(index, 25., id + 1, V2::new(100., 0.));
                            assert_eq!(world.tanks[index].health, health - 25.);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn live_projectiles_cannot_hurt_a_base_defender_but_do_after_they_leave() {
        use crate::{PlayerInput, V2, World};
        let mut world = World::new(
            WorldConfig {
                mode: Mode::TwoTeams,
                target_count: 0,
                dynamic_arena: false,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let defender = world.add_player("Defender", false);
        let attacker = world.add_player("Attacker", false);
        world.assign_team(defender, 1);
        world.assign_team(attacker, 2);
        world.tanks[0].pos = world.team_home(1);
        world.tanks[1].pos = world.tanks[0].pos + V2::new(450., 0.);
        let health = world.tanks[0].health;
        world.set_input(
            attacker,
            PlayerInput {
                aim: std::f32::consts::PI,
                fire: true,
                ..Default::default()
            },
        );
        for _ in 0..240 {
            world.step();
        }
        assert_eq!(world.tanks[0].health, health);
        let base = world.config.team_base(1).unwrap();
        world.tanks[0].pos.x = base.x + base.w + world.tanks[0].radius + 2.;
        for _ in 0..150 {
            world.step();
        }
        assert!(world.tanks[0].health < health);
    }

    fn guarded_world(mode: Mode) -> World {
        World::new(
            WorldConfig {
                mode,
                target_count: 0,
                dynamic_arena: false,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        )
    }

    fn guards(world: &World, team: u8) -> usize {
        world
            .projectiles
            .iter()
            .filter(|p| p.owner == base_guard_owner(team) && p.lifetime > 0.)
            .count()
    }

    #[test]
    fn every_team_base_is_garrisoned_and_ffa_has_no_guardians() {
        for (mode, expected) in [
            (Mode::TwoTeams, 6),
            (Mode::FourTeams, 4),
            (Mode::Domination, 6),
            (Mode::CaptureCore, 0),
            (Mode::Mothership, 0),
        ] {
            let mut world = guarded_world(mode);
            for _ in 0..30 {
                world.step();
            }
            for team in 1..=mode.teams() {
                assert_eq!(guards(&world, team), expected, "{mode:?} team {team}");
                let base = world.config.team_base(team).unwrap();
                for guard in world
                    .projectiles
                    .iter()
                    .filter(|p| p.owner == base_guard_owner(team))
                {
                    assert_eq!(guard.team, team);
                    assert!(base.distance(guard.pos) < 1.);
                }
            }
        }
        for mode in [Mode::Ffa, Mode::Infection, Mode::CoopPve] {
            let mut world = guarded_world(mode);
            world.add_player("Pilot", false);
            for _ in 0..30 {
                world.step();
            }
            assert!(
                world
                    .projectiles
                    .iter()
                    .all(|p| base_guard_team(p.owner).is_none())
            );
        }
        assert_eq!(base_guard_team(base_guard_owner(3)), Some(3));
        assert_eq!(base_guard_team(42), None);
        assert_eq!(base_guard_team(BASE_GUARD_OWNER), None);
    }

    #[test]
    fn guardians_attack_raiders_but_ignore_allies_and_distant_enemies() {
        let mut world = guarded_world(Mode::TwoTeams);
        let ally = world.add_player("Ally", false);
        let raider = world.add_player("Raider", false);
        let distant = world.add_player("Distant", false);
        world.assign_team(ally, 1);
        world.assign_team(raider, 2);
        world.assign_team(distant, 2);
        for _ in 0..30 {
            world.step();
        }
        let base = world.config.team_base(1).unwrap();
        let index = |world: &World, id| world.tanks.iter().position(|t| t.id == id).unwrap();
        let (a, r, d) = (
            index(&world, ally),
            index(&world, raider),
            index(&world, distant),
        );
        world.tanks[a].pos = V2::new(base.x + base.w + 120., base.center().y);
        world.tanks[r].pos = V2::new(base.x + base.w + 60., base.center().y + 300.);
        world.tanks[d].pos = world.config.bounds().center();
        let health = [
            world.tanks[a].health,
            world.tanks[r].health,
            world.tanks[d].health,
        ];
        for _ in 0..240 {
            world.step();
            // Hold position: guardians must come to the raider.
            for i in [a, r, d] {
                world.tanks[i].vel = V2::ZERO;
            }
        }
        assert_eq!(world.tanks[a].health, health[0], "allies are never hit");
        assert_eq!(
            world.tanks[d].health, health[2],
            "distant enemies are left alone"
        );
        let raider_hit = world.tanks[r].health < health[1] || !world.tanks[r].alive;
        assert!(raider_hit, "a raider beside the base is attacked");
    }

    #[test]
    fn destroyed_guardians_are_replaced_and_cannot_farm_shapes() {
        let mut world = guarded_world(Mode::TwoTeams);
        for _ in 0..30 {
            world.step();
        }
        let base = world.config.team_base(1).unwrap();
        let shape = world.id();
        world.targets.push(crate::Target {
            id: shape,
            pos: base.center(),
            vel: V2::ZERO,
            radius: 20.,
            health: 50.,
            max_health: 50.,
            kind: crate::TargetKind::Square,
            angle: 0.,
            xp: 10,
        });
        world.projectiles.retain(|p| p.owner != base_guard_owner(1));
        assert_eq!(guards(&world, 1), 0);
        for _ in 0..(60 * 16) {
            world.step();
            if let Some(target) = world.targets.iter_mut().find(|t| t.id == shape) {
                target.pos = base.center();
            }
        }
        assert_eq!(guards(&world, 1), 6);
        assert!(
            world
                .targets
                .iter()
                .any(|t| t.id == shape && t.health == 50.)
        );
    }
}
