//! The Railgunner in action: the sight it hunts along, rails charging coil by
//! coil, a shot whose line stays in the air after it has gone, the plasma
//! stream it swings, and the long way it comes apart when it dies.
//!
//! The sight and the charge are how players know to move, so they show at
//! every Effects setting and hold still with motion reduced.
use super::fire::{self, Plume};
use super::{BossFx, Flash, Flight, Fuel, Round, Spray, direction, ease_out, perpendicular};
use crate::emission::{Emission, Layer, Stop};
use crate::lighting::Light;
use crate::map_sounds::BossCue;
use crate::render::{View, polygon};
use crate::ui::{alpha, mix};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};
use tankyard_client::boss_model::{
    Pose, RAIL_COILS, RAIL_FRINGE, RAIL_ION, RAIL_THROW, RailBarrel,
};
use tankyard_client::tank_model::OUTLINE;
use tankyard_core::{
    BEAM_ROUND_RADIUS, Boss, BossKind, EntityId, LETHAL_DAMAGE, RAIL_LOCK, RAIL_MUZZLE, RAIL_SIGHT,
    Rect, V2, is_beam_round,
};

/// How long a rail shot's line hangs in the air after the round lands.
const TRACER_SECONDS: f32 = 1.05;
const MOST_TRACERS: usize = 10;
const MOST_WRECKS: usize = 3;
/// How long a destroyed Railgunner takes to finish coming apart.
const WRECK_SECONDS: f32 = 1.4;
/// Seconds between the plasma hum's restarts, a little under its length.
const HUM_EVERY: f32 = 2.8;
/// Most rounds of one stream drawn: a full stream has about 32 in the air.
const STREAM_POINTS: usize = 48;
/// Most points of a stream once its corners are rounded.
const ROUNDED_POINTS: usize = (STREAM_POINTS + 1) * 2;
/// Streams drawn at once, more than an arena will ever hold.
const MOST_STREAMS: usize = 3;
/// Rounds further apart than this are two streams, not one. They fly every
/// other tick, about 70 units apart.
const STREAM_GAP: f32 = 240.;

/// The Railgunner's color ramp: white-hot at 0, through ion blue, to violet.
pub(super) fn ion(t: f32) -> Color {
    const STOPS: [Color; 4] = [
        Color::new(1., 1., 1., 1.),
        Color::new(0.62, 0.97, 1., 1.),
        Color::new(0.2, 0.62, 1., 1.),
        Color::new(0.62, 0.26, 1., 1.),
    ];
    let at = t.clamp(0., 1.) * 3.;
    let index = (at as usize).min(2);
    mix(STOPS[index], STOPS[index + 1], at - index as f32)
}

/// What the renderer keeps of a Railgunner between frames.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RailShown {
    pub(super) recoil: f32,
    beam: f32,
    vent: f32,
    /// How far its barrel has split open, and how fast it's moving.
    open: f32,
    open_rate: f32,
    /// Last frame's windup, to catch the moment its aim locks.
    windup: f32,
    beaming: bool,
    venting: bool,
    /// Part-made puffs of steam and sparks, so the rate holds at any frame rate.
    steam: f32,
    motes: f32,
    /// Seconds until the beam's hum is due to start over.
    hum: f32,
}

/// How far the barrel should be open for what it's doing. The armor cracks as
/// the beam starts charging, eases wider as the charge builds, and throws fully
/// open at the lock, so the shape of the gun says what's coming. A rail shot
/// only strains at its latches.
fn barrel_goal(boss: &Boss) -> f32 {
    if boss.beaming() || boss.venting() {
        return 1.;
    }
    if boss.attack_windup <= 0. {
        return 0.;
    }
    let total = boss.warning_seconds().max(0.01);
    let done = (1. - boss.attack_windup / total).clamp(0., 1.);
    if boss.attack_style().1 != 2 {
        return if boss.attack_style().1 == 1 {
            0.16 * done
        } else {
            0.
        };
    }
    if boss.attack_windup < RAIL_LOCK {
        return 1.;
    }
    let locks_at = (1. - RAIL_LOCK / total).max(0.2);
    0.26 + 0.42 * ((done - 0.1) / (locks_at - 0.1)).clamp(0., 1.)
}

/// One exact step of a spring that rings a little before it settles, the same
/// at any frame rate. `damping` is under 1.
fn ring_to(value: f32, speed: f32, goal: f32, (rate, damping): (f32, f32), dt: f32) -> (f32, f32) {
    let off = value - goal;
    let decay = (-damping * rate * dt).exp();
    let turn = rate * (1. - damping * damping).max(1e-4).sqrt();
    let (sin, cos) = (turn * dt).sin_cos();
    let swing = (speed + damping * rate * off) / turn;
    (
        goal + decay * (off * cos + swing * sin),
        decay * (speed * cos - (damping * rate * swing + turn * off) * sin),
    )
}

/// A rail shot's line through the air.
#[derive(Clone, Copy, Debug)]
pub(super) struct Tracer {
    /// The round it follows, until that lands.
    round: Option<EntityId>,
    from: V2,
    to: V2,
    radius: f32,
    /// Seconds since the round landed.
    age: f32,
    seed: f32,
}

/// A destroyed Railgunner, still coming apart.
#[derive(Clone, Copy, Debug)]
pub(super) struct Wreck {
    at: V2,
    aim: f32,
    radius: f32,
    age: f32,
    /// Seconds until the next charge cooks off.
    fuse: f32,
}

/// The live rounds of each plasma stream in view this frame.
pub struct Streams {
    owners: [EntityId; MOST_STREAMS],
    counts: [usize; MOST_STREAMS],
    points: [[(Vec2, f32); STREAM_POINTS]; MOST_STREAMS],
}

impl Default for Streams {
    fn default() -> Self {
        Self {
            owners: [0; MOST_STREAMS],
            counts: [0; MOST_STREAMS],
            points: [[(Vec2::ZERO, 0.); STREAM_POINTS]; MOST_STREAMS],
        }
    }
}

impl Streams {
    /// Note a round of `owner`'s stream, at `at` on screen, `age` seconds old.
    pub fn note(&mut self, owner: EntityId, at: Vec2, age: f32) {
        let slot = self
            .owners
            .iter()
            .position(|&id| id == owner)
            .or_else(|| self.owners.iter().position(|&id| id == 0));
        let Some(slot) = slot else {
            return;
        };
        self.owners[slot] = owner;
        if self.counts[slot] < STREAM_POINTS {
            self.points[slot][self.counts[slot]] = (at, age);
            self.counts[slot] += 1;
        }
    }
}

/// How a stream's band runs at one point: its half width, color and offset.
type Shape = (f32, Color, f32);

/// Lay a band through `spine`, each point shaped by how far along it is.
pub(super) fn lay(
    light: &mut Emission,
    layer: Layer,
    spine: &[Vec2],
    shape: impl Fn(usize, f32) -> Shape,
) {
    let count = spine.len();
    if count < 2 {
        return;
    }
    let mut stops = [Stop {
        at: Vec2::ZERO,
        half: 0.,
        color: Color::new(0., 0., 0., 0.),
    }; ROUNDED_POINTS];
    let count = count.min(ROUNDED_POINTS);
    for (index, stop) in stops[..count].iter_mut().enumerate() {
        let (before, after) = (
            spine[index.saturating_sub(1)],
            spine[(index + 1).min(count - 1)],
        );
        let across = perpendicular((after - before).normalize_or_zero());
        let (half, color, side) = shape(index, index as f32 / (count - 1) as f32);
        *stop = Stop {
            at: spine[index] + across * side,
            half,
            color,
        };
    }
    light.band(layer, &stops[..count]);
}

/// A point `u` of the way along `spine`.
pub(super) fn along<P: Copy + Lerp>(spine: &[P], u: f32) -> P {
    let at = u.clamp(0., 1.) * (spine.len() - 1) as f32;
    let index = (at as usize).min(spine.len() - 2);
    spine[index].mixed(spine[index + 1], at - index as f32)
}

/// A point type a line can be walked along, on screen or in the world.
pub(super) trait Lerp {
    fn mixed(self, other: Self, t: f32) -> Self;
}
impl Lerp for Vec2 {
    fn mixed(self, other: Self, t: f32) -> Self {
        self.lerp(other, t)
    }
}
impl Lerp for V2 {
    fn mixed(self, other: Self, t: f32) -> Self {
        self.lerp(other, t)
    }
}

/// `spine` with a point added between each pair, on the curve through their
/// neighbors, so a bending beam bends instead of kinking. However uneven the
/// points, it never strays more than a quarter of the gap from the straight line.
fn rounded(spine: &[Vec2], out: &mut [Vec2; ROUNDED_POINTS]) -> usize {
    let count = spine.len().min(STREAM_POINTS + 1);
    let mut used = 0;
    for index in 0..count {
        let here = spine[index];
        out[used] = here;
        used += 1;
        let Some(&next) = spine[..count].get(index + 1) else {
            break;
        };
        let before = if index > 0 {
            spine[index - 1]
        } else {
            here * 2. - next
        };
        let after = spine[..count]
            .get(index + 2)
            .copied()
            .unwrap_or(next * 2. - here);
        let bow = (here + next - before - after) / 16.;
        out[used] = (here + next) * 0.5 + bow.clamp_length_max(here.distance(next) * 0.25);
        used += 1;
    }
    used
}

/// A piece of the stream from the first to the last of `spine`.
pub(super) fn stream(
    light: &mut Emission,
    spine: &[Vec2],
    half: f32,
    clock: f32,
    level: f32,
    open: bool,
    spectrum: bool,
) {
    if spine.len() < 2 {
        return;
    }
    let mut smooth = [Vec2::ZERO; ROUNDED_POINTS];
    let used = rounded(spine, &mut smooth);
    let spine = &smooth[..used];
    let moving = clock != 0.;
    let tint = |u: f32, ordinary: Color, core: f32| {
        if spectrum {
            mix(
                tankyard_client::boss_model::silk(clock * 0.12 + u * 0.9),
                WHITE,
                core,
            )
        } else {
            ordinary
        }
    };
    // It swells out of the muzzle and tapers where it gives out.
    let body = |u: f32| {
        let start = if open {
            0.35 + 0.65 * (u / 0.035).min(1.)
        } else {
            (u / 0.06).min(1.)
        };
        start * ((1. - u) / 0.09).clamp(0., 1.).sqrt()
    };
    let shimmer = |index: usize| {
        if moving {
            0.93 + 0.07 * (clock * 37. - index as f32 * 0.65).sin()
        } else {
            1.
        }
    };
    // A wide violet haze around all of it.
    lay(light, Layer::Wash, spine, |_, u| {
        (
            half * 5.2 * body(u),
            alpha(tint(u, RAIL_FRINGE, 0.), 0.2 * body(u) * level),
            0.,
        )
    });
    lay(light, Layer::Wash, spine, |_, u| {
        (
            half * 3.3 * body(u),
            alpha(tint(u, ion(0.9), 0.), 0.38 * body(u) * level),
            0.,
        )
    });
    lay(light, Layer::Wash, spine, |index, u| {
        (
            half * 2. * body(u) * shimmer(index),
            alpha(tint(u, ion(0.56), 0.18), 0.62 * body(u) * level),
            0.,
        )
    });
    lay(light, Layer::Core, spine, |index, u| {
        (
            half * 1.02 * body(u) * shimmer(index),
            alpha(tint(u, ion(0.3), 0.48), 0.95 * body(u) * level),
            0.,
        )
    });
    lay(light, Layer::Core, spine, |index, u| {
        (
            half * 0.42 * body(u) * shimmer(index + 6),
            alpha(tint(u, ion(0.), 0.86), body(u) * level.max(0.7)),
            0.,
        )
    });
    if !moving {
        return;
    }
    // Two filaments twist round each other down its length.
    for (strand, tint) in [(0., ion(0.22)), (PI, mix(RAIL_FRINGE, WHITE, 0.3))] {
        lay(light, Layer::Core, spine, |index, u| {
            let phase = index as f32 * 0.41 - clock * 19. + strand;
            (
                (half * 0.11).max(0.8) * (0.7 + 0.3 * phase.cos()),
                alpha(tint, (0.42 + 0.42 * phase.cos()) * body(u) * level),
                phase.sin() * half * (0.86 + 0.14 * (index as f32 * 0.15 + clock * 5.).sin()),
            )
        });
    }
    // Knots of brighter plasma riding out along it.
    for knot in 0..6 {
        let u = (clock * 1.05 + knot as f32 / 6.).fract();
        let at = along(spine, u);
        let way = (along(spine, (u + 0.02).min(1.)) - along(spine, (u - 0.02).max(0.)))
            .normalize_or_zero();
        let strength = (u * PI).sin() * body(u);
        light.oval(
            Layer::Wash,
            at,
            way * half * 4.6,
            half * 1.25,
            alpha(ion(0.5), 0.34 * strength * level),
        );
        light.oval(
            Layer::Core,
            at,
            way * half * 2.7,
            half * 0.52,
            alpha(ion(0.05), 0.72 * strength * level),
        );
    }
}

impl BossFx {
    /// Moving attachments sampled on the same frame as the hull.
    pub fn model_pose(&self, boss: &Boss) -> Pose {
        match self.bosses.get(&boss.id) {
            Some(shown) if boss.kind == BossKind::SiegeEngine => Pose {
                chassis: Some(shown.chassis),
                ..Pose::default()
            },
            Some(shown) if boss.kind == BossKind::Railgunner => Pose {
                recoil: shown.rail.recoil,
                beam: shown.rail.beam,
                vent: shown.rail.vent,
                open: shown.rail.open,
                ..Pose::default()
            },
            Some(shown) if matches!(boss.kind, BossKind::Jester | BossKind::Carillon) => {
                let (source, _) = boss.attack_style();
                Pose {
                    recoil: (shown.kick.length() / (boss.radius * super::MOST_KICK)).clamp(0., 1.),
                    bells: shown.bells.map(|b| b.angle),
                    mask: (source != boss.kind).then_some(source),
                    ..Pose::default()
                }
            }
            _ => Pose::default(),
        }
    }

    pub(super) fn rail_flight(&self, flight: &Flight) -> bool {
        match self.bosses.get(&flight.owner).and_then(|p| p.kind) {
            Some(BossKind::Railgunner) => true,
            Some(BossKind::Jester) => is_beam_round(flight.kind, flight.vel),
            _ => false,
        }
    }

    /// The sounds due this frame, and where each comes from.
    pub fn take_cues(&mut self) -> Vec<(BossCue, Option<V2>)> {
        std::mem::take(&mut self.cues)
    }

    /// How hard the last frame's explosions shook the camera.
    pub fn take_shake(&mut self) -> f32 {
        std::mem::take(&mut self.pending_shake)
    }

    /// The wrecks still standing, as (position, radius, aim).
    pub fn wrecks(&self) -> impl Iterator<Item = (V2, f32, f32)> + '_ {
        self.wrecks.iter().map(|wreck| {
            let shudder = if self.look.calm {
                V2::ZERO
            } else {
                V2::new(
                    (wreck.age * 47.).sin() * wreck.age * 2.6,
                    (wreck.age * 39. + 1.).cos() * wreck.age * 2.6,
                )
            };
            (wreck.at + shudder, wreck.radius, wreck.aim)
        })
    }

    fn cue(&mut self, cue: BossCue, at: V2) {
        self.push_cue(cue, at);
    }

    pub(super) fn push_cue(&mut self, cue: BossCue, at: V2) {
        if self.cues.len() < 8 {
            self.cues.push((cue, Some(at)));
        }
    }

    /// One frame of a Railgunner: ease its firing pose and set off whatever
    /// just started.
    pub(super) fn rail_frame(&mut self, boss: &Boss, known: bool, dt: f32) {
        let look = self.look;
        let Some(presence) = self.bosses.get_mut(&boss.id) else {
            return;
        };
        let (aim, pos, radius) = (presence.aim, presence.pos, presence.radius);
        let rail = &mut presence.rail;
        let was = *rail;
        let (beaming, venting) = (boss.beaming(), boss.venting());
        let (beam_goal, vent_goal) = (f32::from(u8::from(beaming)), f32::from(u8::from(venting)));
        let open_goal = barrel_goal(boss);
        if look.calm {
            rail.recoil = 0.;
            rail.beam = beam_goal;
            rail.vent = vent_goal;
            rail.open = open_goal;
            rail.open_rate = 0.;
        } else {
            // It throws open hard and rings, and shuts slower, like something
            // heavy being lowered into place.
            let spring = if open_goal > rail.open {
                (15., 0.52)
            } else {
                (8., 0.78)
            };
            (rail.open, rail.open_rate) = ring_to(rail.open, rail.open_rate, open_goal, spring, dt);
            rail.recoil *= (-dt * 5.4).exp();
            let beam_rate = if beaming { 15. } else { 5. };
            let vent_rate = if venting { 9. } else { 3.1 };
            rail.beam += (beam_goal - rail.beam) * (1. - (-dt * beam_rate).exp());
            rail.vent += (vent_goal - rail.vent) * (1. - (-dt * vent_rate).exp());
        }
        rail.windup = boss.attack_windup;
        rail.beaming = beaming;
        rail.venting = venting;
        let mut hum = false;
        if beaming {
            rail.hum -= dt;
            if rail.hum <= 0. {
                rail.hum = HUM_EVERY;
                hum = true;
            }
        } else {
            rail.hum = 0.;
        }
        let mut steam = 0;
        let mut motes = 0;
        if look.live() {
            if venting {
                rail.steam += 34. * dt * look.level();
                steam = rail.steam as usize;
                rail.steam -= steam as f32;
            }
            if beaming {
                rail.motes += 70. * dt * look.level();
                motes = rail.motes as usize;
                rail.motes -= motes as f32;
            }
        }
        if known {
            if was.windup <= 0. && boss.attack_windup > 0. {
                self.cue(BossCue::Charge, pos);
            }
            if was.windup >= RAIL_LOCK && boss.attack_windup > 0. && boss.attack_windup < RAIL_LOCK
            {
                // Whoever it's locked onto hears it wherever they are.
                if boss.target != 0 && boss.target == self.local {
                    self.cues.push((BossCue::Lock, None));
                } else {
                    self.cue(BossCue::Lock, pos);
                }
            }
            if venting && !was.venting {
                self.cue(BossCue::Vent, pos);
            }
        }
        if hum {
            self.cue(BossCue::Beam, pos);
        }
        let way = direction(aim);
        let muzzle = pos + V2::new(way.x, way.y) * radius * RAIL_MUZZLE;
        if motes > 0 {
            self.spray(Spray {
                at: muzzle,
                heading: Some((aim, 0.5)),
                count: motes,
                speed: (260., 880.),
                life: (0.12, 0.34),
                size: (1., 2.6),
                color: ion(0.25),
            });
        }
        for _ in 0..steam {
            // Steam blowing off the fins.
            let out = if self.rand() < 0.5 { 1. } else { -1. };
            let root = aim + out * self.between((1.94, 2.78));
            let from = pos + V2::from_angle(root) * radius * self.between((1.02, 1.5));
            let size = radius * self.between((0.1, 0.26));
            let life = self.between((0.5, 1.15));
            self.steam(from, root, size, life);
        }
    }

    fn steam(&mut self, at: V2, angle: f32, size: f32, life: f32) {
        let speed = self.between((40., 150.));
        let spread = angle + (self.rand() - 0.5) * 0.7;
        if self.bursts.len() < super::MOST_BURSTS {
            self.bursts.push(super::Burst {
                at,
                vel: V2::from_angle(spread) * speed,
                flash: Flash::Smoke,
                fuel: Fuel::Plasma,
                size,
                color: Color::new(0.5, 0.62, 0.74, 1.),
                age: 0.,
                life,
            });
        }
    }

    /// Whether a round of `owner`'s is one of its plasma stream.
    pub fn is_stream_round(
        &self,
        owner: EntityId,
        kind: tankyard_content::ProjectileKind,
        vel: V2,
    ) -> bool {
        is_beam_round(kind, vel)
            && self
                .bosses
                .get(&owner)
                .is_some_and(|p| matches!(p.kind, Some(BossKind::Jester | BossKind::Railgunner)))
    }

    /// A Railgunner's round leaving the barrel. Returns how hard that shakes
    /// the camera.
    pub(super) fn rail_launched(&mut self, flight: &Flight) -> f32 {
        use tankyard_content::ProjectileKind::Shell;
        if is_beam_round(flight.kind, flight.vel) {
            return 0.;
        }
        let look = self.look;
        let heading = vec2(flight.vel.x, flight.vel.y).normalize_or_zero();
        let cannon = flight.kind == Shell;
        let lethal = flight.damage >= LETHAL_DAMAGE;
        if let Some(presence) = self.bosses.get_mut(&flight.owner)
            && cannon
            && !look.calm
        {
            presence.rail.recoil = if lethal { 1. } else { 0.7 };
            let shove = if lethal { 8.5 } else { 4.5 };
            presence.kick_rate -= heading * shove * 60.;
        }
        if cannon {
            self.cue(BossCue::Shot, flight.at);
        }
        if !look.live() {
            return if cannon { 2. } else { 0. };
        }
        if !cannon {
            self.fueled_burst(
                flight.at,
                Flash::Muzzle(heading),
                Fuel::Ion,
                flight.radius * 1.5,
                RAIL_ION,
                0.14,
            );
            return 0.;
        }
        self.follow_shot(flight);
        let strength = if lethal { 1. } else { 0.62 };
        let r = flight.radius;
        let at = flight.at;
        let angle = heading.y.atan2(heading.x);
        self.fueled_burst(
            at,
            Flash::Muzzle(heading),
            Fuel::Ion,
            r * 2.3 * strength,
            RAIL_ION,
            0.32,
        );
        self.burst(at, Flash::Blast, r * 1.1 * strength, ion(0.55), 0.42);
        self.burst(at, Flash::Release, r * 3.6 * strength, ion(0.3), 0.5);
        // Ions blown straight down the line, and off to the sides of the
        // muzzle's prongs.
        for (spread, count, speed) in [(0.13, 70., (900., 3000.)), (0.42, 26., (300., 1300.))] {
            self.spray(Spray {
                at,
                heading: Some((angle, spread)),
                count: look.many(count * strength),
                speed,
                life: (0.14, 0.5),
                size: (1.4, 3.6),
                color: ion(0.22),
            });
        }
        for side in [-1., 1.] {
            self.spray(Spray {
                at,
                heading: Some((angle + side * 1.5, 0.3)),
                count: look.many(22. * strength),
                speed: (240., 980.),
                life: (0.12, 0.36),
                size: (1.2, 2.8),
                color: mix(RAIL_FRINGE, WHITE, 0.2),
            });
        }
        if lethal { 3.2 } else { 1.4 }
    }

    /// Start or carry on the line behind a rail shot, wherever it is first
    /// seen from.
    fn follow_shot(&mut self, flight: &Flight) {
        if let Some(tracer) = self
            .tracers
            .iter_mut()
            .find(|tracer| tracer.round == Some(flight.id))
        {
            tracer.to = flight.at;
            return;
        }
        if self.tracers.len() >= MOST_TRACERS {
            self.tracers.remove(0);
        }
        let way = flight.vel.normalized();
        let from = self.bosses.get(&flight.owner).map_or(flight.at, |shown| {
            let start = shown.pos + way * shown.radius * RAIL_MUZZLE;
            // Only a line that really runs back to the barrel starts there.
            let run = flight.at - start;
            if run.length() < 1. || run.normalized().dot(way) > 0.995 {
                start
            } else {
                flight.at
            }
        });
        self.tracers.push(Tracer {
            round: Some(flight.id),
            from,
            to: flight.at,
            radius: flight.radius,
            age: 0.,
            seed: (flight.id % 97) as f32 * 0.61,
        });
    }

    /// A Railgunner's round on its way.
    pub(super) fn rail_track(&mut self, flight: &Flight) {
        if flight.kind == tankyard_content::ProjectileKind::Shell {
            self.follow_shot(flight);
        }
    }

    /// A Railgunner's round is gone.
    pub(super) fn rail_landed(&mut self, flight: &Flight, struck: bool) {
        use tankyard_content::ProjectileKind::Shell;
        let look = self.look;
        if let Some(tracer) = self
            .tracers
            .iter_mut()
            .find(|tracer| tracer.round == Some(flight.id))
        {
            tracer.round = None;
            tracer.to = flight.at;
        }
        if !look.live() {
            return;
        }
        let (at, r) = (flight.at, flight.radius);
        if is_beam_round(flight.kind, flight.vel) {
            if struck {
                self.stream_round_gone(flight.owner, flight.id, at, r);
            }
            return;
        }
        if flight.kind != Shell {
            self.fueled_burst(at, Flash::Hit, Fuel::Ion, r, RAIL_ION, 0.28);
            self.spray(Spray {
                at,
                heading: None,
                count: look.many(7.),
                speed: (90., 380.),
                life: (0.16, 0.42),
                size: (1., 2.4),
                color: ion(0.34),
            });
            return;
        }
        let strength = if flight.damage >= LETHAL_DAMAGE {
            1.
        } else {
            0.62
        };
        self.hold_blast(
            flight.owner,
            at,
            r,
            ion(0.45),
            super::clash::Blast::Rail(strength),
        );
    }

    /// The rail round going off: a white flash, ion rings, and a crater.
    pub(super) fn rail_blast(&mut self, at: V2, r: f32, strength: f32) {
        let look = self.look;
        self.burst(at, Flash::Blast, r * 1.7 * strength, ion(0.45), 0.66);
        self.burst(at, Flash::Phase, r * 3.3 * strength, ion(0.3), 0.8);
        self.fueled_burst(
            at,
            Flash::Hit,
            Fuel::Ion,
            r * 1.5 * strength,
            RAIL_FRINGE,
            0.42,
        );
        for (count, speed, life, size, tint) in [
            (90., (240., 1500.), (0.22, 0.8), (1.5, 4.2), 0.2),
            (34., (60., 340.), (0.5, 1.3), (1.2, 2.8), 0.65),
        ] {
            self.spray(Spray {
                at,
                heading: None,
                count: look.many(count * strength),
                speed,
                life,
                size,
                color: ion(tint),
            });
        }
        for _ in 0..look.many(6. * strength) {
            let angle = self.rand() * TAU;
            let from = at + V2::from_angle(angle) * r * self.between((0.2, 1.3));
            let size = r * self.between((0.9, 1.6));
            let life = self.between((0.8, 1.5));
            self.smoke(from, angle, (20., 90.), size, life);
        }
        self.scar(at, r * 2.9 * strength);
        self.pending_shake += 1.6 * strength;
    }

    /// A round of the stream striking something.
    pub(super) fn stream_hit(&mut self, at: V2, r: f32, silk: Option<Color>) {
        let look = self.look;
        let (fuel, core, spark) = silk.map_or((Fuel::Ion, RAIL_ION, ion(0.3)), |color| {
            (Fuel::Spectrum, color, mix(color, WHITE, 0.25))
        });
        self.fueled_burst(at, Flash::Hit, fuel, r * 0.8, core, 0.18);
        self.spray(Spray {
            at,
            heading: None,
            count: look.many(5.),
            speed: (120., 620.),
            life: (0.1, 0.3),
            size: (1., 2.4),
            color: spark,
        });
    }

    /// A Railgunner has been destroyed. It takes a moment to go up.
    pub(super) fn rail_defeated(&mut self, at: V2, aim: f32, radius: f32) {
        if self.wrecks.len() >= MOST_WRECKS {
            self.wrecks.remove(0);
        }
        self.wrecks.push(Wreck {
            at,
            aim,
            radius,
            age: 0.,
            fuse: 0.,
        });
        self.pending_shake += 1.2;
    }

    /// Run the tracers and wrecks on by `dt`.
    pub(super) fn run_rail(&mut self, dt: f32) {
        for tracer in &mut self.tracers {
            if tracer.round.is_none() {
                tracer.age += dt;
            }
        }
        // A round whose landing went unseen lets its line go in the end.
        for tracer in &mut self.tracers {
            if let Some(round) = tracer.round
                && !self.trails.contains_key(&round)
            {
                tracer.round = None;
            }
        }
        self.tracers.retain(|tracer| tracer.age < TRACER_SECONDS);
        let mut index = 0;
        while index < self.wrecks.len() {
            self.wrecks[index].age += dt;
            self.wrecks[index].fuse -= dt;
            let wreck = self.wrecks[index];
            if wreck.age >= WRECK_SECONDS {
                self.wrecks.remove(index);
                self.wreck_goes_up(wreck);
                continue;
            }
            if wreck.fuse <= 0. {
                let fuse = self.between((0.09, 0.2)) * (1. - 0.45 * wreck.age / WRECK_SECONDS);
                self.wrecks[index].fuse = fuse;
                self.wreck_pops(wreck);
            }
            index += 1;
        }
    }

    /// A charge cooking off somewhere on the hull or down the barrel.
    fn wreck_pops(&mut self, wreck: Wreck) {
        let look = self.look;
        let way = V2::from_angle(wreck.aim);
        let across = V2::new(-way.y, way.x);
        let on_barrel = self.rand() < 0.4;
        let spot = if on_barrel {
            let run = self.between((0.7, RAIL_MUZZLE));
            wreck.at + way * wreck.radius * run
        } else {
            let turn = self.rand() * TAU;
            wreck.at + V2::from_angle(turn) * wreck.radius * self.between((0.1, 0.95))
        } + across * self.between((-0.14, 0.14)) * wreck.radius;
        let size = wreck.radius * self.between((0.2, 0.42));
        self.fueled_burst(spot, Flash::Blast, Fuel::Ion, size * 0.6, ion(0.42), 0.42);
        self.spray(Spray {
            at: spot,
            heading: None,
            count: look.many(26.),
            speed: (160., 860.),
            life: (0.2, 0.6),
            size: (1.3, 3.4),
            color: ion(0.3),
        });
        self.plume(Plume {
            at: spot,
            scatter: size * 0.4,
            heading: None,
            speed: (30., 240.),
            size: (size * 0.3, size * 0.7),
            life: (0.25, 0.6),
            count: look.many(16.),
            burn: fire::STREAM,
        });
        self.pending_shake += 0.5;
    }

    /// The last of it.
    fn wreck_goes_up(&mut self, wreck: Wreck) {
        let look = self.look;
        let (at, radius) = (wreck.at, wreck.radius);
        self.cue(BossCue::Fall, at);
        self.burst(at, Flash::Fall, radius * 1.15, RAIL_ION, 2.1);
        self.burst(at, Flash::Phase, radius * 1.6, RAIL_FRINGE, 1.3);
        // The barrel goes last, in a line of blasts from breech to muzzle.
        let way = V2::from_angle(wreck.aim);
        for step in 0..5 {
            let run = 0.5 + step as f32 * 0.45;
            self.burst(
                at + way * radius * run,
                Flash::Blast,
                radius * 0.3,
                ion(0.38),
                0.5 + step as f32 * 0.09,
            );
        }
        for (count, speed, life, size, tint) in [
            (200., (240., 1700.), (0.5, 1.7), (1.6, 5.), 0.2),
            (90., (60., 420.), (1., 2.4), (1.2, 3.), 0.6),
        ] {
            self.spray(Spray {
                at,
                heading: None,
                count: look.many(count),
                speed,
                life,
                size,
                color: ion(tint),
            });
        }
        self.plume(Plume {
            at,
            scatter: radius * 0.8,
            heading: None,
            speed: (60., 760.),
            size: (radius * 0.24, radius * 0.6),
            life: (0.5, 1.4),
            count: look.many(170.),
            burn: fire::DENSE,
        });
        for _ in 0..look.many(14.) {
            let angle = self.rand() * TAU;
            let start = self.between((0.1, 0.9));
            let size = radius * self.between((0.45, 0.85));
            let life = self.between((1.5, 2.6));
            self.smoke(
                at + V2::from_angle(angle) * radius * start,
                angle,
                (14., 70.),
                size,
                life,
            );
        }
        self.scar(at, radius * 1.7);
        self.pending_shake += 3.6;
    }

    /// Everything lit on its hull: the coils, capacitors, engines, fins and
    /// the bore. Charging always shows; it's the warning.
    pub fn rail_lights(&self, light: &mut Emission, boss: &Boss, p: Vec2, r: f32, time: f32) {
        let Some(shown) = self.bosses.get(&boss.id) else {
            return;
        };
        let calm = self.look.calm;
        let level = self.look.level().clamp(0.45, 1.);
        let rail = shown.rail;
        let aim = shown.aim;
        let d = direction(aim);
        let n = perpendicular(d);
        let thrown = rail.recoil * RAIL_THROW;
        let at = |run: f32, out: f32| p + d * r * run + n * r * out;
        let energy = shown.charge.max(rail.beam);
        let barrel = RailBarrel::at(rail.open);
        let wide = rail.open.clamp(0., 1.);
        // The bore between the rails, which floods with light as it opens.
        light.beam(
            Layer::Wash,
            at(0.34 - thrown, 0.),
            at(2.34 - thrown, 0.),
            r * (0.2 + 0.1 * wide),
            alpha(ion(0.7), (0.08 + 0.42 * energy + 0.12 * wide) * level),
        );
        light.beam(
            Layer::Core,
            at(0.34 - thrown, 0.),
            at(2.34 - thrown, 0.),
            r * (0.016 + 0.03 * energy),
            alpha(ion(0.2 - 0.2 * energy), (0.32 + 0.68 * energy) * level),
        );
        if wide > 0.03 {
            // The rails themselves, hot along their inner faces.
            let heated = energy.max(rail.vent * 0.7);
            for out in [-1., 1.] {
                let (from, to) = (
                    at(0.36 - thrown, out * (barrel.rail_root - 0.03)),
                    at(2.27 - thrown, out * (barrel.rail_tip - 0.025)),
                );
                let tint = mix(Color::new(1., 0.5, 0.2, 1.), ion(0.18), energy);
                light.beam(
                    Layer::Wash,
                    from,
                    to,
                    r * 0.06,
                    alpha(tint, 0.3 * wide * heated * level),
                );
                light.beam(
                    Layer::Core,
                    from,
                    to,
                    (r * 0.01).max(0.7),
                    alpha(light.hot(tint, 0.5), wide * (0.25 + 0.6 * heated) * level),
                );
            }
        }
        // Each coil fires in turn, faster the closer it is to going off.
        let coils = RAIL_COILS.len() as f32;
        for (index, run) in RAIL_COILS.into_iter().enumerate() {
            let u = index as f32 / (coils - 1.);
            let lit = if calm {
                energy.max(0.14)
            } else if rail.beam > 0.02 {
                rail.beam * (0.72 + 0.28 * (time * 42. - u * 9.).sin())
            } else {
                // A wave running up the barrel, sharpening as it charges.
                let wave = 0.5 + 0.5 * (time * (1.4 + 15. * shown.charge) - u * 6.4).sin();
                0.12 + 0.2 * wave + shown.charge * (0.28 + 0.6 * wave.powi(3))
            };
            // Light on each half of the coil, wherever the armor has put it.
            let reach = 0.335 - index as f32 * 0.014;
            for out in [-1., 1.] {
                let run = barrel.coil_along(index, run);
                let on = |off: f32| {
                    let (along, across) = barrel.on_armor(out, run - thrown, off);
                    at(along, across)
                };
                let (from, to) = (on(barrel.gaps[index] + 0.04), on(reach - 0.04));
                light.beam(
                    Layer::Wash,
                    from,
                    to,
                    r * 0.075,
                    alpha(ion(0.55), 0.5 * lit * level),
                );
                light.beam(
                    Layer::Core,
                    from,
                    to,
                    (r * 0.017).max(0.8),
                    alpha(ion(0.12), lit.min(1.) * level),
                );
            }
        }
        // Capacitors around the turret, filling as it charges.
        const CELLS: usize = 10;
        let filled = shown.charge * CELLS as f32;
        for cell in 0..CELLS {
            let turn = aim + (cell as f32 + 0.5) / CELLS as f32 * TAU;
            let lit = ((filled - cell as f32).clamp(0., 1.)).max(rail.beam * 0.8);
            let breathe = if calm {
                0.6
            } else {
                0.5 + 0.5 * (time * 2.4 + cell as f32 * 0.63).sin()
            };
            let c = p + direction(turn) * r * 0.545;
            light.spot(
                Layer::Wash,
                c,
                r * (0.1 + 0.08 * lit),
                alpha(ion(0.5), (0.14 + 0.1 * breathe + 0.5 * lit) * level),
            );
            light.spot(
                Layer::Core,
                c,
                r * 0.05,
                alpha(ion(0.15), (0.3 + 0.2 * breathe + 0.5 * lit) * level),
            );
        }
        light.arc(
            Layer::Core,
            p,
            r * 0.4,
            (r * 0.012).max(0.8),
            aim + PI * 0.5,
            TAU * shown.charge,
            alpha(ion(0.2), 0.85 * shown.charge),
        );
        // Engine light, reaching further the harder it's driving.
        let drive = (boss.vel.length() / 150.).min(1.);
        let flicker = if calm {
            1.
        } else {
            0.88 + 0.12 * (time * 31.).sin()
        };
        for out in [-1., 1.] {
            let nozzle = at(-1.31, out * 0.7);
            light.spot(
                Layer::Wash,
                nozzle,
                r * (0.3 + 0.18 * drive),
                alpha(ion(0.62), (0.3 + 0.2 * drive) * level * flicker),
            );
            light.oval(
                Layer::Core,
                nozzle - d * r * (0.05 + 0.2 * drive),
                d * r * (0.12 + 0.36 * drive) * flicker,
                r * 0.09,
                alpha(ion(0.16), (0.6 + 0.4 * drive) * level),
            );
        }
        // Fins glowing as they dump heat, white at the hinge.
        if rail.vent > 0.02 {
            let heat = Color::new(1., 0.52, 0.16, 1.);
            for out in [-1., 1.] {
                for (index, root) in [1.94_f32, 2.36, 2.78].into_iter().enumerate() {
                    let hinge = aim + out * root;
                    let lean = aim + out * (root + rail.vent * (0.36 - index as f32 * 0.18));
                    let base = p + direction(hinge) * r * 0.9;
                    let tip = base + direction(lean) * r * (0.5 + 0.17 * rail.vent);
                    light.streak(
                        Layer::Wash,
                        tip,
                        base,
                        r * 0.14,
                        alpha(heat, 0.2 * rail.vent),
                        alpha(heat, 0.72 * rail.vent),
                    );
                    light.streak(
                        Layer::Core,
                        tip,
                        base,
                        r * 0.035,
                        alpha(light.hot(heat, 0.3), 0.4 * rail.vent),
                        alpha(light.hot(heat, 0.92), rail.vent),
                    );
                }
            }
        }
        // The arc across the muzzle while the beam runs.
        if rail.beam > 0.02 {
            let tip = at(RAIL_MUZZLE - thrown, 0.);
            let beat = if calm {
                0.6
            } else {
                0.5 + 0.5 * (time * 33.).sin()
            };
            light.spot(
                Layer::Wash,
                tip,
                r * (0.62 + 0.12 * beat),
                alpha(ion(0.82), 0.55 * rail.beam * level),
            );
            light.spot(
                Layer::Wash,
                tip,
                r * (0.34 + 0.06 * beat),
                alpha(ion(0.42), 0.8 * rail.beam * level),
            );
            light.spot(
                Layer::Core,
                tip,
                r * (0.17 + 0.03 * beat),
                alpha(ion(0.), rail.beam),
            );
            for out in [-1., 1.] {
                for which in 0..2 {
                    let (_, claw) = barrel.claw(out, which);
                    let claw = at(claw.0 - thrown, claw.1);
                    light.spot(
                        Layer::Core,
                        claw,
                        (r * 0.04).max(1.),
                        alpha(ion(0.1), 0.9 * rail.beam),
                    );
                    super::shots::bolt(
                        light,
                        claw,
                        tip + d * r * 0.1,
                        (r * 0.016).max(0.8),
                        alpha(ion(0.1), (0.85 - which as f32 * 0.25) * rail.beam),
                        (time * 21.).floor() * 1.7 + out + which as f32 * 2.3,
                    );
                }
            }
        }
    }

    /// The red thread from its barrel to where it's aiming, a bracket on the
    /// tank it has marked, and the mark tightening as the shot gets close.
    /// Drawn even when the Railgunner itself is off screen.
    #[allow(clippy::too_many_arguments)]
    pub fn rail_sight(
        &self,
        light: &mut Emission,
        view: &View,
        boss: &Boss,
        at: V2,
        walls: impl Iterator<Item = Rect>,
        mark: Option<V2>,
        time: f32,
    ) {
        if boss.arrival > 0. || boss.beaming() || boss.venting() {
            return;
        }
        let Some(shown) = self.bosses.get(&boss.id) else {
            return;
        };
        let z = view.zoom;
        let calm = self.look.calm;
        let aim = shown.aim;
        let way = V2::from_angle(aim);
        let from = at + way * boss.radius * RAIL_MUZZLE;
        let far = at + way * RAIL_SIGHT;
        let stop = walls
            .filter_map(|wall| tankyard_core::physics::sweep_rect(from, far, 2., wall))
            .map(|(t, _)| t)
            .fold(1_f32, f32::min);
        let end = from.lerp(far, stop);
        let (a, b) = (view.screen(from), view.screen(end));
        let windup = boss.attack_windup;
        let charging = windup > 0.;
        let locked = charging && windup < RAIL_LOCK;
        let tension = if charging {
            (1. - windup / boss.warning_seconds()).clamp(0., 1.)
        } else {
            0.
        };
        let deep = Color::new(1., 0.16, 0.26, 1.);
        let line = mix(deep, ion(0.3), tension * 0.25);
        // Only a target gets the thread: a boss with nobody to shoot has no
        // business pointing at the floor.
        if boss.target != 0 {
            let strength = 0.34 + 0.42 * tension + if locked { 0.24 } else { 0. };
            light.beam(
                Layer::Wash,
                a,
                b,
                (5.5 + 5. * tension) * z,
                alpha(line, 0.16 + 0.24 * tension),
            );
            light.beam(
                Layer::Core,
                a,
                b,
                ((0.95 + 0.7 * tension) * z).max(0.75),
                alpha(light.hot(line, 0.28 + 0.65 * tension), strength),
            );
            if !calm {
                // Beads running along it toward wherever it lands.
                let span = a.distance(b);
                let d = (b - a).normalize_or_zero();
                let beads = (span / (170. * z)).clamp(1., 14.) as usize;
                for bead in 0..beads {
                    let u = (time * (0.5 + 1.2 * tension) + bead as f32 / beads as f32).fract();
                    light.oval(
                        Layer::Core,
                        a + d * span * u,
                        d * (9. + 12. * tension) * z,
                        (1.5 * z).max(0.8),
                        alpha(light.hot(line, 0.5), 0.6 * (u * PI).sin()),
                    );
                }
            }
            // A dot where it strikes cover.
            light.spot(Layer::Wash, b, (15. + 13. * tension) * z, alpha(line, 0.46));
            light.spot(
                Layer::Core,
                b,
                (3.2 + 2.2 * tension) * z,
                alpha(light.hot(line, 0.7), 0.85),
            );
        }
        // Brackets on whoever it has marked.
        if let Some(mark) = mark {
            let c = view.screen(mark);
            let spin = if calm {
                0.
            } else {
                time * (0.55 + 1.6 * tension)
            };
            let radius = (70. - 26. * ease_out(tension)) * z;
            let tint = mix(deep, WHITE, if locked { 0.55 } else { 0.05 });
            let width = ((1.3 + 1.2 * tension) * z).max(0.9);
            for quarter in 0..4 {
                let start = spin + quarter as f32 * PI * 0.5 + 0.34;
                light.arc(
                    Layer::Wash,
                    c,
                    radius,
                    width * 3.4,
                    start,
                    0.9,
                    alpha(deep, 0.2 + 0.28 * tension),
                );
                light.arc(
                    Layer::Core,
                    c,
                    radius,
                    width,
                    start,
                    0.9,
                    alpha(light.hot(tint, 0.3), 0.7 + 0.3 * tension),
                );
                let tick = direction(start + 0.45);
                light.streak(
                    Layer::Core,
                    c + tick * (radius + 11. * z),
                    c + tick * (radius - 7. * z),
                    width,
                    alpha(tint, 0.),
                    alpha(light.hot(tint, 0.4), 0.8),
                );
            }
        }
    }

    /// A rail round in flight: a white-hot slug in a sleeve of ions.
    pub(super) fn rail_round(&self, light: &mut Emission, round: &Round) {
        let Round {
            p,
            r,
            heading,
            opacity,
            damage,
            z,
            ..
        } = *round;
        let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
        let n = perpendicular(d);
        let level = self.look.level().clamp(0.55, 1.);
        let clock = self.flicker(round);
        let scale = if damage >= LETHAL_DAMAGE { 1. } else { 0.86 };
        let rim = (1.8 * z).max(1.);
        // The solid body, so it shows on any floor and at any setting.
        let (tip, tail) = (p + d * r * 1.25, p - d * r * 1.05);
        let half = r * 0.56;
        let edge = alpha(mix(RAIL_ION, OUTLINE, 0.82), opacity);
        draw_triangle(tip + d * rim, p + n * (half + rim), tail - d * rim, edge);
        draw_triangle(tip + d * rim, p - n * (half + rim), tail - d * rim, edge);
        let body = alpha(ion(0.28), opacity);
        draw_triangle(tip, p + n * half, tail, body);
        draw_triangle(tip, p - n * half, tail, body);
        polygon(p, 0, r * 0.3, 0., alpha(WHITE, opacity));
        light.oval(
            Layer::Wash,
            p - d * r * 2.4,
            d * r * 6.,
            r * 1.65,
            alpha(ion(0.92), 0.36 * opacity * level * scale),
        );
        light.oval(
            Layer::Wash,
            p - d * r * 0.9,
            d * r * 3.4,
            r * 1.2,
            alpha(ion(0.5), 0.66 * opacity * level),
        );
        light.oval(
            Layer::Core,
            p,
            d * r * 2.3,
            r * 0.6,
            alpha(ion(0.04), opacity * level.max(0.8)),
        );
        // Rings of ions shed behind it.
        if clock != 0. {
            for collar in 0..4 {
                let u = (clock * 7. + collar as f32 / 4.).fract();
                let at = p - d * r * (0.5 + u * 5.4);
                let across = r * (0.95 + 0.7 * u);
                light.oval(
                    Layer::Core,
                    at,
                    n * across,
                    (r * 0.1).max(1.),
                    alpha(ion(0.25 + 0.5 * u), 0.7 * (1. - u) * opacity * level),
                );
            }
        }
    }

    /// The plasma streams in view.
    pub fn rail_streams(
        &self,
        light: &mut Emission,
        view: &View,
        bosses: &[Boss],
        streams: &mut Streams,
        time: f32,
    ) {
        let level = self.look.level().clamp(0.6, 1.);
        let clock = if self.look.calm { 0. } else { time };
        // Drawn a little wider than the rounds that do the damage, so what
        // looks like it should have hit you did.
        let half = BEAM_ROUND_RADIUS * 1.3 * view.zoom;
        let reach = STREAM_GAP * view.zoom;
        for slot in 0..MOST_STREAMS {
            let (owner, count) = (streams.owners[slot], streams.counts[slot]);
            if owner == 0 || count == 0 {
                continue;
            }
            let points = &mut streams.points[slot][..count];
            points.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
            let shown = self.bosses.get(&owner);
            let spectrum = shown.is_some_and(|p| p.kind == Some(BossKind::Jester));
            let running = bosses
                .iter()
                .find(|boss| boss.id == owner)
                .is_some_and(Boss::beaming);
            // While it runs the stream starts at the muzzle itself.
            let muzzle = shown.filter(|_| running).map(|shown| {
                let way = direction(shown.aim);
                let thrown = if spectrum {
                    0.
                } else {
                    shown.rail.recoil * RAIL_THROW
                };
                let muzzle = if spectrum {
                    tankyard_core::encounters::CARD_MUZZLE
                } else {
                    RAIL_MUZZLE
                };
                view.screen(shown.pos + shown.kick_v2())
                    + way * shown.radius * (muzzle - thrown) * view.zoom
            });
            let mut spine = [Vec2::ZERO; STREAM_POINTS + 1];
            let mut used = 0;
            if let Some(tip) = muzzle {
                spine[0] = tip;
                used = 1;
            }
            let mut first = true;
            for &(at, _) in points.iter() {
                let joined = used == 0 || spine[used - 1].distance(at) <= reach;
                if !joined {
                    stream(
                        light,
                        &spine[..used],
                        half,
                        clock,
                        level,
                        first && muzzle.is_some(),
                        spectrum,
                    );
                    first = false;
                    used = 0;
                }
                spine[used] = at;
                used += 1;
            }
            stream(
                light,
                &spine[..used],
                half,
                clock,
                level,
                first && muzzle.is_some(),
                spectrum,
            );
            // Where it's landing right now.
            if used >= 2 && running {
                let end = spine[used - 1];
                let beat = if clock == 0. {
                    0.6
                } else {
                    0.5 + 0.5 * (clock * 29.).sin()
                };
                light.spot(
                    Layer::Wash,
                    end,
                    half * (4.4 + 1.2 * beat),
                    alpha(ion(0.8), 0.42 * level),
                );
                light.spot(
                    Layer::Core,
                    end,
                    half * (1.2 + 0.5 * beat),
                    alpha(ion(0.05), 0.85 * level),
                );
            }
        }
    }

    /// Lines of past shots, hanging in the air and twisting apart.
    pub fn rail_tracers(&self, light: &mut Emission, view: &View) {
        if !self.look.live() {
            return;
        }
        let level = self.look.level().min(1.);
        for tracer in &self.tracers {
            let t = (tracer.age / TRACER_SECONDS).clamp(0., 1.);
            let fade = (1. - t) * (1. - t);
            let (a, b) = (view.screen(tracer.from), view.screen(tracer.to));
            let span = a.distance(b);
            if span < 2. {
                continue;
            }
            let d = (b - a) / span;
            let n = perpendicular(d);
            let r = tracer.radius * view.zoom;
            let half = r * (0.42 + 1.3 * ease_out(t));
            light.beam(
                Layer::Wash,
                a,
                b,
                half * 2.2,
                alpha(ion(0.96), 0.2 * fade * level),
            );
            light.beam(
                Layer::Wash,
                a,
                b,
                half,
                alpha(ion(0.56), 0.42 * fade * level),
            );
            light.beam(
                Layer::Core,
                a,
                b,
                (r * 0.2 * (1. - t)).max(0.8),
                alpha(ion(0.06), 0.95 * fade * level),
            );
            // Two strands winding round the line and drifting apart.
            const STOPS: usize = 40;
            let turns = (span / (r * 5.5).max(10.)).clamp(3., 18.);
            let spread = r * (0.34 + 1.2 * ease_out(t));
            for (strand, tint) in [(0., ion(0.3)), (PI, mix(RAIL_FRINGE, WHITE, 0.25))] {
                let mut stops = [Stop {
                    at: a,
                    half: 0.,
                    color: Color::new(0., 0., 0., 0.),
                }; STOPS];
                for (index, stop) in stops.iter_mut().enumerate() {
                    let u = index as f32 / (STOPS - 1) as f32;
                    let phase = u * turns * TAU + tracer.seed + strand + tracer.age * 3.;
                    let ends = (u / 0.04).min(1.) * ((1. - u) / 0.06).min(1.);
                    *stop = Stop {
                        at: a + d * span * u + n * phase.sin() * spread,
                        half: (r * 0.07).max(0.8) * (0.7 + 0.3 * phase.cos()),
                        color: alpha(tint, (0.38 + 0.42 * phase.cos()) * fade * ends * level),
                    };
                }
                light.band(Layer::Core, &stops);
            }
        }
    }

    /// Light its shots and beams throw on a dark arena.
    pub(super) fn rail_world_lights(&self, view: &View, out: &mut Vec<Light>) {
        let z = view.zoom;
        for tracer in &self.tracers {
            let fade = 1. - (tracer.age / TRACER_SECONDS).clamp(0., 1.);
            for step in 0..4 {
                let at = tracer.from.lerp(tracer.to, (step as f32 + 0.5) / 4.);
                if view.visible(at, 320.) {
                    crate::lighting::gather(
                        out,
                        Light::Spot {
                            at: view.screen(at),
                            radius: 320. * z,
                            color: alpha(ion(0.5), 0.34 * fade * fade),
                        },
                    );
                }
            }
        }
        for shown in self.bosses.values() {
            if shown.fuel != Fuel::Ion || shown.rail.beam <= 0.02 || shown.unseen > 0. {
                continue;
            }
            let way = V2::from_angle(shown.aim);
            for step in 0..6 {
                let reach = shown.radius * RAIL_MUZZLE + step as f32 * 340.;
                let at = shown.pos + way * reach;
                if view.visible(at, 340.) {
                    crate::lighting::gather(
                        out,
                        Light::Spot {
                            at: view.screen(at),
                            radius: 340. * z,
                            color: alpha(ion(0.46), 0.5 * shown.rail.beam),
                        },
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_ramp_runs_from_white_through_ion_to_violet() {
        let (white, middle, violet) = (ion(0.), ion(0.5), ion(1.));
        assert!(white.r > 0.99 && white.g > 0.99 && white.b > 0.99);
        assert!(middle.b > middle.r);
        assert!(violet.b > violet.g && violet.r > violet.g);
        for n in -2..=12 {
            let c = ion(n as f32 / 10.);
            assert!(c.r.is_finite() && c.g.is_finite() && c.b.is_finite());
        }
    }

    #[test]
    fn a_stream_keeps_its_points_and_never_outgrows_its_room() {
        let mut streams = Streams::default();
        for n in 0..(STREAM_POINTS + 20) {
            streams.note(7, vec2(n as f32, 0.), n as f32);
        }
        assert_eq!(streams.counts[0], STREAM_POINTS);
        for owner in 8..12 {
            streams.note(owner, Vec2::ZERO, 0.);
        }
        assert_eq!(
            streams.owners.iter().filter(|&&id| id != 0).count(),
            MOST_STREAMS
        );
    }

    #[test]
    fn rounding_a_stream_keeps_its_points_and_softens_its_corners() {
        let bent = [
            vec2(0., 0.),
            vec2(70., 0.),
            vec2(140., 0.),
            vec2(190., 50.),
            vec2(190., 120.),
        ];
        let mut out = [Vec2::ZERO; ROUNDED_POINTS];
        let used = rounded(&bent, &mut out);
        assert_eq!(used, bent.len() * 2 - 1);
        for (index, point) in bent.iter().enumerate() {
            assert_eq!(out[index * 2], *point);
        }
        // The new point rides outside the chord, toward the outside of the bend.
        let chord = (bent[2] + bent[3]) * 0.5;
        assert!(out[5].x > chord.x && out[5].y < chord.y);

        // A straight line stays straight, and bad spacing can't throw it wide.
        let line: [Vec2; 4] = [vec2(0., 0.), vec2(1., 0.), vec2(500., 0.), vec2(501., 0.)];
        let used = rounded(&line, &mut out);
        assert!(out[..used].iter().all(|point| point.y == 0.));
        let wild = [
            vec2(0., 900.),
            vec2(0., 0.),
            vec2(10., 0.),
            vec2(10., -900.),
        ];
        rounded(&wild, &mut out);
        assert!(out[3].distance(vec2(5., 0.)) <= 2.5 + 1e-3);

        // More points than a stream can hold are cut off, never overrun.
        let long = [Vec2::ONE; STREAM_POINTS + 30];
        assert!(rounded(&long, &mut out) <= ROUNDED_POINTS);
    }

    #[test]
    fn along_a_spine_walks_from_end_to_end() {
        let spine = [vec2(0., 0.), vec2(10., 0.), vec2(10., 10.)];
        assert_eq!(along(&spine, 0.), spine[0]);
        assert_eq!(along(&spine, 1.), spine[2]);
        assert_eq!(along(&spine, 0.5), spine[1]);
        assert!(along(&spine, 9.).is_finite());
    }

    #[test]
    fn a_wreck_comes_apart_and_clears_itself() {
        let mut fx = BossFx::default();
        fx.rail_defeated(V2::new(500., 500.), 0.4, 132.);
        assert_eq!(fx.wrecks().count(), 1);
        let mut shook = 0.;
        for _ in 0..120 {
            fx.update(1. / 60.);
            shook += fx.take_shake();
        }
        assert_eq!(fx.wrecks().count(), 0);
        assert!(
            shook > 3.,
            "it should shake the camera when it goes: {shook}"
        );
        assert!(
            fx.take_cues().iter().any(|(cue, _)| *cue == BossCue::Fall),
            "the last blast has a sound"
        );
    }

    #[test]
    fn a_tracer_outlives_its_round_then_fades() {
        let mut fx = BossFx::default();
        fx.tracers.push(Tracer {
            round: None,
            from: V2::ZERO,
            to: V2::new(800., 0.),
            radius: 30.,
            age: 0.,
            seed: 0.,
        });
        fx.update(TRACER_SECONDS * 0.5);
        assert_eq!(fx.tracers.len(), 1);
        fx.update(TRACER_SECONDS);
        assert!(fx.tracers.is_empty());
    }
}
