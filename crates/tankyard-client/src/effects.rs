use crate::ui::{AMBER, CYAN, RED, alpha, mix};
use macroquad::prelude::*;
use std::collections::{HashMap, HashSet};
use tankyard_content::{Easing, ProjectileKind};
use tankyard_core::{EntityId, EventKind, GameEvent, Projectile, Tank, V2, sanitize_name};

/// How a map's Zoom view frames the local tank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The zoom: 1 as usual.
    pub scale: f32,
    /// How far the camera turns from the tank toward a spot, from 0 to 1.
    pub focus: f32,
    /// That spot, as fractions of the arena's size, times `focus`; kept
    /// this way so two looks blend along a straight line.
    pub pull: [f32; 2],
}
impl Look {
    pub const USUAL: Self = Self {
        scale: 1.,
        focus: 0.,
        pull: [0., 0.],
    };
    /// Partway from `self` to `to`: `k` 0 is `self` and 1 is `to`, past
    /// either for curves that overshoot. Zooms blend evenly in steps of
    /// scale, so going in feels as quick as coming out.
    fn toward(self, to: Self, k: f32) -> Self {
        let blend = |a: f32, b: f32| a + (b - a) * k;
        Self {
            scale: blend(self.scale.ln(), to.scale.ln()).exp(),
            focus: blend(self.focus, to.focus),
            pull: [
                blend(self.pull[0], to.pull[0]),
                blend(self.pull[1], to.pull[1]),
            ],
        }
    }
    /// Where the camera looks for a tank at `tank` in an arena `size` wide
    /// and high.
    pub fn camera(&self, tank: V2, size: [f32; 2]) -> V2 {
        tank * (1. - self.focus) + V2::new(self.pull[0] * size[0], self.pull[1] * size[1])
    }
}

/// A map's Zoom view on the local tank, easing from the look the view had
/// when it began.
#[derive(Clone, Copy, Debug)]
struct MapView {
    from: Look,
    to: Look,
    /// Seconds since it began.
    age: f32,
    /// How long it lasts, its return included; None until the next view.
    seconds: Option<f32>,
    /// Seconds each change takes.
    ease: f32,
    easing: Easing,
}
impl MapView {
    /// The change's length: a short view spends half its time going in
    /// and half coming back.
    fn ease(&self) -> f32 {
        self.seconds
            .map_or(self.ease, |seconds| self.ease.min(seconds * 0.5))
            .max(0.01)
    }
    fn look(&self) -> Look {
        let ease = self.ease();
        let look = self
            .from
            .toward(self.to, self.easing.apply(self.age / ease));
        match self.seconds {
            Some(seconds) if self.age > seconds - ease => look.toward(
                Look::USUAL,
                self.easing.apply((self.age - (seconds - ease)) / ease),
            ),
            _ => look,
        }
    }
    fn done(&self) -> bool {
        match self.seconds {
            Some(seconds) => self.age >= seconds,
            None => self.to == Look::USUAL && self.age >= self.ease(),
        }
    }
}

pub struct Particle {
    pub pos: V2,
    pub vel: V2,
    pub life: f32,
    pub total: f32,
    pub size: f32,
    pub color: Color,
    pub ring: bool,
    pub spin: f32,
    pub spark: bool,
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
    /// Seconds it had left when last seen. A round gone with time to spare
    /// hit something; one at the end of its life just burned out.
    lifetime: f32,
    vel: V2,
    damage: f32,
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
    /// Drawn larger, for a hit worth reading across the room.
    pub big: bool,
}
/// How long the big words and numbers from a parry clash stay up.
pub const BIG_POPUP_SECONDS: f32 = 1.9;
/// The color of a rift event's end, 0 for an entry and 1 for an exit, as
/// the viewer sees the rifts of the player behind it.
fn rift_color(event: &GameEvent, viewer: (EntityId, u8)) -> Color {
    let end = if event.value == 0 {
        tankyard_core::RiftEnd::Entry
    } else {
        tankyard_core::RiftEnd::Exit
    };
    let foe = crate::rifts::hostile(event.subject, event.team, viewer.0, viewer.1);
    crate::rifts::color(end, foe)
}
pub struct HullVisual {
    pub radius: f32,
    pub health: f32,
}
pub struct Effects {
    round: Option<u32>,
    /// Map sounds still to play, such as a Play sound's repeats.
    pub map_cues: crate::map_sounds::CuePlayer,
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
    /// A map's Shake screen on the local tank: strength from 0 to 1, seconds
    /// left and its full length.
    map_shake: (f32, f32, f32),
    /// A map's Zoom view on the local tank.
    map_view: Option<MapView>,
    pub notice: String,
    pub notice_life: f32,
    pub boss_arrival: Option<(tankyard_core::BossKind, f32)>,
    pub arena_scale: Option<f32>,
    /// Who destroyed the local tank most recently; zero for the arena itself.
    pub killed_by: Option<EntityId>,
    /// Counts the local tank's deaths, so each one can start its own card.
    pub deaths: u32,
    /// "Sentinel was killed by Pilot": the boss, its killer and time left.
    pub boss_defeat: Option<(String, EntityId, f32)>,
    /// Bosses seen in recent snapshots, by ID, so a kill can be recognized
    /// after the boss itself is gone.
    bosses: HashMap<EntityId, (&'static str, tankyard_core::BossKind)>,
    /// Rifts' ages and time left, run on between snapshots.
    pub rift_clocks: crate::rifts::Clocks,
    /// How bosses and their rounds are being drawn from frame to frame.
    pub boss_fx: crate::boss_fx::BossFx,
    last_tick: u64,
    random: u64,
}
impl Effects {
    pub fn new() -> Self {
        Self {
            round: None,
            map_cues: Default::default(),
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
            map_shake: (0., 0., 0.),
            map_view: None,
            notice: String::new(),
            notice_life: 0.0,
            boss_arrival: None,
            arena_scale: None,
            killed_by: None,
            deaths: 0,
            boss_defeat: None,
            bosses: HashMap::new(),
            rift_clocks: Default::default(),
            boss_fx: Default::default(),
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
    /// Remember the bosses in this snapshot.
    pub fn note_bosses(&mut self, bosses: &[tankyard_core::Boss]) {
        if self.bosses.len() > 64 {
            self.bosses
                .retain(|id, _| bosses.iter().any(|b| b.id == *id));
        }
        for boss in bosses {
            self.bosses.insert(
                boss.id,
                (
                    boss.worm_part()
                        .map_or(boss.kind.name(), |part| part.name()),
                    boss.kind,
                ),
            );
        }
    }
    /// Run the rifts in this snapshot on by `dt`.
    pub fn note_rifts(&mut self, rifts: &[tankyard_core::Rift], dt: f32) {
        self.rift_clocks.note(rifts, dt);
    }
    /// Whether the boss banner at the top is showing.
    pub fn boss_banner(&self) -> bool {
        self.boss_arrival.is_some() || self.boss_defeat.is_some()
    }
    /// Takes in a frame's events for the player `local` on `local_team`,
    /// once each, and returns the new ones for sounds.
    pub fn ingest(
        &mut self,
        events: &[GameEvent],
        (local, local_team): (EntityId, u8),
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
                && let Some(kind) = tankyard_core::BossKind::from_index(event.value)
            {
                self.boss_arrival = Some((kind, 5.));
            }
            let (color, count, ring) = match event.kind {
                EventKind::Shot => {
                    self.recoil.insert(event.subject, 0.10);
                    (CYAN, 0, false)
                }
                EventKind::Impact if self.boss_fx.is_worm(event.subject) => {
                    // It bleeds where it's hit, rather than sparking.
                    self.boss_fx.worm_hit(event.pos, event.value as f32);
                    (AMBER, 0, false)
                }
                EventKind::Impact => {
                    self.flashes.insert(event.subject, 0.12);
                    (AMBER, 2, false)
                }
                EventKind::Destroy
                    if event.value == tankyard_core::BOSS_REWARD
                        && self.boss_fx.is_worm(event.subject) =>
                {
                    self.boss_fx.worm_died(event.subject);
                    (RED, 0, false)
                }
                EventKind::Destroy => (RED, 6, false),
                EventKind::Kill => (RED, 0, false),
                EventKind::LevelUp => (CYAN, 0, false),
                EventKind::Evolve => (AMBER, 4, false),
                EventKind::Ability => (CYAN, 6, true),
                EventKind::Capture => (AMBER, 6, true),
                EventKind::BossSpawn => (RED, 10, true),
                EventKind::Victory => (AMBER, 10, true),
                EventKind::Parry => match tankyard_core::clash_of(event.value) {
                    Some((style, bits, damage)) => {
                        let boss = self.boss_fx.clash_boss(style, bits, event.pos);
                        self.boss_fx
                            .clash(style, event.pos, boss, damage, event.subject == local);
                        (crate::boss_fx::clash_color(style), 0, false)
                    }
                    None => (CYAN, 8, true),
                },
                EventKind::BodyParry => (AMBER, 30, true),
                EventKind::StasisRelease => (CYAN, 24, true),
                // The worm's physical breach and blood are drawn by its own
                // effect system; don't add a generic glowing red blast too.
                EventKind::BossAttack if self.boss_fx.is_worm(event.subject) => (RED, 0, false),
                // Axiom's attacks each have a flare of their own.
                EventKind::BossAttack => {
                    match self
                        .boss_fx
                        .axiom_cast(event.subject, event.value, event.pos)
                    {
                        Some(kick) => {
                            self.shake = (self.shake + kick).min(4.);
                            (AMBER, 0, false)
                        }
                        None => (RED, 3, true),
                    }
                }
                EventKind::PocketEject => (AMBER, 6, true),
                EventKind::MapShake => {
                    if event.subject == local {
                        let (strength, seconds) = tankyard_core::shake_of(event.value);
                        // A stronger shake takes over; a weaker one waits its turn.
                        if strength * seconds >= self.map_shake.0 * self.map_shake.1 {
                            self.map_shake = (strength, seconds, seconds);
                        }
                    }
                    (AMBER, 0, false)
                }
                EventKind::MapZoom => {
                    if event.subject == local {
                        let view = tankyard_core::zoom_of(event.value);
                        // The newest zoom takes over from wherever the view
                        // is; a MapFocus right after it may turn it.
                        self.map_view = Some(MapView {
                            from: self.map_look(),
                            to: Look {
                                scale: view.scale,
                                ..Look::USUAL
                            },
                            age: 0.,
                            seconds: view.seconds,
                            ease: view.ease,
                            easing: view.easing,
                        });
                    }
                    (AMBER, 0, false)
                }
                EventKind::MapFocus => {
                    if event.subject == local
                        && let Some(view) = &mut self.map_view
                    {
                        let (spot, focus) = tankyard_core::focus_of(event.value);
                        view.to.focus = focus;
                        view.to.pull = [spot[0] * focus, spot[1] * focus];
                    }
                    (AMBER, 0, false)
                }
                // Heard rather than seen.
                EventKind::MapSound => (AMBER, 0, false),
                EventKind::Launch => (Color::from_rgba(166, 204, 60, 255), 8, true),
                EventKind::RiftOpen => (rift_color(event, (local, local_team)), 14, true),
                EventKind::RiftClose => (
                    mix(rift_color(event, (local, local_team)), WHITE, 0.3),
                    28,
                    true,
                ),
                // A trip is seen leaving the entry and arriving at the exit.
                EventKind::RiftTravel if event.value == 0 => (
                    mix(rift_color(event, (local, local_team)), WHITE, 0.45),
                    10,
                    true,
                ),
                EventKind::RiftTravel => {
                    let exit = GameEvent {
                        value: 1,
                        ..event.clone()
                    };
                    (
                        mix(rift_color(&exit, (local, local_team)), WHITE, 0.3),
                        3,
                        false,
                    )
                }
                EventKind::RiftFizzle => (Color::new(0.74, 0.74, 0.84, 1.), 8, true),
            };
            let count = (count as f32 * intensity) as usize;
            for _ in 0..count {
                if self.particles.len() >= 2048 {
                    break;
                }
                let angle = self.rand() * std::f32::consts::TAU;
                let spark = matches!(
                    event.kind,
                    EventKind::BodyParry
                        | EventKind::Parry
                        | EventKind::StasisRelease
                        | EventKind::RiftOpen
                        | EventKind::RiftClose
                        | EventKind::RiftFizzle
                );
                let speed = if spark {
                    160. + self.rand() * 460.
                } else {
                    40. + self.rand() * 150.
                };
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
                    spark,
                });
            }
            if ring && intensity > 0. && self.particles.len() < 2048 {
                self.particles.push(Particle {
                    pos: event.pos,
                    vel: V2::ZERO,
                    life: 0.55,
                    total: 0.55,
                    size: 20.0,
                    color,
                    ring: true,
                    spin: 0.0,
                    spark: false,
                });
            }
            // A boss is also the subject of each explosion it sets off; only
            // its own death carries the reward.
            if matches!(event.kind, EventKind::Destroy)
                && self.bosses.get(&event.subject).is_some_and(|&(_, kind)| {
                    kind != tankyard_core::BossKind::Sunderworm
                        && event.value == tankyard_core::BOSS_REWARD
                })
            {
                self.boss_fx.defeated(event.subject, event.pos);
            }
            if matches!(event.kind, EventKind::Kill)
                && event
                    .elimination
                    .as_ref()
                    .is_some_and(|elimination| elimination.victim == local)
            {
                self.killed_by = Some(event.subject);
                self.deaths = self.deaths.wrapping_add(1);
            }
            if matches!(event.kind, EventKind::Kill)
                && let Some(boss) = event
                    .elimination
                    .as_ref()
                    .and_then(|kill| self.bosses.get(&kill.victim))
            {
                self.boss_defeat = Some((boss.0.to_owned(), event.subject, 5.));
                self.boss_arrival = None;
            }
            if matches!(event.kind, EventKind::Kill)
                && let Some(victim) = event.elimination.as_ref().map(|kill| kill.victim)
                && self.boss_fx.is_axiom(victim)
            {
                self.boss_fx.defeated(victim, event.pos);
                self.boss_defeat = Some((
                    tankyard_core::creator_boss::NAME.to_owned(),
                    event.subject,
                    5.,
                ));
                self.boss_arrival = None;
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
                        name: tankyard_core::display_player_name(
                            &sanitize_name(&elimination.name),
                            elimination.game_creator,
                            elimination.server_badge,
                        ),
                        life: 3.5,
                    });
                }
            }
            if event.subject == local {
                let text = match event.kind {
                    EventKind::LevelUp => Some(format!("LEVEL {}", event.value)),
                    EventKind::Evolve => Some("Tank upgraded".into()),
                    EventKind::PocketEject => Some("A double forced you out".into()),
                    EventKind::Kill => None, // The name belongs in the top-screen feed.
                    EventKind::Parry => Some(
                        tankyard_core::clash_of(event.value)
                            .map_or("Parried", |(style, ..)| crate::boss_fx::headline(style))
                            .into(),
                    ),
                    EventKind::BodyParry => Some("SMASHER COUNTERED".into()),
                    EventKind::StasisRelease => Some("RELEASE".into()),
                    EventKind::RiftFizzle => {
                        Some("No room for a rift there: your shot came back".into())
                    }
                    _ => None,
                };
                if let Some(text) = text {
                    if matches!(
                        event.kind,
                        EventKind::Kill
                            | EventKind::Parry
                            | EventKind::BodyParry
                            | EventKind::StasisRelease
                    ) {
                        if self.popups.len() < 32 {
                            let clash = matches!(event.kind, EventKind::Parry)
                                && tankyard_core::clash_of(event.value).is_some();
                            self.popups.push(Popup {
                                pos: event.pos,
                                text,
                                life: if clash { BIG_POPUP_SECONDS } else { 1.5 },
                                color,
                                big: clash,
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
            let shown = projectile
                .previous
                .lerp(projectile.pos, fraction.clamp(0.0, 1.0));
            if self.boss_fx.is_boss(projectile.owner)
                || self.boss_fx.is_star_round(projectile.owner, projectile.id)
            {
                let flight = crate::boss_fx::Flight {
                    id: projectile.id,
                    owner: projectile.owner,
                    at: shown,
                    vel: projectile.vel,
                    radius: projectile.radius,
                    kind: projectile.kind,
                    damage: projectile.damage,
                };
                // Only a round seen as it leaves the gun gets a muzzle flash:
                // joining a fight doesn't set them all off at once.
                if !self.tracked_projectiles.contains_key(&projectile.id) && projectile.age < 0.12 {
                    let kick = self.boss_fx.launched(&flight);
                    if kick > 0. {
                        self.shake = (self.shake + kick).min(4.);
                    }
                }
                self.boss_fx.track(&flight);
            }
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
                    pos: shown,
                    radius: projectile.radius,
                    team: projectile.team,
                    owner: projectile.owner,
                    seen_at: tick,
                    kind: projectile.kind,
                    angle,
                    shots,
                    recoil,
                    lifetime: projectile.lifetime,
                    vel: projectile.vel,
                    damage: projectile.damage,
                },
            );
        }
        if self.projectile_tick == Some(tick) {
            return;
        }
        self.projectile_tick = Some(tick);
        let fades = &mut self.projectile_fades;
        let boss_fx = &mut self.boss_fx;
        self.tracked_projectiles.retain(|&id, prior| {
            if prior.seen_at == tick {
                return true;
            }
            if boss_fx.is_boss(prior.owner) || boss_fx.is_star_round(prior.owner, id) {
                boss_fx.landed(
                    &crate::boss_fx::Flight {
                        id,
                        owner: prior.owner,
                        at: prior.pos,
                        vel: prior.vel,
                        radius: prior.radius,
                        kind: prior.kind,
                        damage: prior.damage,
                    },
                    prior.lifetime > 0.05,
                );
                return false;
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
    pub fn update(&mut self, dt: f32, reduced_motion: bool) {
        if reduced_motion {
            self.particles.clear();
            self.projectile_fades.clear();
        }
        if let Some((_, life)) = &mut self.boss_arrival {
            *life -= dt;
            if *life <= 0. {
                self.boss_arrival = None;
            }
        }
        if let Some((_, _, life)) = &mut self.boss_defeat {
            *life -= dt;
            if *life <= 0. {
                self.boss_defeat = None;
            }
        }
        self.boss_fx.update(dt);
        self.shake = (self.shake + self.boss_fx.take_shake()).min(4.);
        for (pos, damage, color) in self.boss_fx.take_strikes() {
            if self.popups.len() < 32 {
                self.popups.push(Popup {
                    pos,
                    text: format!("-{}", crate::hud::grouped(u64::from(damage))),
                    life: BIG_POPUP_SECONDS,
                    color,
                    big: true,
                });
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
            if !reduced_motion {
                p.pos.y -= dt * 30.0;
            }
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
        // A worm shakes the ground harder than any blast, and for as long
        // as it is moving under it.
        self.shake = self.shake.max(self.boss_fx.worm_quake());
        let (strength, left, total) = self.map_shake;
        if left > 0. {
            // Full strength until the last third, then easing away. Players'
            // Camera shake setting scales this like any other shake.
            let level = strength * 40. * (left / total.max(0.01) * 3.).min(1.);
            self.shake = self.shake.max(level);
            self.map_shake.1 = (left - dt).max(0.);
        }
        if let Some(view) = &mut self.map_view {
            view.age += dt;
            if view.done() {
                self.map_view = None;
            }
        }
        self.notice_life = (self.notice_life - dt).max(0.0);
    }
    /// How a map's Zoom view frames the local tank now.
    pub fn map_look(&self) -> Look {
        self.map_view.map_or(Look::USUAL, |view| {
            let look = view.look();
            Look {
                scale: look.scale.clamp(0.25, 4.),
                ..look
            }
        })
    }
    /// Ease a map's Zoom view back to the usual one, as when the local tank
    /// is destroyed.
    pub fn end_map_view(&mut self) {
        if let Some(view) = self.map_view
            && view.to != Look::USUAL
        {
            self.map_view = Some(MapView {
                from: self.map_look(),
                to: Look::USUAL,
                age: 0.,
                seconds: None,
                ..view
            });
        }
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
        effects.update(1. / 60., false);
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
        effects.update(1., false);
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
    fn boss_kills_are_announced_and_each_local_death_counts_once() {
        let mut world = tankyard_core::World::new(
            tankyard_core::WorldConfig::default(),
            tankyard_content::Content::builtin(),
        );
        let boss = world.spawn_boss(tankyard_core::BossKind::Sentinel);
        let mut effects = Effects::new();
        effects.note_bosses(&world.bosses);
        let kill = |tick: u64, subject: EntityId, victim: EntityId, name: &str| GameEvent {
            tick,
            kind: EventKind::Kill,
            pos: V2::ZERO,
            subject,
            value: 0,
            team: 0,
            elimination: Some(tankyard_core::Elimination {
                victim,
                name: name.into(),
                game_creator: false,
                server_badge: Default::default(),
            }),
        };
        // The boss is already gone from the snapshot when its kill arrives.
        effects.note_bosses(&[]);
        effects.ingest(&[kill(10, 7, boss, "Sentinel")], (1, 0), 1.);
        let (name, killer, _) = effects.boss_defeat.clone().unwrap();
        assert_eq!((name.as_str(), killer), ("Sentinel", 7));
        assert_eq!(effects.deaths, 0);
        // A player named like a boss is still just a player.
        effects.boss_defeat = None;
        effects.ingest(&[kill(11, 7, 3, "Sentinel")], (1, 0), 1.);
        assert!(effects.boss_defeat.is_none());
        // Reliable replays of the same death don't start a second card.
        let death = kill(12, 7, 1, "Me");
        effects.ingest(std::slice::from_ref(&death), (1, 0), 1.);
        effects.ingest(&[death], (1, 0), 1.);
        assert_eq!((effects.deaths, effects.killed_by), (1, Some(7)));
        effects.update(5.1, false);
        assert!(!effects.boss_banner());
    }

    #[test]
    fn only_a_bosss_death_sets_off_its_defeat_blast() {
        for kind in [
            tankyard_core::BossKind::Foundry,
            tankyard_core::BossKind::Railgunner,
            tankyard_core::BossKind::Sunderworm,
        ] {
            let mut world = tankyard_core::World::new(
                tankyard_core::WorldConfig {
                    target_count: 0,
                    spawn_protection: 0.,
                    ..Default::default()
                },
                tankyard_content::Content::builtin(),
            );
            world.add_player("Pilot", false);
            assert_ne!(world.spawn_boss(kind), 0);
            let mut boss = world.bosses.remove(0);
            if kind == tankyard_core::BossKind::Sunderworm {
                boss.radius = tankyard_core::sunderworm::WORM_FRONT_RADIUS;
            }
            let destroy = |tick, value| GameEvent {
                tick,
                kind: EventKind::Destroy,
                pos: V2::ZERO,
                subject: boss.id,
                value,
                team: 0,
                elimination: None,
            };
            let mut effects = Effects::new();
            effects.note_bosses(std::slice::from_ref(&boss));
            effects.boss_fx.sync(
                std::iter::once(crate::boss_fx::Shown {
                    tick: 0,
                    body: None,
                    body_time: crate::boss_fx::BodyTime::Local(1.),
                    boss: &boss,
                    pos: boss.pos,
                    aim: boss.aim,
                    color: WHITE,
                }),
                1. / 60.,
            );
            effects.boss_fx.take_cues();
            // Its own explosive going off has it as the subject too.
            effects.ingest(&[destroy(5, 80)], (1, 0), 1.);
            assert!(effects.boss_fx.is_boss(boss.id), "still alive and tracked");
            assert!(
                !effects
                    .boss_fx
                    .take_cues()
                    .iter()
                    .any(|(cue, _)| *cue == crate::map_sounds::BossCue::WormDeath)
            );
            effects.ingest(&[destroy(6, tankyard_core::BOSS_REWARD)], (1, 0), 1.);
            if kind == tankyard_core::BossKind::Sunderworm {
                assert_eq!(
                    effects
                        .boss_fx
                        .take_cues()
                        .iter()
                        .filter(|(cue, _)| *cue == crate::map_sounds::BossCue::WormDeath)
                        .count(),
                    1
                );
            } else {
                assert!(!effects.boss_fx.is_boss(boss.id), "its death ends it");
            }
        }
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
        assert_eq!(
            effects
                .ingest(std::slice::from_ref(&event), (1, 0), 0.)
                .len(),
            1
        );
        assert!(
            effects.particles.is_empty(),
            "zero intensity must suppress rings too"
        );
        effects.update(4.9, false);
        assert!(effects.boss_arrival.is_some());
        assert!(
            effects
                .ingest(std::slice::from_ref(&event), (1, 0), 0.)
                .is_empty()
        );
        effects.update(0.11, false);
        assert!(effects.boss_arrival.is_none());
        effects.sync_arena(1., 0.016);
        effects.sync_arena(0.9, 0.016);
        assert!((0.9..1.).contains(&effects.arena_scale.unwrap()));
    }
    #[test]
    fn reduced_motion_stops_particles_and_popup_drift_without_removing_feedback() {
        let mut effects = Effects::new();
        let event = GameEvent {
            tick: 1,
            kind: EventKind::BodyParry,
            pos: V2::new(100., 200.),
            subject: 7,
            value: 0,
            team: 0,
            elimination: None,
        };
        effects.ingest(&[event], (7, 0), 1.);
        assert!(!effects.particles.is_empty());
        let position = effects.popups[0].pos;
        effects.update(0.2, true);
        assert!(effects.particles.is_empty());
        assert_eq!(effects.popups[0].pos, position);
        assert!(effects.popups[0].life < 1.5);
        effects.update(2., true);
        assert!(effects.popups.is_empty());
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
            effects
                .ingest(std::slice::from_ref(&newest), (7, 0), 1.0)
                .len(),
            1
        );
        assert_eq!(
            effects
                .ingest(std::slice::from_ref(&late), (7, 0), 1.0)
                .len(),
            1
        );
        assert_eq!(effects.notice, "LEVEL 15");
        assert!(effects.ingest(&[newest, late], (7, 0), 1.0).is_empty());
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
            last_seen: None,
            frozen: None,
            stasis_used: false,
        };
        effects.sync_projectiles(100, &[projectile], 1.0, 1.0 / 60.0, false);
        effects.sync_projectiles(100, &[], 1.0, 1.0 / 60.0, false); // Same tick: no phantom collision.
        assert!(effects.projectile_fades.is_empty());
        effects.sync_projectiles(101, &[], 1.0, 1.0 / 60.0, false);
        assert_eq!(effects.projectile_fades.len(), 1);
        effects.sync_projectiles(102, &[], 1.0, 1.0 / 60.0, false);
        assert_eq!(effects.projectile_fades.len(), 1);
        effects.update(0.2, false);
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
            last_seen: None,
            frozen: None,
            stasis_used: false,
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
                spawn_protection: 0.,
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
                spawn_protection: 0.,
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
        effects.update(1.0, false);

        tank.barrel_shots[0] = 0;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        assert_eq!(effects.barrels[&id].recoil[0], 0.0);
        tank.barrel_shots[0] = 1;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        assert!(effects.barrels[&id].recoil[0] > 0.0);

        tank.barrel_shots[0] = u32::MAX;
        effects.sync_barrels(std::slice::from_ref(&tank), 0.016, false);
        effects.update(1.0, false);
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
                (0, 0),
                1.0,
            );
        }
        assert!(effects.particles.len() <= 2048);
        effects.update(3.0, false);
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
                game_creator: false,
                server_badge: Default::default(),
            }),
        };
        let batch = vec![make(8), make(9)];
        assert_eq!(effects.ingest(&batch, (7, 0), 1.).len(), 2);
        assert_eq!(effects.kills.len(), 2);
        assert_eq!(effects.kills[0].name, "Rival 8");
        assert!(effects.ingest(&batch, (7, 0), 1.).is_empty());
        for victim in 10..=14 {
            effects.ingest(&[make(victim)], (7, 0), 1.);
        }
        assert_eq!(effects.kills.len(), 3);
        assert_eq!(effects.kills[2].name, "Rival 14");
        effects.update(4., false);
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
                    game_creator: false,
                    server_badge: Default::default(),
                }),
            }],
            (7, 0),
            1.,
        );
        assert_eq!(effects.kills[0].name.chars().count(), 24);
        effects.sync_round(1);
        effects.sync_round(2);
        assert!(effects.seen.is_empty());
    }
    fn zoom(tick: u64, view: tankyard_core::ZoomView) -> GameEvent {
        GameEvent {
            tick,
            kind: EventKind::MapZoom,
            pos: V2::ZERO,
            subject: 7,
            value: tankyard_core::zoom_event(&view),
            team: 0,
            elimination: None,
        }
    }

    #[test]
    fn map_zooms_ease_hold_and_take_over_without_jumping() {
        let mut effects = Effects::new();
        let wide = tankyard_core::ZoomView {
            scale: 0.5,
            seconds: None,
            ease: 1.,
            easing: Easing::default(),
        };
        effects.ingest(&[zoom(1, wide)], (7, 0), 1.);
        assert_eq!(effects.map_look().scale, 1., "starts where the view was");
        effects.update(0.5, false);
        let halfway = effects.map_look().scale;
        assert!(halfway < 0.9 && halfway > 0.6, "{halfway}");
        effects.update(0.6, false);
        assert!((effects.map_look().scale - 0.5).abs() < 1e-3);
        effects.update(100., false);
        assert!((effects.map_look().scale - 0.5).abs() < 1e-3, "held");
        // The next view eases from this one, not from the usual zoom.
        let close = tankyard_core::ZoomView {
            scale: 2.,
            seconds: Some(4.),
            ease: 0.5,
            easing: Easing::default(),
        };
        effects.ingest(&[zoom(2, close)], (7, 0), 1.);
        assert!((effects.map_look().scale - 0.5).abs() < 1e-3);
        effects.update(1., false);
        assert!((effects.map_look().scale - 2.).abs() < 1e-3);
        effects.update(3.1, false);
        assert_eq!(effects.map_look(), Look::USUAL, "back after its time");
        // Someone else's zoom is theirs.
        effects.ingest(&[zoom(3, wide)], (8, 0), 1.);
        effects.update(2., false);
        assert_eq!(effects.map_look(), Look::USUAL);
    }

    #[test]
    fn short_zooms_go_both_ways_and_focus_turns_the_camera() {
        let mut effects = Effects::new();
        let quick = tankyard_core::ZoomView {
            scale: 0.5,
            seconds: Some(0.4),
            ease: 3.,
            easing: Easing::LINEAR,
        };
        effects.ingest(&[zoom(1, quick)], (7, 0), 1.);
        effects.update(0.19, false);
        assert!(effects.map_look().scale < 0.55, "all the way out");
        effects.update(0.22, false);
        assert_eq!(effects.map_look(), Look::USUAL, "and back in time");
        let turn = tankyard_core::ZoomView {
            scale: 1.,
            seconds: None,
            ease: 0.5,
            easing: Easing::LINEAR,
        };
        let focus = GameEvent {
            kind: EventKind::MapFocus,
            value: tankyard_core::focus_event([0.25, 0.75], 100),
            ..zoom(2, turn)
        };
        effects.ingest(&[zoom(2, turn), focus], (7, 0), 1.);
        let tank = V2::new(100., 100.);
        assert_eq!(effects.map_look().camera(tank, [4000., 2000.]), tank);
        effects.update(1., false);
        let camera = effects.map_look().camera(tank, [4000., 2000.]);
        assert!(camera.distance(V2::new(1000., 1500.)) < 1., "{camera:?}");
        effects.end_map_view();
        effects.update(0.25, false);
        let back = effects.map_look().camera(tank, [4000., 2000.]);
        assert!(back.distance(tank) > 100. && back.distance(camera) > 100.);
        effects.update(0.3, false);
        assert_eq!(effects.map_look(), Look::USUAL);
        assert!(effects.map_view.is_none());
    }
}
