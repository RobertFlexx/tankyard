use crate::ui::{AMBER, CYAN, RED, alpha};
use macroquad::prelude::*;
use std::collections::{HashMap, HashSet};
use tankyard_content::ProjectileKind;
use tankyard_core::{EntityId, EventKind, GameEvent, Projectile, Tank, V2, sanitize_name};

pub struct Particle {
    pub pos: V2,
    pub vel: V2,
    pub life: f32,
    pub total: f32,
    pub size: f32,
    pub color: Color,
    pub ring: bool,
    pub spin: f32,
}
#[derive(Clone, Copy)]
struct LastProjectile {
    pos: V2,
    radius: f32,
    team: u8,
    owner: EntityId,
    seen_at: u64,
    kind: ProjectileKind,
    angle: f32,
    shots: u32,
    recoil: f32,
}

pub struct ProjectileFade {
    pub pos: V2,
    pub radius: f32,
    pub team: u8,
    pub owner: EntityId,
    pub life: f32,
    pub total: f32,
    pub kind: ProjectileKind,
    pub angle: f32,
}

#[derive(Default)]
pub struct BarrelVisuals {
    class: String,
    shots: Vec<u32>,
    pub recoil: Vec<f32>,
    recoil_velocity: Vec<f32>,
    pub aim: Vec<f32>,
}

pub struct KillNotice {
    pub name: String,
    pub life: f32,
}
pub struct Popup {
    pub pos: V2,
    pub text: String,
    pub life: f32,
    pub color: Color,
}
pub struct HullVisual {
    pub radius: f32,
    pub health: f32,
}
pub struct Effects {
    round: Option<u32>,
    pub hulls: HashMap<EntityId, HullVisual>,
    pub particles: Vec<Particle>,
    pub projectile_fades: Vec<ProjectileFade>,
    tracked_projectiles: HashMap<EntityId, LastProjectile>,
    projectile_tick: Option<u64>,
    pub popups: Vec<Popup>,
    pub kills: Vec<KillNotice>,
    pub flashes: HashMap<EntityId, f32>,
    pub recoil: HashMap<EntityId, f32>,
    pub display_aim: HashMap<EntityId, f32>,
    pub barrels: HashMap<EntityId, BarrelVisuals>,
    active_tanks: HashSet<EntityId>,
    seen: HashSet<(u64, u8, EntityId, u32, EntityId)>,
    pub shake: f32,
    pub notice: String,
    pub notice_life: f32,
    pub boss_arrival: Option<(tankyard_core::BossKind, f32)>,
    pub arena_scale: Option<f32>,
    last_tick: u64,
    random: u64,
}
impl Effects {
    pub fn new() -> Self {
        Self {
            round: None,
            hulls: HashMap::new(),
            particles: Vec::with_capacity(2048),
            projectile_fades: Vec::with_capacity(256),
            tracked_projectiles: HashMap::with_capacity(1024),
            projectile_tick: None,
            popups: Vec::with_capacity(32),
            kills: Vec::with_capacity(3),
            flashes: HashMap::new(),
            recoil: HashMap::new(),
            display_aim: HashMap::new(),
            barrels: HashMap::new(),
            active_tanks: HashSet::new(),
            seen: HashSet::new(),
            shake: 0.0,
            notice: String::new(),
            notice_life: 0.0,
            boss_arrival: None,
            arena_scale: None,
            last_tick: 0,
            random: 7919,
        }
    }
    pub fn sync_round(&mut self, round: u32) {
        if self.round.is_some_and(|previous| previous != round) {
            *self = Self::new();
        }
        self.round = Some(round);
    }
    pub fn sync_arena(&mut self, scale: f32, dt: f32) {
        let shown = self.arena_scale.get_or_insert(scale);
        *shown += (scale - *shown) * (1. - (-dt * 16.).exp());
    }
    fn rand(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        ((self.random >> 32) as u32) as f32 / u32::MAX as f32
    }
    pub fn ingest(
        &mut self,
        events: &[GameEvent],
        local: EntityId,
        intensity: f32,
    ) -> Vec<GameEvent> {
        let mut sounds = Vec::new();
        for event in events {
            let key = (
                event.tick,
                event.kind as u8,
                event.subject,
                event.value,
                event.elimination.as_ref().map_or(0, |kill| kill.victim),
            );
            if !self.seen.insert(key) {
                continue;
            }
            sounds.push(event.clone());
            if matches!(event.kind, EventKind::BossSpawn)
                && let Some(&kind) = tankyard_core::BossKind::ALL.get(event.value as usize)
            {
                self.boss_arrival = Some((kind, 5.));
            }
            let (color, count, ring) = match event.kind {
                EventKind::Shot => {
                    self.recoil.insert(event.subject, 0.10);
                    (CYAN, 0, false)
                }
                EventKind::Impact => {
                    self.flashes.insert(event.subject, 0.12);
                    (AMBER, 2, false)
                }
                EventKind::Destroy => (RED, 6, false),
                EventKind::Kill => (RED, 0, false),
                EventKind::LevelUp => (CYAN, 0, false),
                EventKind::Evolve => (AMBER, 4, false),
                EventKind::Ability => (CYAN, 6, true),
                EventKind::Capture => (AMBER, 6, true),
                EventKind::BossSpawn => (RED, 10, true),
                EventKind::Victory => (AMBER, 10, true),
                EventKind::Parry => (CYAN, 5, true),
            };
            let count = (count as f32 * intensity) as usize;
            for _ in 0..count {
                if self.particles.len() >= 2048 {
                    break;
                }
                let angle = self.rand() * std::f32::consts::TAU;
                let speed = 40.0 + self.rand() * 150.0;
                let life = 0.25 + self.rand() * 0.5;
                let size = 2.0 + self.rand() * 4.0;
                self.particles.push(Particle {
                    pos: event.pos,
                    vel: V2::from_angle(angle) * speed,
                    life,
                    total: life,
                    size,
                    color,
                    ring: false,
                    spin: angle,
                });
            }
            if ring && self.particles.len() < 2048 {
                self.particles.push(Particle {
                    pos: event.pos,
                    vel: V2::ZERO,
                    life: 0.55,
                    total: 0.55,
                    size: 20.0,
                    color,
                    ring: true,
                    spin: 0.0,
                });
            }
            if matches!(event.kind, EventKind::Kill) && event.subject == local {
                self.shake = (self.shake + 1.0).min(4.0);
                if let Some(elimination) = &event.elimination {
                    // The name already went through the server's player-name sanitizer.
                    // Never let a backlog of reliable events grow the HUD without bound.
                    if self.kills.len() == 3 {
                        self.kills.remove(0);
                    }
                    self.kills.push(KillNotice {
                        name: sanitize_name(&elimination.name),
                        life: 3.5,
                    });
                }
            }
            if event.subject == local {
                let text = match event.kind {
                    EventKind::LevelUp => Some(format!("LEVEL {}", event.value)),
                    EventKind::Evolve => Some("Tank upgraded".into()),
                    EventKind::Kill => None, // The name belongs in the top-screen feed.
                    EventKind::Parry => Some("Parried".into()),
                    _ => None,
                };
                if let Some(text) = text {
                    if matches!(event.kind, EventKind::Kill | EventKind::Parry) {
                        if self.popups.len() < 32 {
                            self.popups.push(Popup {
                                pos: event.pos,
                                text,
                                life: 1.5,
                                color,
                            });
                        }
                    } else {
                        self.notice = text;
                        self.notice_life = 2.5;
                    }
                }
            }
            self.last_tick = self.last_tick.max(event.tick);
        }
        self.seen
            .retain(|(tick, _, _, _, _)| tick.saturating_add(180) >= self.last_tick);
        sounds
    }
    /// Track rendered projectile positions each frame. Only a fresh simulation
    /// or network tick can remove a projectile and start its fade.
    /// When a bullet vanishes (wall, tank, shape, expiry), render its last
    /// confirmed position as a fading ghost. No additional network messages.
    pub fn sync_projectiles(
        &mut self,
        tick: u64,
        projectiles: &[Projectile],
        fraction: f32,
        dt: f32,
        reduced_motion: bool,
    ) {
        let turn = if reduced_motion {
            1.0
        } else {
            1.0 - (-dt.clamp(0.0, 0.1) * 20.0).exp()
        };
        for projectile in projectiles {
            let angle = match projectile.kind {
                ProjectileKind::Minion => {
                    let target = projectile
                        .gun
                        .map_or_else(|| projectile.vel.angle(), |gun| gun.aim);
                    self.tracked_projectiles
                        .get(&projectile.id)
                        .map_or(target, |prior| {
                            crate::session::lerp_angle(prior.angle, target, turn)
                        })
                }
                ProjectileKind::Drone => {
                    let target = projectile.vel.angle();
                    let previous = self
                        .tracked_projectiles
                        .get(&projectile.id)
                        .map_or(target, |prior| prior.angle);
                    // Contact impulses can leave tiny velocities in a resting
                    // swarm. Keep the last heading instead of flickering.
                    if projectile.vel.length_squared() < 64.0 {
                        previous
                    } else {
                        crate::session::lerp_angle(previous, target, turn)
                    }
                }
                ProjectileKind::Trap | ProjectileKind::Mine => projectile.age * 0.1,
                _ => 0.0,
            };
            let shots = projectile.gun.map_or(0, |gun| gun.shots);
            let recoil = if reduced_motion {
                0.0
            } else {
                self.tracked_projectiles
                    .get(&projectile.id)
                    .map_or(0.0, |prior| {
                        if shots != prior.shots {
                            projectile.radius * 0.16
                        } else {
                            prior.recoil * (-dt.clamp(0.0, 0.1) * 24.0).exp()
                        }
                    })
            };
            self.tracked_projectiles.insert(
                projectile.id,
                LastProjectile {
                    pos: projectile
                        .previous
                        .lerp(projectile.pos, fraction.clamp(0.0, 1.0)),
                    radius: projectile.radius,
                    team: projectile.team,
                    owner: projectile.owner,
                    seen_at: tick,
                    kind: projectile.kind,
                    angle,
                    shots,
                    recoil,
                },
            );
        }
        if self.projectile_tick == Some(tick) {
            return;
        }
        self.projectile_tick = Some(tick);
        let fades = &mut self.projectile_fades;
        self.tracked_projectiles.retain(|_, prior| {
            if prior.seen_at == tick {
                return true;
            }
            // Avoid explosive allocation spikes when a server resets a match.
            if fades.len() < 1024 {
                fades.push(ProjectileFade {
                    pos: prior.pos,
                    radius: prior.radius,
                    team: prior.team,
                    owner: prior.owner,
                    life: 0.16,
                    total: 0.16,
                    kind: prior.kind,
                    angle: prior.angle,
                });
            }
            false
        });
    }

    pub fn drone_angle(&self, projectile: &Projectile) -> f32 {
        self.tracked_projectiles
            .get(&projectile.id)
            .map_or_else(|| projectile.vel.angle(), |visual| visual.angle)
    }

    pub fn minion_recoil(&self, id: EntityId) -> f32 {
        self.tracked_projectiles
            .get(&id)
            .map_or(0.0, |visual| visual.recoil)
    }

    pub fn sync_barrels(&mut self, tanks: &[Tank], dt: f32, reduced_motion: bool) {
        self.active_tanks.clear();
        self.active_tanks
            .extend(tanks.iter().filter(|tank| tank.alive).map(|tank| tank.id));
        self.barrels.retain(|id, _| self.active_tanks.contains(id));
        self.hulls.retain(|id, _| self.active_tanks.contains(id));
        self.display_aim
            .retain(|id, _| self.active_tanks.contains(id));
        let easing = if reduced_motion {
            1.0
        } else {
            1.0 - (-dt.clamp(0.0, 0.1) * 22.0).exp()
        };
        for tank in tanks.iter().filter(|tank| tank.alive) {
            let health = (tank.health / tank.max_health).clamp(0.0, 1.0);
            let hull = self.hulls.entry(tank.id).or_insert(HullVisual {
                radius: tank.radius,
                health,
            });
            hull.radius += (tank.radius - hull.radius) * easing;
            hull.health += (health - hull.health) * easing;
            let visual = self.barrels.entry(tank.id).or_default();
            if visual.class != tank.class {
                visual.class.clone_from(&tank.class);
                visual.shots.fill(0);
                visual.recoil.fill(0.0);
                visual.recoil_velocity.fill(0.0);
                visual.aim.clone_from(&tank.barrel_aims);
            }
            visual.shots.resize(tank.barrel_shots.len(), 0);
            visual.recoil.resize(tank.barrel_shots.len(), 0.0);
            visual.recoil_velocity.resize(tank.barrel_shots.len(), 0.0);
            let previous_len = visual.aim.len();
            visual.aim.resize(tank.barrel_aims.len(), 0.0);
            for (index, (angle, target)) in visual.aim.iter_mut().zip(&tank.barrel_aims).enumerate()
            {
                *angle = if index >= previous_len {
                    *target
                } else {
                    crate::session::lerp_angle(*angle, *target, easing)
                };
            }
            for (((previous, recoil), velocity), current) in visual
                .shots
                .iter_mut()
                .zip(&mut visual.recoil)
                .zip(&mut visual.recoil_velocity)
                .zip(&tank.barrel_shots)
            {
                if current != previous {
                    if current.wrapping_sub(*previous) < 0x8000_0000 && !reduced_motion {
                        *recoil = recoil.max(0.035);
                        *velocity = 14.;
                    } else {
                        *recoil = 0.;
                        *velocity = 0.;
                    }
                }
                if reduced_motion {
                    *recoil = 0.;
                    *velocity = 0.;
                }
                // Resets and wrapping counters must both advance the baseline.
                *previous = *current;
            }
        }
    }
    pub fn update(&mut self, dt: f32) {
        if let Some((_, life)) = &mut self.boss_arrival {
            *life -= dt;
            if *life <= 0. {
                self.boss_arrival = None;
            }
        }
        for p in &mut self.projectile_fades {
            p.life -= dt;
        }
        self.projectile_fades.retain(|p| p.life > 0.0);
        for p in &mut self.particles {
            p.life -= dt;
            p.pos += p.vel * dt;
            p.vel *= (-3.0 * dt).exp();
            p.spin += dt * 3.0;
        }
        self.particles.retain(|p| p.life > 0.0);
        for p in &mut self.popups {
            p.life -= dt;
            p.pos.y -= dt * 30.0;
        }
        self.popups.retain(|p| p.life > 0.0);
        for kill in &mut self.kills {
            kill.life -= dt;
        }
        self.kills.retain(|kill| kill.life > 0.0);
        self.flashes.retain(|_, v| {
            *v -= dt;
            *v > 0.0
        });
        self.recoil.retain(|_, v| {
            *v *= (-18.0 * dt).exp();
            *v > 0.004
        });
        for visual in self.barrels.values_mut() {
            for (recoil, velocity) in visual.recoil.iter_mut().zip(&mut visual.recoil_velocity) {
                advance_recoil(recoil, velocity, dt);
            }
        }
        self.shake *= (-8.0 * dt).exp();
        self.notice_life = (self.notice_life - dt).max(0.0);
    }
    pub fn notice(&mut self, text: &str) {
        self.notice = text.into();
        self.notice_life = 3.0;
    }
    pub fn particle_color(p: &Particle) -> Color {
        alpha(p.color, (p.life / p.total).clamp(0.0, 1.0))
    }
}

fn advance_recoil(position: &mut f32, velocity: &mut f32, dt: f32) {
    // Exact critically damped spring: a short kick followed by a smooth
    // return, with the same response at different render frame rates.
    let dt = dt.clamp(0., 1.);
    let rate = 36.;
    let decay = (-rate * dt).exp();
    let term = *velocity + rate * *position;
    *position = (*position + term * dt) * decay;
    *velocity = (*velocity - rate * term * dt) * decay;
    if *position > 0.18 {
        *position = 0.18;
        *velocity = velocity.min(0.);
    }
    if *position < 0.001 && velocity.abs() < 0.05 {
        *position = 0.;
        *velocity = 0.;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recoil_kicks_then_returns_consistently_and_respects_reduced_motion() {
        let mut effects = Effects::new();
        let mut tank = tank();
        effects.sync_barrels(std::slice::from_ref(&tank), 1. / 60., false);
        tank.barrel_shots[0] += 1;
        effects.sync_barrels(std::slice::from_ref(&tank), 1. / 60., false);
        let kick = effects.barrels[&tank.id].recoil[0];
        effects.update(1. / 60.);
        assert!(effects.barrels[&tank.id].recoil[0] > kick);
        let mut slow = (0.035, 14.);
        let mut fast = slow;
        for _ in 0..6 {
            advance_recoil(&mut slow.0, &mut slow.1, 1. / 30.);
        }
        for _ in 0..48 {
            advance_recoil(&mut fast.0, &mut fast.1, 1. / 240.);
        }
        assert!((slow.0 - fast.0).abs() < 0.001);
        effects.sync_barrels(std::slice::from_ref(&tank), 1. / 60., true);
        assert_eq!(effects.barrels[&tank.id].recoil[0], 0.);
        effects.update(1.);
        assert_eq!(effects.barrels[&tank.id].recoil[0], 0.);
    }

    #[test]
    fn hull_health_and_growth_ease_then_reset_on_death_or_a_new_round() {
        let mut effects = Effects::new();
        effects.sync_round(1);
        let mut tank = tank();
        effects.sync_barrels(std::slice::from_ref(&tank), 1. / 60., false);
        let original = effects.hulls[&tank.id].radius;
        tank.radius *= 1.5;
        tank.health = tank.max_health * 0.4;
        effects.sync_barrels(std::slice::from_ref(&tank), 1. / 60., false);
        let hull = &effects.hulls[&tank.id];
        assert!(hull.radius > original && hull.radius < tank.radius);
        assert!(hull.health > 0.4 && hull.health < 1.);
        effects.sync_barrels(std::slice::from_ref(&tank), 1. / 60., true);
        assert_eq!(effects.hulls[&tank.id].radius, tank.radius);
        assert_eq!(effects.hulls[&tank.id].health, 0.4);
        effects.sync_round(2);
        assert!(effects.hulls.is_empty() && effects.barrels.is_empty());
        effects.sync_barrels(std::slice::from_ref(&tank), 1. / 60., false);
        tank.alive = false;
        effects.sync_barrels(&[tank], 1. / 60., false);
        assert!(effects.hulls.is_empty());
    }

    #[test]
    fn boss_arrival_expires_after_five_seconds_and_reliable_replays_do_not_restart_it() {
        let mut effects = Effects::new();
        let event = GameEvent {
            tick: 120,
            kind: EventKind::BossSpawn,
            pos: V2::ZERO,
            subject: 42,
            value: tankyard_core::BossKind::Guardian as u32,
            team: 0,
            elimination: None,
        };
        assert_eq!(effects.ingest(std::slice::from_ref(&event), 1, 0.).len(), 1);
        effects.update(4.9);
        assert!(effects.boss_arrival.is_some());
        assert!(
            effects
                .ingest(std::slice::from_ref(&event), 1, 0.)
                .is_empty()
        );
        effects.update(0.11);
        assert!(effects.boss_arrival.is_none());
        effects.sync_arena(1., 0.016);
        effects.sync_arena(0.9, 0.016);
        assert!((0.9..1.).contains(&effects.arena_scale.unwrap()));
    }

    #[test]
    fn reliable_events_arriving_late_are_not_lost_or_replayed() {
        let mut effects = Effects::new();
        let newest = GameEvent {
            tick: 40,
            kind: EventKind::Shot,
            pos: V2::ZERO,
            subject: 7,
            value: 0,
            team: 0,
            elimination: None,
        };
        let late = GameEvent {
            tick: 37,
            kind: EventKind::LevelUp,
            pos: V2::ZERO,
            subject: 7,
            value: 15,
            team: 0,
            elimination: None,
        };
        assert_eq!(
            effects.ingest(std::slice::from_ref(&newest), 7, 1.0).len(),
            1
        );
        assert_eq!(effects.ingest(std::slice::from_ref(&late), 7, 1.0).len(), 1);
        assert_eq!(effects.notice, "LEVEL 15");
        assert!(effects.ingest(&[newest, late], 7, 1.0).is_empty());
    }

    #[test]
    fn projectile_fades_decay_and_never_accumulate() {
        let mut effects = Effects::new();
        let projectile = Projectile {
            id: 1,
            owner: 7,
            team: 0,
            pos: V2::new(10.0, 20.0),
            previous: V2::ZERO,
            vel: V2::ZERO,
            cruise_speed: 0.,
            radius: 5.0,
            damage: 10.0,
            penetration: 1.0,
            lifetime: 0.1,
            kind: tankyard_content::ProjectileKind::Bullet,
            ricochets: 0,
            age: 0.3,
            hit: Vec::new(),
            gun: None,
            homing_target: 0,
        };
        effects.sync_projectiles(100, &[projectile], 1.0, 1.0 / 60.0, false);
        effects.sync_projectiles(100, &[], 1.0, 1.0 / 60.0, false); // Same tick: no phantom collision.
        assert!(effects.projectile_fades.is_empty());
        effects.sync_projectiles(101, &[], 1.0, 1.0 / 60.0, false);
        assert_eq!(effects.projectile_fades.len(), 1);
        effects.sync_projectiles(102, &[], 1.0, 1.0 / 60.0, false);
        assert_eq!(effects.projectile_fades.len(), 1);
        effects.update(0.2);
        assert!(effects.projectile_fades.is_empty());
    }

    #[test]
    fn projectile_fades_keep_the_last_rendered_position_and_shape() {
        let mut effects = Effects::new();
        let projectile = Projectile {
            id: 1,
            owner: 7,
            team: 0,
            pos: V2::new(100.0, 80.0),
            previous: V2::new(20.0, 40.0),
            vel: V2::new(0.0, 10.0),
            cruise_speed: 10.,
            radius: 5.0,
            damage: 10.0,
            penetration: 1.0,
            lifetime: 0.1,
            kind: ProjectileKind::Trap,
            ricochets: 0,
            age: 2.0,
            hit: Vec::new(),
            gun: None,
            homing_target: 0,
        };
        effects.sync_projectiles(
            100,
            std::slice::from_ref(&projectile),
            0.1,
            1.0 / 60.0,
            false,
        );
        effects.sync_projectiles(100, &[projectile], 0.75, 1.0 / 60.0, false);
        effects.sync_projectiles(101, &[], 0.0, 1.0 / 60.0, false);
        let fade = &effects.projectile_fades[0];
        assert_eq!(fade.pos, V2::new(80.0, 70.0));
        assert_eq!(fade.kind, ProjectileKind::Trap);
        assert!((fade.angle - 0.2).abs() < 0.0001);
    }

    #[test]
    fn drone_heading_keeps_its_resting_direction_and_turns_consistently_across_frame_rates() {
        let mut world = tankyard_core::World::new(
            tankyard_core::WorldConfig {
                mode: tankyard_core::Mode::Sandbox,
                target_count: 0,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let owner = world.add_player("Pilot", false);
        assert!(world.sandbox_class(owner, "flock"));
        world.tanks[0].input.fire = true;
        world.step();
        // Use a real projectile so this also follows the current wire fields.
        let mut drone = world.projectiles.remove(0);
        assert_eq!(drone.kind, ProjectileKind::Drone);
        drone.vel = V2::new(0.0, 300.0);
        let initial = drone.vel.angle();
        let mut settled = Vec::new();
        for frames in [30, 60, 144] {
            let mut effects = Effects::new();
            let dt = 1.0 / frames as f32;
            effects.sync_projectiles(1, std::slice::from_ref(&drone), 1.0, dt, false);
            drone.vel = V2::new(0.01, -0.01);
            effects.sync_projectiles(2, std::slice::from_ref(&drone), 1.0, dt, false);
            assert!((effects.drone_angle(&drone) - initial).abs() < 0.001);
            drone.vel = V2::new(-300.0, 0.0);
            for _ in 0..frames / 3 {
                effects.sync_projectiles(3, std::slice::from_ref(&drone), 1.0, dt, false);
            }
            settled.push(effects.drone_angle(&drone));
            effects.sync_projectiles(4, std::slice::from_ref(&drone), 1.0, dt, true);
            assert!((effects.drone_angle(&drone) - drone.vel.angle()).abs() < 0.001);
            drone.vel = V2::new(0.0, 300.0);
        }
        assert!(
            settled
                .iter()
                .all(|angle| (angle - settled[0]).abs() < 0.001)
        );
    }

    fn tank() -> Tank {
        let mut world = tankyard_core::World::new(
            tankyard_core::WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        world.add_player("Pilot", false);
        let mut tank = world.tanks.remove(0);
        tank.barrel_shots = vec![0];
        tank.barrel_aims = vec![0.0];
        tank
    }

    #[test]
    fn recoil_survives_counter_resets_and_wraps() {
        let mut effects = Effects::new();
        let mut tank = tank();
        let id = tank.id;
        tank.barrel_shots[0] = 27;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        effects.update(1.0);

        tank.barrel_shots[0] = 0;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        assert_eq!(effects.barrels[&id].recoil[0], 0.0);
        tank.barrel_shots[0] = 1;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        assert!(effects.barrels[&id].recoil[0] > 0.0);

        tank.barrel_shots[0] = u32::MAX;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        effects.update(1.0);
        tank.barrel_shots[0] = 0;
        effects.sync_barrels(&[tank], 0.016, false);
        assert!(effects.barrels[&id].recoil[0] > 0.0);
    }

    #[test]
    fn respawning_clears_old_barrel_and_aim_animation() {
        let mut effects = Effects::new();
        let mut tank = tank();
        let id = tank.id;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        effects.display_aim.insert(id, 2.0);
        tank.alive = false;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        assert!(!effects.barrels.contains_key(&id));
        assert!(!effects.display_aim.contains_key(&id));

        tank.alive = true;
        tank.barrel_aims[0] = -1.0;
        tank.barrel_shots[0] = 1;
        effects.sync_barrels(&[tank], 0.016, false);
        assert_eq!(effects.barrels[&id].aim[0], -1.0);
        assert!(effects.barrels[&id].recoil[0] > 0.0);
    }

    #[test]
    fn cannon_tracking_eases_across_wrap_and_respects_reduced_motion() {
        let mut effects = Effects::new();
        let mut tank = tank();
        let id = tank.id;
        tank.barrel_aims[0] = 3.1;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        tank.barrel_aims[0] = -3.1;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        let eased = effects.barrels[&id].aim[0];
        assert!(eased > 3.1 && eased < 3.184);
        effects.sync_barrels(&[tank], 0.016, true);
        assert!((effects.barrels[&id].aim[0] - 3.1831853).abs() < 0.0001);
    }

    #[test]
    fn particles_remain_bounded_and_expire() {
        let mut effects = Effects::new();
        for tick in 1..=300 {
            effects.ingest(
                &[GameEvent {
                    tick,
                    kind: EventKind::Destroy,
                    pos: V2::ZERO,
                    subject: tick,
                    value: 0,
                    team: 0,
                    elimination: None,
                }],
                0,
                1.0,
            );
        }
        assert!(effects.particles.len() <= 2048);
        effects.update(3.0);
        assert!(effects.particles.is_empty());
    }
}

#[cfg(test)]
mod elimination_regressions {
    use super::*;
    use tankyard_core::Elimination;

    #[test]
    fn distinct_same_tick_kills_survive_retries_and_expire() {
        let mut effects = Effects::new();
        let make = |victim| GameEvent {
            tick: 100,
            kind: EventKind::Kill,
            pos: V2::ZERO,
            subject: 7,
            value: 150,
            team: 0,
            elimination: Some(Elimination {
                victim,
                name: format!("Rival {victim}"),
            }),
        };
        let batch = vec![make(8), make(9)];
        assert_eq!(effects.ingest(&batch, 7, 1.).len(), 2);
        assert_eq!(effects.kills.len(), 2);
        assert_eq!(effects.kills[0].name, "Rival 8");
        assert!(effects.ingest(&batch, 7, 1.).is_empty());
        for victim in 10..=14 {
            effects.ingest(&[make(victim)], 7, 1.);
        }
        assert_eq!(effects.kills.len(), 3);
        assert_eq!(effects.kills[2].name, "Rival 14");
        effects.update(4.);
        assert!(effects.kills.is_empty());
        effects.ingest(
            &[GameEvent {
                tick: 101,
                kind: EventKind::Kill,
                pos: V2::ZERO,
                subject: 7,
                value: 150,
                team: 0,
                elimination: Some(Elimination {
                    victim: 50,
                    name: "A".repeat(200) + "\nBAD",
                }),
            }],
            7,
            1.,
        );
        assert_eq!(effects.kills[0].name.chars().count(), 24);
        effects.sync_round(1);
        effects.sync_round(2);
        assert!(effects.seen.is_empty());
    }
}
