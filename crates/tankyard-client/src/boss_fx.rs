//! How bosses look in action: the warning before a special, the core that
//! charges for it, the rounds they fire and everything those rounds set off.
//!
//! All of it is presentation. It follows snapshots and never touches the
//! simulation, so a boss looks the same whoever hosts it. Light goes through
//! [`crate::emission::Emission`], which keeps it soft and bright; warnings
//! stay visible even with effects turned off, because players need them to
//! dodge.
mod axiom;
mod blasts;
mod clash;
mod fire;
mod hull;
mod railgun;
mod ribbon;
mod shots;
mod warnings;
mod worm;

pub use axiom::ShownAxiom;
pub use clash::{clash_color, headline};
pub use hull::Armor;
pub use railgun::Streams;
pub use shots::tracer;

use crate::lighting::Light;
use crate::render::View;
use crate::ui::{AMBER, alpha, mix};
use fire::{Plume, Puff, ramp};
use macroquad::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{E, PI, TAU};
use tankyard_client::boss_model::AXIOM_GOLD;
use tankyard_content::ProjectileKind;
use tankyard_core::creator_boss::CANNONS;
use tankyard_core::{BASTION_ROUND_DAMAGE, Boss, BossKind, EntityId, LETHAL_DAMAGE, V2};

/// Most sparks alive at once, so a long fight can't slow the frame.
const MOST_SPARKS: usize = 720;
const MOST_BURSTS: usize = 180;
const MOST_TRAILS: usize = 640;
const MOST_WAKES: usize = 200;
const MOST_SCARS: usize = 28;
/// Points kept along each round's trail.
const TRAIL_POINTS: usize = 14;
/// Past this far from the camera nobody can see the air move a trail.
const AIR_RANGE: f32 = 3000.;
/// How long a trail lingers after its round has gone.
const WAKE_SECONDS: f32 = 0.24;
/// How long a scorch mark stays on the floor.
const SCAR_SECONDS: f32 = 7.;
/// How quickly a boss's drawn aim settles on its real aim.
const AIM_RATE: f32 = 26.;
/// How quickly the shove of firing settles.
const KICK_RATE: f32 = 17.;
/// The furthest one shot can shove a hull, as a share of its radius.
const MOST_KICK: f32 = 0.09;
/// How far along its barrel the Siege Engine's round leaves, in hull radii.
const SIEGE_MUZZLE: f32 = 1.62;
/// The same for Bastion's.
const BASTION_MUZZLE: f32 = 1.52;
/// How long the pale stretch on a boss's health bar waits after a burst of
/// damage, and how quickly it then catches up.
const TRAIL_HOLD: f32 = 0.42;
const TRAIL_RATE: f32 = 5.5;
/// The orange of a big explosion.
const FIRE: Color = Color::new(1., 0.55, 0.16, 1.);
/// How long a boss's exhaust stays lit in the air behind it.
const EXHAUST_LIFE: f32 = 0.95;
/// Slower than this it isn't really driving, and leaves nothing.
const EXHAUST_SPEED: f32 = 14.;
/// Sets how finely its exhaust is laid, as a trail's radius would.
const EXHAUST_GAP: f32 = 20.;

/// How strongly effects are drawn, from the player's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The Effects setting: 0 for none, 1 as designed.
    pub strength: f32,
    /// Reduced motion: no flashes, no trails, nothing moving for show.
    pub calm: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            strength: 1.,
            calm: false,
        }
    }
}

impl Look {
    /// The look for the Effects setting, whose default of 0.8 is full strength.
    pub fn of(effects: f32, reduced_motion: bool) -> Self {
        Self {
            strength: if effects.is_finite() {
                (effects / 0.8).clamp(0., 1.25)
            } else {
                0.
            },
            calm: reduced_motion,
        }
    }
    /// Whether anything purely decorative should happen at all.
    fn live(self) -> bool {
        !self.calm && self.strength > 0.01
    }
    fn level(self) -> f32 {
        if self.calm { 0. } else { self.strength }
    }
    /// `count` scaled to the setting, keeping at least one while it's on.
    fn many(self, count: f32) -> usize {
        ((count * self.level()) as usize).max(usize::from(self.live()))
    }
}

/// What a boss's guns burn with, which sets how its fire looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fuel {
    /// Clean bright bolts.
    Plasma,
    /// Flame, embers and smoke.
    Fire,
    /// Electricity that forks and jumps.
    Arc,
    /// Light split into color.
    Spectrum,
    /// A dark heart with light bending around it.
    Void,
    /// Axiom's gold starlight.
    Star,
    /// The Railgunner's ions: white at the heart, blue, violet at the edge.
    Ion,
    /// The Sunderworm's bone, bile and blood: nothing it throws burns.
    Flesh,
}

impl Fuel {
    fn of(kind: BossKind) -> Self {
        match kind {
            BossKind::SiegeEngine | BossKind::Foundry => Self::Fire,
            BossKind::Tempest | BossKind::Skirmisher => Self::Arc,
            BossKind::Prism => Self::Spectrum,
            BossKind::Eclipse | BossKind::FallenOverlord | BossKind::FallenBooster => Self::Void,
            BossKind::Railgunner => Self::Ion,
            BossKind::Sunderworm => Self::Flesh,
            BossKind::Sentinel
            | BossKind::Bastion
            | BossKind::Guardian
            | BossKind::Summoner
            | BossKind::Defender
            | BossKind::Trapper => Self::Plasma,
        }
    }
}

/// A boss as this frame shows it.
#[derive(Clone, Copy)]
pub struct Shown<'a> {
    pub tick: u64,
    pub body: Option<&'a tankyard_core::sunderworm::WormBody>,
    pub fraction: f32,
    pub boss: &'a Boss,
    pub pos: V2,
    /// Its real aim, carried between ticks.
    pub aim: f32,
    pub color: Color,
}

/// What the renderer remembers about a boss between frames.
#[derive(Clone, Copy, Debug)]
struct Presence {
    /// Which boss it is; Axiom is a tank, so it has none.
    kind: Option<BossKind>,
    pos: V2,
    radius: f32,
    color: Color,
    fuel: Fuel,
    /// The aim as drawn, and how fast it is turning.
    aim: f32,
    aim_rate: f32,
    /// The real aim last frame, and how fast that has been turning.
    aim_target: f32,
    aim_drive: f32,
    /// How far its special has charged, eased between snapshots.
    charge: f32,
    /// The glow of being hit, 0 to 1.
    heat: f32,
    phase: u8,
    health: f32,
    /// The end of the pale stretch on its health bar, as a share of the
    /// whole, and how long until that starts to catch up.
    trail: f32,
    trail_hold: f32,
    /// How far firing has shoved the hull, in world units, and how fast.
    kick: Vec2,
    kick_rate: Vec2,
    winding: bool,
    /// Seconds since it was last in a snapshot.
    unseen: f32,
    /// Part-made embers, smoke and flame, so the rate holds at any frame rate.
    embers: f32,
    smoke: f32,
    flame: f32,
    /// How far back each of Axiom's cannons is, from the one on its aim.
    guns: [f32; CANNONS],
    /// How fast Axiom is driving, in world units a second.
    drive: Vec2,
    /// Exhaust follows the last steady travel direction through stops and turns.
    exhaust_aim: f32,
    /// A Railgunner's firing pose and what it was doing last frame.
    rail: railgun::RailShown,
    /// The gas its drive has left in the air.
    exhaust: [Trail; 2],
}

impl Presence {
    fn new(shown: &Shown<'_>) -> Self {
        let boss = shown.boss;
        Self {
            kind: Some(boss.kind),
            pos: shown.pos,
            radius: boss.radius,
            color: shown.color,
            fuel: Fuel::of(boss.kind),
            aim: shown.aim,
            aim_rate: 0.,
            aim_target: shown.aim,
            aim_drive: 0.,
            charge: 0.,
            heat: 0.,
            phase: boss.phase,
            health: boss.health,
            trail: health_share(boss),
            trail_hold: 0.,
            kick: Vec2::ZERO,
            kick_rate: Vec2::ZERO,
            winding: false,
            unseen: 0.,
            embers: 0.,
            smoke: 0.,
            flame: 0.,
            guns: [0.; CANNONS],
            drive: Vec2::ZERO,
            exhaust_aim: shown.aim,
            rail: railgun::RailShown::default(),
            exhaust: exhaust_for(shown.pos, shown.color, Fuel::of(boss.kind)),
        }
    }
    /// How far firing has shoved its hull, in the world.
    fn kick_v2(&self) -> V2 {
        V2::new(self.kick.x, self.kick.y)
    }
}

/// A boss's two wisps of exhaust, with nothing laid yet.
fn exhaust_for(pos: V2, color: Color, fuel: Fuel) -> [Trail; 2] {
    let mut wisp = Trail::new(pos, EXHAUST_GAP, color, fuel, false);
    wisp.linger = EXHAUST_LIFE;
    wisp.stir = 0.42;
    let mut other = wisp;
    other.seed = 3.1;
    [wisp, other]
}

/// Where a boss's exhaust leaves it: the Railgunner's two real engines, and
/// for the rest the back corners of wherever it's driving.
fn exhaust_from(kind: BossKind, pos: V2, aim: f32, vel: V2, radius: f32) -> [V2; 2] {
    if kind == BossKind::Railgunner {
        let ahead = V2::from_angle(aim);
        let side = V2::new(-ahead.y, ahead.x);
        return [-1., 1.].map(|out| pos - ahead * (radius * 1.31) + side * (radius * out * 0.7));
    }
    let way = vel.normalized();
    let side = V2::new(-way.y, way.x);
    [-1., 1.].map(|out| pos - way * (radius * 0.82) + side * (radius * out * 0.46))
}

/// `health` as a share of what this boss can hold.
fn share_of(health: f32, boss: &Boss) -> f32 {
    (health / boss.max_health.max(1.)).clamp(0., 1.)
}
fn health_share(boss: &Boss) -> f32 {
    share_of(boss.health, boss)
}

/// One frame of the pale stretch on a health bar, as (end, seconds held). A
/// new burst of damage holds it up for a moment; after that it catches up, and
/// more hits while it does don't hold it again, so it can't grow without end
/// under steady fire.
fn follow_trail(
    (mut trail, mut hold): (f32, f32),
    before: f32,
    now: f32,
    dt: f32,
    calm: bool,
) -> (f32, f32) {
    if now < before && trail <= before {
        trail = before;
        hold = TRAIL_HOLD;
    }
    hold = (hold - dt).max(0.);
    if calm || now >= trail {
        return (now, 0.);
    }
    if hold > 0. {
        return (trail, hold);
    }
    let eased = trail + (now - trail) * (1. - (-dt * TRAIL_RATE).exp());
    (if eased - now < 0.002 { now } else { eased }, 0.)
}

/// The shortest way around from angle `a` to angle `b`.
fn turn_between(a: f32, b: f32) -> f32 {
    (b - a + PI).rem_euclid(TAU) - PI
}

/// One step of an exactly solved critically damped spring toward zero, the
/// same at any frame rate: returns the new offset and its speed.
fn settle(offset: f32, speed: f32, rate: f32, dt: f32) -> (f32, f32) {
    let decay = (-rate * dt).exp();
    let term = speed + rate * offset;
    (
        (offset + term * dt) * decay,
        (speed - rate * term * dt) * decay,
    )
}

/// Ease a drawn angle onto a target that went from `before` to `target` this
/// step and is turning at `drive`. The spring works on the error alone, in a
/// frame turning with the target, so a steady turn is followed with no lag
/// and only its starts and stops are rounded off.
fn follow_angle(
    value: f32,
    rate: f32,
    before: f32,
    target: f32,
    drive: f32,
    dt: f32,
) -> (f32, f32) {
    let (error, relative) = settle(turn_between(before, value), rate - drive, AIM_RATE, dt);
    (target + error, relative + drive)
}

fn ease_out(t: f32) -> f32 {
    let u = 1. - t.clamp(0., 1.);
    1. - u * u * u
}

fn perpendicular(d: Vec2) -> Vec2 {
    vec2(-d.y, d.x)
}

fn direction(angle: f32) -> Vec2 {
    vec2(angle.cos(), angle.sin())
}

/// A trail of where a round has been, newest point last written.
#[derive(Clone, Copy, Debug)]
struct Trail {
    points: [V2; TRAIL_POINTS],
    /// The push each point's gas still has from whatever left it.
    drift: [V2; TRAIL_POINTS],
    /// When each point was laid, on `clock`.
    laid: [f32; TRAIL_POINTS],
    head: u8,
    count: u8,
    /// The round's own last position, where the trail's bright end meets it.
    latest: V2,
    /// How far it has come in all.
    flown: f32,
    /// Seconds a stretch of it stays lit.
    linger: f32,
    /// How much the air moves its gas: more for a big round than a small one.
    stir: f32,
    radius: f32,
    color: Color,
    fuel: Fuel,
    /// The Siege Engine's round, which burns.
    heavy: bool,
    /// Distance flown since it last left flame behind.
    fed: f32,
    seen: bool,
    /// Seconds since it began.
    clock: f32,
    /// Sets where its sway starts, so no two writhe in step.
    seed: f32,
}

impl Trail {
    fn new(pos: V2, radius: f32, color: Color, fuel: Fuel, heavy: bool) -> Self {
        Self {
            points: [pos; TRAIL_POINTS],
            drift: [V2::ZERO; TRAIL_POINTS],
            laid: [0.; TRAIL_POINTS],
            head: 0,
            count: 1,
            latest: pos,
            flown: 0.,
            linger: ribbon::LINGER,
            stir: (radius / 16.).clamp(0.35, 1.2),
            radius,
            color,
            fuel,
            heavy,
            fed: 0.,
            seen: true,
            clock: 0.,
            seed: 0.,
        }
    }
    /// Note the round at `pos`, dropping a point once it has moved far
    /// enough, so the trail's shape doesn't depend on the frame rate.
    fn note(&mut self, pos: V2) {
        let stepped = pos.distance(self.latest);
        self.latest = pos;
        self.seen = true;
        let head = usize::from(self.head);
        let last = self.points[head];
        // A jump means a rift or a bounce back through a teleport, and a
        // trail drawn across it would streak over the whole arena.
        if pos.distance(last) > 260. {
            self.points[0] = pos;
            self.drift[0] = V2::ZERO;
            self.laid[0] = self.clock;
            self.head = 0;
            self.count = 1;
            return;
        }
        if stepped.is_finite() {
            self.flown += stepped;
        }
        if pos.distance(last) >= self.spacing() {
            // The gas keeps a little of the speed it was left at.
            let took = (self.clock - self.laid[head]).max(1. / 240.);
            let push = (pos - last) * (ribbon::CARRY / took);
            let speed = push.length();
            self.head = (self.head + 1) % TRAIL_POINTS as u8;
            let head = usize::from(self.head);
            self.points[head] = pos;
            self.drift[head] = if speed > ribbon::MOST_CARRY {
                push * (ribbon::MOST_CARRY / speed)
            } else {
                push
            };
            self.laid[head] = self.clock;
            self.count = (self.count + 1).min(TRAIL_POINTS as u8);
        }
    }
    /// Let the gas it has left move with the air for `dt`. Big rounds stir up
    /// more of it than small ones.
    fn billow(&mut self, dt: f32, air: f32, camera: V2) {
        let settle = (-ribbon::CARRY_DRAG * dt).exp();
        let watched = self.points[usize::from(self.head)].distance(camera) < AIR_RANGE;
        for back in 0..usize::from(self.count) {
            let at = (usize::from(self.head) + TRAIL_POINTS - back) % TRAIL_POINTS;
            let loose = ((self.clock - self.laid[at]) / ribbon::HOLDS).clamp(0., 1.);
            let carried = if watched && loose > 0. {
                ribbon::air(self.points[at], air + self.seed) * (loose * loose * self.stir)
            } else {
                V2::ZERO
            };
            self.points[at] += (self.drift[at] + carried) * dt;
            self.drift[at] *= settle;
        }
    }
    /// How far the round flies between points.
    fn spacing(&self) -> f32 {
        (self.radius * 0.7).max(6.)
    }
    /// The longest its ribbon runs in the world, kept inside the points it
    /// has, so the tail never reaches the end of what it remembers.
    fn reach(&self) -> f32 {
        self.spacing() * (TRAIL_POINTS - 2) as f32
    }
    /// How long ago the `age`th newest point was laid.
    fn age(&self, age: usize) -> f32 {
        let at = (usize::from(self.head) + TRAIL_POINTS - age % TRAIL_POINTS) % TRAIL_POINTS;
        (self.clock - self.laid[at]).max(0.)
    }
    /// The `age`th newest point.
    fn point(&self, age: usize) -> V2 {
        let at = (usize::from(self.head) + TRAIL_POINTS - age % TRAIL_POINTS) % TRAIL_POINTS;
        self.points[at]
    }
}

/// A trail whose round has landed, fading where it lies.
#[derive(Clone, Copy, Debug)]
struct Wake {
    trail: Trail,
    life: f32,
}

#[derive(Clone, Copy, Debug)]
struct Spark {
    pos: V2,
    vel: V2,
    life: f32,
    total: f32,
    size: f32,
    color: Color,
    drag: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Flash {
    /// Leaving a barrel, along this direction.
    Muzzle(Vec2),
    /// A round striking something.
    Hit,
    /// A round running out of life in the open.
    Fizzle,
    /// An explosive going off.
    Blast,
    /// A special attack leaving the core.
    Release,
    /// A boss crossing into a harder phase.
    Phase,
    /// A boss destroyed.
    Fall,
    /// A boss arriving.
    Arrival,
    /// A drifting puff of smoke.
    Smoke,
    /// A soft bloom of light with no shock wave.
    Bloom,
}

#[derive(Clone, Copy, Debug)]
struct Burst {
    at: V2,
    vel: V2,
    flash: Flash,
    fuel: Fuel,
    size: f32,
    color: Color,
    age: f32,
    life: f32,
}

/// A spray of sparks.
#[derive(Clone, Copy, Debug)]
struct Spray {
    at: V2,
    /// The way they fly, and how far either side of it; None for all around.
    heading: Option<(f32, f32)>,
    count: usize,
    speed: (f32, f32),
    life: (f32, f32),
    size: (f32, f32),
    color: Color,
}

/// Soot a heavy blast left on the floor.
#[derive(Clone, Copy, Debug)]
struct Scar {
    at: V2,
    radius: f32,
    age: f32,
}

/// A boss's round in the world, for following it from frame to frame.
#[derive(Clone, Copy, Debug)]
pub struct Flight {
    pub id: EntityId,
    pub owner: EntityId,
    /// Where it is drawn right now.
    pub at: V2,
    pub vel: V2,
    pub radius: f32,
    pub kind: ProjectileKind,
    pub damage: f32,
}

/// One round as drawn this frame.
#[derive(Clone, Copy, Debug)]
pub struct Round {
    pub id: EntityId,
    pub owner: EntityId,
    /// Where it is on screen, and its radius there.
    pub p: Vec2,
    pub r: f32,
    /// Its velocity in the world, which is also the way it's flying.
    pub heading: Vec2,
    pub kind: ProjectileKind,
    pub damage: f32,
    pub color: Color,
    pub opacity: f32,
    /// Seconds since it was fired.
    pub age: f32,
    pub z: f32,
}

/// What a boss has been doing, drawn frame to frame.
pub struct BossFx {
    bosses: HashMap<EntityId, Presence>,
    trails: HashMap<EntityId, Trail>,
    wakes: Vec<Wake>,
    bursts: Vec<Burst>,
    sparks: Vec<Spark>,
    flames: Vec<Puff>,
    scars: Vec<Scar>,
    casts: Vec<axiom::Cast>,
    tracers: Vec<railgun::Tracer>,
    wrecks: Vec<railgun::Wreck>,
    clashes: Vec<clash::Clash>,
    /// Plasma rounds that just vanished, held a frame in case a clash took them.
    gone: Vec<clash::GoneRound>,
    /// Heavy rounds' blasts, waiting to see whether a clash took the round.
    blasts: Vec<clash::HeldBlast>,
    /// Damage numbers for strikes the player landed, ready to show.
    strikes: Vec<(V2, u32, Color)>,
    /// Seconds left of the instant a hard hit holds everything still.
    hold: f32,
    /// A clock for the air that trails drift on.
    air: f32,
    /// Sounds due, each from a place or, with none, right at the player.
    cues: Vec<(crate::map_sounds::BossCue, Option<V2>)>,
    /// The player's tank, so a Railgunner's lock on them can be louder.
    local: EntityId,
    /// Where the camera is, so a clash far away doesn't shake it.
    camera: V2,
    /// Camera shake from blasts set off between frames.
    pending_shake: f32,
    /// Whether any of Axiom or its rounds are about, so that every other
    /// game skips looking for them.
    stars: bool,
    /// The Sunderworms, their bodies and the mess they make.
    worms: worm::Worms,
    look: Look,
    random: u32,
}

impl Default for BossFx {
    fn default() -> Self {
        Self {
            bosses: HashMap::new(),
            trails: HashMap::new(),
            wakes: Vec::with_capacity(64),
            bursts: Vec::with_capacity(64),
            sparks: Vec::with_capacity(256),
            flames: Vec::with_capacity(256),
            scars: Vec::with_capacity(MOST_SCARS),
            casts: Vec::new(),
            tracers: Vec::new(),
            wrecks: Vec::new(),
            clashes: Vec::new(),
            gone: Vec::new(),
            blasts: Vec::new(),
            strikes: Vec::new(),
            hold: 0.,
            air: 0.,
            cues: Vec::new(),
            local: 0,
            camera: V2::ZERO,
            pending_shake: 0.,
            stars: false,
            worms: worm::Worms::default(),
            look: Look::default(),
            random: 0x9e37_79b9,
        }
    }
}

impl BossFx {
    fn rand(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random as f32 / u32::MAX as f32
    }
    fn between(&mut self, (low, high): (f32, f32)) -> f32 {
        low + (high - low) * self.rand()
    }
    /// Say which tank is the player's.
    pub fn set_local(&mut self, id: EntityId) {
        self.local = id;
    }
    pub fn set_look(&mut self, look: Look) {
        self.look = look;
        if !look.live() {
            self.worm_quiet();
            self.sparks.clear();
            self.wakes.clear();
            self.trails.clear();
            self.bursts.clear();
            self.flames.clear();
            self.scars.clear();
            self.casts.clear();
            self.tracers.clear();
            self.wrecks.clear();
            // Pending strikes also carry the local player's damage feedback.
            // Keep their short timers when decorative effects are disabled.
            self.gone.clear();
            self.blasts.clear();
            self.hold = 0.;
            self.pending_shake = 0.;
        }
    }
    /// Whether `id` is a boss seen recently. Axiom counts only while it's in
    /// the form, because the tank underneath goes back to firing normal shots.
    pub fn is_boss(&self, id: EntityId) -> bool {
        self.bosses
            .get(&id)
            .is_some_and(|shown| shown.fuel != Fuel::Star || shown.unseen <= 0.)
    }
    /// The angle to draw a boss's guns at.
    pub fn aim(&self, boss: &Boss) -> f32 {
        self.bosses
            .get(&boss.id)
            .map_or(boss.aim, |shown| shown.aim)
    }
    /// How far its special has charged, eased, 0 to 1.
    pub fn charge(&self, boss: &Boss) -> f32 {
        self.bosses.get(&boss.id).map_or(0., |shown| shown.charge)
    }
    /// Where the pale just-lost stretch on its health bar ends, as a share.
    pub fn health_trail(&self, boss: &Boss) -> f32 {
        self.bosses
            .get(&boss.id)
            .map_or_else(|| health_share(boss), |shown| shown.trail)
    }
    /// How far firing has shoved its hull on screen.
    pub fn kick(&self, boss: &Boss, zoom: f32) -> Vec2 {
        self.bosses
            .get(&boss.id)
            .map_or(Vec2::ZERO, |shown| shown.kick * zoom)
    }

    /// Follow this frame's bosses: ease their aim and charge, and notice
    /// what changed since the last frame.
    pub fn sync<'a>(&mut self, shown: impl Iterator<Item = Shown<'a>>, dt: f32) {
        let dt = dt.clamp(0., 0.1);
        for presence in self.bosses.values_mut() {
            presence.unseen += dt;
        }
        let look = self.look;
        for item in shown {
            let boss = item.boss;
            let known = self.bosses.contains_key(&boss.id);
            let presence = self
                .bosses
                .entry(boss.id)
                .or_insert_with(|| Presence::new(&item));
            let was = *presence;
            presence.unseen = 0.;
            presence.pos = item.pos;
            presence.radius = boss.radius;
            presence.color = item.color;
            if look.calm || !item.aim.is_finite() || dt <= 0. {
                if item.aim.is_finite() {
                    presence.aim = item.aim;
                }
                presence.aim_rate = 0.;
                presence.aim_drive = 0.;
            } else {
                let moved = turn_between(presence.aim_target, item.aim);
                presence.aim_drive += (moved / dt - presence.aim_drive) * (1. - (-dt * 20.).exp());
                let (aim, rate) = follow_angle(
                    presence.aim,
                    presence.aim_rate,
                    presence.aim_target,
                    item.aim,
                    presence.aim_drive,
                    dt,
                );
                presence.aim = aim;
                presence.aim_rate = rate;
            }
            if item.aim.is_finite() {
                presence.aim_target = item.aim;
            }
            let goal = if boss.attack_windup > 0. {
                (1. - boss.attack_windup / boss.kind.windup()).clamp(0., 1.)
            } else {
                0.
            };
            presence.charge = if look.calm {
                goal
            } else {
                let rate = if goal > presence.charge { 26. } else { 9. };
                presence.charge + (goal - presence.charge) * (1. - (-dt * rate).exp())
            };
            presence.winding = boss.attack_windup > 0.;
            if boss.max_health > 0. && boss.health < was.health {
                let share = (was.health - boss.health) / boss.max_health;
                presence.heat = (presence.heat + 0.2 + share * 26.).min(1.);
            }
            (presence.trail, presence.trail_hold) = follow_trail(
                (presence.trail, presence.trail_hold),
                share_of(was.health, boss),
                health_share(boss),
                dt,
                look.calm,
            );
            presence.health = boss.health;
            presence.heat *= (-dt * 6.5).exp();
            presence.phase = boss.phase;
            let (x, vx) = settle(presence.kick.x, presence.kick_rate.x, KICK_RATE, dt);
            let (y, vy) = settle(presence.kick.y, presence.kick_rate.y, KICK_RATE, dt);
            presence.kick = vec2(x, y);
            presence.kick_rate = vec2(vx, vy);
            // A worm is flesh, not a machine: no exhaust, embers or flashes.
            if boss.kind == BossKind::Sunderworm {
                self.worm_frame(&item, dt);
                continue;
            }
            // Its exhaust stays in the air, so it bends where the boss turned.
            if boss.vel.length() > EXHAUST_SPEED * 0.5 {
                presence.exhaust_aim +=
                    turn_between(presence.exhaust_aim, boss.vel.angle()) * (1. - (-dt * 10.).exp());
            }
            let nozzles = exhaust_from(
                boss.kind,
                item.pos,
                presence.aim,
                V2::from_angle(presence.exhaust_aim),
                boss.radius,
            );
            let (driving, air, camera) = (boss.vel.length() > EXHAUST_SPEED, self.air, self.camera);
            for (wisp, nozzle) in presence.exhaust.iter_mut().zip(nozzles) {
                wisp.clock += dt;
                wisp.color = item.color;
                if look.live() {
                    if driving {
                        wisp.note(nozzle);
                    }
                    wisp.billow(dt, air, camera);
                }
            }
            let (color, radius, pos, phase) = (presence.color, boss.radius, item.pos, boss.phase);
            let mut due = (0, 0, 0);
            if look.live() && phase >= 2 {
                let (embers, smoke, flame) = if phase >= 3 {
                    (15., 1.8, 16.)
                } else {
                    (5., 0., 4.)
                };
                presence.embers += embers * dt * look.level();
                presence.smoke += smoke * dt * look.level();
                presence.flame += flame * dt * look.level();
                due = (
                    presence.embers as usize,
                    presence.smoke as usize,
                    presence.flame as usize,
                );
                presence.embers -= due.0 as f32;
                presence.smoke -= due.1 as f32;
                presence.flame -= due.2 as f32;
            }
            // Its pose runs even with effects off: the warnings depend on it.
            if boss.kind == BossKind::Railgunner {
                self.rail_frame(boss, known, dt);
            }
            if !look.live() {
                continue;
            }
            if !known && boss.arrival > 0. {
                self.burst(pos, Flash::Arrival, radius, color, boss.arrival.max(0.5));
            }
            if known && boss.phase > was.phase {
                self.phase_change(pos, radius, color);
            }
            if known && was.winding && boss.attack_windup <= 0. {
                self.burst(pos, Flash::Release, radius, color, 0.46);
            }
            self.wounds(pos, radius, phase, due);
        }
        self.bosses.retain(|_, presence| presence.unseen < 1.5);
        self.worm_gone(dt);
    }

    /// Embers, flame and smoke coming off a wounded hull.
    fn wounds(
        &mut self,
        at: V2,
        radius: f32,
        phase: u8,
        (embers, smoke, flame): (usize, usize, usize),
    ) {
        let heat = if phase >= 3 {
            Color::new(1., 0.36, 0.2, 1.)
        } else {
            Color::new(1., 0.68, 0.26, 1.)
        };
        for _ in 0..embers {
            let angle = self.rand() * TAU;
            let from = at + V2::from_angle(angle) * radius * self.between((0.7, 1.02));
            self.spray(Spray {
                at: from,
                heading: Some((angle, 0.55)),
                count: 1,
                speed: (26., 92.),
                life: (0.45, 1.05),
                size: (1.2, 2.5),
                color: heat,
            });
        }
        for _ in 0..flame {
            let angle = self.rand() * TAU;
            let from = at + V2::from_angle(angle) * radius * self.between((0.4, 0.92));
            self.plume(Plume {
                at: from,
                scatter: radius * 0.05,
                heading: Some((angle, 0.7)),
                speed: (24., 96.),
                size: (radius * 0.09, radius * 0.2),
                life: (0.26, 0.6),
                count: 1,
                burn: fire::OPEN,
            });
        }
        for _ in 0..smoke {
            let angle = self.rand() * TAU;
            let from = at + V2::from_angle(angle) * radius * self.between((0.45, 0.95));
            let size = radius * self.between((0.34, 0.6));
            self.smoke(from, angle, (10., 26.), size, 1.5);
        }
    }

    fn smoke(&mut self, at: V2, angle: f32, drift: (f32, f32), size: f32, life: f32) {
        let speed = self.between(drift);
        if self.bursts.len() < MOST_BURSTS {
            self.bursts.push(Burst {
                at,
                vel: V2::from_angle(angle) * speed,
                flash: Flash::Smoke,
                fuel: Fuel::Plasma,
                size,
                color: Color::new(0.09, 0.08, 0.09, 1.),
                age: 0.,
                life,
            });
        }
    }

    fn phase_change(&mut self, at: V2, radius: f32, color: Color) {
        self.burst(at, Flash::Phase, radius, color, 0.9);
        self.spray(Spray {
            at,
            heading: None,
            count: self.look.many(34.),
            speed: (140., 460.),
            life: (0.35, 0.95),
            size: (1.5, 3.4),
            color: mix(color, WHITE, 0.25),
        });
    }

    fn burst(&mut self, at: V2, flash: Flash, size: f32, color: Color, life: f32) {
        self.fueled_burst(at, flash, Fuel::Plasma, size, color, life);
    }

    fn fueled_burst(
        &mut self,
        at: V2,
        flash: Flash,
        fuel: Fuel,
        size: f32,
        color: Color,
        life: f32,
    ) {
        if self.bursts.len() < MOST_BURSTS {
            self.bursts.push(Burst {
                at,
                vel: V2::ZERO,
                flash,
                fuel,
                size,
                color,
                age: 0.,
                life,
            });
        }
    }

    fn spray(&mut self, spray: Spray) {
        for _ in 0..spray.count {
            if self.sparks.len() >= MOST_SPARKS {
                break;
            }
            let angle = match spray.heading {
                Some((heading, spread)) => heading + (self.rand() * 2. - 1.) * spread,
                None => self.rand() * TAU,
            };
            let life = self.between(spray.life);
            let speed = self.between(spray.speed);
            let size = self.between(spray.size);
            let drag = 2.4 + self.rand() * 2.6;
            self.sparks.push(Spark {
                pos: spray.at,
                vel: V2::from_angle(angle) * speed,
                life,
                total: life,
                size,
                color: spray.color,
                drag,
            });
        }
    }

    fn scar(&mut self, at: V2, radius: f32) {
        if self.scars.len() >= MOST_SCARS {
            self.scars.remove(0);
        }
        self.scars.push(Scar {
            at,
            radius,
            age: 0.,
        });
    }

    /// A boss's round has just left its gun. Returns how hard that shakes the
    /// camera, which is not at all for an ordinary shot.
    pub fn launched(&mut self, flight: &Flight) -> f32 {
        if self.is_worm(flight.owner) {
            self.worm_launched(flight.at, flight.vel, flight.kind);
            return 0.;
        }
        if self.is_axiom(flight.owner) {
            return self.star_launched(flight);
        }
        if self.is_railgunner(flight.owner) {
            return self.rail_launched(flight);
        }
        let look = self.look;
        let Some(presence) = self.bosses.get_mut(&flight.owner) else {
            return 0.;
        };
        let (color, fuel, body, hull) =
            (presence.color, presence.fuel, presence.pos, presence.radius);
        let heading = vec2(flight.vel.x, flight.vel.y).normalize_or_zero();
        // Firing shoves the hull back, more for a heavier round.
        let lethal = flight.damage >= LETHAL_DAMAGE;
        let cannon = flight.damage >= BASTION_ROUND_DAMAGE;
        let (shove, shake) = if lethal {
            (5.2, 2.2)
        } else if cannon {
            (3.2, 1.1)
        } else {
            ((flight.radius / 22.).clamp(0.12, 0.8), 0.)
        };
        if !look.calm {
            presence.kick_rate -= heading * shove * 60.;
            let most = presence.radius * MOST_KICK * KICK_RATE * E;
            if presence.kick_rate.length() > most {
                presence.kick_rate = presence.kick_rate.normalize_or_zero() * most;
            }
        }
        if !look.live() || flight.kind.is_drone() {
            return shake;
        }
        if lethal {
            let muzzle = body + V2::new(heading.x, heading.y) * hull * SIEGE_MUZZLE;
            self.cannon_blast(muzzle, flight.radius, heading);
            return shake;
        }
        if cannon {
            let muzzle = body + V2::new(heading.x, heading.y) * hull * BASTION_MUZZLE;
            self.slug_blast(muzzle, flight.radius, heading, color);
            return shake;
        }
        self.fueled_burst(
            flight.at,
            Flash::Muzzle(heading),
            fuel,
            flight.radius * 1.05,
            color,
            0.14,
        );
        if fuel == Fuel::Fire {
            self.plume(Plume {
                at: flight.at,
                scatter: flight.radius * 0.4,
                heading: Some((heading.y.atan2(heading.x), 0.45)),
                speed: (60., 340.),
                size: (flight.radius * 0.4, flight.radius * 0.9),
                life: (0.12, 0.3),
                count: look.many(8.),
                burn: fire::OPEN,
            });
        }
        0.
    }

    /// Bastion's cannon firing: a hard flash and a fan of steel sparks, with
    /// a little smoke and no flame.
    fn slug_blast(&mut self, at: V2, r: f32, heading: Vec2, color: Color) {
        let look = self.look;
        let angle = heading.y.atan2(heading.x);
        self.fueled_burst(
            at,
            Flash::Muzzle(heading),
            Fuel::Plasma,
            r * 1.5,
            color,
            0.24,
        );
        self.burst(at, Flash::Blast, r * 0.55, color, 0.26);
        for _ in 0..look.many(4.) {
            let spread = angle + (self.rand() - 0.5) * 2.;
            let size = r * self.between((0.5, 0.9));
            let life = self.between((0.5, 0.9));
            self.smoke(at, spread, (30., 100.), size, life);
        }
        self.spray(Spray {
            at,
            heading: Some((angle, 0.36)),
            count: look.many(26.),
            speed: (300., 900.),
            life: (0.18, 0.5),
            size: (1.4, 3.2),
            color: fire::steel(0.2),
        });
    }

    /// Bastion's round landing: a sharp burst and a smaller scorch.
    fn slug_impact(&mut self, at: V2, r: f32, color: Color) {
        self.fueled_burst(at, Flash::Blast, Fuel::Plasma, r * 1.35, color, 0.5);
        self.slug_debris(at, r);
    }

    /// What's left of Bastion's slug after it hits: steel sparks and a scorch.
    fn slug_debris(&mut self, at: V2, r: f32) {
        let look = self.look;
        self.spray(Spray {
            at,
            heading: None,
            count: look.many(28.),
            speed: (120., 620.),
            life: (0.25, 0.7),
            size: (1.3, 3.),
            color: fire::steel(0.25),
        });
        self.scar(at, r * 2.);
    }

    /// The Siege Engine firing: a jet of flame from the barrel, more blown
    /// sideways out of its muzzle brake, smoke, and a spray of embers.
    fn cannon_blast(&mut self, at: V2, r: f32, heading: Vec2) {
        let look = self.look;
        let angle = heading.y.atan2(heading.x);
        self.fueled_burst(at, Flash::Muzzle(heading), Fuel::Fire, r * 1.5, FIRE, 0.3);
        self.burst(at, Flash::Blast, r * 0.7, FIRE, 0.34);
        self.plume(Plume {
            at,
            scatter: r * 0.35,
            heading: Some((angle, 0.32)),
            speed: (240., 1240.),
            size: (r * 0.6, r * 1.6),
            life: (0.18, 0.6),
            count: look.many(64.),
            burn: fire::DENSE,
        });
        for side in [-1., 1.] {
            self.plume(Plume {
                at,
                scatter: r * 0.2,
                heading: Some((angle + side * 1.4, 0.28)),
                speed: (160., 620.),
                size: (r * 0.22, r * 0.6),
                life: (0.14, 0.36),
                count: look.many(14.),
                burn: fire::STREAM,
            });
        }
        for _ in 0..look.many(8.) {
            let spread = angle + (self.rand() - 0.5) * 2.4;
            let size = r * self.between((0.6, 1.1));
            let life = self.between((0.7, 1.3));
            self.smoke(at, spread, (30., 120.), size, life);
        }
        self.spray(Spray {
            at,
            heading: Some((angle, 0.42)),
            count: look.many(46.),
            speed: (320., 1050.),
            life: (0.25, 0.7),
            size: (1.8, 4.2),
            color: mix(FIRE, AMBER, 0.4),
        });
    }

    /// The Siege Engine's round going off where it landed.
    fn detonate(&mut self, at: V2, r: f32) {
        self.fueled_burst(at, Flash::Blast, Fuel::Fire, r * 1.5, FIRE, 0.72);
        self.fireball(at, r);
    }

    /// The Siege Engine's fire going up: flame, smoke, embers and a scorch.
    fn fireball(&mut self, at: V2, r: f32) {
        let look = self.look;
        self.plume(Plume {
            at,
            scatter: r * 0.5,
            heading: None,
            speed: (40., 560.),
            size: (r * 0.6, r * 1.7),
            life: (0.3, 0.95),
            count: look.many(96.),
            burn: fire::DENSE,
        });
        for _ in 0..look.many(9.) {
            let angle = self.rand() * TAU;
            let from = at + V2::from_angle(angle) * r * self.between((0.2, 1.));
            let size = r * self.between((0.8, 1.4));
            let life = self.between((0.9, 1.7));
            self.smoke(from, angle, (20., 90.), size, life);
        }
        self.spray(Spray {
            at,
            heading: None,
            count: look.many(44.),
            speed: (140., 720.),
            life: (0.3, 0.95),
            size: (1.5, 3.6),
            color: mix(FIRE, AMBER, 0.5),
        });
        self.scar(at, r * 2.6);
    }

    /// Note a boss's round at its place this frame.
    pub fn track(&mut self, flight: &Flight) {
        let look = self.look;
        // Nothing a worm throws burns, so nothing leaves a trail of light.
        if !look.live() || self.is_worm(flight.owner) {
            return;
        }
        if self.is_railgunner(flight.owner) {
            // A plasma stream is one ribbon, drawn through all its rounds.
            if tankyard_core::is_beam_round(flight.kind, flight.vel) {
                return;
            }
            self.rail_track(flight);
        }
        if !self.trails.contains_key(&flight.id) {
            if self.trails.len() < MOST_TRAILS
                && let Some(presence) = self.bosses.get(&flight.owner)
            {
                let heavy = if presence.fuel == Fuel::Star {
                    Self::star_heavy(flight.kind, flight.radius)
                } else {
                    flight.damage >= BASTION_ROUND_DAMAGE
                };
                let mut trail = Trail::new(
                    flight.at,
                    flight.radius,
                    presence.color,
                    presence.fuel,
                    heavy,
                );
                trail.seed = (flight.id % 97) as f32 * 0.37;
                self.trails.insert(flight.id, trail);
            }
            return;
        }
        let Some(trail) = self.trails.get_mut(&flight.id) else {
            return;
        };
        let from = trail.latest;
        trail.note(flight.at);
        let moved = from.distance(flight.at);
        let burns =
            trail.fuel == Fuel::Fire || (trail.fuel == Fuel::Star && Self::star_burns(flight.kind));
        if !burns || moved > 260. {
            return;
        }
        // Flame is left by distance flown, so it's as dense at any frame rate.
        let heavy = trail.heavy && trail.fuel == Fuel::Fire;
        let r = flight.radius;
        let spacing = (if heavy { r * 0.3 } else { r * 1.25 }).max(2.);
        trail.fed += moved;
        let due = (trail.fed / spacing) as usize;
        trail.fed -= due as f32 * spacing;
        let back = flight.vel.y.atan2(flight.vel.x) + PI;
        for step in 0..due.min(24) {
            let u = (step as f32 + 0.5) / due.min(24) as f32;
            let at = from.lerp(flight.at, u);
            let plume = if heavy {
                Plume {
                    at,
                    scatter: r * 0.66,
                    heading: Some((back, 0.45)),
                    speed: (30., 260.),
                    size: (r * 0.7, r * 1.55),
                    life: (0.2, 0.62),
                    count: (3. * look.level()).ceil() as usize,
                    burn: fire::STREAM,
                }
            } else {
                Plume {
                    at,
                    scatter: r * 0.4,
                    heading: Some((back, 0.6)),
                    speed: (20., 110.),
                    size: (r * 0.4, r * 0.8),
                    life: (0.12, 0.3),
                    count: 1,
                    burn: fire::OPEN,
                }
            };
            self.plume(plume);
        }
    }

    /// A boss's round is gone: hit something if it `struck`, or spent.
    pub fn landed(&mut self, flight: &Flight, struck: bool) {
        if self.is_worm(flight.owner) {
            self.trails.remove(&flight.id);
            return;
        }
        if self.is_railgunner(flight.owner) {
            self.trails.remove(&flight.id);
            self.rail_landed(flight, struck);
            return;
        }
        let trail = self.trails.remove(&flight.id);
        let look = self.look;
        if !look.live() {
            return;
        }
        if let Some(trail) = trail
            && self.wakes.len() < MOST_WAKES
        {
            self.wakes.push(Wake {
                trail,
                life: WAKE_SECONDS,
            });
        }
        let Some((color, fuel)) = self
            .bosses
            .get(&flight.owner)
            .map(|presence| (presence.color, presence.fuel))
            .or(trail.map(|trail| (trail.color, trail.fuel)))
        else {
            return;
        };
        if fuel == Fuel::Star || trail.is_some_and(|trail| trail.fuel == Fuel::Star) {
            self.star_landed(flight, struck, color);
            return;
        }
        let (at, radius, kind) = (flight.at, flight.radius, flight.kind);
        if flight.damage >= LETHAL_DAMAGE {
            self.hold_blast(flight.owner, at, radius, color, clash::Blast::Siege);
            return;
        }
        if flight.damage >= BASTION_ROUND_DAMAGE {
            self.hold_blast(flight.owner, at, radius, color, clash::Blast::Bastion);
            return;
        }
        if matches!(
            kind,
            ProjectileKind::Drone | ProjectileKind::Trap | ProjectileKind::Mine
        ) {
            self.burst(at, Flash::Fizzle, radius, color, 0.22);
            return;
        }
        if kind == ProjectileKind::Explosive {
            self.fueled_burst(at, Flash::Blast, fuel, radius * 1.7, color, 0.6);
            self.spray(Spray {
                at,
                heading: None,
                count: look.many(22.),
                speed: (120., 430.),
                life: (0.28, 0.75),
                size: (1.4, 3.),
                color: mix(color, AMBER, 0.5),
            });
            if fuel == Fuel::Fire {
                self.plume(Plume {
                    at,
                    scatter: radius * 0.5,
                    heading: None,
                    speed: (30., 300.),
                    size: (radius * 0.4, radius * 1.),
                    life: (0.22, 0.55),
                    count: look.many(22.),
                    burn: fire::STREAM,
                });
                self.scar(at, radius * 2.2);
            }
            return;
        }
        if !struck {
            self.burst(at, Flash::Fizzle, radius, color, 0.2);
            return;
        }
        self.fueled_burst(at, Flash::Hit, fuel, radius, color, 0.3);
        self.spray(Spray {
            at,
            heading: None,
            count: look.many(4.),
            speed: (70., 250.),
            life: (0.16, 0.38),
            size: (1., 2.1),
            color: mix(color, WHITE, 0.2),
        });
    }

    /// A boss has been destroyed.
    pub fn defeated(&mut self, id: EntityId, at: V2) {
        let look = self.look;
        if !look.live() {
            return;
        }
        if let Some(presence) = self.bosses.get(&id)
            && presence.fuel == Fuel::Ion
        {
            let (pos, aim, radius) = (presence.pos, presence.aim, presence.radius);
            self.bosses.remove(&id);
            self.rail_defeated(pos, aim, radius);
            return;
        }
        let (at, radius, color) = self
            .bosses
            .remove(&id)
            .map_or((at, 90., Color::new(1., 0.6, 0.3, 1.)), |presence| {
                (presence.pos, presence.radius, presence.color)
            });
        self.burst(at, Flash::Fall, radius, color, 1.7);
        for (count, speed, life, size, tint) in [
            (64., (180., 760.), (0.5, 1.45), (1.6, 4.6), 0.25),
            (40., (60., 240.), (0.9, 2.1), (1.2, 2.8), 0.),
        ] {
            self.spray(Spray {
                at,
                heading: None,
                count: look.many(count),
                speed,
                life,
                size,
                color: mix(mix(color, AMBER, 0.45), WHITE, tint),
            });
        }
        // The wreck goes up in flame.
        self.plume(Plume {
            at,
            scatter: radius * 0.7,
            heading: None,
            speed: (60., 640.),
            size: (radius * 0.25, radius * 0.62),
            life: (0.45, 1.25),
            count: look.many(120.),
            burn: fire::DENSE,
        });
        for _ in 0..look.many(9.) {
            let angle = self.rand() * TAU;
            let start = self.between((0.1, 0.8));
            let size = radius * self.between((0.5, 0.95));
            let life = self.between((1.3, 2.3));
            self.smoke(
                at + V2::from_angle(angle) * radius * start,
                angle,
                (14., 58.),
                size,
                life,
            );
        }
        self.scar(at, radius * 1.9);
    }

    /// Run sparks, flashes and trails on by `dt`.
    pub fn update(&mut self, dt: f32) {
        // Strikes keep to the host's clock even while the sparks hang still.
        self.run_clashes(dt);
        let dt = self.held(dt);
        self.air = (self.air + dt) % 3600.;
        self.worm_update(dt);
        let (air, camera) = (self.air, self.camera);
        for burst in &mut self.bursts {
            burst.age += dt;
            burst.at += burst.vel * dt;
        }
        self.bursts.retain(|burst| burst.age < burst.life);
        for spark in &mut self.sparks {
            spark.life -= dt;
            spark.pos += spark.vel * dt;
            spark.vel *= (-spark.drag * dt).exp();
        }
        self.sparks.retain(|spark| spark.life > 0.);
        self.run_flames(dt);
        for scar in &mut self.scars {
            scar.age += dt;
        }
        self.scars.retain(|scar| scar.age < SCAR_SECONDS);
        for wake in &mut self.wakes {
            wake.life -= dt;
            wake.trail.clock += dt;
            wake.trail.billow(dt, air, camera);
        }
        self.wakes.retain(|wake| wake.life > 0.);
        self.run_casts(dt);
        self.run_rail(dt);
        // A round gone without a word, as across a resync, takes its trail.
        self.trails.retain(|_, trail| {
            trail.clock += dt;
            trail.billow(dt, air, camera);
            std::mem::take(&mut trail.seen)
        });
    }

    /// Light this frame's bosses and blasts throw onto a dark arena.
    pub fn lights(&self, view: &View, out: &mut Vec<Light>) {
        let z = view.zoom;
        for shown in self.bosses.values() {
            if shown.unseen > 0. || shown.charge <= 0.01 {
                continue;
            }
            crate::lighting::gather(
                out,
                Light::Spot {
                    at: view.screen(shown.pos),
                    radius: (shown.radius * 3.4 + 160.) * z * (0.7 + 0.5 * shown.charge),
                    color: alpha(mix(shown.color, WHITE, 0.25), 0.75 * shown.charge),
                },
            );
        }
        if self.stars {
            self.axiom_lights(view, out);
        }
        if !self.look.live() {
            return;
        }
        self.rail_world_lights(view, out);
        self.clash_lights(view, out);
        for trail in self.trails.values() {
            if trail.heavy && view.visible(trail.latest, 420.) {
                let (reach, tint) = match trail.fuel {
                    Fuel::Fire => (420., alpha(ramp(0.28), 0.95)),
                    Fuel::Star => (380., alpha(mix(AXIOM_GOLD, WHITE, 0.3), 0.9)),
                    Fuel::Ion => (360., alpha(railgun::ion(0.42), 0.9)),
                    _ => (300., alpha(fire::steel(0.35), 0.8)),
                };
                crate::lighting::gather(
                    out,
                    Light::Spot {
                        at: view.screen(trail.latest),
                        radius: reach * z,
                        color: tint,
                    },
                );
            }
        }
        let level = self.look.level().min(1.);
        for burst in &self.bursts {
            let t = (burst.age / burst.life).clamp(0., 1.);
            let (reach, strength) = match burst.flash {
                Flash::Muzzle(_) => (burst.size * 9. + 60., (1. - t) * 0.8),
                Flash::Hit => (burst.size * 7. + 40., (1. - t).powi(2) * 0.7),
                Flash::Blast => (burst.size * 9. + 90., (1. - t).powi(2)),
                Flash::Bloom => (burst.size * 6. + 60., (1. - t).powi(2) * 0.8),
                Flash::Release => (burst.size * 3.6 + 110., (1. - t).powi(2) * 0.9),
                Flash::Phase => (burst.size * 4.6 + 160., (1. - t).powi(2)),
                Flash::Fall => (burst.size * 7. + 260., (1. - t).powi(2)),
                Flash::Arrival => (burst.size * 3. + 120., (t * PI).sin() * 0.8),
                Flash::Fizzle | Flash::Smoke => continue,
            };
            if view.visible(burst.at, reach) {
                crate::lighting::gather(
                    out,
                    Light::Spot {
                        at: view.screen(burst.at),
                        radius: reach * z,
                        color: alpha(mix(burst.color, WHITE, 0.3), strength * level),
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boss(id: EntityId) -> Boss {
        let mut world = tankyard_core::World::new(
            tankyard_core::WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        world.add_player("Pilot", false);
        assert_ne!(world.spawn_boss(BossKind::Sentinel), 0);
        let mut boss = world.bosses.remove(0);
        boss.id = id;
        boss.arrival = 0.;
        boss
    }
    fn sync(fx: &mut BossFx, boss: &Boss, dt: f32) {
        fx.sync(
            std::iter::once(Shown {
                tick: 0,
                body: None,
                fraction: 1.,
                boss,
                pos: boss.pos,
                aim: boss.aim,
                color: WHITE,
            }),
            dt,
        );
    }
    fn flight(id: EntityId, owner: EntityId, at: V2, damage: f32) -> Flight {
        Flight {
            id,
            owner,
            at,
            vel: V2::new(300., 0.),
            radius: 12.,
            kind: ProjectileKind::Shell,
            damage,
        }
    }

    #[test]
    fn the_drawn_aim_settles_the_same_at_any_frame_rate_and_across_the_wrap() {
        let mut results = Vec::new();
        for frames in [30, 60, 144, 240] {
            let mut fx = BossFx::default();
            let mut boss = boss(7);
            boss.aim = 3.1;
            sync(&mut fx, &boss, 1. / frames as f32);
            boss.aim = -3.1;
            for _ in 0..frames {
                sync(&mut fx, &boss, 1. / frames as f32);
            }
            results.push(fx.aim(&boss));
        }
        for aim in results {
            assert!(turn_between(aim, -3.1).abs() < 0.01, "{aim}");
        }
    }

    #[test]
    fn a_steady_turn_is_followed_closely_and_never_overshoots_a_stop() {
        let mut fx = BossFx::default();
        let mut boss = boss(7);
        let dt = 1. / 120.;
        boss.aim = 0.;
        sync(&mut fx, &boss, dt);
        for _ in 0..120 {
            boss.aim += 2.8 * dt;
            sync(&mut fx, &boss, dt);
        }
        assert!(
            turn_between(fx.aim(&boss), boss.aim).abs() < 0.03,
            "follows a full-speed turn: {} against {}",
            fx.aim(&boss),
            boss.aim
        );
        let target = boss.aim;
        let mut furthest = 0_f32;
        for _ in 0..120 {
            sync(&mut fx, &boss, dt);
            furthest = furthest.max(turn_between(target, fx.aim(&boss)));
        }
        assert!(furthest < 0.2, "eases to a stop: {furthest}");
        assert!(turn_between(fx.aim(&boss), target).abs() < 0.01);
    }

    #[test]
    fn reduced_motion_shows_the_real_aim_and_no_decoration() {
        let mut fx = BossFx::default();
        fx.set_look(Look {
            strength: 1.,
            calm: true,
        });
        let mut boss = boss(7);
        sync(&mut fx, &boss, 1. / 60.);
        boss.aim = 2.;
        boss.phase = 3;
        sync(&mut fx, &boss, 1. / 60.);
        assert_eq!(fx.aim(&boss), 2.);
        let shot = flight(99, 7, boss.pos, 20.);
        fx.launched(&shot);
        fx.track(&shot);
        fx.landed(&shot, true);
        fx.defeated(7, boss.pos);
        assert!(fx.bursts.is_empty() && fx.sparks.is_empty() && fx.trails.is_empty());
        assert!(fx.flames.is_empty() && fx.scars.is_empty());
    }

    #[test]
    fn a_phase_change_and_a_defeat_each_flash_once() {
        let mut fx = BossFx::default();
        let mut boss = boss(7);
        sync(&mut fx, &boss, 1. / 60.);
        boss.phase = 2;
        sync(&mut fx, &boss, 1. / 60.);
        sync(&mut fx, &boss, 1. / 60.);
        let phases = fx
            .bursts
            .iter()
            .filter(|burst| burst.flash == Flash::Phase)
            .count();
        assert_eq!(phases, 1);
        fx.defeated(7, boss.pos);
        fx.defeated(7, boss.pos);
        assert!(fx.bursts.iter().any(|burst| burst.flash == Flash::Fall));
        assert!(!fx.is_boss(7));
    }

    #[test]
    fn trails_follow_rounds_and_drop_when_they_jump_or_vanish() {
        let mut trail = Trail::new(V2::new(0., 0.), 8., WHITE, Fuel::Plasma, false);
        for step in 1..40 {
            trail.note(V2::new(step as f32 * 7., 0.));
        }
        assert_eq!(usize::from(trail.count), TRAIL_POINTS);
        assert!(trail.point(0).x > trail.point(TRAIL_POINTS - 1).x);
        // Through a rift: no streak across the arena.
        trail.note(V2::new(3000., 900.));
        assert_eq!(trail.count, 1);
        assert_eq!(trail.point(0), V2::new(3000., 900.));

        let mut fx = BossFx::default();
        let boss = boss(7);
        sync(&mut fx, &boss, 1. / 60.);
        fx.track(&flight(50, 7, V2::new(1., 1.), 8.));
        assert!(fx.trails.contains_key(&50));
        fx.update(1. / 60.);
        assert!(fx.trails.contains_key(&50), "seen this frame");
        fx.update(1. / 60.);
        assert!(
            !fx.trails.contains_key(&50),
            "unseen rounds drop their trail"
        );
    }

    #[test]
    fn only_boss_rounds_are_tracked_and_everything_stays_bounded() {
        let mut fx = BossFx::default();
        fx.track(&flight(1, 12345, V2::ZERO, 5.));
        assert!(fx.trails.is_empty());
        let boss = boss(7);
        sync(&mut fx, &boss, 1. / 60.);
        for id in 0..(MOST_TRAILS as u64 * 2) {
            fx.track(&flight(id + 100, 7, V2::ZERO, 5.));
        }
        assert!(fx.trails.len() <= MOST_TRAILS);
        for id in 0..2000 {
            let mut shot = flight(id + 100, 7, V2::ZERO, 5.);
            shot.kind = ProjectileKind::Explosive;
            fx.landed(&shot, true);
            fx.landed(&flight(id + 5000, 7, V2::ZERO, 1e7), true);
            fx.launched(&flight(id + 9000, 7, V2::ZERO, 1e7));
        }
        assert!(fx.sparks.len() <= MOST_SPARKS);
        assert!(fx.bursts.len() <= MOST_BURSTS);
        assert!(fx.wakes.len() <= MOST_WAKES);
        assert!(fx.flames.len() <= fire::MOST_PUFFS);
        assert!(fx.scars.len() <= MOST_SCARS);
        for _ in 0..300 {
            fx.update(0.05);
        }
        assert!(fx.sparks.is_empty() && fx.bursts.is_empty() && fx.wakes.is_empty());
        assert!(fx.flames.is_empty() && fx.scars.is_empty());
    }

    #[test]
    fn firing_shoves_the_hull_a_little_and_it_settles_back() {
        let mut fx = BossFx::default();
        let boss = boss(7);
        sync(&mut fx, &boss, 1. / 60.);
        let mut shot = flight(8, 7, boss.pos, 1e7);
        shot.radius = 24.;
        assert!(fx.launched(&shot) > 0., "the heavy round shakes the camera");
        let mut furthest = 0_f32;
        for _ in 0..120 {
            sync(&mut fx, &boss, 1. / 60.);
            let kick = fx.kick(&boss, 1.);
            assert!(kick.x <= 0.001, "pushed away from the shot");
            furthest = furthest.max(kick.length());
        }
        assert!(furthest > 0.5 && furthest <= boss.radius * MOST_KICK * 1.05);
        assert!(fx.kick(&boss, 1.).length() < 0.02);
    }

    #[test]
    fn bastions_cannon_kicks_less_than_the_siege_gun_and_leaves_a_mark() {
        let mut fx = BossFx::default();
        let boss = boss(7);
        sync(&mut fx, &boss, 1. / 60.);
        let plain = fx.launched(&flight(1, 7, boss.pos, 40.));
        let cannon = flight(2, 7, boss.pos, BASTION_ROUND_DAMAGE);
        let kick = fx.launched(&cannon);
        let siege = fx.launched(&flight(3, 7, boss.pos, LETHAL_DAMAGE));
        assert_eq!(plain, 0.);
        assert!(kick > 0. && kick < siege, "{kick} {siege}");
        fx.track(&cannon);
        assert!(fx.trails[&2].heavy);
        let scars = fx.scars.len();
        fx.landed(&cannon, true);
        // A heavy blast waits a moment in case a parry took the round.
        assert_eq!(fx.scars.len(), scars);
        fx.update(1. / 60.);
        fx.update(1. / 60.);
        assert_eq!(fx.scars.len(), scars + 1);
    }

    #[test]
    fn the_heavy_round_burns_all_the_way_and_leaves_a_scorch_where_it_lands() {
        let mut fx = BossFx::default();
        let mut owner = boss(7);
        owner.kind = BossKind::SiegeEngine;
        sync(&mut fx, &owner, 1. / 60.);
        let mut shot = flight(8, 7, V2::new(0., 0.), 2e6);
        shot.radius = 24.;
        shot.vel = V2::new(1500., 0.);
        fx.launched(&shot);
        fx.track(&shot);
        fx.update(1. / 60.);
        let at_launch = fx.flames.len();
        assert!(at_launch > 30, "the muzzle throws fire: {at_launch}");
        // Flame is laid by distance, so the frame rate doesn't thin it out.
        let count_after = |frames: usize| {
            let mut fx = BossFx::default();
            sync(&mut fx, &owner, 1. / 60.);
            let mut shot = shot;
            shot.at = V2::ZERO;
            fx.track(&shot);
            for frame in 1..=frames {
                shot.at = V2::new(480. * frame as f32 / frames as f32, 0.);
                fx.track(&shot);
            }
            fx.flames.len()
        };
        let (slow, fast) = (count_after(8), count_after(32));
        assert!(slow.abs_diff(fast) <= 6, "{slow} at 30 fps, {fast} at 120");
        fx.landed(&shot, true);
        fx.update(1. / 60.);
        fx.update(1. / 60.);
        assert_eq!(fx.scars.len(), 1);
        assert!(fx.flames.len() > at_launch);
    }

    #[test]
    fn every_boss_has_fuel_and_the_siege_engine_burns() {
        for kind in BossKind::ALL {
            let _ = Fuel::of(kind);
        }
        assert_eq!(Fuel::of(BossKind::SiegeEngine), Fuel::Fire);
        assert_eq!(Fuel::of(BossKind::Tempest), Fuel::Arc);
    }

    #[test]
    fn the_pale_health_stretch_holds_then_catches_up_even_under_steady_fire() {
        let dt = 1. / 60.;
        // A hit from full holds the old health up for a moment.
        let mut state = follow_trail((1., 0.), 1., 0.8, dt, false);
        assert_eq!(state.0, 1.);
        for _ in 0..20 {
            state = follow_trail(state, 0.8, 0.8, dt, false);
        }
        assert_eq!(state.0, 1., "still holding");
        for _ in 0..120 {
            state = follow_trail(state, 0.8, 0.8, dt, false);
        }
        assert_eq!(state.0, 0.8, "caught up");
        // Under a stream of small hits it stays a short way behind.
        let (mut health, mut state) = (1_f32, (1_f32, 0_f32));
        for _ in 0..600 {
            let next = health - 0.001;
            state = follow_trail(state, health, next, dt, false);
            health = next;
        }
        assert!(state.0 - health < 0.06, "{}", state.0 - health);
        // Healing past it, or reduced motion, leaves no stretch.
        assert_eq!(follow_trail((0.5, 0.2), 0.4, 0.7, dt, false), (0.7, 0.));
        assert_eq!(follow_trail((1., 0.), 1., 0.6, dt, true), (0.6, 0.));
    }
}
