use crate::*;
use tankyard_content::{Ability, ProjectileKind};

pub const STASIS_SECONDS: f32 = 3.;
pub const STASIS_RADIUS: f32 = 330.;
const MAX_CAPTURED: usize = 96;

fn freezable(kind: ProjectileKind) -> bool {
    matches!(
        kind,
        ProjectileKind::Bullet
            | ProjectileKind::Shell
            | ProjectileKind::Piercing
            | ProjectileKind::Scatter
            | ProjectileKind::Homing
            | ProjectileKind::Explosive
            | ProjectileKind::Fragment
    )
}

impl World {
    pub(crate) fn update_stasis(&mut self) {
        let active: Vec<_> = self
            .tanks
            .iter()
            .filter(|t| {
                t.alive
                    && t.health > 0.
                    && t.stasis > 0.
                    && self
                        .content
                        .class(&t.class)
                        .is_some_and(|c| c.ability == Ability::Stasis)
            })
            .map(|t| (t.id, t.team, t.pos))
            .collect();
        let mut captured: Vec<usize> = active
            .iter()
            .map(|(id, _, _)| {
                self.projectiles
                    .iter()
                    .filter(|p| p.frozen.is_some_and(|f| f.anchor == *id))
                    .count()
            })
            .collect();
        let mut releases = Vec::new();
        for shot in &mut self.projectiles {
            if shot.lifetime <= 0. || shot.penetration <= 0. {
                continue;
            }
            if let Some(frozen) = shot.frozen {
                if let Some((_, _, center)) = active.iter().find(|(id, _, _)| *id == frozen.anchor)
                {
                    let next = *center + frozen.offset;
                    // A field carries rounds with its hull but cannot carry them
                    // through cover or beyond the arena.
                    if !self.config.bounds().contains_circle(next, shot.radius)
                        || self.walls.iter().any(|&wall| {
                            physics::sweep_rect(shot.pos, next, shot.radius, wall).is_some()
                        })
                    {
                        shot.lifetime = 0.;
                    } else {
                        shot.previous = shot.pos;
                        shot.pos = next;
                    }
                    continue;
                }
                let anchor = self.tanks.iter().find(|t| t.id == frozen.anchor);
                if let Some(tank) = anchor.filter(|t| t.alive && t.health > 0.) {
                    let direction = if frozen.offset.length_squared() > 0.01 {
                        frozen.offset.normalized()
                    } else {
                        V2::from_angle(shot.id as f32 * 2.399_963)
                    };
                    shot.owner = tank.id;
                    shot.team = tank.team;
                    shot.kind = ProjectileKind::Bullet;
                    shot.homing_target = 0;
                    shot.cruise_speed = frozen.velocity.length().clamp(380., 720.);
                    shot.vel = direction * shot.cruise_speed;
                    shot.damage = shot.damage.clamp(8., 55.);
                    shot.penetration = shot.penetration.min(3.);
                    shot.hit.clear();
                    releases.push((tank.id, tank.pos, tank.team));
                } else {
                    // Killing or disconnecting the caster cancels conversion.
                    shot.vel = frozen.velocity;
                }
                shot.previous = shot.pos;
                shot.frozen = None;
                continue;
            }
            if shot.stasis_used || !freezable(shot.kind) {
                continue;
            }
            for (index, &(id, team, center)) in active.iter().enumerate() {
                if captured[index] >= MAX_CAPTURED
                    || (shot.owner != id
                        && team > 0
                        && team == shot.team
                        && !self.config.friendly_fire)
                    || shot.pos.distance(center) > STASIS_RADIUS
                    || self
                        .walls
                        .iter()
                        .any(|&w| physics::sweep_rect(center, shot.pos, 2., w).is_some())
                {
                    continue;
                }
                shot.frozen = Some(FrozenShot {
                    anchor: id,
                    offset: shot.pos - center,
                    velocity: shot.vel,
                });
                shot.stasis_used = true;
                shot.vel = V2::ZERO;
                shot.previous = shot.pos;
                captured[index] += 1;
                break;
            }
        }
        releases.sort_unstable_by_key(|r| r.0);
        releases.dedup_by_key(|r| r.0);
        for (id, pos, team) in releases {
            self.event(EventKind::StasisRelease, pos, id, 0, team);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (World, EntityId, EntityId) {
        let mut w = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                spawn_protection: 0.,
                dynamic_arena: false,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let caster = w.add_player("Clock", false);
        let enemy = w.add_player("Enemy", false);
        w.sandbox_class(caster, "chronarch");
        w.tanks[0].pos = V2::new(1000., 1000.);
        w.tanks[1].pos = V2::new(1800., 1000.);
        w.tanks[0].stasis = STASIS_SECONDS;
        let p = w.simple_projectile(
            enemy,
            0,
            V2::new(1200., 1000.),
            V2::new(-500., 0.),
            ProjectileKind::Bullet,
            30.,
            6.,
            1.8,
            2.,
        );
        w.projectiles.push(p);
        (w, caster, enemy)
    }
    #[test]
    fn rounds_follow_the_caster_without_aging_then_release_once_in_a_radial_wave() {
        let (mut w, caster, _) = setup();
        let id = w.projectiles[0].id;
        w.step();
        let life = w.projectiles[0].lifetime;
        w.tanks[0].pos.y += 100.;
        w.step();
        assert_eq!(w.projectiles[0].pos, V2::new(1200., 1100.));
        assert_eq!(w.projectiles[0].lifetime, life);
        for _ in 0..181 {
            w.step();
        }
        let p = w.projectiles.iter().find(|p| p.id == id).unwrap();
        assert_eq!(p.owner, caster);
        assert!(p.frozen.is_none() && p.stasis_used && p.vel.x > 0.);
        w.tanks[0].stasis = STASIS_SECONDS;
        w.update_stasis();
        assert!(
            w.projectiles[0].frozen.is_none(),
            "a round can only be captured once"
        );
    }
    #[test]
    fn caster_death_and_removal_restore_the_original_round_without_stealing_credit() {
        for remove in [false, true] {
            let (mut w, caster, enemy) = setup();
            w.update_stasis();
            if remove {
                w.remove_player(caster);
            } else {
                w.tanks[0].alive = false;
            }
            w.update_stasis();
            assert!(w.projectiles[0].frozen.is_none());
            assert_eq!(w.projectiles[0].owner, enemy);
            assert_eq!(w.projectiles[0].vel, V2::new(-500., 0.));
        }
    }
    #[test]
    fn field_cannot_stop_drones_traps_allied_rounds_or_rounds_through_cover() {
        let (mut w, _, _) = setup();
        for kind in [
            ProjectileKind::Drone,
            ProjectileKind::Trap,
            ProjectileKind::Mine,
        ] {
            w.projectiles[0].kind = kind;
            w.update_stasis();
            assert!(w.projectiles[0].frozen.is_none());
        }
        w.projectiles[0].kind = ProjectileKind::Bullet;
        w.walls.push(Rect {
            x: 1090.,
            y: 900.,
            w: 20.,
            h: 200.,
        });
        w.update_stasis();
        assert!(w.projectiles[0].frozen.is_none());
        w.walls.clear();
        w.tanks[0].team = 1;
        w.projectiles[0].team = 1;
        w.update_stasis();
        assert!(w.projectiles[0].frozen.is_none());
    }
}
