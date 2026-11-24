//! The Sunderworm as the arena sees it: the body laid along the path its head
//! takes, the floor it breaks coming up and going under, the ridge it pushes
//! up while it tunnels, and what comes out of it when it is hurt or torn.
//!
//! Snapshots carry the authoritative segment positions. Drawing interpolates
//! them between snapshots, including immediately after joining or reconnecting.
//! A local path is retained for environmental effects and preview models.
//!
//! Its warnings draw with effects turned off, as every boss's do. Debris,
//! dust and gore are decoration and follow the Effects setting.
use super::{BossFx, Look, Shown};
use crate::emission::{Emission, Layer};
use crate::map_sounds::BossCue;
use crate::render::View;
use crate::ui::{RED, alpha, mix};
use macroquad::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::f32::consts::{PI, TAU};
use tankyard_client::tank_model::{OUTLINE, Primitive};
use tankyard_client::worm_model::{self, BILE, BLOOD, FLESH, RAW, Segment, WormPose};
use tankyard_content::ProjectileKind;
use tankyard_core::sunderworm::{
    ERUPT_RADIUS, Ring, WORM_RINGS, WORM_SPACING, WORM_TEAR, WormAct, WormPart, WormPath, WormSign,
    lay_rings, ring_radius,
};
use tankyard_core::{Boss, EntityId, V2};

/// Most of each kind of debris at once, so a long fight can't slow the frame.
const MOST_CHUNKS: usize = 240;
const MOST_GORE: usize = 360;
const MOST_JETS: usize = 32;
const MOST_DUST: usize = 140;
const MOST_STAINS: usize = 110;
const MOST_HOLES: usize = 20;
const MOST_FURROW: usize = 420;
/// How hard things fall, in units a second each second, as the eye reads it
/// from above.
const GRAVITY: f32 = 2100.;
/// How long a stain or a crater stays on the floor.
const STAIN_SECONDS: f32 = 40.;
const HOLE_SECONDS: f32 = 22.;
const FURROW_SECONDS: f32 = 9.;
/// How far a camera can be from it and still be shaken by it.
const QUAKE_REACH: f32 = 3000.;
/// The longest the strike lane is drawn.
const LANE: f32 = 640.;

/// A piece of floor or a clod of earth thrown into the air.
#[derive(Clone, Copy, Debug)]
struct Chunk {
    pos: V2,
    vel: V2,
    /// Height above the floor, and how fast that is changing.
    height: f32,
    rise: f32,
    angle: f32,
    spin: f32,
    size: f32,
    /// Floor tile or earth.
    earth: bool,
    bounces: u8,
    age: f32,
    life: f32,
}

/// Blood or flesh in the air.
#[derive(Clone, Copy, Debug)]
struct Gore {
    pos: V2,
    vel: V2,
    height: f32,
    rise: f32,
    size: f32,
    angle: f32,
    spin: f32,
    /// A lump of flesh rather than a drop.
    flesh: bool,
    /// Bile rather than blood.
    bile: bool,
    age: f32,
}

/// The short, coherent part of a gush, before it separates into drops.
#[derive(Clone, Copy, Debug)]
struct Jet {
    pos: V2,
    vel: V2,
    rise: f32,
    width: f32,
    age: f32,
    life: f32,
}

/// A soft cloud of dust or grit.
#[derive(Clone, Copy, Debug)]
struct Dust {
    pos: V2,
    vel: V2,
    size: f32,
    grow: f32,
    age: f32,
    life: f32,
    shade: f32,
}

/// A mark on the floor: blood, or bile.
#[derive(Clone, Copy, Debug)]
struct Stain {
    pos: V2,
    size: f32,
    /// Which way it splashed, for its streaks.
    heading: f32,
    seed: u32,
    bile: bool,
    age: f32,
}

/// Floor broken open where it came up or went under.
#[derive(Clone, Copy, Debug)]
struct Hole {
    pos: V2,
    size: f32,
    seed: u32,
    age: f32,
}

/// Ground cracking over where it is about to come up.
#[derive(Clone, Copy, Debug)]
struct Cracking {
    worm: EntityId,
    pos: V2,
    size: f32,
    seed: u32,
    /// How far the cracks have spread, 0 to 1.
    spread: f32,
    /// Whether it is still spreading; once it breaks open it fades.
    live: bool,
    fade: f32,
}

/// What the renderer keeps about one worm between frames.
#[derive(Clone, Debug)]
pub(super) struct WormShown {
    part: WormPart,
    path: WormPath,
    rings: Vec<Ring>,
    body_previous: Vec<(V2, f32)>,
    body_current: Vec<(V2, f32)>,
    body_tick: Option<u64>,
    sign: WormSign,
    depth: f32,
    jaw: f32,
    jaw_speed: f32,
    rear: f32,
    bile: f32,
    hurt: f32,
    blink: f32,
    next_blink: f32,
    health: f32,
    max_health: f32,
    pos: V2,
    aim: f32,
    unseen: f32,
    /// Part-made drops and puffs, so rates hold at any frame rate.
    drips: f32,
    puffs: f32,
    /// Where it last left a furrow point while tunnelling.
    furrow_at: V2,
    /// Seconds to the next rumble while it is under the floor.
    rumble: f32,
    dead: bool,
}

impl WormShown {
    fn new(boss: &Boss, pos: V2, aim: f32) -> Self {
        let part = boss.worm_part().unwrap_or_default();
        let sign = WormSign::of(boss);
        let path = WormPath::straight(pos, aim, sign.depth, part.length());
        let mut shown = Self {
            part,
            path,
            rings: Vec::with_capacity(WORM_RINGS),
            body_previous: Vec::new(),
            body_current: Vec::new(),
            body_tick: None,
            sign,
            depth: sign.depth,
            jaw: 0.,
            jaw_speed: 0.,
            rear: 0.,
            bile: 0.,
            hurt: 0.,
            blink: 0.,
            next_blink: 2.,
            health: boss.health,
            max_health: boss.max_health,
            pos,
            aim,
            unseen: 0.,
            drips: 0.,
            puffs: 0.,
            furrow_at: pos,
            rumble: 0.,
            dead: false,
        };
        shown.lay();
        shown
    }
    fn lay(&mut self) {
        let mut rings = std::mem::take(&mut self.rings);
        lay_rings(&self.path, self.part, &mut rings);
        self.rings = rings;
    }
    fn authoritative_body(
        &mut self,
        body: &tankyard_core::sunderworm::WormBody,
        fraction: f32,
        tick: u64,
    ) {
        if self.body_tick != Some(tick) {
            self.body_tick = Some(tick);
            std::mem::swap(&mut self.body_previous, &mut self.body_current);
            self.body_current.clone_from(&body.centers);
            if self.body_previous.len() != self.body_current.len() {
                self.body_previous.clone_from(&self.body_current);
            }
        }
        self.rings.clear();
        for (k, (&(before, was_depth), &(now, depth))) in self
            .body_previous
            .iter()
            .zip(&self.body_current)
            .enumerate()
        {
            self.rings.push(Ring {
                pos: before.lerp(now, fraction.clamp(0., 1.)),
                depth: was_depth + (depth - was_depth) * fraction.clamp(0., 1.),
                radius: ring_radius(self.part.rings().start + k),
            });
        }
    }

    /// The number of its first ring along the whole worm.
    fn first(&self) -> usize {
        self.part.rings().start
    }
    /// A circle around all of it, for culling.
    fn reach(&self) -> (V2, f32) {
        // The body may already be curved when a player joins. Its actual
        // rings, rather than a newly initialized head trail, bound the draw.
        let (mut lo, mut hi) = (self.pos, self.pos);
        for ring in &self.rings {
            let r = V2::new(ring.radius * 1.6, ring.radius * 1.6);
            lo.x = lo.x.min(ring.pos.x - r.x);
            lo.y = lo.y.min(ring.pos.y - r.y);
            hi.x = hi.x.max(ring.pos.x + r.x);
            hi.y = hi.y.max(ring.pos.y + r.y);
        }
        let middle = (lo + hi) * 0.5;
        (middle, middle.distance(hi))
    }
    /// How fierce it has become, 0 to 1, from its real phase.
    fn rage(&self) -> f32 {
        f32::from(self.sign.phase.clamp(1, 7) - 1) / 6.
    }
}

/// Everything the worms are doing to the arena.
#[derive(Default)]
pub(super) struct Worms {
    shown: HashMap<EntityId, WormShown>,
    chunks: Vec<Chunk>,
    gore: Vec<Gore>,
    jets: Vec<Jet>,
    dust: Vec<Dust>,
    stains: Vec<Stain>,
    holes: Vec<Hole>,
    cracks: Vec<Cracking>,
    furrow: VecDeque<(V2, f32, f32)>,
    /// A shove to the camera that dies away, in pixels.
    quake: f32,
    /// The steady shaking, in pixels, easing toward what the worms asked
    /// for this frame.
    tremor: f32,
    tremor_goal: f32,
    random: u32,
}

impl Worms {
    fn rand(&mut self) -> f32 {
        if self.random == 0 {
            self.random = 0x2545_F491;
        }
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random as f32 / u32::MAX as f32
    }
    fn between(&mut self, (low, high): (f32, f32)) -> f32 {
        low + (high - low) * self.rand()
    }
    fn seed(&mut self) -> u32 {
        (self.rand() * u32::MAX as f32) as u32
    }
}

/// Moves `value` toward `goal` like a stiff, well-damped spring.
fn spring(value: &mut f32, speed: &mut f32, goal: f32, stiffness: f32, dt: f32) {
    // Exact critically damped response: a delayed frame cannot fling its jaw
    // past the stop or change the feel of a bite at a different frame rate.
    let rate = stiffness.sqrt();
    let offset = *value - goal;
    let travel = *speed + offset * rate;
    let decay = (-rate * dt).exp();
    *value = goal + (offset + travel * dt) * decay;
    *speed = (*speed - rate * travel * dt) * decay;
}

fn ease(value: f32, goal: f32, rate: f32, dt: f32) -> f32 {
    value + (goal - value) * (1. - (-rate * dt).exp())
}

/// How far `point` is from the camera, as a share of how far a quake carries.
fn nearness(camera: V2, point: V2) -> f32 {
    (1. - camera.distance(point) / QUAKE_REACH).clamp(0., 1.)
}

/// What its mouth, its rearing and its throat should be doing for `sign`.
fn pose_goal(sign: WormSign, time: f32) -> (f32, f32, f32) {
    let p = sign.progress;
    match (sign.act, sign.stage) {
        (WormAct::Rise, 0) | (WormAct::Tunnel, _) | (WormAct::Erupt, 0) => (0., 0., 0.),
        (WormAct::Rise, _) => (1.05, 0.8, 0.),
        (WormAct::Roar, _) => (1.07 + 0.035 * (time * 13.).sin(), 0.85 * (1. - p * 0.6), 0.),
        (WormAct::Tear, _) => (1.04 + 0.04 * (time * 11.).sin(), 0.55, 0.),
        (WormAct::Lunge, 0) => (0.3 + 0.7 * p, p, 0.),
        // The windup opens the jaws; release snaps them shut with the bite.
        (WormAct::Lunge, 1) => (0.08, 0.15, 0.),
        (WormAct::Lunge, _) => (0.35 * (1. - p), 0., 0.),
        (WormAct::Spit, 0) => (0.2 + 0.6 * p, 0.25 * p, 0.),
        (WormAct::Spit, 1) => (0.9 + 0.1 * (time * 18.).sin(), 0.2, 0.),
        (WormAct::Bile, 0) => (0.3 + 0.5 * p, 0.3 * p, p),
        (WormAct::Bile, 1) => (0.88, 0.25, 1.),
        (WormAct::Erupt, 1) => (1.08, 0.7, 0.),
        (WormAct::Erupt, _) => (0.5 * (1. - p), 0.3 * (1. - p), 0.),
        (WormAct::Thrash, _) => (0.6 + 0.15 * (time * 9.).sin(), 0.1, 0.),
        (WormAct::Brood, _) => (0.3, 0., 0.),
        (WormAct::Dive, _) => (0.12, 0., 0.),
        _ => (0.14 + 0.08 * (time * 1.3).sin(), 0., 0.),
    }
}

impl BossFx {
    pub(super) fn worm_quiet(&mut self) {
        self.worms.gore.clear();
        self.worms.jets.clear();
        self.worms.dust.clear();
        self.worms.chunks.clear();
        self.worms.stains.clear();
        self.worms.furrow.clear();
    }

    /// Whether `id` is a Sunderworm or half of one seen lately.
    pub fn is_worm(&self, id: EntityId) -> bool {
        self.worms.shown.contains_key(&id)
    }

    /// Where a worm's head is drawn.
    pub fn worm_head(&self, id: EntityId) -> Option<V2> {
        self.worms
            .shown
            .get(&id)
            .and_then(|shown| shown.rings.first().map(|ring| ring.pos))
    }

    /// Follow a worm in this frame's snapshot. Run for every worm every
    /// frame, effects or not: its body and warnings depend on it.
    pub(super) fn worm_frame(&mut self, item: &Shown<'_>, dt: f32) {
        let boss = item.boss;
        let look = self.look;
        let camera = self.camera;
        let time = self.air;
        let id = boss.id;
        let part = boss.worm_part().unwrap_or_default();
        let sign = WormSign::of(boss);
        let mut adopted = None;
        let fresh = !self.worms.shown.contains_key(&id);
        if fresh {
            // A tail torn off a worm already here carries on along the rest
            // of that worm's path.
            let mut shown = WormShown::new(boss, item.pos, item.aim);
            if part == WormPart::Back {
                let arc = WORM_TEAR as f32 * WORM_SPACING;
                if let Some(front) = self.worms.shown.values().find(|other| {
                    other.part != WormPart::Back && other.path.at(arc).0.distance(item.pos) < 260.
                }) {
                    shown.path = front.path.behind(arc);
                    shown.lay();
                    adopted = Some(front.path.at(arc - WORM_SPACING * 0.5).0);
                }
            }
            self.worms.shown.insert(id, shown);
        }
        let worms = &mut self.worms;
        let shown = worms.shown.get_mut(&id).expect("inserted above");
        let was = shown.sign;
        let was_part = shown.part;
        shown.unseen = 0.;
        shown.sign = sign;
        shown.part = part;
        shown.max_health = boss.max_health;
        // Its head drops or rises smoothly between snapshots.
        shown.depth = if look.calm {
            sign.depth
        } else {
            ease(shown.depth, sign.depth, 14., dt)
        };
        // Moved a long way at once: a map change, not motion.
        if shown.pos.distance(item.pos) > 600. {
            shown.path = WormPath::straight(item.pos, item.aim, shown.depth, part.length());
        }
        shown.pos = item.pos;
        shown.aim = item.aim;
        shown
            .path
            .follow(item.pos, shown.depth, part.length() + WORM_SPACING * 2.);
        if let Some(body) = item.body {
            shown.authoritative_body(body, item.fraction, item.tick);
        } else {
            shown.lay();
        }
        let (jaw, rear, bile) = pose_goal(sign, time);
        let snap = if matches!((sign.act, sign.stage), (WormAct::Lunge, 1)) {
            900.
        } else {
            260.
        };
        spring(&mut shown.jaw, &mut shown.jaw_speed, jaw, snap, dt);
        shown.jaw = shown.jaw.clamp(0., 1.2);
        shown.rear = ease(shown.rear, rear, 9., dt);
        shown.bile = ease(shown.bile, bile, 6., dt);
        if boss.health < shown.health && boss.max_health > 0. {
            let share = (shown.health - boss.health) / boss.max_health;
            shown.hurt = (shown.hurt + 0.12 + share * 60.).min(1.);
        }
        shown.health = boss.health;
        shown.hurt *= (-dt * 7.).exp();
        shown.next_blink -= dt;
        if shown.next_blink <= 0. {
            shown.blink = 1.;
            shown.next_blink = 1.6 + (time * 7.13 + id as f32).sin().abs() * 3.4;
        }
        shown.blink = (shown.blink - dt * 7.).max(0.);
        let changed = fresh || sign.act != was.act || sign.stage != was.stage;
        let head = shown.rings.first().map_or(item.pos, |ring| ring.pos);
        let heading = V2::from_angle(item.aim);
        let rage = shown.rage();
        let up = shown
            .rings
            .iter()
            .filter(|ring| ring.surfaced())
            .map(|ring| (ring.pos, ring.radius))
            .collect::<Vec<_>>();
        let torn_end = match part {
            WormPart::Front => shown.rings.last().map(|ring| (ring.pos, ring.radius, -1.)),
            WormPart::Back => shown.rings.first().map(|ring| (ring.pos, ring.radius, 1.)),
            WormPart::Whole => None,
        };
        let jaw_now = shown.jaw;
        let tunnelling = shown.depth > 0.55;
        let mut furrow_point = None;
        if tunnelling && shown.furrow_at.distance(item.pos) > 34. {
            shown.furrow_at = item.pos;
            furrow_point = Some(item.pos);
        }
        shown.rumble -= dt;
        let rumble_due = tunnelling && shown.rumble <= 0.;
        if rumble_due {
            shown.rumble = 1.25;
        }
        // Steady drips: drool from an open mouth, blood from a torn end
        // and, late in the fight, from every pore.
        let mut drips = 0;
        let mut puffs = 0;
        if look.live() {
            shown.drips += dt
                * look.level()
                * (if part != WormPart::Back {
                    2.5 * jaw_now
                } else {
                    0.
                } + if torn_end.is_some() { 16. } else { 0. }
                    + 7. * (rage - 0.4).max(0.));
            shown.puffs += dt * look.level() * if tunnelling { 26. } else { 0. };
            drips = shown.drips as usize;
            puffs = shown.puffs as usize;
            shown.drips -= drips as f32;
            shown.puffs -= puffs as f32;
        }
        let first_seen_rising = changed && sign.act == WormAct::Rise && sign.stage == 0;
        // The tear, seen from the front half: it was whole a moment ago.
        let tore = !fresh && was_part == WormPart::Whole && part == WormPart::Front;
        let cut = tore.then(|| shown.rings.last().map_or(shown.pos, |ring| ring.pos));
        // Steady shaking from what it is doing under the floor.
        let near = nearness(camera, item.pos);
        let tremor = match (sign.act, sign.stage) {
            (WormAct::Rise, 0) => 3. + 10. * sign.progress,
            (WormAct::Erupt, 0) => 2. + 6. * sign.progress,
            (WormAct::Tunnel | WormAct::Dive, _) => 4. * near,
            (WormAct::Roar | WormAct::Tear, _) => 2.5,
            _ => 0.,
        } * near;
        worms.tremor_goal = worms.tremor_goal.max(tremor);
        // Cracks spreading where it is about to come up.
        if matches!((sign.act, sign.stage), (WormAct::Rise | WormAct::Erupt, 0)) {
            let size = if sign.act == WormAct::Rise {
                ERUPT_RADIUS * 1.05
            } else {
                ERUPT_RADIUS
            };
            if let Some(cracking) = worms.cracks.iter_mut().find(|c| c.worm == id && c.live) {
                cracking.spread = cracking.spread.max(sign.progress);
                cracking.pos = item.pos;
            } else {
                let seed = worms.seed();
                if worms.cracks.len() >= 8 {
                    worms.cracks.remove(0);
                }
                worms.cracks.push(Cracking {
                    worm: id,
                    pos: item.pos,
                    size,
                    seed,
                    spread: sign.progress,
                    live: true,
                    fade: 1.,
                });
            }
        } else {
            for cracking in worms.cracks.iter_mut().filter(|c| c.worm == id) {
                cracking.live = false;
            }
        }
        if let Some(point) = furrow_point {
            if worms.furrow.len() >= MOST_FURROW {
                worms.furrow.pop_front();
            }
            worms.furrow.push_back((point, ring_radius(0) * 0.55, 0.));
        }
        // What happens on the moment an act or stage begins.
        let mut cues: Vec<(BossCue, V2)> = Vec::new();
        if changed || adopted.is_some() || tore {
            match (sign.act, sign.stage) {
                (WormAct::Rise, 0) if first_seen_rising => {
                    cues.push((BossCue::WormRumble, item.pos))
                }
                (WormAct::Rise, 1) | (WormAct::Erupt, 1) => {
                    let big = sign.act == WormAct::Rise;
                    cues.push((BossCue::WormBurst, item.pos));
                    worms.quake = worms.quake.max(if big { 19. } else { 13. } * near);
                    if look.live() {
                        breach(worms, item.pos, heading, if big { 1.4 } else { 1. }, look);
                    }
                    worms.hole(item.pos, ERUPT_RADIUS * if big { 0.78 } else { 0.62 });
                }
                (WormAct::Erupt, 0) => cues.push((BossCue::WormRumble, item.pos)),
                (WormAct::Roar, _) => {
                    cues.push((BossCue::WormRoar, head));
                    worms.quake = worms.quake.max(8. * near);
                    if look.live() {
                        for _ in 0..look.many(26.) {
                            let angle = worms.rand() * TAU;
                            let speed = worms.between((380., 760.));
                            worms.puff(
                                head + V2::from_angle(angle) * ring_radius(0) * 0.9,
                                V2::from_angle(angle) * speed,
                                (30., 70.),
                                0.62,
                            );
                        }
                    }
                }
                (WormAct::Lunge, 0) => cues.push((BossCue::WormHiss, head)),
                (WormAct::Lunge, 1) => {
                    cues.push((BossCue::WormSnap, head));
                    worms.quake = worms.quake.max(3.5 * near);
                }
                (WormAct::Spit, 1) => cues.push((BossCue::WormSpit, head)),
                (WormAct::Bile, 0) => cues.push((BossCue::WormGurgle, head)),
                (WormAct::Bile, 1) | (WormAct::Bleed, 1) => cues.push((BossCue::WormSpray, head)),
                (WormAct::Dive, _) => {
                    cues.push((BossCue::WormDive, head));
                    if look.live() {
                        for _ in 0..look.many(12.) {
                            let spread = worms.between((-1.1, 1.1));
                            let angle = item.aim + PI + spread;
                            let speed = worms.between((180., 420.));
                            worms.chunk(
                                head,
                                V2::from_angle(angle) * speed,
                                (220., 520.),
                                (8., 18.),
                                true,
                            );
                        }
                    }
                }
                (WormAct::Brood, 1) => cues.push((BossCue::WormBrood, head)),
                (WormAct::Thrash, 1) | (WormAct::Whip, 1) => cues.push((BossCue::WormThrash, head)),
                (WormAct::Sting, 1) => cues.push((BossCue::WormSting, head)),
                (WormAct::Tear, _) if !tore => cues.push((BossCue::WormStrain, head)),
                _ => {}
            }
        }
        // It went under: a hole where it went.
        if was.depth < 0.45 && sign.depth >= 0.45 && !changed {
            worms.hole(item.pos, ring_radius(0) * 0.95);
        }
        if let Some(at) = cut {
            cues.push((BossCue::WormTear, at));
            worms.quake = worms.quake.max(15. * nearness(camera, at));
            if look.live() {
                gush(worms, at, heading, 1.6, look);
            }
        }
        if let Some(at) = adopted.filter(|_| look.live()) {
            gush(worms, at, heading * -1., 0.8, look);
        }
        if rumble_due {
            cues.push((BossCue::WormRumble, item.pos));
        }
        // Grit thrown up along the ridge it pushes through the floor.
        for _ in 0..puffs {
            let jitter =
                V2::new(worms.between((-1., 1.)), worms.between((-1., 1.))) * ring_radius(0) * 0.6;
            let drift = V2::new(worms.between((-40., 40.)), worms.between((-40., 40.)));
            worms.puff(item.pos + jitter, drift, (24., 52.), 0.45);
            if worms.rand() < 0.22 {
                let angle = worms.rand() * TAU;
                let speed = worms.between((60., 220.));
                let earth = worms.rand() < 0.5;
                worms.chunk(
                    item.pos + jitter,
                    V2::from_angle(angle) * speed,
                    (160., 360.),
                    (5., 12.),
                    earth,
                );
            }
        }
        for _ in 0..drips {
            let roll = worms.rand();
            let (from, out, bile_drip) = match torn_end {
                Some((at, radius, side)) if roll < 0.7 => {
                    let along = heading * side;
                    let spread =
                        V2::new(worms.between((-1., 1.)), worms.between((-1., 1.))) * radius * 0.5;
                    (at + along * radius * 0.4 + spread, along, false)
                }
                _ if roll < 0.85 && part != WormPart::Back && jaw_now > 0.2 => {
                    (head + heading * ring_radius(0) * 0.7, heading, bile > 0.5)
                }
                _ => {
                    let Some(&(at, radius)) =
                        up.get((worms.rand() * up.len() as f32) as usize % up.len().max(1))
                    else {
                        continue;
                    };
                    let angle = worms.rand() * TAU;
                    (
                        at + V2::from_angle(angle) * radius * 0.7,
                        V2::from_angle(angle),
                        false,
                    )
                }
            };
            let speed = worms.between((20., 140.));
            worms.drop(from, out * speed, (40., 160.), (2.2, 5.), bile_drip);
            if torn_end.is_some() && roll < 0.12 {
                let pressure = worms.between((230., 390.));
                worms.jet(from, out * pressure, 220., 2.8);
            }
        }
        self.cues
            .extend(cues.into_iter().map(|(cue, at)| (cue, Some(at))));
    }

    /// Forget worms gone from the snapshot, and give a dead one its end.
    pub(super) fn worm_gone(&mut self, dt: f32) {
        let worms = &mut self.worms;
        let mut ended = Vec::new();
        for (id, shown) in worms.shown.iter_mut() {
            shown.unseen += dt;
            if shown.unseen > 0.12
                && shown.unseen - dt <= 0.12
                && shown.health <= shown.max_health * 0.2
                && !shown.dead
            {
                ended.push(*id);
            }
        }
        worms.shown.retain(|_, shown| shown.unseen < 1.5);
        for id in ended {
            self.worm_died(id);
        }
    }

    /// The destruction event is authoritative, including a hit that kills a
    /// half from above 20% health between two snapshots. Play the ending once.
    pub fn worm_died(&mut self, id: EntityId) {
        let Some(shown) = self.worms.shown.get_mut(&id).filter(|s| !s.dead) else {
            return;
        };
        shown.dead = true;
        let rings = shown.rings.clone();
        let Some(first) = rings.first() else {
            return;
        };
        self.cues.push((BossCue::WormDeath, Some(first.pos)));
        let worms = &mut self.worms;
        worms.quake = worms.quake.max(12. * nearness(self.camera, first.pos));
        if !self.look.live() {
            return;
        }
        for (k, ring) in rings.iter().enumerate().filter(|(_, ring)| ring.surfaced()) {
            if k % 2 == 0 {
                let angle = worms.rand() * TAU;
                gush(worms, ring.pos, V2::from_angle(angle), 0.35, self.look);
            }
            worms.stain(ring.pos, ring.radius * 0.9, k as f32 * 2.399, false);
        }
    }

    /// One frame of everything in the air or on the floor.
    pub(super) fn worm_update(&mut self, dt: f32) {
        let worms = &mut self.worms;
        worms.quake *= (-dt * 3.2).exp();
        // Builds quickly and settles a little slower, so it never snaps.
        let rate = if worms.tremor_goal > worms.tremor {
            10.
        } else {
            4.
        };
        worms.tremor = ease(worms.tremor, worms.tremor_goal, rate, dt);
        worms.tremor_goal = 0.;
        let mut landed: Vec<(V2, f32, f32, bool)> = Vec::new();
        for chunk in &mut worms.chunks {
            chunk.age += dt;
            chunk.rise -= GRAVITY * dt;
            chunk.height += chunk.rise * dt;
            chunk.pos += chunk.vel * dt;
            chunk.angle += chunk.spin * dt;
            if chunk.height <= 0. && chunk.rise < 0. {
                chunk.height = 0.;
                if chunk.bounces < 2 && chunk.rise < -160. {
                    chunk.rise *= -0.3;
                    chunk.vel *= 0.5;
                    chunk.spin *= 0.4;
                    chunk.bounces += 1;
                } else {
                    chunk.rise = 0.;
                    chunk.vel *= (-dt * 9.).exp();
                    chunk.spin *= (-dt * 9.).exp();
                }
            }
        }
        worms.chunks.retain(|chunk| chunk.age < chunk.life);
        for gore in &mut worms.gore {
            gore.age += dt;
            let until_floor = (gore.rise
                + (gore.rise * gore.rise + 2. * GRAVITY * gore.height.max(0.)).sqrt())
                / GRAVITY;
            let step = dt.min(until_floor);
            gore.height += gore.rise * step - 0.5 * GRAVITY * step * step;
            gore.rise -= GRAVITY * step;
            gore.pos += gore.vel * step;
            gore.angle += gore.spin * step;
            if until_floor <= dt {
                landed.push((gore.pos, gore.size, gore.vel.angle(), gore.bile));
                gore.age = f32::INFINITY;
            }
        }
        worms
            .gore
            .retain(|gore| gore.age.is_finite() && gore.age < 4.);
        for (pos, size, heading, bile) in landed {
            worms.stain(pos, size * 2.2, heading, bile);
        }
        for jet in &mut worms.jets {
            jet.age += dt;
        }
        worms.jets.retain(|jet| jet.age < jet.life);
        for dust in &mut worms.dust {
            dust.age += dt;
            dust.pos += dust.vel * dt;
            dust.vel *= (-dt * 2.6).exp();
            dust.size += dust.grow * dt;
            dust.grow *= (-dt * 1.4).exp();
        }
        worms.dust.retain(|dust| dust.age < dust.life);
        for stain in &mut worms.stains {
            stain.age += dt;
        }
        worms.stains.retain(|stain| stain.age < STAIN_SECONDS);
        for hole in &mut worms.holes {
            hole.age += dt;
        }
        worms.holes.retain(|hole| hole.age < HOLE_SECONDS);
        for cracking in &mut worms.cracks {
            if !cracking.live {
                cracking.fade -= dt / 1.6;
            }
        }
        worms.cracks.retain(|cracking| cracking.fade > 0.);
        for point in worms.furrow.iter_mut() {
            point.2 += dt;
        }
        while worms
            .furrow
            .front()
            .is_some_and(|point| point.2 > FURROW_SECONDS)
        {
            worms.furrow.pop_front();
        }
    }

    /// How hard the worms are shaking the camera, in pixels.
    pub fn worm_quake(&self) -> f32 {
        if self.look.calm {
            0.
        } else {
            self.worms.quake.max(self.worms.tremor)
        }
    }

    /// A round struck a worm: it bleeds where it was hit.
    pub fn worm_hit(&mut self, at: V2, damage: f32) {
        if !self.look.live() {
            return;
        }
        let look = self.look;
        let worms = &mut self.worms;
        let count = look.many((damage / 9.).clamp(1., 8.));
        for _ in 0..count {
            let angle = worms.rand() * TAU;
            let speed = worms.between((60., 260.));
            worms.drop(
                at,
                V2::from_angle(angle) * speed,
                (90., 260.),
                (1.8, 4.2),
                false,
            );
        }
    }

    /// A worm's round has just left it: a wet spatter where it came from.
    pub(super) fn worm_launched(&mut self, at: V2, vel: V2, kind: ProjectileKind) {
        if !self.look.live() {
            return;
        }
        let worms = &mut self.worms;
        let (count, bile) = match kind {
            ProjectileKind::Mine => (2, worms.rand() < 0.7),
            ProjectileKind::Drone => (3, false),
            ProjectileKind::Piercing | ProjectileKind::Homing => (1, false),
            _ => (0, false),
        };
        let way = vel.normalized();
        for _ in 0..count {
            let spread = V2::new(worms.between((-0.6, 0.6)), worms.between((-0.6, 0.6)));
            let speed = worms.between((80., 240.));
            worms.drop(
                at,
                (way + spread).normalized() * speed,
                (60., 200.),
                (1.8, 3.6),
                bile,
            );
        }
    }

    /// The floor under the worms: stains, broken ground, cracks, furrows.
    /// Drawn before anything standing on the floor.
    pub fn worm_floor(&self, view: &View, dark: bool) {
        let worms = &self.worms;
        if worms.shown.is_empty()
            && worms.stains.is_empty()
            && worms.holes.is_empty()
            && worms.furrow.is_empty()
            && worms.cracks.is_empty()
        {
            return;
        }
        let z = view.zoom;
        let draw = &mut crate::render::draw_model_primitive;
        let floor = floor_colors(dark);
        // The furrow a tunnel leaves, settling back down.
        let mut last: Option<(V2, f32, f32)> = None;
        for &(at, width, age) in &worms.furrow {
            if let Some((from, from_width, from_age)) = last
                && from.distance(at) < 120.
                && (view.visible(at, width * 2.) || view.visible(from, width * 2.))
            {
                let fade = |age: f32| (1. - age / FURROW_SECONDS).clamp(0., 1.);
                let (a, b) = (fade(from_age), fade(age));
                let pa = view.screen(from);
                let pb = view.screen(at);
                let way = (pb - pa).normalize_or_zero();
                let side = vec2(-way.y, way.x);
                let wa = from_width * z * (0.55 + 0.45 * a);
                let wb = width * z * (0.55 + 0.45 * b);
                draw(Primitive::Quad {
                    vertices: [
                        pa + side * wa,
                        pb + side * wb,
                        pb - side * wb,
                        pa - side * wa,
                    ],
                    color: alpha(floor.rubble, 0.5 * a.min(b)),
                });
                draw(Primitive::Quad {
                    vertices: [
                        pa + side * wa * 0.3,
                        pb + side * wb * 0.3,
                        pb - side * wb * 0.3,
                        pa - side * wa * 0.3,
                    ],
                    color: alpha(floor.pit, 0.55 * a.min(b)),
                });
            }
            last = Some((at, width, age));
        }
        for hole in &worms.holes {
            if view.visible(hole.pos, hole.size * 1.6) {
                broken_ground(
                    draw,
                    view.screen(hole.pos),
                    hole.size * z,
                    hole.seed,
                    (1. - hole.age / HOLE_SECONDS)
                        .clamp(0., 1.)
                        .min(hole.age * 6.),
                    &floor,
                );
            }
        }
        for stain in &worms.stains {
            if view.visible(stain.pos, stain.size * 2.) {
                let fade = (1. - stain.age / STAIN_SECONDS).clamp(0., 1.);
                splat(
                    draw,
                    view.screen(stain.pos),
                    stain.size * z,
                    stain.heading,
                    stain.seed,
                    (fade.min(stain.age * 12.), stain.age),
                    stain.bile,
                );
            }
        }
        for cracking in &worms.cracks {
            if view.visible(cracking.pos, cracking.size * 1.3) {
                cracks(
                    draw,
                    view.screen(cracking.pos),
                    cracking.size * z,
                    cracking.seed,
                    cracking.spread,
                    cracking.fade,
                    &floor,
                    self.air,
                );
            }
        }
        // Where its body goes into the floor, the floor is torn open round it.
        for shown in worms.shown.values() {
            for pair in shown.rings.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if a.surfaced() != b.surfaced() {
                    let at = a.pos.lerp(b.pos, 0.5);
                    let size = a.radius.max(b.radius) * 1.2;
                    if view.visible(at, size * 1.5) {
                        let seed = (at.x as i32 / 50 * 31 + at.y as i32 / 50) as u32;
                        broken_ground(draw, view.screen(at), size * z, seed, 1., &floor);
                    }
                }
            }
        }
    }

    /// A worm's body and its warnings, in place of a hull. Its warnings show
    /// with effects off; players need them to dodge.
    pub fn worm_body(&self, light: &mut Emission, view: &View, boss: &Boss, time: f32, dark: bool) {
        let Some(shown) = self.worms.shown.get(&boss.id) else {
            return;
        };
        let z = view.zoom;
        let draw = &mut crate::render::draw_model_primitive;
        let sign = shown.sign;
        let head = shown.rings.first().map_or(shown.pos, |ring| ring.pos);
        let ahead = V2::from_angle(shown.aim);
        // Warnings first, under the body.
        match (sign.act, sign.stage) {
            (WormAct::Lunge, 0) if shown.part != WormPart::Back => {
                lane(
                    draw,
                    light,
                    view,
                    head,
                    ahead,
                    ring_radius(0) * 0.85,
                    sign.progress,
                    time,
                );
            }
            (WormAct::Erupt | WormAct::Rise, 0) => {
                let size = if sign.act == WormAct::Rise {
                    ERUPT_RADIUS * 0.9
                } else {
                    ERUPT_RADIUS
                };
                circle_warning(
                    draw,
                    light,
                    view.screen(shown.pos),
                    size * z,
                    sign.progress,
                    time,
                    z,
                );
            }
            _ => {}
        }
        let (middle, reach) = shown.reach();
        if !view.visible(middle, reach) {
            return;
        }
        // Under the floor all that shows is the ground heaving over it.
        if shown.depth > 0.55 && sign.act != WormAct::Rise {
            ridge(draw, view, head, ahead, time, dark);
        }
        if sign.act == WormAct::Rise && sign.stage == 0 {
            return;
        }
        self.worm_geometry(boss.id, view, time, &mut |_, primitive| draw(primitive));
        // A faint light in its throat when there is something in it.
        if self.look.live() && shown.part != WormPart::Back && shown.jaw > 0.3 && shown.depth < 0.4
        {
            let mouth = view.screen(head + ahead * ring_radius(0) * 0.62);
            let throat = mix(Color::new(0.9, 0.2, 0.18, 1.), BILE, shown.bile);
            light.spot(
                Layer::Wash,
                mouth,
                ring_radius(0) * z * 0.9 * shown.jaw.min(1.),
                alpha(
                    throat,
                    0.16 * shown.jaw.min(1.) * self.look.level().max(0.4),
                ),
            );
        }
    }

    /// The exact body model, shared by the color and motion passes.
    pub fn worm_geometry(
        &self,
        id: EntityId,
        view: &View,
        time: f32,
        emit: &mut impl FnMut(usize, Primitive),
    ) {
        let Some(shown) = self.worms.shown.get(&id) else {
            return;
        };
        if shown.sign.act == WormAct::Rise && shown.sign.stage == 0 {
            return;
        }
        let (middle, reach) = shown.reach();
        if !view.visible(middle, reach) {
            return;
        }
        let z = view.zoom;
        let ahead = V2::from_angle(shown.aim);
        let count = shown.rings.len().min(WORM_RINGS);
        let mut segments = [Segment {
            at: Vec2::ZERO,
            radius: 0.,
            depth: 1.,
            along: Vec2::X,
        }; WORM_RINGS];
        for (k, ring) in shown.rings.iter().take(count).enumerate() {
            let toward = if k == 0 {
                // The mouth follows the aimed strike, while the neck follows
                // its path. This also keeps drool and warnings at the lips.
                ahead
            } else {
                (shown.rings[k - 1].pos - shown.rings.get(k + 1).map_or(ring.pos, |r| r.pos))
                    .normalized()
            };
            let toward = if toward == V2::ZERO { ahead } else { toward };
            segments[k] = Segment {
                at: view.screen(ring.pos),
                radius: ring.radius * z,
                depth: ring.depth,
                along: vec2(toward.x, toward.y),
            };
        }
        let motion = if self.look.calm { 0. } else { 1. };
        worm_model::worm_geometry_tagged(
            &segments[..count],
            &WormPose {
                time,
                jaw: shown.jaw,
                rear: shown.rear,
                hurt: shown.hurt,
                rage: shown.rage(),
                bile: shown.bile,
                blink: shown.blink,
                head: shown.part != WormPart::Back,
                tail: shown.part != WormPart::Front,
                torn_front: shown.part == WormPart::Back,
                torn_back: shown.part == WormPart::Front,
                first: shown.first(),
                motion,
            },
            emit,
        );
    }

    /// Ring positions as actually drawn, with conservative culling radii.
    pub fn worm_motion(&self, id: EntityId) -> impl Iterator<Item = (usize, V2, f32)> + '_ {
        self.worms.shown.get(&id).into_iter().flat_map(|shown| {
            shown
                .rings
                .iter()
                .enumerate()
                .filter(|(_, ring)| ring.surfaced())
                .map(|(k, ring)| (k, ring.pos, ring.radius * 2.4))
        })
    }

    /// Floor, earth and gore in the air, drawn over the tanks.
    pub fn worm_air(&self, view: &View, dark: bool) {
        let worms = &self.worms;
        if worms.chunks.is_empty()
            && worms.gore.is_empty()
            && worms.jets.is_empty()
            && worms.dust.is_empty()
        {
            return;
        }
        let z = view.zoom;
        let draw = &mut crate::render::draw_model_primitive;
        let floor = floor_colors(dark);
        for dust in &worms.dust {
            if !view.visible(dust.pos, dust.size) {
                continue;
            }
            let t = dust.age / dust.life;
            let fade = (1. - t) * (t * 8.).min(1.);
            let p = view.screen(dust.pos);
            let color = mix(floor.dust, floor.rubble, dust.shade);
            draw(Primitive::Polygon {
                center: p,
                sides: 18,
                radius: dust.size * z,
                angle: 0.,
                color: alpha(color, 0.22 * fade),
            });
            draw(Primitive::Polygon {
                center: p + vec2(-0.2, -0.3) * dust.size * z * 0.3,
                sides: 14,
                radius: dust.size * z * 0.62,
                angle: 0.,
                color: alpha(color, 0.18 * fade),
            });
        }
        for chunk in &worms.chunks {
            if !view.visible(chunk.pos, chunk.size * 3. + chunk.height) {
                continue;
            }
            let fade = (1. - (chunk.age - chunk.life + 0.6).max(0.) / 0.6).clamp(0., 1.);
            let p = view.screen(chunk.pos);
            let lift = chunk.height * z;
            let scale = 1. + chunk.height / 700.;
            let size = chunk.size * z * scale;
            let shadow = p + vec2(0.55, 0.83) * lift * 0.45 + vec2(0.55, 0.83) * size * 0.25;
            let corners = |at: Vec2, size: f32| {
                let (s, c) = chunk.angle.sin_cos();
                let a = vec2(c, s) * size * 0.5;
                let b = vec2(-s, c) * size * 0.5 * if chunk.earth { 0.8 } else { 1. };
                [at + a + b, at - a + b, at - a - b, at + a - b]
            };
            draw(Primitive::Quad {
                vertices: corners(shadow, size * 0.95),
                color: alpha(BLACK, 0.2 * fade / (1. + chunk.height / 300.)),
            });
            let top = p - vec2(0., lift * 0.18);
            // Its broken side, then its top face.
            draw(Primitive::Quad {
                vertices: corners(top + vec2(0.4, 0.6) * size * 0.16, size * 1.04),
                color: alpha(if chunk.earth { floor.pit } else { floor.side }, fade),
            });
            draw(Primitive::Quad {
                vertices: corners(top, size),
                color: alpha(
                    if chunk.earth {
                        floor.rubble
                    } else {
                        floor.tile
                    },
                    fade,
                ),
            });
            if !chunk.earth && size > 4. {
                // A line of the floor's grid still painted across it.
                let [a, b, _, _] = corners(top, size * 0.92);
                let [_, _, c, d] = corners(top, size * 0.92);
                let mid = (a + d) * 0.5;
                let mid2 = (b + c) * 0.5;
                draw(Primitive::Quad {
                    vertices: [
                        mid,
                        mid2,
                        mid2 + (b - mid2).normalize_or_zero() * size * 0.06,
                        mid + (a - mid).normalize_or_zero() * size * 0.06,
                    ],
                    color: alpha(floor.grid, 0.8 * fade),
                });
            }
        }
        for jet in &worms.jets {
            let reach = jet.vel.length() * jet.life + 100.;
            if view.visible(jet.pos, reach) {
                liquid_jet(draw, view, jet);
            }
        }
        for gore in &worms.gore {
            if !view.visible(gore.pos, gore.size * 3. + gore.height) {
                continue;
            }
            let p = view.screen(gore.pos);
            let lift = gore.height * z;
            let size = gore.size * z * (1. + gore.height / 600.);
            let top = p - vec2(0., lift * 0.18);
            let shadow = p + vec2(0.55, 0.83) * lift * 0.4;
            draw(Primitive::Polygon {
                center: shadow,
                sides: 8,
                radius: size,
                angle: 0.,
                color: alpha(BLACK, 0.16),
            });
            let color = if gore.bile {
                mix(BILE, Color::new(0.3, 0.38, 0.05, 1.), 0.3)
            } else if gore.flesh {
                FLESH
            } else {
                mix(RAW, BLOOD, 0.35)
            };
            if gore.flesh {
                draw(Primitive::Polygon {
                    center: top,
                    sides: 5,
                    radius: size + (z * 0.8).max(0.6),
                    angle: gore.angle,
                    color: OUTLINE,
                });
                draw(Primitive::Polygon {
                    center: top,
                    sides: 5,
                    radius: size,
                    angle: gore.angle,
                    color,
                });
                draw(Primitive::Polygon {
                    center: top + vec2(0.3, 0.2) * size,
                    sides: 5,
                    radius: size * 0.55,
                    angle: gore.angle + 0.6,
                    color: RAW,
                });
            } else {
                // Surface tension rounds the leading edge; a narrow tail
                // follows the actual projected velocity, including its fall.
                let way = vec2(gore.vel.x, gore.vel.y - gore.rise * 0.18).normalize_or_zero();
                let stretch = (gore.vel.length() / 500.).clamp(0., 1.4);
                liquid_oval(
                    draw,
                    top,
                    way,
                    size * (1. + 0.7 * stretch),
                    size * 0.8,
                    alpha(BLOOD, 0.95),
                );
                liquid_oval(
                    draw,
                    top - way * size * 0.1,
                    way,
                    size * (0.88 + 0.6 * stretch),
                    size * 0.66,
                    color,
                );
                liquid_oval(
                    draw,
                    top + vec2(-0.22, -0.28) * size,
                    way,
                    size * 0.4,
                    size * 0.15,
                    alpha(Color::new(1., 0.75, 0.69, 1.), 0.4),
                );
            }
        }
    }

    /// A round a worm threw, drawn as what it is: a shard of bone, a glob of
    /// bile or blood, a larva, a spine, a barb or a lump of the floor.
    pub fn worm_round(&self, round: &super::Round, draw: &mut impl FnMut(Primitive)) {
        let p = round.p;
        let r = round.r.max(1.);
        let o = round.opacity;
        let way = round.heading.normalize_or_zero();
        let way = if way == Vec2::ZERO { vec2(1., 0.) } else { way };
        let side = vec2(-way.y, way.x);
        let edge = (r * 0.18).max(0.8);
        let bone = Color::new(0.9, 0.85, 0.71, o);
        let bone_shade = Color::new(0.55, 0.48, 0.37, o);
        let wobble = (self.air * 9. + round.id as f32).sin();
        match round.kind {
            ProjectileKind::Piercing | ProjectileKind::Bullet => {
                // A splinter of bone, point first.
                let long = if round.kind == ProjectileKind::Piercing {
                    2.4
                } else {
                    2.9
                };
                let wide = if round.kind == ProjectileKind::Piercing {
                    0.62
                } else {
                    0.38
                };
                let tip = p + way * r * long;
                let back = p - way * r * long * 0.55;
                draw(Primitive::Quad {
                    vertices: [
                        tip + way * edge,
                        p + side * (r * wide + edge),
                        back - way * edge,
                        p - side * (r * wide + edge),
                    ],
                    color: alpha(OUTLINE, o),
                });
                draw(Primitive::Quad {
                    vertices: [tip, p + side * r * wide, back, p - side * r * wide],
                    color: bone_shade,
                });
                draw(Primitive::Triangle {
                    vertices: [tip, p + side * r * wide * 0.5, back + side * r * 0.1],
                    color: bone,
                });
            }
            ProjectileKind::Mine => {
                // A glob that wobbles as it flies and settles where it lands.
                let bile = round.damage > 21.5;
                let fill = if bile {
                    Color::new(0.58, 0.74, 0.16, o * 0.92)
                } else {
                    alpha(mix(RAW, BLOOD, 0.45), o * 0.95)
                };
                let size = r * (1. + 0.08 * wobble);
                draw(Primitive::Polygon {
                    center: p,
                    sides: 14,
                    radius: size + edge,
                    angle: 0.,
                    color: alpha(
                        if bile {
                            Color::new(0.2, 0.26, 0.03, 1.)
                        } else {
                            BLOOD
                        },
                        o * 0.8,
                    ),
                });
                draw(Primitive::Polygon {
                    center: p,
                    sides: 14,
                    radius: size,
                    angle: 0.,
                    color: fill,
                });
                for n in 0..3 {
                    let a = n as f32 * 2.1 + round.id as f32 * 0.7;
                    draw(Primitive::Polygon {
                        center: p + vec2(a.cos(), a.sin()) * size * (0.9 + 0.1 * wobble),
                        sides: 8,
                        radius: size * 0.34,
                        angle: 0.,
                        color: fill,
                    });
                }
                draw(Primitive::Polygon {
                    center: p + vec2(-0.32, -0.38) * size,
                    sides: 8,
                    radius: size * 0.26,
                    angle: 0.,
                    color: alpha(WHITE, o * 0.42),
                });
            }
            ProjectileKind::Drone => {
                // A larva: a short pale grub that wriggles toward its prey.
                let segments = 4;
                let mut points = [p; 4];
                for (n, point) in points.iter_mut().enumerate() {
                    let u = n as f32 / (segments - 1) as f32;
                    let sway = (self.air * 14. - u * 3. + round.id as f32).sin() * r * 0.35 * u;
                    *point = p - way * r * 1.5 * u + side * sway;
                }
                for (n, &point) in points.iter().enumerate().rev() {
                    let size = r * (0.95 - 0.16 * n as f32);
                    draw(Primitive::Polygon {
                        center: point,
                        sides: 12,
                        radius: size + edge,
                        angle: 0.,
                        color: alpha(OUTLINE, o),
                    });
                    draw(Primitive::Polygon {
                        center: point,
                        sides: 12,
                        radius: size,
                        angle: 0.,
                        color: alpha(mix(FLESH, WHITE, 0.15), o),
                    });
                }
                draw(Primitive::Polygon {
                    center: p + way * r * 0.45,
                    sides: 8,
                    radius: r * 0.3,
                    angle: 0.,
                    color: alpha(mix(RAW, BLOOD, 0.4), o),
                });
            }
            ProjectileKind::Homing => {
                // A hooked barb.
                let tip = p + way * r * 2.2;
                let hook = p - way * r * 0.4 + side * r * (1.1 + 0.15 * wobble);
                let tail = p - way * r * 1.6;
                draw(Primitive::Triangle {
                    vertices: [tip + way * edge, hook + side * edge, tail - way * edge],
                    color: alpha(OUTLINE, o),
                });
                draw(Primitive::Triangle {
                    vertices: [tip, hook, tail],
                    color: bone_shade,
                });
                draw(Primitive::Triangle {
                    vertices: [tip, p + side * r * 0.3, tail],
                    color: bone,
                });
            }
            ProjectileKind::Fragment => {
                // A lump of the arena floor.
                let angle = round.age * 7. + round.id as f32;
                draw(Primitive::Polygon {
                    center: p,
                    sides: 5,
                    radius: r * 1.15 + edge,
                    angle,
                    color: alpha(OUTLINE, o),
                });
                draw(Primitive::Polygon {
                    center: p,
                    sides: 5,
                    radius: r * 1.15,
                    angle,
                    color: Color::new(0.55, 0.5, 0.46, o),
                });
                draw(Primitive::Polygon {
                    center: p - vec2(0.2, 0.25) * r,
                    sides: 5,
                    radius: r * 0.6,
                    angle: angle + 0.4,
                    color: Color::new(0.7, 0.67, 0.63, o),
                });
            }
            _ => {
                // An egg sac left in its wake.
                let pulse = 1. + 0.06 * (self.air * 5. + round.id as f32).sin();
                draw(Primitive::Polygon {
                    center: p,
                    sides: 14,
                    radius: r * pulse + edge,
                    angle: 0.,
                    color: alpha(OUTLINE, o),
                });
                draw(Primitive::Polygon {
                    center: p,
                    sides: 14,
                    radius: r * pulse,
                    angle: 0.,
                    color: alpha(mix(FLESH, Color::new(0.85, 0.82, 0.55, 1.), 0.4), o),
                });
                for n in 0..4 {
                    let a = n as f32 * TAU / 4. + 0.4;
                    draw(Primitive::Polygon {
                        center: p + vec2(a.cos(), a.sin()) * r * 0.42,
                        sides: 8,
                        radius: r * 0.2,
                        angle: 0.,
                        color: alpha(Color::new(0.3, 0.2, 0.12, 1.), o * 0.75),
                    });
                }
            }
        }
    }
}

impl Worms {
    fn chunk(&mut self, at: V2, vel: V2, rise: (f32, f32), size: (f32, f32), earth: bool) {
        if self.chunks.len() >= MOST_CHUNKS {
            self.chunks.swap_remove(0);
        }
        let rise = self.between(rise);
        let size = self.between(size);
        let spin = self.between((-9., 9.));
        let angle = self.rand() * TAU;
        let life = self.between((2.6, 4.2));
        self.chunks.push(Chunk {
            pos: at,
            vel,
            height: 4.,
            rise,
            angle,
            spin,
            size,
            earth,
            bounces: 0,
            age: 0.,
            life,
        });
    }

    fn drop(&mut self, at: V2, vel: V2, rise: (f32, f32), size: (f32, f32), bile: bool) {
        if self.gore.len() >= MOST_GORE {
            self.gore.swap_remove(0);
        }
        let rise = self.between(rise);
        let size = self.between(size);
        self.gore.push(Gore {
            pos: at,
            vel,
            height: 6.,
            rise,
            size,
            angle: 0.,
            spin: 0.,
            flesh: false,
            bile,
            age: 0.,
        });
    }

    fn gib(&mut self, at: V2, vel: V2, rise: f32, size: f32) {
        if self.gore.len() >= MOST_GORE {
            self.gore.swap_remove(0);
        }
        let spin = self.between((-12., 12.));
        let angle = self.rand() * TAU;
        self.gore.push(Gore {
            pos: at,
            vel,
            height: 10.,
            rise,
            size,
            angle,
            spin,
            flesh: true,
            bile: false,
            age: 0.,
        });
    }

    fn puff(&mut self, at: V2, vel: V2, size: (f32, f32), life: f32) {
        if self.dust.len() >= MOST_DUST {
            self.dust.swap_remove(0);
        }
        let size = self.between(size);
        let grow = size * self.between((0.8, 1.6));
        let shade = self.rand();
        let life = life * self.between((0.75, 1.3));
        self.dust.push(Dust {
            pos: at,
            vel,
            size,
            grow,
            age: 0.,
            life,
            shade,
        });
    }

    fn stain(&mut self, at: V2, size: f32, heading: f32, bile: bool) {
        // Close drops join a wet pool, instead of leaving hundreds of
        // overlapping circles. Dried stains keep their established outline.
        if size < 30.
            && let Some(pool) = self.stains.iter_mut().find(|pool| {
                pool.bile == bile
                    && pool.age < 6.
                    && pool.size < 90.
                    && pool.pos.distance(at) < (pool.size + size) * 0.38
            })
        {
            let area = pool.size * pool.size;
            let added = size * size;
            let weight = added / (area + added).max(1.);
            pool.pos = pool.pos.lerp(at, weight);
            pool.size = (area + added * 0.7).sqrt().min(96.);
            pool.age *= 1. - weight;
            return;
        }
        if self.stains.len() >= MOST_STAINS {
            self.stains.remove(0);
        }
        let seed = self.seed();
        self.stains.push(Stain {
            pos: at,
            size,
            heading,
            seed,
            bile,
            age: 0.,
        });
    }

    fn jet(&mut self, at: V2, vel: V2, rise: f32, width: f32) {
        if self.jets.len() >= MOST_JETS {
            self.jets.swap_remove(0);
        }
        let life = ((rise + (rise * rise + 12. * GRAVITY).sqrt()) / GRAVITY).min(0.7);
        self.jets.push(Jet {
            pos: at,
            vel,
            rise,
            width,
            age: 0.,
            life,
        });
    }

    fn hole(&mut self, at: V2, size: f32) {
        if self
            .holes
            .iter()
            .any(|hole| hole.pos.distance(at) < size * 0.3 && hole.age < 1.)
        {
            return;
        }
        if self.holes.len() >= MOST_HOLES {
            self.holes.remove(0);
        }
        let seed = self.seed();
        self.holes.push(Hole {
            pos: at,
            size,
            seed,
            age: 0.,
        });
    }
}

/// The floor bursting open: slabs of it and clods of earth thrown high,
/// dust rolling out, and a ring of grit racing along the floor.
fn breach(worms: &mut Worms, at: V2, heading: V2, size: f32, look: Look) {
    let slabs = look.many(46. * size);
    for _ in 0..slabs {
        let angle = worms.rand() * TAU;
        let out = V2::from_angle(angle);
        // Thrown hardest the way it is coming out.
        let toward = 0.6 + 0.4 * out.dot(heading).max(0.);
        let speed = worms.between((160., 620.)) * toward * size.sqrt();
        let from = at + out * worms.between((0., ERUPT_RADIUS * 0.55));
        let earth = worms.rand() < 0.35;
        worms.chunk(
            from,
            out * speed,
            (520., 1250.),
            if earth { (9., 20.) } else { (14., 34.) },
            earth,
        );
    }
    for _ in 0..look.many(34. * size) {
        let angle = worms.rand() * TAU;
        let speed = worms.between((260., 900.));
        let from = at + V2::from_angle(angle) * ERUPT_RADIUS * worms.between((0.3, 0.8));
        worms.puff(from, V2::from_angle(angle) * speed, (36., 90.), 1.3);
    }
    for _ in 0..look.many(16. * size) {
        let angle = worms.rand() * TAU;
        let from = at + V2::from_angle(angle) * worms.between((0., ERUPT_RADIUS * 0.4));
        worms.puff(from, V2::from_angle(angle) * 40., (70., 130.), 2.4);
    }
}

/// Blood and flesh bursting out of it, mostly along `heading`.
fn gush(worms: &mut Worms, at: V2, heading: V2, size: f32, look: Look) {
    for _ in 0..look.many(5. * size) {
        let spread = worms.between((-0.9, 0.9));
        let way = V2::from_angle(heading.angle() + spread);
        let speed = worms.between((380., 760.));
        let rise = worms.between((250., 550.));
        let width = worms.between((3., 6.)) * size.sqrt();
        worms.jet(at, way * speed, rise, width);
    }
    for _ in 0..look.many(70. * size) {
        let spread = V2::new(worms.between((-1., 1.)), worms.between((-1., 1.)));
        let way = (heading * 0.9 + spread).normalized();
        let speed = worms.between((140., 720.));
        worms.drop(at, way * speed, (180., 620.), (2.4, 6.4), false);
    }
    for _ in 0..look.many(14. * size) {
        let angle = worms.rand() * TAU;
        let speed = worms.between((120., 460.));
        let rise = worms.between((300., 760.));
        let lump = worms.between((5., 12.)) * size.sqrt();
        worms.gib(at, V2::from_angle(angle) * speed, rise, lump);
    }
    for n in 0..(6. * size) as usize {
        let angle = heading.angle() + (n as f32 - 2.5) * 0.45;
        let reach = worms.between((20., 160.)) * size;
        let splash = worms.between((24., 60.)) * size.sqrt();
        worms.stain(at + V2::from_angle(angle) * reach, splash, angle, false);
    }
}

/// The floor's colors, for slabs thrown out of it.
struct Floor {
    tile: Color,
    side: Color,
    grid: Color,
    rubble: Color,
    pit: Color,
    dust: Color,
}

fn floor_colors(dark: bool) -> Floor {
    if dark {
        Floor {
            tile: Color::new(0.2, 0.23, 0.27, 1.),
            side: Color::new(0.1, 0.11, 0.13, 1.),
            grid: Color::new(0.28, 0.31, 0.35, 1.),
            rubble: Color::new(0.26, 0.22, 0.2, 1.),
            pit: Color::new(0.07, 0.05, 0.05, 1.),
            dust: Color::new(0.36, 0.33, 0.3, 1.),
        }
    } else {
        Floor {
            tile: Color::new(0.8, 0.8, 0.8, 1.),
            side: Color::new(0.5, 0.49, 0.48, 1.),
            grid: Color::new(0.72, 0.72, 0.72, 1.),
            rubble: Color::new(0.52, 0.45, 0.39, 1.),
            pit: Color::new(0.16, 0.12, 0.11, 1.),
            dust: Color::new(0.72, 0.67, 0.6, 1.),
        }
    }
}

/// A small hash for shapes that stay put while they are drawn.
fn noise(seed: u32, n: u32) -> f32 {
    let mut x = seed ^ n.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x & 0xFFFF) as f32 / 65_535.
}

/// Floor broken open: a dark pit ringed with tilted slabs.
fn broken_ground(
    draw: &mut impl FnMut(Primitive),
    p: Vec2,
    size: f32,
    seed: u32,
    fade: f32,
    floor: &Floor,
) {
    if fade <= 0.01 || size < 1. {
        return;
    }
    let edge = |n: u32, around: f32| {
        let a = n as f32 / 14. * TAU;
        vec2(a.cos(), a.sin()) * size * around * (0.82 + 0.3 * noise(seed, n))
    };
    // Earth thrown up round the rim, then the pit.
    for n in 0..14 {
        draw(Primitive::Triangle {
            vertices: [p, p + edge(n, 1.15), p + edge(n + 1, 1.15)],
            color: alpha(floor.rubble, 0.75 * fade),
        });
    }
    for n in 0..14 {
        draw(Primitive::Triangle {
            vertices: [p, p + edge(n, 0.72), p + edge(n + 1, 0.72)],
            color: alpha(floor.pit, 0.92 * fade),
        });
    }
    // Slabs of floor tipped up round the edge.
    for n in 0..9 {
        let a = noise(seed, n + 40) * TAU;
        let out = vec2(a.cos(), a.sin());
        let at = p + out * size * (0.8 + 0.25 * noise(seed, n + 60));
        let slab = size * (0.16 + 0.14 * noise(seed, n + 80));
        let turn = a + noise(seed, n + 90) * 1.2;
        let (s, c) = turn.sin_cos();
        let u = vec2(c, s) * slab;
        let v = vec2(-s, c) * slab * 0.7;
        draw(Primitive::Quad {
            vertices: [
                at + u + v + vec2(1., 1.5),
                at - u + v + vec2(1., 1.5),
                at - u - v + vec2(1., 1.5),
                at + u - v + vec2(1., 1.5),
            ],
            color: alpha(floor.side, 0.9 * fade),
        });
        draw(Primitive::Quad {
            vertices: [at + u + v, at - u + v, at - u - v, at + u - v],
            color: alpha(floor.tile, fade),
        });
    }
}

/// Blood or bile splashed on the floor: a pool, streaks the way it flew,
/// A lit, rounded drop or highlight. All liquid geometry uses the ordinary
/// effects pass, so it takes the same arena light and cover as its source.
fn liquid_oval(
    draw: &mut impl FnMut(Primitive),
    center: Vec2,
    axis: Vec2,
    long: f32,
    wide: f32,
    color: Color,
) {
    let axis = if axis.length_squared() < 0.01 {
        Vec2::X
    } else {
        axis
    };
    let side = axis.perp();
    let point = |n: usize| {
        let a = n as f32 / 12. * TAU;
        center + axis * (a.cos() * long) + side * (a.sin() * wide)
    };
    let mut last = point(0);
    for n in 1..=12 {
        let next = point(n);
        draw(Primitive::Triangle {
            vertices: [center, last, next],
            color,
        });
        last = next;
    }
}

/// A ballistic ribbon with a thicker leading drop and a thinning tail. It
/// survives only until the liquid hits the ground; droplets make its pool.
fn liquid_jet(draw: &mut impl FnMut(Primitive), view: &View, jet: &Jet) {
    let z = view.zoom;
    let tail = (jet.age - 0.15).max(0.);
    let fade = ((jet.life - jet.age) / 0.1).clamp(0., 1.) * (jet.age / 0.035).min(1.);
    let point = |u: f32| {
        let t = tail + (jet.age - tail) * u;
        let height = 6. + jet.rise * t - 0.5 * GRAVITY * t * t;
        let at = view.screen(jet.pos + jet.vel * t) - vec2(0., height.max(0.) * z * 0.18);
        let velocity = vec2(jet.vel.x, jet.vel.y - (jet.rise - GRAVITY * t) * 0.18);
        let side = velocity.normalize_or_zero().perp();
        (
            at,
            side,
            jet.width * z * (0.25 + 0.75 * u.sqrt()) * fade.sqrt(),
        )
    };
    for pass in 0..2 {
        let mut a = point(0.);
        for n in 1..=12 {
            let b = point(n as f32 / 12.);
            let width = if pass == 0 { 1. } else { 0.46 };
            draw(Primitive::Quad {
                vertices: [
                    a.0 + a.1 * a.2 * width,
                    b.0 + b.1 * b.2 * width,
                    b.0 - b.1 * b.2 * width,
                    a.0 - a.1 * a.2 * width,
                ],
                color: alpha(if pass == 0 { BLOOD } else { RAW }, fade * 0.9),
            });
            a = b;
        }
    }
}

/// An irregular continuous pool: a drying rim, a wet center, directional
/// runnels and satellite drops. Its outline is stable as it spreads.
fn splat(
    draw: &mut impl FnMut(Primitive),
    p: Vec2,
    size: f32,
    heading: f32,
    seed: u32,
    (fade, age): (f32, f32),
    bile: bool,
) {
    if fade <= 0.01 || size < 0.6 {
        return;
    }
    let (wet, dry) = if bile {
        (
            Color::new(0.5, 0.62, 0.12, 1.),
            Color::new(0.26, 0.33, 0.05, 1.),
        )
    } else {
        (
            Color::new(0.48, 0.025, 0.045, 1.),
            Color::new(0.23, 0.018, 0.035, 1.),
        )
    };
    let wetness = (-age * 0.38).exp();
    let color = mix(dry, wet, wetness);
    let size = size * (0.58 + 0.42 * (1. - (-age * 8.).exp()));
    let opacity = 0.88 * fade.sqrt();
    let way = vec2(heading.cos(), heading.sin());
    let side = way.perp();
    let outline = |n: usize| {
        let a = n as f32 / 28. * TAU;
        let ripple = 1.
            + 0.15 * (a * 3. + noise(seed, 0) * TAU).sin()
            + 0.09 * (a * 5. + noise(seed, 1) * TAU).sin()
            + 0.035 * (a * 7. + noise(seed, 2) * TAU).sin();
        (way * a.cos() + side * a.sin() * 0.83) * size * 0.65 * ripple
    };
    for (scale, tint) in [(1.03, dry), (0.96, color)] {
        let mut before = p + outline(0) * scale;
        for n in 1..=28 {
            let next = p + outline(n) * scale;
            draw(Primitive::Triangle {
                vertices: [p, before, next],
                color: alpha(tint, opacity),
            });
            before = next;
        }
    }
    for n in 0..4 {
        let spread = (noise(seed, n + 30) - 0.5) * 1.2;
        let out = (way + side * spread).normalize_or_zero();
        let length = size * (0.5 + 0.9 * noise(seed, n + 40));
        let width = size * (0.026 + 0.025 * noise(seed, n + 50));
        let from = p + out * size * 0.38;
        let normal = out.perp();
        let mut a = from;
        for k in 1..=5 {
            let u = k as f32 / 5.;
            let b = from + out * length * u + normal * size * 0.055 * (u * PI).sin();
            let before = width * (1. - (k - 1) as f32 / 6.);
            let after = width * (1. - u * 0.83);
            draw(Primitive::Quad {
                vertices: [
                    a + normal * before,
                    b + normal * after,
                    b - normal * after,
                    a - normal * before,
                ],
                color: alpha(color, opacity),
            });
            a = b;
        }
        liquid_oval(
            draw,
            a + out * size * 0.1,
            out,
            width * 1.5,
            width,
            alpha(color, opacity),
        );
    }
    if wetness > 0.025 {
        liquid_oval(
            draw,
            p + vec2(-0.17, -0.19) * size,
            way,
            size * 0.16,
            size * 0.027,
            alpha(Color::new(1., 0.79, 0.72, 1.), 0.28 * wetness * fade),
        );
        liquid_oval(
            draw,
            p + vec2(0.14, 0.09) * size,
            side,
            size * 0.08,
            size * 0.018,
            alpha(WHITE, 0.12 * wetness * fade),
        );
    }
}

/// Cracks spreading across the floor from where it is about to break out.
#[allow(clippy::too_many_arguments)]
fn cracks(
    draw: &mut impl FnMut(Primitive),
    p: Vec2,
    size: f32,
    seed: u32,
    spread: f32,
    fade: f32,
    floor: &Floor,
    time: f32,
) {
    let spread = spread.clamp(0., 1.);
    let fade = fade.clamp(0., 1.);
    if size < 1. || fade <= 0.01 {
        return;
    }
    let reach = size * (0.25 + 0.95 * spread);
    // The middle heaves up and darkens first.
    draw(Primitive::Polygon {
        center: p,
        sides: 24,
        radius: size * 0.35 * (0.4 + 0.6 * spread),
        angle: 0.,
        color: alpha(floor.pit, 0.25 * fade * spread),
    });
    for n in 0..11 {
        let mut angle = n as f32 / 11. * TAU + noise(seed, n) * 0.5;
        let mut at = p;
        let mut left = reach * (0.7 + 0.5 * noise(seed, n + 20));
        let mut width = (size * 0.022).max(1.);
        let mut step = 0;
        while left > 1. && step < 6 {
            let length = left.min(size * 0.22 * (0.7 + 0.6 * noise(seed, n * 7 + step)));
            angle += (noise(seed, n * 13 + step + 5) - 0.5) * 0.9;
            let jitter = (time * 30. + n as f32).sin() * 0.3 * spread;
            let next = at + vec2(angle.cos(), angle.sin()) * length + vec2(jitter, -jitter);
            let way = (next - at).normalize_or_zero();
            let side = vec2(-way.y, way.x);
            draw(Primitive::Quad {
                vertices: [
                    at + side * width,
                    next + side * width * 0.7,
                    next - side * width * 0.7,
                    at - side * width,
                ],
                color: alpha(floor.pit, 0.9 * fade),
            });
            // A short branch now and then.
            if noise(seed, n * 17 + step) < 0.4 {
                let fork = angle + (noise(seed, n * 19 + step) - 0.5) * 2.2;
                let tip = at + vec2(fork.cos(), fork.sin()) * length * 0.55;
                draw(Primitive::Triangle {
                    vertices: [at + side * width * 0.6, tip, at - side * width * 0.6],
                    color: alpha(floor.pit, 0.8 * fade),
                });
            }
            left -= length;
            at = next;
            width *= 0.72;
            step += 1;
        }
    }
}

/// The ridge of broken floor heaving over its head while it tunnels.
fn ridge(
    draw: &mut impl FnMut(Primitive),
    view: &View,
    head: V2,
    ahead: V2,
    time: f32,
    dark: bool,
) {
    let floor = floor_colors(dark);
    let z = view.zoom;
    let size = ring_radius(0) * 1.05;
    let p = view.screen(head);
    let across = V2::new(-ahead.y, ahead.x);
    // A mound pushed up ahead of it.
    for n in 0..16u32 {
        let u = noise(17, n) * 2. - 1.;
        let v = noise(29, n) * 2. - 1.;
        let at = head + ahead * (size * 0.45 * v + size * 0.2) + across * size * 0.7 * u;
        let heave = (time * 22. + n as f32 * 1.7).sin() * 2.5;
        let slab = size * (0.12 + 0.12 * noise(41, n));
        let turn = noise(53, n) * TAU + (time * 3. + n as f32).sin() * 0.15;
        let (s, c) = turn.sin_cos();
        let q = view.screen(at) + vec2(heave * 0.4, heave) * z;
        let u2 = vec2(c, s) * slab * z;
        let v2 = vec2(-s, c) * slab * z * 0.75;
        draw(Primitive::Quad {
            vertices: [
                q + u2 + v2 + vec2(1.5, 2.) * z,
                q - u2 + v2 + vec2(1.5, 2.) * z,
                q - u2 - v2 + vec2(1.5, 2.) * z,
                q + u2 - v2 + vec2(1.5, 2.) * z,
            ],
            color: alpha(BLACK, 0.25),
        });
        draw(Primitive::Quad {
            vertices: [q + u2 + v2, q - u2 + v2, q - u2 - v2, q + u2 - v2],
            color: if n % 3 == 0 { floor.rubble } else { floor.tile },
        });
    }
    draw(Primitive::Polygon {
        center: p,
        sides: 20,
        radius: size * 0.42 * z,
        angle: 0.,
        color: alpha(floor.pit, 0.35),
    });
}

/// The lane its jaws are about to come down: it fills as the strike nears.
#[allow(clippy::too_many_arguments)]
fn lane(
    draw: &mut impl FnMut(Primitive),
    light: &mut Emission,
    view: &View,
    head: V2,
    ahead: V2,
    half: f32,
    progress: f32,
    time: f32,
) {
    let z = view.zoom;
    let start = head + ahead * ring_radius(0) * 0.5;
    let end = start + ahead * LANE;
    let a = view.screen(start);
    let b = view.screen(end);
    let way = (b - a).normalize_or_zero();
    let side = vec2(-way.y, way.x) * half * z;
    let urgency = progress.clamp(0., 1.);
    let pulse = 0.5 + 0.5 * (time * (8. + 10. * urgency)).sin();
    draw(Primitive::Quad {
        vertices: [a + side, b + side, b - side, a - side],
        color: alpha(RED, 0.07 + 0.11 * urgency),
    });
    let filled = a.lerp(b, urgency);
    draw(Primitive::Quad {
        vertices: [a + side, filled + side, filled - side, a - side],
        color: alpha(RED, 0.1 + 0.06 * pulse),
    });
    let line = (1.6 * z).max(1.2);
    for edge in [1., -1.] {
        let offset = side * edge;
        let normal = vec2(-way.y, way.x) * line * edge;
        draw(Primitive::Quad {
            vertices: [
                a + offset,
                b + offset,
                b + offset - normal,
                a + offset - normal,
            ],
            color: alpha(RED, 0.55 + 0.35 * pulse * urgency),
        });
    }
    // Chevrons racing up the lane.
    for n in 0..5 {
        let u = ((n as f32 + time * (1.2 + 2.4 * urgency)) / 5.).fract();
        let at = a.lerp(b, u);
        let size = half * z * 0.42;
        draw(Primitive::Triangle {
            vertices: [
                at + side * 0.4 - way * size * 0.5,
                at + way * size * 0.6,
                at - side * 0.4 - way * size * 0.5,
            ],
            color: alpha(RED, 0.22 * (1. - (u - 0.5).abs() * 2.) * (0.5 + urgency)),
        });
    }
    light.streak(
        Layer::Wash,
        a,
        b,
        half * z * 0.6,
        alpha(RED, 0.),
        alpha(RED, 0.1 + 0.14 * urgency),
    );
}

/// The ring of floor that is about to break: it pulses faster and its edge
/// closes in as the moment comes.
fn circle_warning(
    draw: &mut impl FnMut(Primitive),
    light: &mut Emission,
    p: Vec2,
    size: f32,
    progress: f32,
    time: f32,
    z: f32,
) {
    let urgency = progress.clamp(0., 1.);
    let pulse = 0.5 + 0.5 * (time * (6. + 14. * urgency)).sin();
    draw(Primitive::Polygon {
        center: p,
        sides: 48,
        radius: size,
        angle: 0.,
        color: alpha(RED, 0.06 + 0.12 * urgency),
    });
    let width = (2.2 * z).max(1.4);
    ring(draw, p, size, width, alpha(RED, 0.55 + 0.4 * pulse));
    // A second ring sweeping in from the edge: when it reaches the middle,
    // the floor goes.
    ring(
        draw,
        p,
        size * (1. - urgency).max(0.04),
        width * 0.8,
        alpha(RED, 0.35 + 0.3 * urgency),
    );
    light.ring(
        Layer::Wash,
        p,
        size,
        (size * 0.06).max(2.),
        alpha(RED, 0.12 + 0.12 * pulse),
    );
}

fn ring(draw: &mut impl FnMut(Primitive), p: Vec2, radius: f32, width: f32, color: Color) {
    const SIDES: usize = 48;
    let point = |n: usize, r: f32| {
        let a = n as f32 / SIDES as f32 * TAU;
        p + vec2(a.cos(), a.sin()) * r
    };
    for n in 0..SIDES {
        draw(Primitive::Quad {
            vertices: [
                point(n, radius + width * 0.5),
                point(n + 1, radius + width * 0.5),
                point(n + 1, (radius - width * 0.5).max(0.)),
                point(n, (radius - width * 0.5).max(0.)),
            ],
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::BossKind;
    use tankyard_core::sunderworm::{WORM_BACK_RADIUS, WORM_FRONT_RADIUS, WORM_RADIUS, WormSign};

    fn worm(id: EntityId, radius: f32, at: V2, sign: WormSign) -> Boss {
        let mut boss = Boss {
            id,
            pos: at,
            vel: V2::ZERO,
            radius,
            health: 1000.,
            max_health: 1000.,
            kind: BossKind::Sunderworm,
            phase: 1,
            timer: 0.,
            aim: 0.,
            team: 0,
            mothership: false,
            target: 0,
            goal: V2::ZERO,
            tactic: tankyard_core::BossTactic::Patrol,
            tactic_time: 1.,
            last_attacker: 0,
            arrival: 0.,
            attack_timer: 5.,
            attack: 0,
            attack_windup: 0.,
            dodge: V2::ZERO,
            read: V2::ZERO,
            leg: at,
            leg_time: 0.,
            rail: Default::default(),
            clash: Default::default(),
            mind: Default::default(),
            worm: None,
        };
        sign.write(&mut boss);
        boss
    }

    fn shown(boss: &Boss) -> Shown<'_> {
        Shown {
            tick: 0,
            body: None,
            fraction: 1.,
            boss,
            pos: boss.pos,
            aim: boss.aim,
            color: FLESH,
        }
    }

    #[test]
    fn the_jaw_spring_has_the_same_response_after_a_delayed_frame() {
        let (mut a, mut av, mut b, mut bv) = (0., 0., 0., 0.);
        spring(&mut a, &mut av, 1.1, 900., 0.3);
        for _ in 0..30 {
            spring(&mut b, &mut bv, 1.1, 900., 0.01);
        }
        assert!((a - b).abs() < 0.00001 && (av - bv).abs() < 0.0001);
        assert!(a > 1. && a <= 1.1);
    }

    #[test]
    fn blood_lands_at_the_same_place_at_different_frame_rates_and_pools_stay_bounded() {
        let land = |dt| {
            let mut fx = BossFx::default();
            fx.worms.drop(
                V2::new(1000., 1000.),
                V2::new(200., -100.),
                (320., 320.),
                (4., 4.),
                false,
            );
            while !fx.worms.gore.is_empty() {
                fx.worm_update(dt);
            }
            assert_eq!(fx.worms.stains.len(), 1);
            fx.worms.stains[0].pos
        };
        assert!(land(1. / 30.).distance(land(1. / 144.)) < 0.01);
        let mut fx = BossFx::default();
        for _ in 0..1000 {
            fx.worms.stain(V2::new(1000., 1000.), 6., 0., false);
        }
        assert!(fx.worms.stains.len() < MOST_STAINS);
        assert!(fx.worms.stains.iter().all(|s| s.size <= 96.));
        for n in 0..200 {
            fx.worms
                .jet(V2::new(n as f32, 0.), V2::new(400., 0.), 300., 4.);
        }
        assert_eq!(fx.worms.jets.len(), MOST_JETS);
        fx.set_look(Look {
            strength: 0.,
            calm: true,
        });
        assert!(fx.worms.stains.is_empty() && fx.worms.jets.is_empty());
    }

    #[test]
    fn an_authoritative_death_plays_once_even_after_a_large_finishing_hit() {
        let mut fx = BossFx::default();
        let boss = worm(
            17,
            WORM_FRONT_RADIUS,
            V2::new(2000., 2000.),
            WormSign {
                act: WormAct::Hunt,
                phase: 6,
                ..Default::default()
            },
        );
        fx.worm_frame(&shown(&boss), 1. / 60.);
        assert!(boss.health > boss.max_health * 0.2);
        fx.worm_died(boss.id);
        let pieces = fx.worms.gore.len();
        assert!(pieces > 0 && !fx.worms.stains.is_empty() && !fx.worms.jets.is_empty());
        fx.worm_died(boss.id);
        assert_eq!(fx.worms.gore.len(), pieces);
        assert_eq!(
            fx.cues
                .iter()
                .filter(|(cue, _)| *cue == BossCue::WormDeath)
                .count(),
            1
        );
    }

    #[test]
    fn joining_mid_fight_uses_the_real_body_and_later_snapshots_interpolate() {
        let mut fx = BossFx::default();
        let mut boss = worm(
            17,
            WORM_RADIUS,
            V2::new(2000., 2000.),
            WormSign {
                act: WormAct::Hunt,
                phase: 2,
                ..Default::default()
            },
        );
        let mut body = tankyard_core::sunderworm::WormBody {
            id: boss.id,
            centers: (0..WORM_RINGS)
                .map(|k| {
                    (
                        V2::new(
                            2000. - k as f32 * 55.,
                            2000. + (k as f32 * 0.2).sin() * 180.,
                        ),
                        0.,
                    )
                })
                .collect(),
        };
        fx.sync(
            std::iter::once(Shown {
                body: Some(&body),
                fraction: 1.,
                ..shown(&boss)
            }),
            1. / 60.,
        );
        let drawn = &fx.worms.shown[&boss.id].rings;
        for (ring, &(at, depth)) in drawn.iter().zip(&body.centers) {
            assert_eq!((ring.pos, ring.depth), (at, depth));
        }
        let (center, reach) = fx.worms.shown[&boss.id].reach();
        assert!(
            drawn
                .iter()
                .all(|ring| ring.pos.distance(center) + ring.radius < reach)
        );
        let before: Vec<_> = drawn.iter().map(|r| r.pos).collect();
        for (at, _) in &mut body.centers {
            at.x += 40.;
        }
        boss.pos.x += 40.;
        fx.sync(
            std::iter::once(Shown {
                body: Some(&body),
                fraction: 0.5,
                tick: 1,
                ..shown(&boss)
            }),
            1. / 60.,
        );
        for (ring, at) in fx.worms.shown[&boss.id].rings.iter().zip(before) {
            assert!(ring.pos.distance(at + V2::new(20., 0.)) < 0.001);
        }
        // The next tick can have identical positions (the worm stopped).
        // Resetting interpolation must not rewind to the older moving tick.
        fx.sync(
            std::iter::once(Shown {
                body: Some(&body),
                tick: 2,
                fraction: 0.,
                ..shown(&boss)
            }),
            1. / 60.,
        );
        for (ring, &(at, _)) in fx.worms.shown[&boss.id].rings.iter().zip(&body.centers) {
            assert!(ring.pos.distance(at) < 0.001);
        }
    }

    #[test]
    fn its_body_follows_where_its_head_went() {
        let mut fx = BossFx::default();
        let sign = WormSign {
            act: WormAct::Hunt,
            phase: 1,
            ..Default::default()
        };
        for step in 0..240 {
            let t = step as f32 / 60.;
            let at = V2::new(1000. + t * 300., 1000. + (t * 2.).sin() * 200.);
            let mut boss = worm(7, WORM_RADIUS, at, sign);
            boss.aim = 0.;
            fx.worm_frame(&shown(&boss), 1. / 60.);
            fx.worm_update(1. / 60.);
        }
        let shown = &fx.worms.shown[&7];
        assert_eq!(shown.rings.len(), WORM_RINGS);
        for pair in shown.rings.windows(2) {
            let gap = pair[0].pos.distance(pair[1].pos);
            assert!(gap < WORM_SPACING + 1., "{gap}");
        }
        assert!(fx.is_worm(7));
    }

    #[test]
    fn a_torn_tail_picks_up_where_the_front_half_ends_and_both_bleed() {
        let mut fx = BossFx::default();
        let hunt = WormSign {
            act: WormAct::Hunt,
            phase: 5,
            ..Default::default()
        };
        for step in 0..200 {
            let at = V2::new(800. + step as f32 * 6., 1200.);
            fx.worm_frame(&shown(&worm(7, WORM_RADIUS, at, hunt)), 1. / 60.);
        }
        let head = V2::new(800. + 200. * 6., 1200.);
        let tail_head = fx.worms.shown[&7]
            .path
            .at(WORM_TEAR as f32 * WORM_SPACING)
            .0;
        let torn = WormSign { phase: 6, ..hunt };
        fx.worm_frame(&shown(&worm(7, WORM_FRONT_RADIUS, head, torn)), 1. / 60.);
        fx.worm_frame(
            &shown(&worm(9, WORM_BACK_RADIUS, tail_head, torn)),
            1. / 60.,
        );
        let front = &fx.worms.shown[&7];
        let back = &fx.worms.shown[&9];
        assert_eq!(front.rings.len(), WORM_TEAR);
        assert_eq!(back.rings.len(), WORM_RINGS - WORM_TEAR);
        let gap = front.rings.last().unwrap().pos.distance(back.rings[0].pos);
        assert!(gap < WORM_SPACING * 1.5, "{gap}");
        assert!(!fx.worms.gore.is_empty() && !fx.worms.stains.is_empty());
        assert!(fx.cues.iter().any(|(cue, _)| *cue == BossCue::WormTear));
    }

    #[test]
    fn coming_up_shakes_hard_throws_the_floor_and_leaves_a_hole() {
        let mut fx = BossFx::default();
        let at = V2::new(1500., 1500.);
        for step in 0..120 {
            let sign = WormSign {
                act: WormAct::Rise,
                stage: 0,
                progress: step as f32 / 120.,
                depth: 1.,
                phase: 1,
            };
            fx.worm_frame(&shown(&worm(3, WORM_RADIUS, at, sign)), 1. / 60.);
            fx.worm_update(1. / 60.);
        }
        // Shaking already, and harder as it comes.
        let rising = fx.worm_quake();
        assert!(rising > 0.);
        let burst = WormSign {
            act: WormAct::Rise,
            stage: 1,
            depth: 0.8,
            phase: 1,
            ..Default::default()
        };
        fx.worm_frame(&shown(&worm(3, WORM_RADIUS, at, burst)), 1. / 60.);
        assert!(fx.worm_quake() > rising);
        assert!(fx.worms.chunks.len() > 20);
        assert_eq!(fx.worms.holes.len(), 1);
        assert!(fx.cues.iter().any(|(cue, _)| *cue == BossCue::WormBurst));
        // Everything settles and goes in time, within its limits.
        for _ in 0..(50. * 60.) as usize {
            fx.worm_update(1. / 60.);
        }
        assert!(
            fx.worms.chunks.is_empty() && fx.worms.gore.is_empty() && fx.worms.holes.is_empty()
        );
    }

    #[test]
    fn nothing_decorative_with_effects_off_but_the_body_still_follows() {
        let mut fx = BossFx::default();
        fx.set_look(Look {
            strength: 0.,
            calm: true,
        });
        let burst = WormSign {
            act: WormAct::Rise,
            stage: 1,
            phase: 1,
            ..Default::default()
        };
        fx.worm_frame(
            &shown(&worm(3, WORM_RADIUS, V2::new(900., 900.), burst)),
            1. / 60.,
        );
        assert!(fx.worms.chunks.is_empty() && fx.worms.dust.is_empty());
        assert_eq!(fx.worm_quake(), 0.);
        assert_eq!(fx.worms.shown[&3].rings.len(), WORM_RINGS);
    }
}
