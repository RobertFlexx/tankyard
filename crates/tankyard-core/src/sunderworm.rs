//! The Sunderworm: something vast that lives under the arena floor. It is in
//! no list and on no menu. It comes up on its own about once in two million
//! boss events, and the Boss Rush roster turns its card up about once in
//! twenty-four thousand visits.
//!
//! It hunts by the shaking of the ground, so walls don't hide anyone from it.
//! It strikes with its jaws, spits bone, sprays bile, breeds, and dives to
//! come up under whoever it wants. Each phase adds to what it does, and near
//! the end it tears itself in two. Both halves go on fighting on their own,
//! the head with its jaws and the tail with what it has left.
//!
//! Protocol 35 sends its own boss kind and its authoritative body rings,
//! so late joiners see the same collision shape without its movement history.
//! How deep its head is and what it is doing ride in fields its kind leaves
//! unused.
use crate::boss_brain::{Read, curved_lead, draw};
use crate::bosses::Volley;
use crate::physics::{circle_rect, sweep_rect};
use crate::*;
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use std::ops::Range;
use tankyard_content::ProjectileKind;

/// The whole worm's head: the largest boss there is.
pub const WORM_RADIUS: f32 = 150.;
/// The half with the head, once it has torn apart.
pub const WORM_FRONT_RADIUS: f32 = 128.;
/// The half with the tail.
pub const WORM_BACK_RADIUS: f32 = 122.;
/// Rings in its whole body, head first.
pub const WORM_RINGS: usize = 28;
/// Where it tears: the front half keeps the rings before this one.
pub const WORM_TEAR: usize = 13;
/// How far apart its rings sit along its body.
pub const WORM_SPACING: f32 = 66.;
/// How far its head travels between the points of its path.
pub const PATH_STEP: f32 = 11.;
/// The share of its health at which the whole worm enters phases 2 to 5.
pub const WORM_PHASES: [f32; 4] = [0.84, 0.67, 0.5, 0.33];
/// The share at which it tears in two.
pub const WORM_SPLIT: f32 = 0.16;
/// Phase boundaries and the split, for the boss health display.
pub const WORM_MARKS: [f32; 5] = [
    WORM_PHASES[0],
    WORM_PHASES[1],
    WORM_PHASES[2],
    WORM_PHASES[3],
    WORM_SPLIT,
];
/// What each half starts with, as a share of the whole worm's full health.
pub const HALF_HEALTH: f32 = 0.15;
/// A ring this deep or shallower is above ground: it can be hit and it
/// bumps into things. Deeper ones are out of reach.
pub const SURFACED: f32 = 0.42;
/// Seconds from when the ground starts to shake to when it bursts out.
pub const RISE_SECONDS: f32 = 3.;
/// How far the ground breaks when it comes up under someone.
pub const ERUPT_RADIUS: f32 = 250.;
/// The chance a random boss event brings it: one in about two million.
pub const WORM_ODDS: f32 = 1. / 2_000_000.;
/// How strong the arena must be, in full-level tanks, for it to come up on
/// its own.
pub const WORM_STRENGTH: f32 = 3.;
/// How far it can feel a tank through the ground.
const SENSE: f32 = 2800.;
/// How far from its prey it circles while it hunts, at first.
const PROWL: f32 = 560.;
/// Its swarm of larvae, at most.
pub const WORM_BROOD: usize = 14;

/// Which of it this is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WormPart {
    #[default]
    Whole,
    /// The half with the head.
    Front,
    /// The half with the tail, torn end first.
    Back,
}

impl WormPart {
    /// The size it goes out on the wire with.
    pub fn radius(self) -> f32 {
        match self {
            Self::Whole => WORM_RADIUS,
            Self::Front => WORM_FRONT_RADIUS,
            Self::Back => WORM_BACK_RADIUS,
        }
    }
    /// Which part a Sunderworm of this size is.
    pub fn of_radius(radius: f32) -> Option<Self> {
        [Self::Whole, Self::Front, Self::Back]
            .into_iter()
            .find(|part| part.radius() == radius)
    }
    /// Its rings, counted along the whole worm from the head.
    pub fn rings(self) -> Range<usize> {
        match self {
            Self::Whole => 0..WORM_RINGS,
            Self::Front => 0..WORM_TEAR,
            Self::Back => WORM_TEAR..WORM_RINGS,
        }
    }
    /// How long it is, from its first ring to its last.
    pub fn length(self) -> f32 {
        (self.rings().len() - 1) as f32 * WORM_SPACING
    }
    /// The name it goes by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Whole => "Sunderworm",
            Self::Front => "Sunderworm head",
            Self::Back => "Sunderworm tail",
        }
    }
}

/// How wide its `k`th ring is, counted along the whole worm from the head:
/// a thick neck just behind the jaws, then a long taper to the tail.
pub fn ring_radius(k: usize) -> f32 {
    let u = (k as f32 / (WORM_RINGS - 1) as f32).clamp(0., 1.);
    let neck = if u < 0.24 {
        (u / 0.24 * PI).sin() * 0.06
    } else {
        0.
    };
    WORM_RADIUS * 0.94 * (1. + neck) * (1. - 0.7 * u.powf(1.5))
}

/// What it is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WormAct {
    /// Coming up out of the ground: shaking it first, then bursting out.
    #[default]
    Rise,
    /// Going after its prey on the surface.
    Hunt,
    /// Rearing back, then striking in a straight line with open jaws.
    Lunge,
    /// A fan of bone shards from its mouth.
    Spit,
    /// A sweeping spray of bile that lies where it lands.
    Bile,
    /// Going under.
    Dive,
    /// Moving underground, a ridge of broken floor above it.
    Tunnel,
    /// Breaking out under someone, with a warning ring first.
    Erupt,
    /// Larvae bursting out of its sides.
    Brood,
    /// Spines from every ring, rolling down its body.
    Thrash,
    /// Circling a tank to close it in.
    Coil,
    /// A new phase: it rears and screams, and the air throws tanks back.
    Roar,
    /// Tearing itself in two.
    Tear,
    /// The tail half swinging round hard.
    Whip,
    /// A torn end spraying blood that lingers.
    Bleed,
    /// Barbs from the tail's tip that follow their mark.
    Sting,
}

impl WormAct {
    pub const ALL: [Self; 16] = [
        Self::Rise,
        Self::Hunt,
        Self::Lunge,
        Self::Spit,
        Self::Bile,
        Self::Dive,
        Self::Tunnel,
        Self::Erupt,
        Self::Brood,
        Self::Thrash,
        Self::Coil,
        Self::Roar,
        Self::Tear,
        Self::Whip,
        Self::Bleed,
        Self::Sting,
    ];
}

/// What a client reads off a Sunderworm in a snapshot: how deep its head
/// is, what it is doing, which stage of it (0 warning, 1 doing it, 2 coming
/// out of it), how far through that stage, and its real phase, 1 to 7.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WormSign {
    pub depth: f32,
    pub act: WormAct,
    pub stage: u8,
    pub progress: f32,
    pub phase: u8,
}

impl WormSign {
    /// Reads it from a boss the host wrote one into.
    pub fn of(boss: &Boss) -> Self {
        let code = boss.goal.y.max(0.);
        let whole = code.floor();
        let index = (whole as usize / 4).min(WormAct::ALL.len() - 1);
        Self {
            depth: boss.goal.x.clamp(0., 1.),
            act: WormAct::ALL[index],
            stage: (whole as u32 % 4).min(2) as u8,
            progress: (code - whole).clamp(0., 1.),
            phase: (boss.tactic_time.round() as u8).clamp(1, 7),
        }
    }
    /// Writes it into the fields a Sunderworm doesn't use, the only ones a
    /// client reads it from. The wire's own phase stays within what older
    /// clients accept.
    pub fn write(self, boss: &mut Boss) {
        let index = WormAct::ALL
            .iter()
            .position(|&act| act == self.act)
            .unwrap_or(0);
        let code = (index * 4 + usize::from(self.stage.min(2))) as f32;
        boss.goal = V2::new(
            self.depth.clamp(0., 1.),
            code + self.progress.clamp(0., 0.999),
        );
        boss.tactic = BossTactic::Patrol;
        boss.tactic_time = f32::from(self.phase.clamp(1, 7));
        boss.phase = self.phase.clamp(1, 3);
        boss.attack = 0;
        boss.attack_windup = 0.;
        boss.attack_timer = 5.;
        boss.timer = 1.;
    }
}

/// The whole worm's phase at `share` of its health, 1 to 5.
pub fn whole_phase(share: f32) -> u8 {
    1 + WORM_PHASES.iter().filter(|&&line| share < line).count() as u8
}

/// Where its head has been, newest first, a point every [`PATH_STEP`]. The
/// host and every client keep one, and lay the same rings along it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WormPath {
    /// The head now, and how deep it is.
    head: (V2, f32),
    points: VecDeque<(V2, f32)>,
}

impl WormPath {
    /// A body lying straight out behind a head at `at` facing `heading`.
    pub fn straight(at: V2, heading: f32, depth: f32, length: f32) -> Self {
        let back = V2::from_angle(heading + PI);
        let count = (length / PATH_STEP).ceil() as usize + 3;
        Self {
            head: (at, depth),
            points: (1..=count)
                .map(|n| (at + back * (n as f32 * PATH_STEP), depth))
                .collect(),
        }
    }
    /// The head now.
    pub fn head(&self) -> (V2, f32) {
        self.head
    }
    /// Move the head to `at`, `depth` deep, keeping `keep` of body behind it.
    pub fn follow(&mut self, at: V2, depth: f32, keep: f32) {
        if !at.finite() {
            return;
        }
        self.head = (at, depth);
        let Some(&(mut last, mut last_depth)) = self.points.front() else {
            self.points.push_front((at, depth));
            return;
        };
        let mut laid = 0;
        loop {
            let gap = at - last;
            let length = gap.length();
            if length < PATH_STEP || laid > 400 {
                break;
            }
            let t = PATH_STEP / length;
            last += gap * t;
            last_depth += (depth - last_depth) * t;
            self.points.push_front((last, last_depth));
            laid += 1;
        }
        self.points.truncate((keep / PATH_STEP).ceil() as usize + 4);
    }
    /// The point `arc` back along the body from the head, and how deep it is.
    pub fn at(&self, arc: f32) -> (V2, f32) {
        let (head, head_depth) = self.head;
        let Some(&(first, first_depth)) = self.points.front() else {
            return self.head;
        };
        let lead = head.distance(first);
        if arc <= lead {
            let t = if lead > 1e-4 { arc / lead } else { 0. };
            return (
                head.lerp(first, t),
                head_depth + (first_depth - head_depth) * t,
            );
        }
        let along = (arc - lead) / PATH_STEP;
        let index = along.floor() as usize;
        let t = along - index as f32;
        match (self.points.get(index), self.points.get(index + 1)) {
            (Some(&(a, da)), Some(&(b, db))) => (a.lerp(b, t), da + (db - da) * t),
            _ => self.points.back().copied().unwrap_or(self.head),
        }
    }
    /// The way the body runs at `arc`, from its tail end toward its head.
    pub fn heading_at(&self, arc: f32) -> V2 {
        let ahead = self.at((arc - PATH_STEP).max(0.)).0;
        let behind = self.at(arc + PATH_STEP).0;
        let way = (ahead - behind).normalized();
        if way == V2::ZERO {
            V2::new(1., 0.)
        } else {
            way
        }
    }
    /// The rest of the path from `arc` back, as a path of its own: what a
    /// torn-off tail follows.
    pub fn behind(&self, arc: f32) -> Self {
        let head = self.at(arc);
        let lead = self
            .points
            .front()
            .map_or(0., |&(first, _)| self.head.0.distance(first));
        let skip = if arc <= lead {
            0
        } else {
            ((arc - lead) / PATH_STEP).floor() as usize + 1
        };
        Self {
            head,
            points: self.points.iter().skip(skip).copied().collect(),
        }
    }
    /// Every point it holds, head first, for drawing the ground it broke.
    pub fn points(&self) -> impl Iterator<Item = (V2, f32)> + '_ {
        std::iter::once(self.head).chain(self.points.iter().copied())
    }
}

/// One ring of its body where it is now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ring {
    pub pos: V2,
    pub radius: f32,
    pub depth: f32,
}

/// The collision shape sent to viewers, including players joining mid-fight.
/// Radii follow the known ring profile rather than costing another float each.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WormBody {
    pub id: EntityId,
    pub centers: Vec<(V2, f32)>,
}

pub(crate) fn snapshot_bodies(bosses: &[Boss], bodies: &mut Vec<WormBody>) {
    let mut count = 0;
    for boss in bosses {
        let Some(work) = boss.worm.as_deref() else {
            continue;
        };
        if count == bodies.len() {
            bodies.push(WormBody {
                id: boss.id,
                centers: Vec::with_capacity(WORM_RINGS),
            });
        }
        let body = &mut bodies[count];
        body.id = boss.id;
        body.centers.clear();
        body.centers
            .extend(work.rings.iter().map(|r| (r.pos, r.depth)));
        count += 1;
    }
    bodies.truncate(count);
}

impl Ring {
    /// Whether it is above ground.
    pub fn surfaced(&self) -> bool {
        self.depth < SURFACED
    }
}

/// Lays the rings of `part` along `path`, head first.
pub fn lay_rings(path: &WormPath, part: WormPart, out: &mut Vec<Ring>) {
    out.clear();
    let first = part.rings().start;
    for k in part.rings() {
        let (pos, depth) = path.at((k - first) as f32 * WORM_SPACING);
        out.push(Ring {
            pos,
            radius: ring_radius(k),
            depth,
        });
    }
}

/// How many kinds of act it keeps a rest timer for.
const ACTS: usize = WormAct::ALL.len();

/// What a Sunderworm is in the middle of. Only the host runs it, so it never
/// travels in a snapshot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WormWork {
    pub part: WormPart,
    pub path: WormPath,
    /// Where each of its rings is this tick, head first.
    pub rings: Vec<Ring>,
    /// A circle around every ring above ground, for a quick first test.
    pub reach: (V2, f32),
    /// How deep its head is, from 0 on the surface to 1 out of reach.
    pub depth: f32,
    heading: f32,
    speed: f32,
    act: WormAct,
    stage: u8,
    /// Seconds into this stage, and how long it lasts.
    clock: f32,
    length: f32,
    /// Where it strikes, comes up or circles.
    spot: V2,
    /// Which way it sweeps or circles.
    turn: f32,
    /// Seconds until each act can come again.
    rest: [f32; ACTS],
    /// Seconds of hunting before it tries anything else.
    lull: f32,
    /// Seconds to the next round of a stream or a wave, and how many more.
    beat: f32,
    left: u8,
    /// How far down its body a wave of spines has got.
    wave: usize,
    /// Tanks this strike has already hit.
    struck: Vec<EntityId>,
    /// Its real phase, 1 to 7.
    pub phase: u8,
    /// The other half, once it has torn.
    pub pair: EntityId,
    /// Its slither, in radians of phase.
    sway: f32,
    /// How much it has been hurt lately, as a share of its health.
    stung: f32,
    /// Its health last tick.
    health: f32,
    /// How long it has had no clear way ahead.
    boxed: f32,
}

impl WormWork {
    /// A worm under the ground at `at`, about to come up facing `heading`.
    pub fn rising(at: V2, heading: f32) -> Self {
        let part = WormPart::Whole;
        let path = WormPath::straight(at, heading, 1., part.length());
        let mut work = Self {
            part,
            path,
            depth: 1.,
            heading,
            act: WormAct::Rise,
            length: RISE_SECONDS - 0.4,
            phase: 1,
            lull: 1.5,
            turn: 1.,
            ..Default::default()
        };
        work.place();
        work
    }
    /// What clients should draw.
    pub fn sign(&self) -> WormSign {
        WormSign {
            depth: self.depth,
            act: self.act,
            stage: self.stage,
            progress: if self.length > 0. {
                self.clock / self.length
            } else {
                0.
            },
            phase: self.phase,
        }
    }
    /// What it is doing now.
    pub fn act(&self) -> (WormAct, u8) {
        (self.act, self.stage)
    }
    /// Lay its rings where its path says.
    fn place(&mut self) {
        let mut rings = std::mem::take(&mut self.rings);
        lay_rings(&self.path, self.part, &mut rings);
        self.rings = rings;
        let (sum, count) = self
            .rings
            .iter()
            .filter(|r| r.surfaced())
            .fold((V2::ZERO, 0), |(sum, count), r| (sum + r.pos, count + 1));
        self.reach = if count == 0 {
            (self.path.head().0, 0.)
        } else {
            let middle = sum / count as f32;
            let reach = self
                .rings
                .iter()
                .filter(|r| r.surfaced())
                .map(|r| r.pos.distance(middle) + r.radius)
                .fold(0., f32::max);
            (middle, reach)
        };
    }
    fn begin(&mut self, act: WormAct, length: f32) {
        self.act = act;
        self.stage = 0;
        self.clock = 0.;
        self.length = length.max(DT);
        self.beat = 0.;
        self.struck.clear();
    }
    fn next_stage(&mut self, length: f32) {
        self.stage += 1;
        self.clock = 0.;
        self.length = length.max(DT);
        self.beat = 0.;
    }
    fn done(&self) -> bool {
        self.clock >= self.length
    }
    fn rest(&mut self, act: WormAct, seconds: f32) {
        if let Some(index) = WormAct::ALL.iter().position(|&a| a == act) {
            self.rest[index] = seconds;
        }
    }
    fn resting(&self, act: WormAct) -> bool {
        WormAct::ALL
            .iter()
            .position(|&a| a == act)
            .is_some_and(|index| self.rest[index] > 0.)
    }
}

/// How fierce it is in `phase`: 0 at first, 1 when only one half is left.
fn fierceness(phase: u8) -> f32 {
    f32::from(phase.clamp(1, 7) - 1) / 6.
}

/// A tank it can feel, and what it needs to know to go after it.
#[derive(Clone, Copy, Debug)]
struct Prey {
    index: usize,
    id: EntityId,
    pos: V2,
    vel: V2,
    radius: f32,
    /// Whether its head has a clear line to it.
    seen: bool,
}

/// Something it does to the world this tick, done once it lets go of itself.
enum Deed {
    /// Damage and a shove to a tank.
    Hurt(usize, f32, V2),
    /// A round.
    Fire(V2, f32, ProjectileKind, f32, f32, f32, f32, f32),
    /// An event for clients.
    Tell(EventKind, V2, u32),
}

impl World {
    /// Turns a defeated whole worm into its two halves instead, so the last
    /// part of the fight always comes, however hard the final blow was.
    pub(crate) fn tear_dying_worms(&mut self) {
        for i in 0..self.bosses.len() {
            let boss = &self.bosses[i];
            if boss.health <= 0.
                && boss.kind == BossKind::Sunderworm
                && boss
                    .worm
                    .as_deref()
                    .is_some_and(|w| w.part == WormPart::Whole)
            {
                self.tear(i);
            }
        }
    }

    /// Splits the whole worm at index `i` in two: it keeps the head, and a
    /// new boss takes the tail.
    fn tear(&mut self, i: usize) -> EntityId {
        let id = self.id();
        let tick = self.tick;
        let front = &mut self.bosses[i];
        let Some(work) = front.worm.as_deref_mut() else {
            return 0;
        };
        let half = front.max_health * HALF_HEALTH;
        let arc = WORM_TEAR as f32 * WORM_SPACING;
        let back_path = work.path.behind(arc);
        let way = work.path.heading_at(arc);
        let (cut, cut_depth) = work.path.at(arc - WORM_SPACING * 0.5);
        work.part = WormPart::Front;
        work.pair = id;
        work.phase = 6;
        work.begin(WormAct::Hunt, 0.6);
        work.lull = 0.6;
        work.health = half;
        work.stung = 0.;
        work.place();
        front.radius = WORM_FRONT_RADIUS;
        front.health = half;
        front.max_health = half;
        let front_id = front.id;
        let team = front.team;
        let mut back = WormWork {
            part: WormPart::Back,
            path: back_path,
            depth: cut_depth,
            heading: way.angle(),
            speed: work.speed * 0.6,
            phase: 6,
            pair: front_id,
            health: half,
            lull: 0.4,
            turn: if tick.is_multiple_of(2) { 1. } else { -1. },
            ..Default::default()
        };
        back.begin(WormAct::Thrash, 0.35);
        back.place();
        let head = back.path.head().0;
        self.bosses.push(Boss {
            id,
            pos: head,
            vel: way * back.speed,
            radius: WORM_BACK_RADIUS,
            health: half,
            max_health: half,
            kind: BossKind::Sunderworm,
            phase: 3,
            timer: 1.,
            aim: way.angle(),
            team,
            mothership: false,
            target: 0,
            goal: V2::ZERO,
            tactic: BossTactic::Patrol,
            tactic_time: 6.,
            last_attacker: 0,
            arrival: 0.,
            attack_timer: 5.,
            attack: 0,
            attack_windup: 0.,
            dodge: V2::ZERO,
            read: V2::ZERO,
            leg: head,
            leg_time: 0.,
            rail: RailWork::default(),
            clash: crate::clash::ClashWork::default(),
            mind: crate::boss_brain::Mind::default(),
            worm: Some(Box::new(back)),
        });
        let index = self.bosses.len() - 1;
        let sign = self.bosses[index].worm.as_deref().unwrap().sign();
        sign.write(&mut self.bosses[index]);
        // Clients see the tail arrive as its own boss; this marks the tear.
        self.event(EventKind::BossAttack, cut, front_id, WORM_TORN, team);
        id
    }

    /// One tick of a Sunderworm, whole or half.
    pub(crate) fn update_worm(&mut self, i: usize, volleys: &mut Vec<Volley>) {
        let bounds = self.config.bounds();
        {
            let b = &mut self.bosses[i];
            match b.worm.as_deref_mut() {
                None => b.worm = Some(Box::new(WormWork::rising(b.pos, b.aim))),
                // Moved by a map change or a script: the body comes along
                // whole instead of stretching across the arena.
                Some(work) if work.path.head().0.distance(b.pos) > 250. => {
                    work.path =
                        WormPath::straight(b.pos, work.heading, work.depth, work.part.length());
                    work.place();
                }
                Some(_) => {}
            }
        }
        // Who it can feel through the ground.
        let (id, team, head) = {
            let b = &self.bosses[i];
            (b.id, b.team, b.pos)
        };
        let prey: Vec<Prey> = self
            .tanks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.tangible()
                    && t.health > 0.
                    && t.protection <= 0.
                    && self.hostile(team, t.team)
                    && !self.config.base_protected(t.team, t.pos, t.radius)
                    && t.pos.distance(head) < SENSE
            })
            .map(|(index, t)| Prey {
                index,
                id: t.id,
                pos: t.pos,
                vel: t.vel,
                radius: t.radius,
                seen: self
                    .walls
                    .iter()
                    .all(|&w| sweep_rect(head, t.pos, 6., w).is_none()),
            })
            .collect();
        let hunted: Vec<EntityId> = self
            .bosses
            .iter()
            .filter(|other| other.id != id && other.health > 0.)
            .map(|other| other.target)
            .collect();
        let pair_target = {
            let pair = self.bosses[i].worm.as_deref().map_or(0, |w| w.pair);
            self.bosses
                .iter()
                .find(|other| other.id == pair && pair != 0 && other.health > 0.)
                .map(|other| (other.target, other.pos))
        };
        let records = self
            .contributions
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let tanks = &self.tanks;
        let walls = &self.walls;
        let roll = next_random(&mut self.rng);
        let think_roll = next_random(&mut self.rng);
        let act_roll = next_random(&mut self.rng);
        let side_roll = next_random(&mut self.rng);
        let b = &mut self.bosses[i];
        b.mind.feel(records, DT);
        b.arrival = (b.arrival - DT).max(0.);
        let alone = pair_target.is_none();
        let work = b.worm.as_deref_mut().expect("made above");
        // Its real phase.
        let share = b.health / b.max_health.max(1.);
        let phase = match work.part {
            WormPart::Whole => whole_phase(share),
            _ if alone && work.pair != 0 => 7,
            _ => 6,
        };
        let new_phase = phase > work.phase && work.part == WormPart::Whole;
        work.phase = work.phase.max(phase);
        let fury = fierceness(work.phase);
        // How hard it has been hit lately; a lot at once sends it under.
        let lost = ((work.health - b.health) / b.max_health.max(1.)).max(0.);
        work.health = b.health;
        work.stung = work.stung * (-DT / 3.).exp() + lost;
        for rest in &mut work.rest {
            *rest = (*rest - DT).max(0.);
        }
        work.lull = (work.lull - DT).max(0.);
        // Choose its prey.
        let reads: Vec<Read> = prey
            .iter()
            .map(|p| {
                let distance = p.pos.distance(head);
                let crowd = prey
                    .iter()
                    .filter(|other| other.id != p.id && other.pos.distance(p.pos) < 420.)
                    .count() as f32;
                let tank = &tanks[p.index];
                let paired = pair_target.is_some_and(|(target, _)| target == p.id);
                Read {
                    id: p.id,
                    near: 1. - (distance / SENSE).min(1.),
                    threat: b.mind.threat(p.id, b.max_health * 0.03),
                    weak: 1. - (tank.health / tank.max_health.max(1.)).clamp(0., 1.),
                    focus: if p.id == b.target {
                        1.
                    } else if paired {
                        0.7
                    } else {
                        0.
                    },
                    crowd: (crowd / 4.).min(1.),
                    strong: f32::from(tank.level) / 45.,
                    front: (V2::from_angle(work.heading).dot((p.pos - head).normalized()) + 1.)
                        * 0.5,
                    fresh: b.mind.fresh(p.id, b.target),
                    hunted: (hunted.iter().filter(|&&t| t == p.id).count() as f32 / 3.).min(1.),
                    quick: (p.vel.length() / 420.).min(1.),
                }
            })
            .collect();
        let current = prey.iter().find(|p| p.id == b.target).copied();
        let committed = matches!(
            work.act,
            WormAct::Lunge | WormAct::Erupt | WormAct::Coil | WormAct::Tear
        ) && current.is_some();
        if current.is_none() {
            b.mind.lost();
        }
        if !committed && (b.mind.due() || current.is_none()) {
            let chosen = draw(BossKind::Sunderworm, work.phase.min(3), &reads, roll);
            b.mind.chose(chosen.unwrap_or(0), b.target, think_roll);
            b.target = chosen.unwrap_or(0);
        }
        let mark = prey.iter().find(|p| p.id == b.target).copied();
        if let Some(mark) = mark {
            b.mind.watch(mark.vel, DT);
        }
        let mut deeds: Vec<Deed> = Vec::new();
        let mut tear_now = false;
        // Where things come from.
        let hunt_speed = 215. + 125. * fury;
        work.clock += DT;
        work.sway += DT * (2.6 + 1.6 * fury);
        let ahead = V2::from_angle(work.heading);
        let lead = |speed: f32, warned: f32| {
            mark.map(|m| curved_lead(head, m.pos, m.vel, b.mind.swerve, speed.max(1.), warned))
        };
        // What it wants to do with its body this tick: a point to steer at,
        // how fast, how sharply it may turn, and how deep it wants to be.
        let mut aim_at = lead(hunt_speed * 1.4, 0.).unwrap_or_else(|| {
            let wander = b.id as f32 * 1.618 + work.sway * 0.05;
            bounds.center() + V2::from_angle(wander) * (bounds.w.min(bounds.h) * 0.28)
        });
        let mut speed = hunt_speed;
        let mut turn_rate = 1.5 + 0.9 * fury;
        let mut want_depth = 0.;
        let mut depth_rate = 2.2;
        let mut slither = 0.24;
        let mut straight = false;
        match (work.act, work.stage) {
            (WormAct::Rise, 0) => {
                // Shaking the ground from below, still.
                speed = 0.;
                want_depth = 1.;
                if let Some(p) = prey
                    .iter()
                    .min_by(|a, c| a.pos.distance(head).total_cmp(&c.pos.distance(head)))
                {
                    work.heading = (p.pos - head).angle();
                }
                if work.done() {
                    work.next_stage(0.55);
                    work.depth = 1.;
                    deeds.push(Deed::Tell(EventKind::BossAttack, head, WORM_RISEN));
                    burst(&prey, head, ERUPT_RADIUS * 0.9, 50., &mut deeds);
                    debris(head, work.heading, 12, 0.25, &mut deeds);
                }
            }
            (WormAct::Rise, 1) => {
                speed = 620.;
                want_depth = 0.;
                depth_rate = 3.2;
                straight = true;
                if work.done() {
                    work.begin(WormAct::Roar, 1.7);
                }
            }
            (WormAct::Roar | WormAct::Tear, _) => {
                // It rears up and slows almost to a stop.
                speed = 70. * (1. - work.clock / work.length).max(0.2);
                turn_rate = 0.8;
                slither = 0.5;
                if work.act == WormAct::Roar && work.clock >= 0.42 && work.clock - DT < 0.42 {
                    roar(&prey, head, &mut deeds);
                    deeds.push(Deed::Tell(EventKind::BossAttack, head, WORM_ROAR));
                }
                if work.act == WormAct::Tear {
                    b.health = b.health.max(1.);
                    slither = 0.9;
                }
                if work.done() {
                    if work.act == WormAct::Tear {
                        tear_now = true;
                    } else {
                        work.begin(WormAct::Hunt, 0.);
                        work.lull = 0.4;
                    }
                }
            }
            (WormAct::Hunt, _) => {
                slither = 0.26 + 0.08 * fury;
                if let Some(mark) = mark {
                    // It circles its prey at striking distance, closing as it
                    // grows fiercer, rather than ploughing into it: the jaws
                    // are for the strike.
                    let ring = PROWL - 140. * fury;
                    let out = head - mark.pos;
                    let distance = out.length();
                    if distance < ring + 380. && distance > 1. {
                        let round = (out / distance).rotate(work.turn * 0.62);
                        aim_at = mark.pos + round * ring;
                    }
                    // Close on it from the side its other half isn't on.
                    if let Some((target, from)) = pair_target
                        && target == mark.id
                        && work.part == WormPart::Back
                    {
                        let far = (mark.pos - from).normalized();
                        aim_at = mark.pos + far * 380.;
                    }
                }
                if work.lull <= 0. && b.arrival <= 0. {
                    let choice = choose(work, mark, &prey, head, act_roll);
                    if let Some(act) = choice {
                        start(work, act, mark, head, fury, side_roll);
                    } else {
                        work.lull = 0.4;
                    }
                }
            }
            (WormAct::Lunge, 0) => {
                // Rearing back, slowing, and lining up the strike.
                speed = hunt_speed * 0.32;
                turn_rate = 3.4;
                slither = 0.08;
                if let Some(point) = lead(1150., work.length - work.clock) {
                    aim_at = point;
                    work.spot = point;
                }
                if work.done() {
                    work.next_stage(0.42 + 0.06 * fury);
                    work.struck.clear();
                    deeds.push(Deed::Tell(EventKind::BossAttack, head, WORM_STRIKE));
                }
            }
            (WormAct::Lunge, 1) => {
                speed = 1150. + 220. * fury;
                straight = true;
                slither = 0.;
                let jaws = work.rings.first().map_or(head, |r| r.pos) + ahead * 40.;
                for p in &prey {
                    if p.pos.distance(jaws) < ring_radius(0) + p.radius + 30.
                        && !work.struck.contains(&p.id)
                    {
                        work.struck.push(p.id);
                        deeds.push(Deed::Hurt(p.index, 115. + 45. * fury, ahead * 900.));
                    }
                }
                if work.done() {
                    work.next_stage(0.55 - 0.2 * fury);
                }
            }
            (WormAct::Lunge, _) => {
                speed = hunt_speed * 0.55;
                turn_rate = 0.9;
                if work.done() {
                    if work.left > 0 && mark.is_some() {
                        work.left -= 1;
                        work.begin(WormAct::Lunge, 0.45);
                    } else {
                        work.begin(WormAct::Hunt, 0.);
                        work.lull = 1.4 + 1.2 * act_roll - 0.8 * fury;
                    }
                }
            }
            (WormAct::Spit, 0) => {
                speed = hunt_speed * 0.45;
                turn_rate = 3.;
                if let Some(point) = lead(660., work.length - work.clock) {
                    aim_at = point;
                }
                if work.done() {
                    work.next_stage(0.32);
                    work.left = if work.phase >= 3 { 2 } else { 1 };
                }
            }
            (WormAct::Spit, 1) => {
                speed = hunt_speed * 0.35;
                turn_rate = 1.2;
                work.beat -= DT;
                if work.beat <= 0. && work.left > 0 {
                    work.left -= 1;
                    work.beat = 0.24;
                    let count = 5 + 2 * usize::from(work.phase.min(4) - 1);
                    let spread = 0.11 + 0.01 * count as f32;
                    let offset = if work.left.is_multiple_of(2) {
                        spread * 0.5
                    } else {
                        0.
                    };
                    let mouth = head + ahead * (ring_radius(0) + 14.);
                    for n in 0..count {
                        let angle =
                            work.heading + (n as f32 - (count - 1) as f32 * 0.5) * spread + offset;
                        deeds.push(Deed::Fire(
                            mouth,
                            angle,
                            ProjectileKind::Piercing,
                            660. + 30. * fury,
                            24. + 6. * fury,
                            9.,
                            2.3,
                            1.6,
                        ));
                    }
                }
                if work.done() {
                    work.next_stage(0.35);
                }
            }
            (WormAct::Bile, 0) => {
                speed = hunt_speed * 0.4;
                turn_rate = 2.6;
                if let Some(point) = lead(700., 0.4) {
                    aim_at = point;
                }
                if work.done() {
                    work.next_stage(1.5);
                    work.spot = V2::from_angle(work.heading - work.turn * 0.65);
                }
            }
            (WormAct::Bile, 1) => {
                // A sweep from one side across to the other.
                speed = hunt_speed * 0.32;
                slither = 0.05;
                let u = work.clock / work.length;
                let sweep = work.spot.angle() + work.turn * 1.3 * u;
                aim_at = head + V2::from_angle(sweep) * 400.;
                turn_rate = 3.4;
                work.beat -= DT;
                if work.beat <= 0. {
                    work.beat += 0.075 - 0.02 * fury;
                    let mouth = head + ahead * (ring_radius(0) + 10.);
                    let spread = (act_roll - 0.5) * 0.16;
                    deeds.push(Deed::Fire(
                        mouth,
                        work.heading + spread,
                        ProjectileKind::Mine,
                        720. + 260. * ((work.clock * 7.3).sin() * 0.5 + 0.5),
                        22. + 6. * fury,
                        15.,
                        6.5,
                        2.,
                    ));
                }
                if work.done() {
                    work.next_stage(0.4);
                }
            }
            (
                WormAct::Spit
                | WormAct::Bile
                | WormAct::Brood
                | WormAct::Thrash
                | WormAct::Whip
                | WormAct::Bleed
                | WormAct::Sting,
                2,
            ) => {
                speed = hunt_speed * 0.6;
                if work.done() {
                    work.begin(WormAct::Hunt, 0.);
                    work.lull = 1.2 + 1.4 * act_roll - 0.8 * fury;
                }
            }
            (WormAct::Dive, _) => {
                speed = hunt_speed * 1.25;
                want_depth = 1.;
                depth_rate = 2.1;
                turn_rate = 1.2;
                if let Some(point) = lead(520., 1.) {
                    aim_at = point;
                }
                if work.depth >= 0.98 {
                    work.begin(WormAct::Tunnel, 3.6 - 1.2 * fury);
                }
            }
            (WormAct::Tunnel, _) => {
                // Fast and straight for where it means to come up, under walls
                // and all; a ridge of broken floor shows the way it goes.
                speed = 470. + 160. * fury;
                want_depth = 1.;
                turn_rate = 2.6;
                slither = 0.1;
                let locked = work.clock > work.length * 0.72;
                if !locked && let Some(point) = lead(speed, 0.9) {
                    work.spot = bounds.clamp_circle(point, ring_radius(0) + 20.);
                }
                aim_at = work.spot;
                if work.phase >= 2 {
                    work.beat -= DT;
                    if work.beat <= 0. && head.distance(work.spot) > 260. {
                        work.beat = 0.42 - 0.12 * fury;
                        deeds.push(Deed::Fire(
                            head,
                            work.heading + PI * 0.5 * work.turn,
                            ProjectileKind::Trap,
                            20.,
                            20. + 6. * fury,
                            13.,
                            3.2 + fury,
                            2.,
                        ));
                    }
                }
                if head.distance(work.spot) < 70. || work.done() {
                    let warn = 1.05 - 0.35 * fury;
                    work.begin(WormAct::Erupt, warn);
                    work.spot = head;
                    deeds.push(Deed::Tell(EventKind::BossAttack, head, WORM_BELOW));
                }
            }
            (WormAct::Erupt, 0) => {
                // Holding still under the ring of breaking floor.
                speed = 0.;
                want_depth = 1.;
                if let Some(m) = mark {
                    work.heading = (m.pos - head).angle() + PI * 0.1 * work.turn;
                }
                if work.done() {
                    work.next_stage(0.38);
                    burst(&prey, head, ERUPT_RADIUS, 95. + 40. * fury, &mut deeds);
                    debris(
                        head,
                        work.heading,
                        14 + (6. * fury) as usize,
                        0.,
                        &mut deeds,
                    );
                    deeds.push(Deed::Tell(EventKind::BossAttack, head, WORM_BURST));
                }
            }
            (WormAct::Erupt, 1) => {
                speed = 680.;
                straight = true;
                want_depth = 0.;
                depth_rate = 3.4;
                if work.done() {
                    work.next_stage(0.45);
                }
            }
            (WormAct::Erupt, _) => {
                speed = hunt_speed * 0.8;
                if work.done() {
                    // Fiercer, it goes straight back under for another.
                    if work.left > 0 {
                        work.left -= 1;
                        work.begin(WormAct::Dive, 2.);
                    } else {
                        work.begin(WormAct::Hunt, 0.);
                        work.lull = 1.2 + act_roll - 0.6 * fury;
                    }
                }
            }
            (WormAct::Brood, 0) => {
                speed = hunt_speed * 0.5;
                slither = 0.55;
                if work.done() {
                    work.next_stage(1.);
                    work.left = 5 + work.phase.min(5);
                }
            }
            (WormAct::Brood, 1) => {
                speed = hunt_speed * 0.5;
                slither = 0.4;
                work.beat -= DT;
                if work.beat <= 0. && work.left > 0 {
                    work.left -= 1;
                    work.beat = 0.1;
                    let up: Vec<&Ring> =
                        work.rings.iter().skip(3).filter(|r| r.surfaced()).collect();
                    if !up.is_empty() {
                        let pick = (act_roll * 7919. + f32::from(work.left) * 3.7) as usize;
                        let ring = up[pick % up.len()];
                        let side = if work.left.is_multiple_of(2) { 1. } else { -1. };
                        let out = V2::from_angle(work.heading + PI * 0.5 * side);
                        deeds.push(Deed::Fire(
                            ring.pos + out * (ring.radius + 14.),
                            out.angle(),
                            ProjectileKind::Drone,
                            250. + 40. * fury,
                            15. + 4. * fury,
                            12.,
                            9.,
                            2.5,
                        ));
                    }
                }
                if work.done() {
                    work.next_stage(0.3);
                }
            }
            (WormAct::Thrash, 0) => {
                speed = hunt_speed * 0.42;
                slither = 0.7;
                if work.done() {
                    work.next_stage(1.25 - 0.3 * fury);
                    work.wave = 0;
                }
            }
            (WormAct::Thrash, 1) => {
                speed = hunt_speed * 0.42;
                slither = 0.5;
                // A wave of spines rolls from its head to its tail.
                let reached = ((work.clock / work.length) * work.rings.len() as f32) as usize;
                while work.wave < reached.min(work.rings.len()) {
                    let ring = work.rings[work.wave];
                    let arc = work.wave as f32 * WORM_SPACING;
                    work.wave += 1;
                    if !ring.surfaced() {
                        continue;
                    }
                    let along = work.path.heading_at(arc);
                    for side in [-1., 1.] {
                        let out = V2::new(-along.y, along.x) * side;
                        let tilt = (work.wave as f32 * 0.7).sin() * 0.25;
                        deeds.push(Deed::Fire(
                            ring.pos + out * (ring.radius + 6.),
                            out.angle() + tilt,
                            ProjectileKind::Bullet,
                            540. + 80. * fury,
                            15. + 4. * fury,
                            7.,
                            1.5,
                            1.,
                        ));
                    }
                }
                if work.done() {
                    work.next_stage(0.4);
                }
            }
            (WormAct::Coil, _) => {
                // Round and round its prey, closing in, then the jaws.
                if let Some(m) = mark {
                    let u = (work.clock / work.length).min(1.);
                    let ring = 470. - 170. * u;
                    let out = head - m.pos;
                    let out = if out.length_squared() > 1. {
                        out.normalized()
                    } else {
                        ahead * -1.
                    };
                    aim_at = m.pos + out.rotate(work.turn * 0.75) * ring;
                    speed = hunt_speed * 1.25;
                    turn_rate = 2.7;
                    slither = 0.08;
                    if m.pos.distance(head) > 1100. {
                        work.length = work.clock;
                    }
                } else {
                    work.length = work.clock;
                }
                if work.done() {
                    work.begin(WormAct::Lunge, 0.5 - 0.15 * fury);
                }
            }
            (WormAct::Whip, 0) => {
                speed = hunt_speed * 0.5;
                if let Some(m) = mark {
                    aim_at = m.pos;
                }
                turn_rate = 2.;
                if work.done() {
                    work.next_stage(0.9);
                }
            }
            (WormAct::Whip, 1) => {
                // A hard swing round: its whole body sweeps the ground.
                speed = 560. + 120. * fury;
                turn_rate = 5.2;
                slither = 0.;
                aim_at = head + V2::from_angle(work.heading + work.turn * 1.4) * 300.;
                if work.done() {
                    work.next_stage(0.4);
                }
            }
            (WormAct::Bleed, 0) => {
                speed = hunt_speed * 0.6;
                if work.done() {
                    work.next_stage(1.2);
                }
            }
            (WormAct::Bleed, 1) => {
                // The torn end sprays what's left in it.
                speed = hunt_speed * 0.7;
                slither = 0.6;
                work.beat -= DT;
                if work.beat <= 0. {
                    work.beat += 0.085;
                    let fan = (work.clock * 9.).sin() * 0.9;
                    deeds.push(Deed::Fire(
                        head + ahead * (ring_radius(WORM_TEAR) * 0.8),
                        work.heading + fan,
                        ProjectileKind::Mine,
                        640. + 200. * act_roll,
                        20. + 5. * fury,
                        13.,
                        5.,
                        2.,
                    ));
                }
                if work.done() {
                    work.next_stage(0.4);
                }
            }
            (WormAct::Sting, 0) => {
                speed = hunt_speed * 0.55;
                if work.done() {
                    work.next_stage(0.5);
                    work.left = 2;
                }
            }
            (WormAct::Sting, 1) => {
                speed = hunt_speed * 0.55;
                work.beat -= DT;
                if work.beat <= 0. && work.left > 0 {
                    work.left -= 1;
                    work.beat = 0.22;
                    if let Some(tip) = work.rings.last().filter(|r| r.surfaced()) {
                        let back = work.path.heading_at(work.part.length()) * -1.;
                        for n in -1..=1 {
                            deeds.push(Deed::Fire(
                                tip.pos + back * (tip.radius + 8.),
                                back.angle() + n as f32 * 0.5,
                                ProjectileKind::Homing,
                                470. + 60. * fury,
                                24. + 6. * fury,
                                9.,
                                3.2,
                                1.5,
                            ));
                        }
                    }
                }
                if work.done() {
                    work.next_stage(0.4);
                }
            }
            (
                WormAct::Spit
                | WormAct::Bile
                | WormAct::Brood
                | WormAct::Thrash
                | WormAct::Whip
                | WormAct::Bleed
                | WormAct::Sting
                | WormAct::Rise,
                _,
            ) => {
                if work.done() {
                    work.begin(WormAct::Hunt, 0.);
                }
            }
        }
        // A new phase: it rears and screams first, wherever it was.
        if new_phase && b.arrival <= 0. && !matches!(work.act, WormAct::Rise | WormAct::Tear) {
            if work.depth > 0.5 {
                work.begin(WormAct::Erupt, 0.6);
                work.spot = head;
            } else {
                work.begin(WormAct::Roar, 1.6);
            }
        }
        // Too much at once, or nowhere to go: it goes under.
        if (work.stung > 0.07 || work.boxed > 0.9)
            && work.act == WormAct::Hunt
            && !work.resting(WormAct::Dive)
        {
            work.stung = 0.;
            work.boxed = 0.;
            work.rest(WormAct::Dive, 8.);
            work.begin(WormAct::Dive, 2.);
            work.left = 0;
        }
        // The whole worm tears at the line.
        let tearing = work.part == WormPart::Whole && share <= WORM_SPLIT;
        if tearing && work.act != WormAct::Tear && b.arrival <= 0. {
            work.begin(WormAct::Tear, 1.3);
            work.depth = work.depth.min(0.2);
        }
        // Turn, slither and move.
        let to = aim_at - head;
        let mut wanted = if to.length_squared() > 1. {
            to.angle()
        } else {
            work.heading
        };
        if !straight {
            wanted += work.sway.sin() * slither;
        }
        let buried = work.depth > 0.5;
        if !buried && speed > 1. {
            // On the surface it steers round walls; boxed in, it dives.
            let (clear, way) = clear_way(head, wanted, ring_radius(0), speed, bounds, walls);
            if clear {
                work.boxed = (work.boxed - DT).max(0.);
            } else {
                work.boxed += DT;
            }
            wanted = way;
        } else {
            // Underground only the arena's edge turns it.
            wanted = away_from_edges(head, wanted, ring_radius(0), bounds);
        }
        let difference = (wanted - work.heading + PI).rem_euclid(TAU) - PI;
        let rate = if straight { 0. } else { turn_rate };
        work.heading += difference.clamp(-rate * DT, rate * DT);
        // Speed eases rather than jumps, except into a strike.
        let ease = if speed > work.speed * 1.8 { 14. } else { 4.5 };
        work.speed += (speed - work.speed) * (1. - (-DT * ease).exp());
        work.depth += (want_depth - work.depth).clamp(-depth_rate * DT, depth_rate * DT);
        let step = V2::from_angle(work.heading) * work.speed * DT;
        let mut next = head + step;
        let margin = ring_radius(0) * 0.6;
        let inside = bounds.clamp_circle(next, margin);
        if inside != next {
            next = inside;
        }
        if !buried
            && walls
                .iter()
                .any(|&w| circle_rect(next, ring_radius(0) * 0.5, w).is_some())
        {
            // A wall it couldn't turn from in time: it slides along it.
            next = head;
            work.speed *= 0.5;
        }
        b.vel = (next - head) / DT;
        b.pos = next;
        b.aim = work.heading;
        let keep = work.part.length() + WORM_SPACING;
        work.path.follow(next, work.depth, keep);
        work.place();
        let sign = work.sign();
        sign.write(b);
        // Do what it did.
        for deed in deeds {
            match deed {
                Deed::Hurt(index, damage, knock) => self.damage_tank(index, damage, id, knock),
                Deed::Fire(origin, angle, kind, speed, damage, radius, life, pen) => {
                    if !bounds.contains_circle(origin, radius)
                        || self
                            .walls
                            .iter()
                            .any(|&w| circle_rect(origin, radius, w).is_some())
                    {
                        continue;
                    }
                    if kind == ProjectileKind::Drone
                        && self
                            .projectiles
                            .iter()
                            .filter(|p| p.owner == id && p.kind.is_drone() && p.lifetime > 0.)
                            .count()
                            >= WORM_BROOD
                    {
                        continue;
                    }
                    volleys.push((
                        id,
                        team,
                        origin,
                        V2::from_angle(angle) * speed,
                        kind,
                        damage,
                        radius,
                        life,
                        pen,
                        0,
                    ));
                }
                Deed::Tell(kind, at, value) => self.event(kind, at, id, value, team),
            }
        }
        if tear_now {
            self.tear(i);
        }
    }
}

/// What its attack events carry: values no other boss's attack has, which
/// clients from before it treat as a plain flash.
pub const WORM_RISEN: u32 = 64;
/// Its jaws striking.
pub const WORM_STRIKE: u32 = 65;
/// The ground starting to break where it will come up.
pub const WORM_BELOW: u32 = 66;
/// The ground breaking as it comes up.
pub const WORM_BURST: u32 = 67;
/// The whole worm tearing in two, from the front half.
pub const WORM_TORN: u32 = 68;
/// The scream at a new phase.
pub const WORM_ROAR: u32 = 69;

/// The tanks near `at` thrown out and hurt as the ground breaks.
fn burst(prey: &[Prey], at: V2, radius: f32, damage: f32, deeds: &mut Vec<Deed>) {
    for p in prey {
        let gap = p.pos - at;
        let distance = gap.length();
        if distance < radius + p.radius {
            let out = if distance > 1. {
                gap / distance
            } else {
                V2::new(1., 0.)
            };
            let near = 1. - (distance / (radius + p.radius)).min(1.) * 0.6;
            deeds.push(Deed::Hurt(p.index, damage * near, out * 850.));
        }
    }
}

/// The scream at a new phase: a shove for everyone close, and a sting.
fn roar(prey: &[Prey], at: V2, deeds: &mut Vec<Deed>) {
    for p in prey {
        let gap = p.pos - at;
        let distance = gap.length();
        if distance < 700. {
            let out = if distance > 1. {
                gap / distance
            } else {
                V2::new(1., 0.)
            };
            let strength = 1. - distance / 700.;
            deeds.push(Deed::Hurt(p.index, 20. * strength, out * 760. * strength));
        }
    }
}

/// A ring of broken floor thrown out from where it came up.
fn debris(at: V2, heading: f32, count: usize, gap: f32, deeds: &mut Vec<Deed>) {
    for n in 0..count {
        let angle = heading + n as f32 * TAU / count as f32 + gap;
        deeds.push(Deed::Fire(
            at + V2::from_angle(angle) * (ERUPT_RADIUS * 0.4),
            angle,
            ProjectileKind::Fragment,
            470. + (n % 3) as f32 * 55.,
            16.,
            10.,
            1.1,
            1.,
        ));
    }
}

/// Which act it goes into next from the hunt, if any is worth it now.
fn choose(
    work: &WormWork,
    mark: Option<Prey>,
    prey: &[Prey],
    head: V2,
    roll: f32,
) -> Option<WormAct> {
    let mark = mark?;
    let range = mark.pos.distance(head);
    let facing = V2::from_angle(work.heading).dot((mark.pos - head).normalized());
    let quick = (mark.vel.length() / 420.).min(1.);
    let near_body = prey
        .iter()
        .filter(|p| {
            work.rings
                .iter()
                .any(|r| r.surfaced() && r.pos.distance(p.pos) < r.radius + 260.)
        })
        .count() as f32;
    let fury = fierceness(work.phase);
    let part = work.part;
    let window = |near: f32, far: f32| {
        if range < near {
            1. - ((near - range) / near).min(1.)
        } else if range > far {
            1. - ((range - far) / far).min(1.)
        } else {
            1.
        }
    };
    let mut options: Vec<(WormAct, f32)> = Vec::with_capacity(8);
    let can = |act: WormAct| !work.resting(act);
    if part != WormPart::Back && can(WormAct::Lunge) && mark.seen {
        options.push((
            WormAct::Lunge,
            1.3 * window(220., 880.) + 0.6 * facing.max(0.) + 0.3 * fury,
        ));
    }
    if part != WormPart::Back && can(WormAct::Spit) && mark.seen {
        options.push((WormAct::Spit, 1.1 * window(380., 1350.)));
    }
    if can(WormAct::Dive) {
        let hidden = if mark.seen { 0. } else { 1.4 };
        let far = if range > 1100. { 0.9 } else { 0. };
        options.push((WormAct::Dive, 0.55 + hidden + far + 0.5 * (1. - quick)));
    }
    if part != WormPart::Back && work.phase >= 3 && can(WormAct::Bile) && mark.seen {
        options.push((WormAct::Bile, 1.2 * window(250., 820.)));
    }
    if part == WormPart::Whole && work.phase >= 4 && can(WormAct::Brood) {
        options.push((WormAct::Brood, 0.8 + 0.25 * prey.len().min(4) as f32));
    }
    if part != WormPart::Front && work.phase >= 5 && can(WormAct::Thrash) {
        options.push((WormAct::Thrash, 0.5 + 0.55 * near_body.min(4.)));
    }
    if part == WormPart::Whole && work.phase >= 2 && can(WormAct::Coil) && range < 760. {
        options.push((WormAct::Coil, 0.6 + 0.9 * (1. - quick)));
    }
    if part == WormPart::Back {
        if can(WormAct::Whip) && range < 640. {
            options.push((WormAct::Whip, 1.5));
        }
        if can(WormAct::Bleed) {
            options.push((WormAct::Bleed, 0.9 * window(200., 700.)));
        }
        if can(WormAct::Sting) {
            options.push((WormAct::Sting, 1.1 * window(350., 1300.)));
        }
    }
    if part == WormPart::Front && can(WormAct::Bleed) {
        options.push((WormAct::Bleed, 0.5));
    }
    let weights: Vec<f32> = options
        .iter()
        .map(|(_, score)| (score * 2.3).exp())
        .collect();
    let total: f32 = weights.iter().sum();
    if total <= 0. {
        return None;
    }
    let mut left = roll.clamp(0., 0.999_999) * total;
    for ((act, _), weight) in options.iter().zip(weights) {
        if left < weight {
            return Some(*act);
        }
        left -= weight;
    }
    options.last().map(|(act, _)| *act)
}

/// Sets `act` going, with its warning shortened by how fierce it is, and
/// rests it for a while after.
fn start(work: &mut WormWork, act: WormAct, mark: Option<Prey>, head: V2, fury: f32, roll: f32) {
    let quick = 1. - 0.4 * fury;
    work.turn = if roll < 0.5 { 1. } else { -1. };
    work.left = 0;
    let (warning, rest) = match act {
        WormAct::Lunge => {
            // In its last phases the jaws come again, and again.
            work.left = match work.phase {
                6 if work.part == WormPart::Front => 1,
                7 => 2,
                5 => u8::from(roll < 0.4),
                _ => 0,
            };
            (0.9 * quick, 3.4 - 1.4 * fury)
        }
        WormAct::Spit => (0.55 * quick, 3. - fury),
        WormAct::Bile => (0.65 * quick, 7. - 2. * fury),
        WormAct::Dive => {
            work.left = u8::from(work.phase >= 2 && roll < 0.25 + 0.35 * fury);
            (2., 9. - 4. * fury)
        }
        WormAct::Brood => (0.75 * quick, 14. - 4. * fury),
        WormAct::Thrash => (0.6 * quick, 9. - 3. * fury),
        WormAct::Coil => (3.2, 12. - 3. * fury),
        WormAct::Whip => (0.5 * quick, 3.2),
        WormAct::Bleed => (0.45 * quick, 6.),
        WormAct::Sting => (0.6 * quick, 4.5),
        _ => (0.5, 3.),
    };
    work.rest(act, rest);
    work.begin(act, warning);
    if let Some(mark) = mark {
        work.spot = mark.pos;
    } else {
        work.spot = head;
    }
}

/// The heading nearest `wanted` with room ahead for a head of `radius`
/// going at `speed`, and whether `wanted` itself was clear.
fn clear_way(
    at: V2,
    wanted: f32,
    radius: f32,
    speed: f32,
    bounds: Rect,
    walls: &[Rect],
) -> (bool, f32) {
    let reach = radius * 0.9 + speed.max(120.) * 0.55;
    let open = |angle: f32| {
        let end = at + V2::from_angle(angle) * reach;
        bounds.contains_circle(end, radius * 0.6)
            && walls
                .iter()
                .all(|&w| sweep_rect(at, end, radius * 0.55, w).is_none())
    };
    if open(wanted) {
        return (true, away_from_edges(at, wanted, radius, bounds));
    }
    for step in 1..=8 {
        for side in [1., -1.] {
            let angle = wanted + side * step as f32 * 0.32;
            if open(angle) {
                return (false, angle);
            }
        }
    }
    (false, wanted + PI)
}

/// `wanted`, bent away from the arena's edge when the head is close to it.
fn away_from_edges(at: V2, wanted: f32, radius: f32, bounds: Rect) -> f32 {
    let room = radius + 260.;
    let mut push = V2::ZERO;
    let edges = [
        (at.x - bounds.x, V2::new(1., 0.)),
        (bounds.x + bounds.w - at.x, V2::new(-1., 0.)),
        (at.y - bounds.y, V2::new(0., 1.)),
        (bounds.y + bounds.h - at.y, V2::new(0., -1.)),
    ];
    for (gap, inward) in edges {
        if gap < room {
            push += inward * (1. - gap / room);
        }
    }
    if push == V2::ZERO {
        return wanted;
    }
    (V2::from_angle(wanted) + push * 1.6).angle()
}

impl Boss {
    /// Every circle it can be hit on and that bumps into things: its hull, or
    /// each ring of a Sunderworm that is above ground.
    pub fn parts(&self) -> impl Iterator<Item = (V2, f32)> + '_ {
        let rings = self.worm.as_deref().map_or(&[][..], |w| w.rings.as_slice());
        let hull = self.worm.is_none().then_some((self.pos, self.radius));
        hull.into_iter().chain(
            rings
                .iter()
                .filter(|ring| ring.surfaced())
                .map(|ring| (ring.pos, ring.radius)),
        )
    }
    /// A circle around every part, for a quick first test; none when it is
    /// all underground.
    pub fn reach(&self) -> Option<(V2, f32)> {
        match self.worm.as_deref() {
            None => Some((self.pos, self.radius)),
            Some(work) if work.reach.1 > 0. => Some(work.reach),
            Some(_) => None,
        }
    }
    /// The part of it nearest `point`, as a center and a radius.
    pub fn nearest_part(&self, point: V2) -> Option<(V2, f32)> {
        self.parts()
            .min_by(|a, b| (a.0.distance(point) - a.1).total_cmp(&(b.0.distance(point) - b.1)))
    }
    /// Which part of a Sunderworm this is; None for any other boss.
    pub fn worm_part(&self) -> Option<WormPart> {
        if self.kind != BossKind::Sunderworm {
            return None;
        }
        Some(self.worm.as_deref().map_or_else(
            || WormPart::of_radius(self.radius).unwrap_or_default(),
            |w| w.part,
        ))
    }
}

/// Which boss a random event brings: `rare` decides whether it's one of the
/// rare ones, and `pick` which of the rest it is otherwise.
pub(crate) fn rare_boss(rare: f32, strength: f32) -> Option<BossKind> {
    (rare < WORM_ODDS && strength >= WORM_STRENGTH).then_some(BossKind::Sunderworm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;

    fn world() -> World {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                spawn_protection: 0.,
                dynamic_arena: false,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.walls.clear();
        world
    }

    fn tank_at(world: &mut World, name: &str, at: V2) -> EntityId {
        let id = world.add_player(name, false);
        let tank = world.tanks.iter_mut().find(|t| t.id == id).unwrap();
        tank.pos = at;
        tank.vel = V2::ZERO;
        tank.level = 45;
        tank.protection = 0.;
        id
    }

    fn worm(world: &mut World, at: V2) -> usize {
        let saved: Vec<_> = world.tanks.iter().map(|t| t.pos).collect();
        for tank in &mut world.tanks {
            tank.pos = V2::new(100., 100.);
        }
        assert_ne!(world.spawn_boss_near(BossKind::Sunderworm, at), 0);
        for (tank, pos) in world.tanks.iter_mut().zip(saved) {
            tank.pos = pos;
        }
        world.bosses.len() - 1
    }

    fn keep_alive(world: &mut World) {
        for tank in &mut world.tanks {
            tank.max_health = 1e7;
            tank.health = tank.max_health;
        }
    }

    #[test]
    fn its_rings_taper_from_a_thick_neck_to_a_thin_tail() {
        assert!(ring_radius(2) > ring_radius(0));
        for k in 4..WORM_RINGS {
            assert!(ring_radius(k) < ring_radius(k - 1));
        }
        assert!(ring_radius(WORM_RINGS - 1) < ring_radius(0) * 0.4);
        // Neighbors overlap, so the body reads as one.
        for k in 1..WORM_RINGS {
            assert!(ring_radius(k) > WORM_SPACING * 0.5);
        }
        assert_eq!(WormPart::Whole.rings().len(), WORM_RINGS);
        assert_eq!(
            WormPart::Front.rings().len() + WormPart::Back.rings().len(),
            WORM_RINGS
        );
        for part in [WormPart::Whole, WormPart::Front, WormPart::Back] {
            assert_eq!(WormPart::of_radius(part.radius()), Some(part));
        }
    }

    #[test]
    fn a_path_keeps_its_points_evenly_spaced_and_finds_any_point_along_it() {
        let mut path = WormPath::straight(V2::new(0., 0.), 0., 0., 600.);
        for step in 1..=200 {
            let t = step as f32 * 0.05;
            path.follow(V2::new(t * 120., (t * 1.3).sin() * 200.), 0., 2000.);
        }
        let points: Vec<_> = path.points().skip(1).collect();
        for pair in points.windows(2) {
            assert!((pair[0].0.distance(pair[1].0) - PATH_STEP).abs() < 0.01);
        }
        // Walking along it by arc length stays on it, a step at a time.
        let mut last = path.at(0.).0;
        assert_eq!(last, path.head().0);
        for n in 1..100 {
            let here = path.at(n as f32 * 10.).0;
            assert!(here.distance(last) <= 10.01, "{n}");
            last = here;
        }
    }

    #[test]
    fn a_torn_path_carries_on_from_where_it_was_cut() {
        let mut path = WormPath::straight(V2::new(0., 0.), 0., 0., 2000.);
        for step in 1..=60 {
            path.follow(V2::new(step as f32 * 20., 0.), 0., 2000.);
        }
        let cut = 500.;
        let back = path.behind(cut);
        assert!(back.head().0.distance(path.at(cut).0) < 0.01);
        for n in 0..20 {
            let arc = n as f32 * 25.;
            assert!(back.at(arc).0.distance(path.at(cut + arc).0) < 0.6, "{n}");
        }
    }

    #[test]
    fn the_sign_preserves_each_attack_phase_and_body_part() {
        let mut world = world();
        tank_at(&mut world, "Prey", V2::new(1200., 1200.));
        let index = worm(&mut world, V2::new(2400., 2400.));
        for act in WormAct::ALL {
            for stage in 0..3 {
                for phase in 1..=7 {
                    let sign = WormSign {
                        depth: 0.37,
                        act,
                        stage,
                        progress: 0.62,
                        phase,
                    };
                    let boss = &mut world.bosses[index];
                    sign.write(boss);
                    let read = WormSign::of(boss);
                    assert_eq!(read.act, act);
                    assert_eq!(read.stage, stage);
                    assert_eq!(read.phase, phase);
                    assert!((read.depth - 0.37).abs() < 1e-4);
                    assert!((read.progress - 0.62).abs() < 1e-3);
                    assert!((1..=3).contains(&boss.phase));
                    assert!(boss.attack <= 2 && boss.attack_windup == 0.);
                    assert!((-BOSS_AIM_HOLD..=30.).contains(&boss.attack_timer));
                    assert!(boss.goal.finite() && boss.goal.y.abs() < 1000.);
                }
            }
        }
        // Its part is separate from its kind: even a torn end is a worm.
        for part in [WormPart::Front, WormPart::Back] {
            assert_eq!(WormPart::of_radius(part.radius()), Some(part));
            assert_eq!(
                BossKind::Sunderworm.from_wire(part.radius()),
                BossKind::Sunderworm
            );
        }
    }

    #[test]
    fn it_rises_out_of_the_ground_and_only_then_can_be_hit() {
        let mut world = world();
        let prey = tank_at(&mut world, "Prey", V2::new(1500., 1500.));
        let index = worm(&mut world, V2::new(2300., 1500.));
        assert_eq!(world.bosses[index].kind.radius(), WORM_RADIUS);
        let boss = world.bosses[index].id;
        // Underground at first: out of reach and harmless.
        world.step();
        assert!(world.bosses[0].reach().is_none());
        assert_eq!(WormSign::of(&world.bosses[0]).act, WormAct::Rise);
        let mut up = false;
        for _ in 0..(6. / DT) as usize {
            keep_alive(&mut world);
            world.step();
            let b = world.bosses.iter().find(|b| b.id == boss).unwrap();
            if b.reach().is_some() {
                up = true;
                break;
            }
        }
        assert!(up, "never came up");
        let _ = prey;
    }

    #[test]
    fn a_long_fight_never_leaves_it_stuck_lost_or_outside_the_arena() {
        let mut world = world();
        let bounds = world.config.bounds();
        world.walls = vec![
            Rect {
                x: 1600.,
                y: 1100.,
                w: 160.,
                h: 900.,
            },
            Rect {
                x: 2600.,
                y: 2400.,
                w: 900.,
                h: 160.,
            },
        ];
        tank_at(&mut world, "One", V2::new(1300., 1300.));
        tank_at(&mut world, "Two", V2::new(3000., 2900.));
        let index = worm(&mut world, V2::new(2400., 1700.));
        let boss = world.bosses[index].id;
        let mut acts = std::collections::BTreeSet::new();
        let mut moved = 0.;
        let mut last = world.bosses[index].pos;
        for tick in 0..(90. / DT) as usize {
            keep_alive(&mut world);
            // Hurt it steadily so it goes through its phases.
            if let Some(b) = world.bosses.iter_mut().find(|b| b.id == boss) {
                b.health -= b.max_health * 0.0005;
                b.health = b.health.max(b.max_health * 0.4);
            }
            world.step();
            let Some(b) = world.bosses.iter().find(|b| b.id == boss) else {
                panic!("vanished at {tick}");
            };
            assert!(
                b.pos.finite() && bounds.contains_circle(b.pos, 0.),
                "left at {tick}"
            );
            let work = b.worm.as_deref().unwrap();
            assert_eq!(work.rings.len(), WORM_RINGS);
            assert!(work.rings.iter().all(|r| r.pos.finite()));
            acts.insert(format!("{:?}", work.act));
            moved += b.pos.distance(last);
            last = b.pos;
        }
        assert!(moved > 4000., "barely moved: {moved}");
        for act in ["Hunt", "Lunge", "Dive", "Tunnel", "Erupt"] {
            assert!(acts.contains(act), "never did {act}: {acts:?}");
        }
    }

    #[test]
    fn it_tears_in_two_and_both_halves_fight_on() {
        let mut world = world();
        tank_at(&mut world, "One", V2::new(1300., 1300.));
        let index = worm(&mut world, V2::new(2400., 1700.));
        let boss = world.bosses[index].id;
        // Let it come up first.
        for _ in 0..(5. / DT) as usize {
            keep_alive(&mut world);
            world.step();
        }
        let b = world.bosses.iter_mut().find(|b| b.id == boss).unwrap();
        b.health = b.max_health * (WORM_SPLIT - 0.01);
        let full = b.max_health;
        for _ in 0..(3. / DT) as usize {
            keep_alive(&mut world);
            world.step();
            if world.bosses.len() == 2 {
                break;
            }
        }
        assert_eq!(world.bosses.len(), 2, "never tore");
        let front = world.bosses.iter().find(|b| b.id == boss).unwrap();
        let back = world.bosses.iter().find(|b| b.id != boss).unwrap();
        assert_eq!(front.worm_part(), Some(WormPart::Front));
        assert_eq!(back.worm_part(), Some(WormPart::Back));
        assert_eq!(front.radius, WORM_FRONT_RADIUS);
        assert_eq!(back.radius, WORM_BACK_RADIUS);
        assert!(
            (front.max_health - full * HALF_HEALTH).abs() < 1.,
            "{} vs {}",
            front.max_health,
            full * HALF_HEALTH
        );
        assert_eq!(front.max_health, back.max_health);
        assert_eq!(front.worm.as_deref().unwrap().pair, back.id);
        assert_eq!(back.worm.as_deref().unwrap().pair, front.id);
        assert_eq!(WormSign::of(back).phase, 6);
        // The back half starts where the front half ends.
        let gap = front
            .worm
            .as_deref()
            .unwrap()
            .rings
            .last()
            .unwrap()
            .pos
            .distance(back.worm.as_deref().unwrap().rings[0].pos);
        assert!(gap < WORM_SPACING * 2., "{gap}");
        // Kill the front: the back fights on, fiercer.
        let front_id = front.id;
        world
            .bosses
            .iter_mut()
            .find(|b| b.id == front_id)
            .unwrap()
            .health = -1.;
        world.step();
        world.step();
        assert_eq!(world.bosses.len(), 1);
        assert_eq!(WormSign::of(&world.bosses[0]).phase, 7);
    }

    #[test]
    fn spawning_reserves_the_tails_so_splitting_cannot_exceed_the_encounter_limit() {
        let mut world = world();
        for _ in 0..8 {
            let id = world.spawn_boss(BossKind::Sunderworm);
            assert_ne!(id, 0);
            let boss = world.bosses.iter_mut().find(|b| b.id == id).unwrap();
            boss.worm = Some(Box::new(WormWork::rising(boss.pos, boss.aim)));
        }
        assert_eq!(world.spawn_boss(BossKind::Sentinel), 0);
        assert_eq!(world.spawn_boss(BossKind::Sunderworm), 0);
        for boss in &mut world.bosses {
            boss.health = -1.;
        }
        world.tear_dying_worms();
        assert_eq!(world.bosses.len(), 16);
        assert!(
            world
                .bosses
                .iter()
                .all(|b| b.worm_part() != Some(WormPart::Whole))
        );
        let snapshot = world.snapshot();
        assert_eq!(snapshot.worm_bodies.len(), 16);
    }

    #[test]
    fn killing_both_halves_together_awards_the_rush_completion_bonus_once() {
        let mut world = world();
        tank_at(&mut world, "One", V2::new(1300., 1300.));
        let index = worm(&mut world, V2::new(2400., 1700.));
        for _ in 0..(5. / DT) as usize {
            keep_alive(&mut world);
            world.step();
        }
        world.bosses[index].health = -1.;
        world.step();
        assert_eq!(world.bosses.len(), 2);
        world.config.mode = Mode::BossRush;
        let before = world.tanks[0].score;
        for boss in &mut world.bosses {
            boss.health = -1.;
        }
        keep_alive(&mut world);
        world.step();
        assert!(world.bosses.is_empty());
        assert_eq!(world.tanks[0].score - before, 3500);
    }

    #[test]
    fn a_crushing_blow_still_tears_it_rather_than_ending_it() {
        let mut world = world();
        tank_at(&mut world, "One", V2::new(1300., 1300.));
        let index = worm(&mut world, V2::new(2400., 1700.));
        for _ in 0..(5. / DT) as usize {
            keep_alive(&mut world);
            world.step();
        }
        world.bosses[index].health = -50.;
        world.step();
        assert_eq!(world.bosses.len(), 2);
        assert!(world.bosses.iter().all(|b| b.health > 0.));
    }

    #[test]
    fn rounds_hit_its_body_anywhere_above_ground_but_never_below() {
        let mut world = world();
        tank_at(&mut world, "One", V2::new(600., 600.));
        let index = worm(&mut world, V2::new(2400., 1700.));
        // Wait for a moment it's all above ground.
        let mut up = false;
        for _ in 0..(30. / DT) as usize {
            keep_alive(&mut world);
            world.step();
            let work = world.bosses[index].worm.as_deref().unwrap();
            if work.rings.iter().all(Ring::surfaced) {
                up = true;
                break;
            }
        }
        assert!(up, "never all above ground");
        let b = &world.bosses[index];
        let work = b.worm.as_deref().unwrap();
        let tail = work.rings[WORM_RINGS - 3];
        assert!(tail.surfaced());
        let parts: Vec<_> = b.parts().collect();
        assert_eq!(
            parts.len(),
            work.rings.iter().filter(|r| r.surfaced()).count()
        );
        let (center, reach) = b.reach().unwrap();
        for ring in &work.rings {
            assert!(ring.pos.distance(center) <= reach);
        }
        let nearest = b.nearest_part(tail.pos).unwrap();
        assert!(nearest.0.distance(tail.pos) < 1.);
    }

    #[test]
    fn it_is_rare_in_the_wild_and_never_comes_for_beginners() {
        assert_eq!(rare_boss(0., WORM_STRENGTH), Some(BossKind::Sunderworm));
        assert_eq!(rare_boss(0., WORM_STRENGTH - 0.1), None);
        assert_eq!(rare_boss(WORM_ODDS * 1.01, 16.), None);
        const {
            assert!(WORM_ODDS < 1. / 1_000_000.);
        }
    }
}
