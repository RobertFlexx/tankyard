use crate::physics::{circle_rect, sweep_rect};
use crate::world::drone_phase;
use crate::*;
use tankyard_content::ProjectileKind;

/// How strongly a boss prefers the way it's already moving when picking a
/// clear path, so it commits to one side of an obstacle instead of hunting
/// between two that score about the same.
const KEEP_HEADING: f32 = 0.14;
/// How far past the edge of its range a target must be before a boss starts
/// to close in or back off. It stops at the edge itself, so a target standing
/// on the line can't flip it every tick and no boss comes any closer for it.
const TACTIC_MARGIN: f32 = 36.;
/// How quickly a boss's sidestep follows the rounds it's reading.
const DODGE_RATE: f32 = 14.;
/// How quickly its read of a target's motion catches up. A sharp turn can
/// still slip a shot, which is what makes dodging worth doing.
const READ_RATE: f32 = 7.;
/// The furthest ahead it will lead a target, in seconds.
const MOST_LEAD: f32 = 1.4;
/// How long before a special it starts bringing the gun round for that shot.
pub(crate) const LINING_UP: f32 = 0.6;
/// How close its aim must be, in radians, before it locks a special.
pub(crate) const LINED_UP: f32 = 0.1;
/// How far around its target one leg takes a circling boss, in radians.
const LEG_TURN: f32 = 0.62;
/// How long a boss keeps hunting for a target that slipped out of sight.
const LOST_SECONDS: f32 = 6.;
/// About how far a boss drives in a straight line before it picks a new one.
const LEG_LENGTH: f32 = 200.;

/// The padded hull can touch cover while the real hull is clear. A slide
/// along or away from that face is a legal escape, including from a corner.
fn clear_leg(from: V2, to: V2, radius: f32, walls: &[Rect]) -> bool {
    walls
        .iter()
        .all(|&wall| match sweep_rect(from, to, radius + 4., wall) {
            None => true,
            Some((0., normal)) => {
                (to - from).dot(normal) >= 0. && circle_rect(to, radius, wall).is_none()
            }
            Some(_) => false,
        })
}

pub(crate) fn boss_clear_steering(
    pos: V2,
    desired: V2,
    current: V2,
    radius: f32,
    bounds: Rect,
    walls: &[Rect],
) -> V2 {
    let heading = current.normalized();
    let mut best = V2::ZERO;
    let mut best_score = f32::NEG_INFINITY;
    for angle in [0., 0.35, -0.35, 0.7, -0.7, 1.2, -1.2, 1.8, -1.8, 2.6, -2.6] {
        let velocity = desired.rotate(angle);
        let next = pos + velocity * 0.75;
        if !bounds.contains_circle(next, radius + 8.) || !clear_leg(pos, next, radius, walls) {
            continue;
        }
        let border = (next.x - bounds.x)
            .min(next.y - bounds.y)
            .min(bounds.x + bounds.w - next.x)
            .min(bounds.y + bounds.h - next.y);
        let way = velocity.normalized();
        let score = way.dot(desired.normalized())
            + (border / 300.).min(1.) * 0.25
            + way.dot(heading) * KEEP_HEADING;
        if score > best_score {
            best = velocity;
            best_score = score;
        }
    }
    best
}

/// Where a boss moving by `step` ends up, and its velocity after: a move into
/// a wall keeps the part that runs along it, so the hull glides past corners
/// instead of stopping dead.
pub(crate) fn boss_glide(
    pos: V2,
    step: V2,
    vel: V2,
    radius: f32,
    bounds: Rect,
    walls: &[Rect],
) -> (V2, V2) {
    let clear = |at: V2| walls.iter().all(|&w| circle_rect(at, radius, w).is_none());
    let next = pos + step;
    let (mut at, mut vel) = if clear(next) {
        (next, vel)
    } else {
        // Try the longer part of the move first.
        let along_x = (V2::new(next.x, pos.y), V2::new(vel.x, vel.y * 0.25));
        let along_y = (V2::new(pos.x, next.y), V2::new(vel.x * 0.25, vel.y));
        let (first, second) = if step.x.abs() >= step.y.abs() {
            (along_x, along_y)
        } else {
            (along_y, along_x)
        };
        if clear(first.0) {
            first
        } else if clear(second.0) {
            second
        } else {
            (pos, vel * 0.6)
        }
    };
    // Don't keep pushing into the arena's edge.
    let kept = bounds.clamp_circle(at, radius);
    if kept.x != at.x {
        vel.x = 0.;
    }
    if kept.y != at.y {
        vel.y = 0.;
    }
    at = kept;
    (at, vel)
}

/// Where to point a gun at `from` so a round at `speed`, fired `warned`
/// seconds from now, meets a target at `at`. If cover stands between the gun
/// and that spot, it aims at the target itself.
pub(crate) fn lead_point(
    from: V2,
    (at, moving): (V2, V2),
    speed: f32,
    warned: f32,
    bounds: Rect,
    walls: &[Rect],
) -> V2 {
    let later = at + moving * warned;
    let flight = crate::boss_brain::intercept_time(later - from, moving, speed)
        .unwrap_or(0.)
        .min(MOST_LEAD);
    let point = bounds.clamp_circle(later + moving * flight, 1.);
    if walls
        .iter()
        .any(|&wall| sweep_rect(from, point, 5., wall).is_some())
    {
        at
    } else {
        point
    }
}

/// [`lead_point`] for a target that is turning by `swerve`, so a boss can
/// meet a curved path where it will be, not where a straight line says.
pub(crate) fn bent_lead_point(
    from: V2,
    (at, moving, swerve): (V2, V2, V2),
    speed: f32,
    warned: f32,
    bounds: Rect,
    walls: &[Rect],
) -> V2 {
    if swerve == V2::ZERO {
        return lead_point(from, (at, moving), speed, warned, bounds, walls);
    }
    let point = bounds.clamp_circle(
        crate::boss_brain::curved_lead(from, at, moving, swerve, speed, warned),
        1.,
    );
    if walls
        .iter()
        .any(|&wall| sweep_rect(from, point, 5., wall).is_some())
    {
        at
    } else {
        point
    }
}

/// A point `ring` from `seen` with a clear line to it, as near `from` as
/// there is one: where a boss goes to look round the cover its target
/// slipped behind.
pub(crate) fn flank_point(
    from: V2,
    seen: V2,
    ring: f32,
    radius: f32,
    bounds: Rect,
    walls: &[Rect],
) -> V2 {
    let out = (from - seen).normalized();
    let out = if out == V2::ZERO {
        V2::new(1., 0.)
    } else {
        out
    };
    let mut best = None;
    let mut nearest = f32::INFINITY;
    for step in 0..12 {
        let side = if step % 2 == 0 { 1. } else { -1. };
        let turn = (step / 2 + step % 2) as f32 * 0.52 * side;
        let point = bounds.clamp_circle(seen + out.rotate(turn) * ring, radius + 8.);
        let open = walls.iter().all(|&wall| {
            sweep_rect(point, seen, 6., wall).is_none()
                && circle_rect(point, radius, wall).is_none()
        });
        let distance = point.distance(from);
        if open && distance < nearest {
            nearest = distance;
            best = Some(point);
        }
    }
    best.unwrap_or(seen)
}

/// What a boss can see of the fight beyond its target.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Outlook {
    /// What it weighs its specials by.
    pub special: crate::boss_brain::Scene,
    /// How hurt its target is, from 0 to 1.
    pub weak: f32,
    /// Which way round it should circle so another boss on the same target
    /// ends up across from it: 1, -1, or 0 for either.
    pub flank: f32,
}

/// How fast the round a boss aims with flies.
fn aimed_speed(kind: BossKind) -> f32 {
    match kind {
        BossKind::SiegeEngine => 390.,
        BossKind::Carillon => 880.,
        BossKind::Jester => 640.,
        BossKind::Railgunner => 1700.,
        BossKind::Sunderworm => 660.,
        BossKind::Skirmisher => 450.,
        BossKind::Trapper => 850.,
        BossKind::Guardian => 355.,
        BossKind::Summoner => 325.,
        BossKind::FallenOverlord => 390.,
        BossKind::Defender => 410.,
        BossKind::Prism => 430.,
        BossKind::FallenBooster => 460.,
        BossKind::Sentinel => 330.,
        BossKind::Foundry => 295.,
        BossKind::Tempest => 285.,
        BossKind::Bastion => 340.,
        BossKind::Eclipse => 370.,
    }
}

/// The same for the rounds of its special attack.
fn special_speed(kind: BossKind) -> f32 {
    match kind {
        BossKind::SiegeEngine => SIEGE_ROUND_SPEED,
        BossKind::Railgunner => RAIL_ROUND_SPEED,
        BossKind::Bastion => BASTION_ROUND_SPEED,
        BossKind::Carillon => 1430.,
        BossKind::Sentinel => 560.,
        BossKind::Skirmisher => 420.,
        _ => aimed_speed(kind),
    }
}

/// The end of a boss's next straight leg: on the line to `foe` when it's
/// closing in or backing off, or a chord around it when circling, keeping the
/// way it's already going unless that way is blocked.
fn next_leg(
    b: &Boss,
    tactic: BossTactic,
    foe: V2,
    ring: f32,
    bounds: Rect,
    walls: &[Rect],
    flank: f32,
) -> V2 {
    let away = b.pos - foe;
    let out = if away.length_squared() > 1. {
        away.normalized()
    } else {
        V2::from_angle(b.aim + std::f32::consts::PI)
    };
    if tactic != BossTactic::Orbit {
        let radial = bounds.clamp_circle(foe + out * ring, b.radius + 8.);
        if b.kind == BossKind::Trapper
            && tactic == BossTactic::Retreat
            && ((radial - b.pos).dot(out) < LEG_LENGTH * 0.55
                || !clear_leg(b.pos, radial, b.radius, walls))
        {
            // Pick a full, clear leg along the edge, even when reaching it
            // means briefly moving closer to the player blocking the exit.
            let mut best = None;
            for n in 0..16 {
                let way = out.rotate(n as f32 * std::f32::consts::TAU / 16.);
                let point = b.pos + way * LEG_LENGTH;
                if bounds.contains_circle(point, b.radius + 8.)
                    && clear_leg(b.pos, point, b.radius, walls)
                {
                    let room = (point.x - bounds.x)
                        .min(bounds.x + bounds.w - point.x)
                        .min(point.y - bounds.y)
                        .min(bounds.y + bounds.h - point.y);
                    let score = point.distance(foe)
                        + room.min(LEG_LENGTH) * 0.9
                        + way.dot(b.vel.normalized()) * 18.;
                    if best.is_none_or(|(previous, _)| score > previous) {
                        best = Some((score, point));
                    }
                }
            }
            if let Some((_, point)) = best {
                return point;
            }
        }
        return radial;
    }
    let spin = out.x * b.vel.y - out.y * b.vel.x;
    // With another boss on the same tank, it circles the other way round, so
    // the two close on it from both sides.
    let way = if flank != 0. {
        flank
    } else if spin.abs() > 1. {
        spin.signum()
    } else if b.id.is_multiple_of(2) {
        1.
    } else {
        -1.
    };
    for side in [way, -way] {
        let point = foe + out.rotate(side * LEG_TURN) * ring;
        if bounds.contains_circle(point, b.radius + 8.)
            && walls
                .iter()
                .all(|&wall| sweep_rect(b.pos, point, b.radius + 4., wall).is_none())
        {
            return point;
        }
    }
    // Boxed in both ways: head on, and let the steering find a way past.
    bounds.clamp_circle(foe + out.rotate(way * LEG_TURN) * ring, b.radius + 8.)
}

/// A round a boss fires this tick: its owner, team, origin, velocity, kind,
/// damage, radius, life, penetration and bounces.
pub(crate) type Volley = (EntityId, u8, V2, V2, ProjectileKind, f32, f32, f32, f32, u8);

/// One round of a boss's volley.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BossRound {
    /// The way it flies, in radians.
    pub angle: f32,
    pub kind: ProjectileKind,
    pub speed: f32,
    pub damage: f32,
    pub radius: f32,
    /// Seconds before it fades.
    pub life: f32,
    pub penetration: f32,
    pub bounces: u8,
}

/// After the warned side mines land, two opposite launchers sweep outward
/// from those flanks. Later rounds travel farther before settling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TrapSpiral {
    aim: f32,
    phase: u8,
    tick: u8,
}
impl TrapSpiral {
    fn steps(phase: u8) -> u8 {
        6 + phase.clamp(1, 3) * 2
    }

    fn pair(aim: f32, phase: u8, step: u8) -> [BossRound; 2] {
        let travel = f32::from(step) / f32::from(Self::steps(phase) - 1);
        let angle = aim + std::f32::consts::FRAC_PI_2 + 3.8 * travel;
        [angle, angle + std::f32::consts::PI].map(|heading| {
            round(
                heading,
                ProjectileKind::Trap,
                350. + 990. * travel,
                29.,
                12.,
                5.,
                3.,
                0,
            )
        })
    }

    fn advance(active: &mut Option<Self>) -> Option<[BossRound; 2]> {
        let spiral = active.as_mut()?;
        spiral.tick += 1;
        let elapsed = spiral.tick.checked_sub(10)?;
        let step = elapsed / 5;
        let pair = elapsed
            .is_multiple_of(5)
            .then(|| Self::pair(spiral.aim, spiral.phase, step));
        if pair.is_some() && step + 1 == Self::steps(spiral.phase) {
            *active = None;
        }
        pair
    }
}

/// How fast the Siege Engine's cannon round flies.
pub const SIEGE_ROUND_SPEED: f32 = 1500.;
/// How wide the Siege Engine's cannon round is.
pub const SIEGE_ROUND_RADIUS: f32 = 24.;
/// How fast Bastion's cannon round flies.
pub const BASTION_ROUND_SPEED: f32 = 1050.;
/// How wide Bastion's cannon round is.
pub const BASTION_ROUND_RADIUS: f32 = 18.;
/// What Bastion's cannon round deals: a bad day, not the end of one. No other
/// boss round comes close, which is how the game knows to draw it as a cannon.
pub const BASTION_ROUND_DAMAGE: f32 = 125.;

#[allow(clippy::too_many_arguments)]
pub(crate) const fn round(
    angle: f32,
    kind: ProjectileKind,
    speed: f32,
    damage: f32,
    radius: f32,
    life: f32,
    penetration: f32,
    bounces: u8,
) -> BossRound {
    BossRound {
        angle,
        kind,
        speed,
        damage,
        radius,
        life,
        penetration,
        bounces,
    }
}

/// The rounds of a boss's special attack: the `attack`th of its three,
/// aimed at `aim`, in `phase`. Its warning shows these same lanes. The first
/// is each boss's signature, the one it opens with; the other two suit other
/// fights, and the third is what it reaches for when badly hurt. Every ring
/// leaves a gap, and every round meets walls, parries and the round limit as
/// any other.
pub fn special_volley(kind: BossKind, attack: u8, aim: f32, phase: u8) -> Vec<BossRound> {
    use ProjectileKind::*;
    use std::f32::consts::{FRAC_PI_2, TAU};
    if kind == BossKind::Jester {
        return crate::encounters::jester_volley(attack, aim, phase);
    }
    if kind == BossKind::Carillon {
        return crate::encounters::carillon_volley(attack.min(2), aim, phase);
    }
    let attack = attack.min(2);
    // Wounded, everything comes thicker.
    let more = usize::from(phase.clamp(1, 3) - 1);
    let mut rounds = Vec::with_capacity(32);
    fn fan(rounds: &mut Vec<BossRound>, count: usize, spread: f32, shot: BossRound) {
        for n in 0..count {
            let offset = (n as f32 - (count - 1) as f32 * 0.5) * spread;
            rounds.push(BossRound {
                angle: shot.angle + offset,
                ..shot
            });
        }
    }
    match (kind, attack) {
        (BossKind::Railgunner, _) => return crate::railgunner::rail_volley(attack, aim, phase),
        (BossKind::Sunderworm | BossKind::Jester | BossKind::Carillon, _) => {}
        (BossKind::SiegeEngine, 0) => {
            // One giant round down the locked line destroys whatever it
            // hits; cover stops it. Wounded, it flanks the line with shells.
            rounds.push(round(
                aim,
                Shell,
                SIEGE_ROUND_SPEED,
                LETHAL_DAMAGE,
                SIEGE_ROUND_RADIUS,
                1.8,
                40.,
                0,
            ));
            if phase >= 2 {
                for side in [-1., 1.] {
                    rounds.push(round(
                        aim + side * 0.2,
                        Explosive,
                        520.,
                        40.,
                        13.,
                        2.4,
                        1.8,
                        0,
                    ));
                }
            }
        }
        (BossKind::SiegeEngine, 1) => {
            // Heavy shells bracketing the line, for a target that won't sit
            // still: wherever it dodges, one lands near.
            fan(
                &mut rounds,
                5 + 2 * more,
                0.13,
                round(aim, Shell, 760., 62., 16., 2.2, 3., 0),
            );
        }
        (BossKind::SiegeEngine, _) => {
            // The big gun, and a carpet of charges either side of it.
            rounds.push(round(
                aim,
                Shell,
                SIEGE_ROUND_SPEED,
                LETHAL_DAMAGE,
                SIEGE_ROUND_RADIUS,
                1.8,
                40.,
                0,
            ));
            for side in [-1., 1.] {
                for n in 0..2 + more {
                    rounds.push(round(
                        aim + side * (0.26 + n as f32 * 0.17),
                        Explosive,
                        470. + n as f32 * 40.,
                        38.,
                        13.,
                        2.4,
                        1.8,
                        0,
                    ));
                }
            }
        }
        (BossKind::Bastion, 0) => {
            // A lighter cannon than the Siege Engine's: it hurts badly rather
            // than ends you. Wounded, it brackets the line with charges.
            rounds.push(round(
                aim,
                Shell,
                BASTION_ROUND_SPEED,
                BASTION_ROUND_DAMAGE,
                BASTION_ROUND_RADIUS,
                1.7,
                8.,
                0,
            ));
            if phase >= 2 {
                for side in [-1., 1.] {
                    rounds.push(round(
                        aim + side * 0.24,
                        Explosive,
                        480.,
                        36.,
                        12.,
                        2.2,
                        1.6,
                        0,
                    ));
                }
            }
        }
        (BossKind::Bastion, 1) => {
            // A wall of mines thrown across the way in, landing in a line.
            for n in 0..9 + 2 * more {
                let side = (n as f32 / (8 + 2 * more) as f32 - 0.5) * 1.5;
                rounds.push(round(
                    aim + side,
                    Mine,
                    300. + side.abs() * 140.,
                    27.,
                    13.,
                    6.5,
                    2.,
                    0,
                ));
            }
        }
        (BossKind::Bastion, _) => {
            rounds.push(round(
                aim,
                Shell,
                BASTION_ROUND_SPEED,
                BASTION_ROUND_DAMAGE,
                BASTION_ROUND_RADIUS,
                1.7,
                8.,
                0,
            ));
            for n in 1..10 {
                rounds.push(round(
                    aim + n as f32 * TAU / 10.,
                    Mine,
                    240.,
                    25.,
                    12.,
                    6.,
                    2.,
                    0,
                ));
            }
        }
        (BossKind::Skirmisher, 0) => {
            for n in [-2., -1., 1., 2.] {
                rounds.push(round(aim + n * 0.18, Bullet, 420., 18., 7., 2.4, 1.2, 0));
            }
        }
        (BossKind::Skirmisher, 1) => {
            // Two pairs angled in from either side: crossfire.
            for side in [-1., 1.] {
                fan(
                    &mut rounds,
                    3 + more,
                    0.06,
                    round(aim + side * 0.42, Bullet, 480., 17., 7., 2.2, 1.2, 0),
                );
            }
        }
        (BossKind::Skirmisher, _) => {
            fan(
                &mut rounds,
                9 + 2 * more,
                0.12,
                round(aim, Bullet, 500., 16., 7., 2.2, 1.2, 0),
            );
        }
        (BossKind::Trapper, 0) => {
            for side in [-1., 1.] {
                for n in -1..=1 {
                    rounds.push(round(
                        aim + side * FRAC_PI_2 + n as f32 * 0.19,
                        Mine,
                        230.,
                        23.,
                        11.,
                        5.,
                        2.2,
                        0,
                    ));
                }
            }
        }
        (BossKind::Trapper, 1) => {
            // An arc of traps ahead, to hem a tank in.
            fan(
                &mut rounds,
                7 + 2 * more,
                0.24,
                round(aim, Trap, 320., 22., 11., 5., 2.5, 0),
            );
        }
        (BossKind::Trapper, _) => {
            // Mines strung down the lane at rising speeds, landing in a row.
            for n in 0..5 + more {
                rounds.push(round(
                    aim + if n % 2 == 0 { 0.05 } else { -0.05 },
                    Mine,
                    180. + n as f32 * 75.,
                    24.,
                    11.,
                    5.,
                    2.2,
                    0,
                ));
            }
        }
        (BossKind::Guardian | BossKind::Summoner | BossKind::FallenOverlord, 0) => {
            let shot = if kind == BossKind::FallenOverlord {
                Homing
            } else {
                Bullet
            };
            for n in 0..12 {
                let angle = aim + (n as f32 - 5.5) * 0.24;
                rounds.push(round(angle, shot, 340., 18., 8., 2.8, 1.3, 0));
            }
        }
        (BossKind::Guardian | BossKind::Summoner | BossKind::FallenOverlord, 1) => {
            // A ring all round, with a gap where it's looking.
            let count = 20 + 4 * more;
            for n in 1..count {
                rounds.push(round(
                    aim + n as f32 * TAU / count as f32,
                    Bullet,
                    300.,
                    16.,
                    8.,
                    3.,
                    1.2,
                    0,
                ));
            }
        }
        (BossKind::Guardian | BossKind::Summoner | BossKind::FallenOverlord, _) => {
            // Lances: tight lines of quick rounds down a few lanes.
            let lanes = 3 + 2 * more;
            for lane in 0..lanes {
                let angle = aim + (lane as f32 - (lanes - 1) as f32 * 0.5) * 0.3;
                for n in 0..4 {
                    rounds.push(round(
                        angle,
                        Piercing,
                        520. + n as f32 * 70.,
                        17.,
                        7.,
                        2.4,
                        1.5,
                        0,
                    ));
                }
            }
        }
        (BossKind::Defender, 0) => {
            for n in 1..12 {
                rounds.push(round(
                    aim + n as f32 * TAU / 12.,
                    Trap,
                    320.,
                    27.,
                    12.,
                    5.,
                    3.5,
                    0,
                ));
            }
            rounds.push(round(aim, Shell, 460., 38., 14., 2.5, 2., 0));
        }
        (BossKind::Defender, 1) => {
            fan(
                &mut rounds,
                5 + more,
                0.16,
                round(aim, Shell, 560., 40., 14., 2.5, 2., 0),
            );
        }
        (BossKind::Defender, _) => {
            for n in 1..12 {
                rounds.push(round(
                    aim + 0.26 + n as f32 * TAU / 12.,
                    Trap,
                    360.,
                    27.,
                    12.,
                    5.,
                    3.5,
                    0,
                ));
            }
            fan(
                &mut rounds,
                3 + more,
                0.22,
                round(aim, Explosive, 420., 34., 12., 2.4, 1.6, 0),
            );
        }
        (BossKind::FallenBooster | BossKind::Tempest, 0) => {
            for n in 0..5 {
                rounds.push(round(
                    aim + (n as f32 - 2.) * 0.22,
                    Homing,
                    350.,
                    18.,
                    8.,
                    3.,
                    1.2,
                    0,
                ));
            }
        }
        (BossKind::FallenBooster | BossKind::Tempest, 1) => {
            // A burst all round, for whatever is close.
            let count = 14 + 4 * more;
            for n in 1..count {
                rounds.push(round(
                    aim + n as f32 * TAU / count as f32,
                    Bullet,
                    420.,
                    16.,
                    7.,
                    1.6,
                    1.,
                    0,
                ));
            }
        }
        (BossKind::FallenBooster | BossKind::Tempest, _) => {
            fan(
                &mut rounds,
                8 + 2 * more,
                0.2,
                round(aim, Homing, 400., 17., 8., 3., 1.2, 0),
            );
        }
        (BossKind::Sentinel, 0) => {
            for n in 0..7 {
                rounds.push(round(
                    aim + (n as f32 - 3.) * 0.13,
                    Piercing,
                    560.,
                    28.,
                    8.,
                    2.5,
                    2.,
                    0,
                ));
            }
        }
        (BossKind::Sentinel, 1) => {
            // Three quick rounds down one line, then two either side.
            for n in 0..3 + more {
                rounds.push(round(
                    aim,
                    Piercing,
                    820. + n as f32 * 90.,
                    34.,
                    9.,
                    2.,
                    2.,
                    0,
                ));
            }
            for side in [-1., 1.] {
                rounds.push(round(aim + side * 0.09, Piercing, 760., 30., 8., 2., 2., 0));
            }
        }
        (BossKind::Sentinel, _) => {
            fan(
                &mut rounds,
                11 + 2 * more,
                0.11,
                round(aim, Piercing, 540., 26., 8., 2.5, 2., 0),
            );
            if phase >= 3 {
                fan(
                    &mut rounds,
                    6,
                    0.11,
                    round(aim, Piercing, 420., 24., 8., 2.5, 2., 0),
                );
            }
        }
        (BossKind::Prism | BossKind::Eclipse, 0 | 1) => {
            let shot = if attack == 1 { Homing } else { Piercing };
            let turn = f32::from(attack) * 0.33;
            for n in 1..16 {
                rounds.push(round(
                    aim + turn + n as f32 * TAU / 16.,
                    shot,
                    300. + n as f32 * 8.,
                    17.,
                    7.,
                    3.,
                    1.5,
                    1,
                ));
            }
        }
        (BossKind::Prism | BossKind::Eclipse, _) => {
            // Two rings at two speeds, turned against each other: a spiral.
            for layer in 0..2 + more.min(1) {
                let twist = layer as f32 * 0.2;
                for n in 1..14 {
                    rounds.push(round(
                        aim + 0.66 + twist + n as f32 * TAU / 14.,
                        Piercing,
                        260. + layer as f32 * 90. + n as f32 * 6.,
                        16.,
                        7.,
                        3.,
                        1.5,
                        1,
                    ));
                }
            }
        }
        (BossKind::Foundry, 0) => {
            for n in 0..5 {
                rounds.push(round(
                    aim + (n as f32 - 2.) * 0.35,
                    Explosive,
                    310.,
                    32.,
                    12.,
                    3.,
                    1.5,
                    0,
                ));
            }
            for n in 0..6 {
                rounds.push(round(
                    aim + n as f32 * TAU / 6.,
                    Mine,
                    240.,
                    25.,
                    11.,
                    5.,
                    2.5,
                    0,
                ));
            }
        }
        (BossKind::Foundry, 1) => {
            // A carpet of mines thrown forward.
            fan(
                &mut rounds,
                8 + 2 * more,
                0.2,
                round(aim, Mine, 330., 25., 11., 5.5, 2.5, 0),
            );
        }
        (BossKind::Foundry, _) => {
            // Two guns set down either side, and charges down the middle.
            for side in [-1., 1.] {
                rounds.push(round(aim + side * 0.7, Turret, 150., 19., 17., 7., 4., 0));
            }
            fan(
                &mut rounds,
                5 + more,
                0.18,
                round(aim, Explosive, 360., 32., 12., 3., 1.5, 0),
            );
        }
    }
    rounds
}

impl World {
    pub(crate) fn boss_drone_goal(&self, boss: &Boss, drone: EntityId, pos: V2, team: u8) -> V2 {
        let (acquisition, leash) = match boss.kind {
            BossKind::Summoner => (820., 1050.),
            BossKind::Guardian => (900., 1200.),
            BossKind::FallenOverlord => (1000., 1350.),
            _ => (760., 1000.),
        };
        let guard = boss.pos
            + V2::from_angle(drone_phase(drone) + self.tick as f32 * DT * 0.25)
                * (boss.radius + 55.);
        if (pos - boss.pos).length_squared() > leash * leash {
            return guard;
        }
        // Acquire from the drone, but never chain pursuit across the map. The
        // owner leash and protected/new-player rules match the boss's awareness.
        self.tanks
            .iter()
            .filter(|tank| {
                tank.tangible()
                    && tank.health > 0.
                    && tank.protection <= 0.
                    && !self.config.base_protected(tank.team, tank.pos, tank.radius)
                    && self.hostile(team, tank.team)
            })
            .filter_map(|tank| {
                // Cache squared distances once per candidate; drones make
                // these decisions every tick, and ordering needs no roots.
                let from_boss = (tank.pos - boss.pos).length_squared();
                let from_drone = (tank.pos - pos).length_squared();
                if (tank.level < 15 && tank.id != boss.last_attacker && from_boss >= 420. * 420.)
                    || from_drone > acquisition * acquisition
                    || from_boss > (leash - 100.) * (leash - 100.)
                    || self
                        .walls
                        .iter()
                        .any(|&wall| sweep_rect(pos, tank.pos, 2., wall).is_some())
                {
                    return None;
                }
                Some((from_drone, tank))
            })
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map_or(guard, |(distance2, tank)| {
                if tank.vel.length_squared() < 1. || boss.kind == BossKind::Sunderworm {
                    return tank.pos;
                }
                // Cut across a circling tank for a short distance. Keep the
                // lead on visible ground and inside this boss's normal leash.
                let distance = distance2.sqrt();
                let lead = (distance / aimed_speed(boss.kind)).min(0.55);
                // Split a distant pursuit across both shoulders. The spacing
                // tapers to zero on contact so a drone still finishes its run.
                let shoulder =
                    (drone_phase(drone).sin() * 85.) * ((distance - 110.) / 320.).clamp(0., 1.);
                let heading = (tank.pos - pos).normalized();
                let point = tank.pos
                    + tank.vel.clamp_length(480.) * lead
                    + V2::new(-heading.y, heading.x) * shoulder;
                if (point - boss.pos).length_squared() <= leash * leash
                    && self.config.bounds().contains(point)
                    && self
                        .walls
                        .iter()
                        .all(|&w| sweep_rect(pos, point, 2., w).is_none())
                {
                    point
                } else {
                    tank.pos
                }
            })
    }

    /// Who boss `i` goes after this tick, and what else it can see. It weighs
    /// everyone in sight now and then and keeps after its choice in between:
    /// see `boss_brain`.
    fn boss_target(&mut self, i: usize) -> (Option<(EntityId, V2, V2)>, Outlook) {
        let b = &self.bosses[i];
        let (kind, pos, aim, id, team, current) = (b.kind, b.pos, b.aim, b.id, b.team, b.target);
        let phase = if b.health < b.max_health * BOSS_PHASES[1] {
            3
        } else if b.health < b.max_health * BOSS_PHASES[0] {
            2
        } else {
            1
        };
        let busy =
            b.attack_windup > 0. || matches!(b.tactic, BossTactic::Windup | BossTactic::Charge);
        let visible: Vec<usize> = (0..self.tanks.len())
            .filter(|&j| {
                let t = &self.tanks[j];
                t.tangible()
                    && t.health > 0.
                    && self.hostile(team, t.team)
                    && t.protection <= 0.
                    && !self.config.base_protected(t.team, t.pos, t.radius)
                    && t.pos.distance(pos) < 1600.
                    && (t.level >= 15
                        || kind.drone_limit() == 0
                        || t.id == b.last_attacker
                        || t.pos.distance(pos) < 420.)
                    && self
                        .walls
                        .iter()
                        .all(|&w| sweep_rect(pos, t.pos, 5., w).is_none())
            })
            .collect();
        let near_foes = self
            .tanks
            .iter()
            .filter(|t| {
                t.tangible()
                    && t.health > 0.
                    && self.hostile(team, t.team)
                    && t.pos.distance(pos) < 750.
            })
            .count() as f32;
        let reads: Vec<crate::boss_brain::Read> = visible
            .iter()
            .map(|&j| {
                let t = &self.tanks[j];
                let crowd = visible
                    .iter()
                    .filter(|&&k| k != j && self.tanks[k].pos.distance(t.pos) < 420.)
                    .count() as f32;
                let hunted = self
                    .bosses
                    .iter()
                    .filter(|o| o.id != id && o.health > 0. && o.target == t.id)
                    .count() as f32;
                crate::boss_brain::Read {
                    id: t.id,
                    near: 1. - (t.pos.distance(pos) / 1600.).min(1.),
                    threat: b.mind.threat(t.id, b.max_health * 0.03),
                    weak: 1. - (t.health / t.max_health.max(1.)).clamp(0., 1.),
                    focus: if t.id == current { 1. } else { 0. },
                    crowd: (crowd / 4.).min(1.),
                    strong: f32::from(t.level) / 45.,
                    front: (V2::from_angle(aim).dot((t.pos - pos).normalized()) + 1.) * 0.5,
                    fresh: b.mind.fresh(t.id, current),
                    hunted: (hunted / 3.).min(1.),
                    quick: (t.vel.length() / 420.).min(1.),
                }
            })
            .collect();
        let roll = self.random();
        let think = self.random();
        let records = self
            .contributions
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let b = &mut self.bosses[i];
        b.mind.feel(records, DT);
        let in_sight = reads.iter().any(|r| r.id == current);
        if !in_sight {
            b.mind.lost();
        }
        let mut chosen = if in_sight { current } else { 0 };
        if ((!busy && b.mind.due()) || chosen == 0)
            && let Some(pick) = crate::boss_brain::draw(kind, phase, &reads, roll)
        {
            b.mind.chose(pick, current, think);
            chosen = pick;
        }
        let target = visible
            .iter()
            .map(|&j| &self.tanks[j])
            .find(|t| t.id == chosen)
            .map(|t| (t.id, t.pos, t.vel));
        let mut outlook = Outlook {
            special: crate::boss_brain::Scene {
                range: 800.,
                foes: near_foes,
                across: 0.,
                hurt: 1. - (b.health / b.max_health.max(1.)).clamp(0., 1.),
            },
            ..Outlook::default()
        };
        if let Some((target_id, at, moving)) = target {
            b.mind.saw(at);
            let line = (at - pos).normalized();
            outlook.special.range = at.distance(pos);
            outlook.special.across = (moving - line * moving.dot(line)).length();
            outlook.weak = reads
                .iter()
                .find(|r| r.id == target_id)
                .map_or(0., |r| r.weak);
            // Another boss on the same tank: take the far side of it.
            if let Some(other) = self
                .bosses
                .iter()
                .find(|o| o.id != id && o.health > 0. && o.target == target_id && !o.mothership)
            {
                let mine = (pos - at).angle();
                let theirs = (other.pos - at).angle();
                let gap = (mine - theirs + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                if gap.abs() < 2.6 {
                    outlook.flank = if gap >= 0. { 1. } else { -1. };
                }
            }
        }
        (target, outlook)
    }

    pub(crate) fn random_boss_event(&mut self) {
        if self.tick < self.boss_roll_at {
            return;
        }
        self.boss_roll_at = self.tick.saturating_add(60);
        if !matches!(
            self.config.mode,
            Mode::Ffa | Mode::TwoTeams | Mode::FourTeams | Mode::Maze | Mode::RandomEvents
        ) {
            return;
        }
        if !self.bosses.is_empty() {
            self.boss_roll_at = self.tick.saturating_add(90 * 60);
            return;
        }
        if !self.tanks.iter().any(|t| t.alive) {
            return;
        }
        // One aggregate 0.25% roll per second, independent of simulation FPS.
        if self.random() < 0.0025 {
            let (rare, pick) = (self.random(), self.random());
            let strength = self.arena_strength();
            self.spawn_boss(crate::railgunner::natural_boss(rare, pick, strength));
        }
    }

    /// How many full-level tanks the arena's tanks are worth, up to 16. A
    /// new tank counts for about an eighth of one at the top level.
    pub(crate) fn arena_strength(&self) -> f32 {
        self.tanks
            .iter()
            .map(|tank| {
                let level = f32::from(tank.level) / f32::from(tankyard_content::MAX_LEVEL);
                0.12 + 0.88 * level.clamp(0., 1.).powf(1.6)
            })
            .sum::<f32>()
            .min(16.)
    }

    /// A boss's full health in this arena: as tough as the arena is strong,
    /// so a fight lasts about as long for one veteran, a party or a crowd of
    /// new players.
    pub(crate) fn boss_health(&self, kind: BossKind) -> f32 {
        self.boss_health_at(kind, self.arena_strength())
    }

    fn boss_health_at(&self, kind: BossKind, strength: f32) -> f32 {
        // A swarm stops growing at four players; past that, its hull does.
        let crowd = if kind.drone_limit() > 0 {
            1. + (strength - 4.).max(0.) * 0.25
        } else {
            1.
        };
        // Each wave is 14% tougher than the first, up to 70% on the last, so
        // a long lineup climbs as far as the usual six and no further.
        let bosses = self.config.rush_lineup().len();
        let climb = if bosses <= 6 {
            0.14
        } else {
            0.7 / (bosses - 1) as f32
        };
        11_000.
            * (1. + f32::from(self.match_state.rush_wave.max(1) - 1) * climb)
            * kind.toughness()
            * (0.3 + 0.7 * strength)
            * crowd
    }

    /// Returns zero if the map cannot accommodate the boss safely, or the
    /// encounter limit is full. A whole worm reserves room for both halves.
    pub fn spawn_boss(&mut self, kind: BossKind) -> EntityId {
        self.spawn_boss_from(kind, None)
    }
    /// A boss arriving as close to `near` as is safe, as map triggers ask.
    pub fn spawn_boss_near(&mut self, kind: BossKind, near: V2) -> EntityId {
        self.spawn_boss_from(kind, Some(near))
    }
    fn spawn_boss_from(&mut self, kind: BossKind, near: Option<V2>) -> EntityId {
        let occupied: usize = self
            .bosses
            .iter()
            .map(|boss| {
                1 + usize::from(boss.worm_part() == Some(crate::sunderworm::WormPart::Whole))
            })
            .sum();
        let needed = 1 + usize::from(kind == BossKind::Sunderworm);
        if occupied + needed > 16 {
            return 0;
        }
        let bounds = self.config.bounds();
        let radius = kind.radius();
        let mut best = None;
        let mut safety = f32::NEG_INFINITY;
        let inside = |p: V2| {
            V2::new(
                crate::within(
                    p.x,
                    bounds.x + radius + 30.,
                    bounds.x + bounds.w - radius - 30.,
                ),
                crate::within(
                    p.y,
                    bounds.y + radius + 30.,
                    bounds.y + bounds.h - radius - 30.,
                ),
            )
        };
        for attempt in 0..128 {
            let pos = match near {
                // Spiral outward from the requested point.
                Some(near) => {
                    let turn = attempt as f32 * 2.4;
                    let reach = attempt as f32 * 14.;
                    inside(near + V2::new(turn.cos(), turn.sin()) * reach)
                }
                None if attempt == 0 => bounds.center(),
                None => V2::new(
                    bounds.x + radius + 30. + self.random() * (bounds.w - 2. * radius - 60.),
                    bounds.y + radius + 30. + self.random() * (bounds.h - 2. * radius - 60.),
                ),
            };
            if !self.position_clear(pos, radius + 25.)
                || self
                    .bosses
                    .iter()
                    .any(|b| b.pos.distance(pos) < b.radius + radius + 40.)
            {
                continue;
            }
            let nearest = self
                .tanks
                .iter()
                .filter(|t| t.alive)
                .map(|t| t.pos.distance(pos) - t.radius - radius)
                .fold(1500., f32::min);
            if near.is_some() && nearest >= 160. {
                // The nearest safe point, not the emptiest corner.
                safety = nearest;
                best = Some(pos);
                break;
            }
            if nearest > safety {
                safety = nearest;
                best = Some(pos);
            }
            if safety > 700. {
                break;
            }
        }
        let Some(pos) = best else {
            return 0;
        };
        // Never materialize a damaging hull on top of a participant.
        if safety < 100. {
            return 0;
        }
        let health = self.boss_health(kind);
        let id = self.id();
        // The worm comes up out of the ground facing whoever is nearest.
        let worm = (kind == BossKind::Sunderworm).then(|| {
            let facing = self
                .tanks
                .iter()
                .filter(|t| t.alive)
                .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
                .map_or(0., |t| (t.pos - pos).angle());
            Box::new(crate::sunderworm::WormWork::rising(pos, facing))
        });
        let arrival = if worm.is_some() {
            crate::sunderworm::RISE_SECONDS
        } else {
            2.
        };
        self.bosses.push(Boss {
            id,
            pos,
            vel: V2::ZERO,
            radius,
            health,
            max_health: health,
            kind,
            phase: 1,
            timer: 2.,
            aim: 0.,
            team: 0,
            mothership: false,
            target: 0,
            goal: pos,
            tactic: BossTactic::Patrol,
            tactic_time: 2.,
            last_attacker: 0,
            arrival,
            attack_timer: 7.,
            attack: 0,
            attack_windup: 0.,
            dodge: V2::ZERO,
            read: V2::ZERO,
            leg: pos,
            leg_time: 0.,
            rail: RailWork::default(),
            clash: crate::clash::ClashWork::default(),
            mind: crate::boss_brain::Mind::default(),
            worm,
        });
        if let Some(boss) = self.bosses.last_mut()
            && let Some(work) = boss.worm.as_deref()
        {
            work.sign().write(boss);
        }
        self.event(EventKind::BossSpawn, pos, id, kind as u32, 0);
        id
    }
    pub(crate) fn update_bosses(&mut self) {
        if self.bosses.is_empty() {
            return;
        }
        self.land_clashes();
        let mut volleys: Vec<Volley> = Vec::new();
        let mut warnings = Vec::new();
        let mut axiom_casts = Vec::new();
        // A swarm grows with the arena: one player can still cut through to
        // the hull, and it reaches its full size against a party of four.
        let strength = self.arena_strength();
        let swarm = (0.25 + 0.1875 * strength).min(1.);
        // Players joining or leaving mid-fight move a wild boss's health with
        // them, keeping how hurt it is. Modes that set their own leave theirs.
        for i in 0..self.bosses.len() {
            let boss = &self.bosses[i];
            if boss.mothership || boss.team != 0 {
                continue;
            }
            let half = matches!(
                boss.worm_part(),
                Some(crate::sunderworm::WormPart::Front | crate::sunderworm::WormPart::Back)
            );
            let full = self.boss_health_at(boss.kind, strength)
                * if half {
                    crate::sunderworm::HALF_HEALTH
                } else {
                    1.
                };
            let boss = &mut self.bosses[i];
            if (full - boss.max_health).abs() > boss.max_health * 0.02 {
                boss.health *= full / boss.max_health.max(1.);
                boss.max_health = full;
            }
        }
        for i in 0..self.bosses.len() {
            let b = &self.bosses[i];
            if b.health <= 0. {
                continue;
            }
            if b.kind == BossKind::Railgunner
                || (b.attack_style().0 == BossKind::Railgunner
                    && (b.attack_windup > 0.
                        || b.attack_timer > 0.
                        || b.rail.beam > 0.
                        || b.rail.vent > 0.
                        || b.rail.echo > 0.))
            {
                self.update_railgunner(i, &mut volleys, &mut warnings);
                continue;
            }
            if b.kind == BossKind::Sunderworm {
                self.update_worm(i, &mut volleys);
                continue;
            }
            let owned = self
                .projectiles
                .iter()
                .filter(|p| {
                    p.owner == b.id && p.kind.is_drone() && p.lifetime > 0. && p.penetration > 0.
                })
                .count();
            let (target, scene) = self.boss_target(i);
            let special_roll = self.random();
            let bounds = self.config.bounds();
            let b = &mut self.bosses[i];
            b.arrival = (b.arrival - DT).max(0.);
            let before = b.phase;
            b.phase = if b.attack_windup > 0.
                && matches!(b.kind, BossKind::Jester | BossKind::Carillon)
            {
                // The marked lanes must survive a health threshold mid-cast.
                before
            } else if b.health < b.max_health * BOSS_PHASES[1] {
                3
            } else if b.health < b.max_health * BOSS_PHASES[0] {
                2
            } else {
                1
            };
            // A boss pushed into a harder phase answers at once with
            // whatever it reaches for when hurt.
            if b.phase > before && before > 0 && b.arrival <= 0. && b.attack_windup <= 0. {
                b.attack_timer = b.attack_timer.min(LINING_UP * 0.5);
            }
            let rage = f32::from(b.phase.clamp(1, 3) - 1);
            if let Some((id, position, moving)) = target {
                let rate = READ_RATE * (1. + 0.22 * rage);
                b.read = if id == b.target {
                    b.read.lerp(moving, 1. - (-DT * rate).exp())
                } else {
                    moving
                };
                b.target = id;
                b.goal = position;
            } else if b.target != 0 && b.mind.hidden() < LOST_SECONDS {
                // Its target slipped behind cover: it goes round to where it
                // can see the last place it was, rather than straight at it.
                let seen = b.mind.seen_at();
                let ring = crate::within(b.pos.distance(seen), b.radius + 220., 700.);
                b.goal = flank_point(b.pos, seen, ring, b.radius, bounds, &self.walls);
            } else if b.pos.distance(b.goal) < 130.
                || !bounds.contains_circle(b.goal, b.radius + 60.)
            {
                b.target = 0;
                let angle = self.tick as f32 * 0.003 + b.id as f32 * 1.618;
                b.goal = bounds.clamp_circle(
                    bounds.center() + V2::from_angle(angle) * (bounds.w.min(bounds.h) * 0.3),
                    b.radius + 80.,
                );
            }
            if let Some((_, _, moving)) = target {
                b.mind.watch(moving, DT);
            }
            let to = b.goal - b.pos;
            let mut lined_up = true;
            let gun_style = b.attack_style().0;
            let traversing = gun_style == BossKind::SiegeEngine && b.attack_windup > 0.60;
            if b.tactic != BossTactic::Windup
                && b.tactic != BossTactic::Charge
                && (b.attack_windup <= 0. || traversing)
                && b.mind.salvo.is_none()
            {
                // Just before a special it aims for that shot instead, at
                // where the target should be once the warning is over.
                let (shot, warned) = if traversing {
                    (special_speed(gun_style), b.attack_windup * 0.65)
                } else if b.attack_timer < LINING_UP {
                    (special_speed(gun_style), gun_style.windup() * 0.45)
                } else {
                    (aimed_speed(gun_style), 0.)
                };
                let swerve = b.mind.swerve * (rage * 0.5);
                let point = target.map_or(b.goal, |(_, position, _)| {
                    bent_lead_point(
                        b.pos,
                        (position, b.read, swerve),
                        shot,
                        warned,
                        bounds,
                        &self.walls,
                    )
                });
                let difference = ((point - b.pos).angle() - b.aim + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                lined_up = difference.abs() < LINED_UP;
                let turn_rate = match gun_style {
                    BossKind::SiegeEngine => 2.25,
                    BossKind::Bastion => 1.7,
                    _ => 3.2,
                } * (1. + 0.12 * rage);
                b.aim += difference.clamp(-turn_rate * DT, turn_rate * DT);
            }
            b.tactic_time = (b.tactic_time - DT).max(0.);
            b.leg_time = (b.leg_time - DT).max(0.);
            let speed = if b.mothership {
                28.
            } else {
                (1. + 0.08 * rage)
                    * match b.kind {
                        BossKind::Tempest => 125.,
                        BossKind::Eclipse => 95.,
                        BossKind::FallenBooster => 180.,
                        BossKind::Guardian => 90.,
                        BossKind::Summoner => 45.,
                        BossKind::FallenOverlord => 65.,
                        BossKind::SiegeEngine => 25.,
                        BossKind::Skirmisher => 145.,
                        BossKind::Trapper => 101.,
                        BossKind::Jester => 151.,
                        BossKind::Carillon => 70.,
                        BossKind::Bastion => 38.,
                        BossKind::Foundry | BossKind::Defender => 32.,
                        _ => 55.,
                    }
            };
            // A wounded target draws it in for the kill.
            let finishing = scene.weak > 0.65 && !b.mothership;
            let preferred = match b.kind {
                BossKind::FallenBooster => 0.,
                BossKind::Guardian => 330.,
                BossKind::Summoner | BossKind::FallenOverlord | BossKind::Foundry => 540.,
                BossKind::Bastion | BossKind::Defender => 650.,
                BossKind::SiegeEngine => 720.,
                BossKind::Skirmisher => 420.,
                BossKind::Trapper => 415.,
                BossKind::Carillon => 570.,
                BossKind::Jester => 370.,
                _ => 450.,
            };
            let drone_limit = (gun_style.drone_limit() as f32 * swarm).ceil() as usize;
            let depleted = drone_limit > 0
                && owned < drone_limit / 3
                && !matches!(b.kind, BossKind::Jester | BossKind::Carillon);
            let range = to.length();
            let preferred = if finishing {
                preferred * 0.75
            } else {
                preferred
            };
            let too_close = preferred * if depleted || b.phase == 3 { 1.15 } else { 0.7 };
            let too_far = preferred + 130.;
            let mut desired = if target.is_none() {
                b.tactic = BossTactic::Patrol;
                // Straight to where it last saw someone, quicker than it wanders.
                to.normalized() * speed * if b.target != 0 { 0.8 } else { 0.45 }
            } else if b.tactic == BossTactic::Windup && b.tactic_time > 0. {
                V2::ZERO
            } else if b.tactic == BossTactic::Windup {
                b.tactic = BossTactic::Charge;
                b.tactic_time = 0.65;
                V2::from_angle(b.aim) * speed * 2.2
            } else if b.tactic == BossTactic::Charge && b.tactic_time > 0. {
                V2::from_angle(b.aim) * speed * 2.2
            } else {
                let tactic = if range < too_close
                    && (b.tactic == BossTactic::Retreat || range < too_close - TACTIC_MARGIN)
                {
                    BossTactic::Retreat
                } else if range > too_far
                    && (b.tactic == BossTactic::Advance || range > too_far + TACTIC_MARGIN)
                {
                    BossTactic::Advance
                } else {
                    BossTactic::Orbit
                };
                // It drives in straight lines, and only picks a new one when
                // this one is done or the fight has changed.
                if tactic != b.tactic || b.leg_time <= 0. || b.pos.distance(b.leg) < speed * 0.2 {
                    let hold = (LEG_LENGTH / speed).clamp(0.8, 3.);
                    let (foe, ring, hold) = match tactic {
                        BossTactic::Advance => (b.goal + b.read * 0.5, preferred, hold * 0.6),
                        BossTactic::Retreat => (b.goal, too_close.max(preferred) + 30., hold),
                        _ => (b.goal, range.max(b.radius + 60.), hold),
                    };
                    b.leg = next_leg(b, tactic, foe, ring, bounds, &self.walls, scene.flank);
                    b.leg_time = hold;
                }
                b.tactic = tactic;
                let run = b.leg - b.pos;
                run.normalized() * speed * (run.length() / (speed * 0.35)).min(1.)
            };
            // Read nearby incoming rounds, never inputs, and sidestep collisions.
            let mut evasion = V2::ZERO;
            for shot in self.projectiles.iter().filter(|p| {
                p.owner != b.id
                    && p.lifetime > 0.
                    && p.penetration > 0.
                    && (p.team == 0 || b.team == 0 || p.team != b.team)
                    && p.pos.distance(b.pos) < 450.
            }) {
                let relative = shot.vel - b.vel;
                let delta = b.pos - shot.pos;
                let time = delta.dot(relative) / relative.length_squared().max(1.);
                if (0.05..0.65).contains(&time)
                    && (delta - relative * time).length() < b.radius + shot.radius + 18.
                {
                    let side = V2::new(-relative.y, relative.x).normalized();
                    evasion += side * if delta.dot(side) < 0. { -1. } else { 1. };
                }
            }
            // Rounds come and go tick to tick, so the sidestep eases toward
            // what they call for rather than snapping between directions.
            b.dodge = b
                .dodge
                .lerp(evasion.clamp_length(1.), 1. - (-DT * DODGE_RATE).exp());
            if b.tactic != BossTactic::Charge
                && b.tactic != BossTactic::Windup
                && b.attack_windup <= 0.
                && b.mind.salvo.is_none()
            {
                desired += b.dodge * speed * 0.7;
            }
            desired = boss_clear_steering(b.pos, desired, b.vel, b.radius, bounds, &self.walls);
            if b.arrival > 0. || b.attack_windup > 0. || b.mind.salvo.is_some() {
                desired = V2::ZERO;
            }
            // A steady push rather than an ease, so a change of course reads
            // as one clean move from one heading to the next.
            let push = if b.tactic == BossTactic::Charge {
                7.
            } else {
                3.6
            };
            b.vel = b.vel.toward(desired, speed * push * DT);
            (b.pos, b.vel) = boss_glide(b.pos, b.vel * DT, b.vel, b.radius, bounds, &self.walls);
            b.timer -= DT;
            let primary_due = b.timer <= 0. && b.arrival <= 0.;
            let previous_windup = b.attack_windup;
            b.attack_windup = (b.attack_windup - DT).max(0.);
            // A warned attack always comes, even at a target gone behind cover.
            let secondary = previous_windup > 0. && b.attack_windup <= 0.;
            if b.arrival <= 0.
                && target.is_some()
                && previous_windup <= 0.
                && b.mind.salvo.is_none()
            {
                b.attack_timer -= DT;
                // A ready special waits a moment for the gun to come round,
                // so its warning points at something.
                if b.attack_timer <= 0. && (lined_up || b.attack_timer < -BOSS_AIM_HOLD) {
                    b.attack = b.mind.pick_special(b.kind, &scene.special, special_roll);
                    b.attack_windup = b.warning_seconds();
                    b.attack_timer = match b.kind {
                        BossKind::Jester => 6.4 - f32::from(b.phase) * 0.6,
                        BossKind::Carillon => 7.2 - f32::from(b.phase) * 0.8,
                        _ => 9.2 - f32::from(b.phase) * 1.7,
                    };
                    warnings.push((b.pos, b.id, b.attack as u32, b.team));
                }
            }
            // Let the warning and secondary pattern read clearly. A primary
            // volley cannot land on the same frame as the special attack.
            let (style, pattern) = b.attack_style();
            let start_spiral = secondary && style == BossKind::Trapper && pattern == 0;
            if start_spiral {
                b.mind.trap_spiral = Some(TrapSpiral {
                    aim: b.aim,
                    phase: b.phase,
                    tick: 0,
                });
                b.timer = b
                    .timer
                    .max(f32::from(TrapSpiral::steps(b.phase) * 5 + 10) * DT + 0.4);
            }
            if secondary
                && let Some(battery) = crate::encounters::Battery::for_attack(style, pattern)
            {
                b.mind.salvo = Some(crate::encounters::Salvo::new(battery, b.aim));
            }
            let staged = b.mind.salvo.is_some();
            let primary = primary_due
                && b.attack_windup <= 0.
                && !secondary
                && b.mind.trap_spiral.is_none()
                && !staged;
            let spiral_pair = TrapSpiral::advance(&mut b.mind.trap_spiral);
            let beat = crate::encounters::Salvo::advance(&mut b.mind.salvo);
            if secondary || beat.is_some() {
                b.timer = b.timer.max(0.4);
            }
            if !secondary
                && beat.is_none()
                && spiral_pair.is_none()
                && (target.is_none() || !primary)
            {
                continue;
            }
            if secondary
                && b.kind == BossKind::Jester
                && let Some(attack) = crate::encounters::axiom_act(b.attack)
            {
                axiom_casts.push((i, attack));
                continue;
            }
            let mut spawned_drones = owned;
            let mut emit = |angle: f32,
                            kind: ProjectileKind,
                            speed: f32,
                            damage: f32,
                            radius: f32,
                            life: f32,
                            pen: f32,
                            bounces: u8| {
                if kind.is_drone() {
                    if spawned_drones >= drone_limit {
                        return;
                    }
                    spawned_drones += 1;
                }
                let origin = b.pos + V2::from_angle(angle) * (b.radius + radius + 3.);
                if !bounds.contains_circle(origin, radius)
                    || self
                        .walls
                        .iter()
                        .any(|&w| sweep_rect(b.pos, origin, radius, w).is_some())
                {
                    return;
                }
                volleys.push((
                    b.id,
                    b.team,
                    origin,
                    V2::from_angle(angle) * speed,
                    kind,
                    damage,
                    radius,
                    life,
                    pen,
                    bounces,
                ));
            };
            let mut emit_round = |r: BossRound| {
                emit(
                    r.angle,
                    r.kind,
                    r.speed,
                    r.damage,
                    r.radius,
                    r.life,
                    r.penetration,
                    r.bounces,
                );
            };
            if let Some(pair) = spiral_pair {
                pair.into_iter().for_each(&mut emit_round);
            }
            if let Some(beat) = beat {
                beat.fire(&mut emit_round);
            }
            if secondary && !staged {
                special_volley(b.kind, b.attack, b.aim, b.phase)
                    .into_iter()
                    .for_each(emit_round);
                // The quick ones dash in behind their volley.
                match style {
                    BossKind::Skirmisher => {
                        b.tactic = BossTactic::Windup;
                        b.tactic_time = 0.55;
                    }
                    BossKind::FallenBooster | BossKind::Tempest
                        if b.tactic != BossTactic::Charge =>
                    {
                        b.tactic = BossTactic::Windup;
                        b.tactic_time = 0.7;
                    }
                    _ => {}
                }
            }
            if !primary || target.is_none() {
                continue;
            }
            match style {
                // The rail mask fires its own coil, flak, rail and beam in the
                // Railgunner controller, including its vent and parry windows.
                BossKind::Railgunner | BossKind::Sunderworm => {}
                BossKind::Jester => {
                    if crate::encounters::axiom_act(b.attack).is_some() {
                        // Axiom's four-step rotation fires from separate muzzles.
                        let step = crate::creator_boss::gun_step(b.mind.gun);
                        b.mind.gun = b.mind.gun.wrapping_add(1);
                        let pos = crate::creator_boss::muzzle(b.pos, b.aim, step, b.radius);
                        let direction = (b.goal - pos).normalized();
                        let shot = crate::creator_boss::primary_round(direction.angle());
                        if bounds.contains_circle(pos, shot.radius)
                            && self
                                .walls
                                .iter()
                                .all(|&w| sweep_rect(b.pos, pos, shot.radius, w).is_none())
                        {
                            volleys.push((
                                b.id,
                                b.team,
                                pos,
                                direction * shot.speed,
                                shot.kind,
                                shot.damage,
                                shot.radius,
                                shot.life,
                                shot.penetration,
                                0,
                            ));
                        }
                        b.timer = crate::creator_boss::GUN_INTERVAL;
                    } else {
                        for side in [-1., 0., 1.] {
                            emit(
                                b.aim + side * 0.16,
                                ProjectileKind::Piercing,
                                640.,
                                32.,
                                9.,
                                2.3,
                                2.5,
                                1,
                            );
                        }
                        b.timer = 1.05;
                    }
                }
                BossKind::Carillon => {
                    // Pressure shot through the centre, with a pair of castings
                    // from the outboard molds. Capped escorts release in pairs.
                    emit(b.aim, ProjectileKind::Shell, 880., 110., 17., 2., 5., 0);
                    for side in [-1., 1.] {
                        emit(
                            b.aim + side * 0.22,
                            ProjectileKind::Piercing,
                            720.,
                            46.,
                            10.,
                            2.,
                            3.,
                            1,
                        );
                        if owned < drone_limit {
                            emit(
                                b.aim + side * 1.16,
                                ProjectileKind::Drone,
                                495.,
                                46.,
                                16.,
                                8.,
                                7.,
                                0,
                            );
                        }
                    }
                    b.timer = 1.22;
                }
                BossKind::SiegeEngine => {
                    emit(b.aim, ProjectileKind::Shell, 390., 34., 15., 3., 2.5, 0);
                    b.timer = 1.65;
                }
                BossKind::Skirmisher => {
                    for n in [-1., 1.] {
                        emit(
                            b.aim + n * 0.055,
                            ProjectileKind::Bullet,
                            450.,
                            14.,
                            7.,
                            2.2,
                            1.3,
                            0,
                        );
                    }
                    b.timer = 0.68;
                }
                BossKind::Trapper => {
                    for n in [-1., 1.] {
                        emit(
                            b.aim + n * 0.19,
                            ProjectileKind::Trap,
                            850.,
                            26.,
                            11.,
                            4.,
                            2.5,
                            0,
                        );
                    }
                    b.timer = 1.1;
                }
                BossKind::Guardian => {
                    for n in 0..4 {
                        emit(
                            b.aim + std::f32::consts::PI + (n as f32 - 1.5) * 0.18,
                            ProjectileKind::Drone,
                            aimed_speed(b.kind),
                            22.,
                            15.,
                            10.,
                            5.,
                            0,
                        );
                    }
                    if b.phase > 1 {
                        // The front point fires a shard while the rear bay
                        // replenishes, so staying ahead of the swarm costs room.
                        emit(b.aim, ProjectileKind::Piercing, 720., 36., 11., 1.65, 3., 0);
                    }
                    b.timer = 1.1;
                }
                BossKind::Summoner => {
                    for n in 0..4 {
                        emit(
                            b.aim + n as f32 * std::f32::consts::FRAC_PI_2,
                            ProjectileKind::Drone,
                            aimed_speed(b.kind),
                            24.,
                            19.,
                            11.,
                            4.5,
                            0,
                        );
                    }
                    if b.phase > 1 {
                        for side in [-1., 1.] {
                            emit(
                                b.aim + side * 0.10,
                                ProjectileKind::Bullet,
                                675.,
                                29.,
                                10.,
                                1.8,
                                2.,
                                0,
                            );
                        }
                    }
                    b.timer = 1.95;
                }
                BossKind::FallenOverlord => {
                    for n in 0..4 {
                        emit(
                            b.aim + n as f32 * std::f32::consts::FRAC_PI_2,
                            ProjectileKind::Drone,
                            aimed_speed(b.kind),
                            27.,
                            17.,
                            10.,
                            5.,
                            0,
                        );
                    }
                    if b.phase > 1 {
                        // Seekers from the bow, so a tank that outruns the
                        // swarm is still being hunted.
                        for side in [-1., 1.] {
                            emit(
                                b.aim + side * 0.16,
                                ProjectileKind::Homing,
                                430.,
                                25.,
                                9.,
                                2.6,
                                1.6,
                                0,
                            );
                        }
                    }
                    b.timer = 1.95;
                }
                BossKind::Defender => {
                    for n in 0..3 {
                        // Out of the launchers on its turning sides.
                        let angle = style.turn(self.tick)
                            + std::f32::consts::PI
                            + n as f32 * std::f32::consts::TAU / 3.;
                        emit(angle, ProjectileKind::Trap, 180., 26., 18., 7., 3., 0);
                        emit(
                            b.aim + (n as f32 - 1.) * 0.1,
                            ProjectileKind::Bullet,
                            410.,
                            17.,
                            8.,
                            2.5,
                            1.5,
                            0,
                        );
                    }
                    b.timer = 0.85;
                }
                BossKind::FallenBooster => {
                    emit(b.aim, ProjectileKind::Bullet, 460., 19., 10., 2., 1.8, 0);
                    for angle in [-0.45_f32, -0.2, 0.2, 0.45] {
                        emit(
                            b.aim + std::f32::consts::PI + angle,
                            ProjectileKind::Bullet,
                            330.,
                            12.,
                            7.,
                            1.6,
                            1.,
                            0,
                        );
                    }
                    b.timer = 0.65;
                }
                BossKind::Sentinel => {
                    for n in 0..(3 + b.phase as usize * 2) {
                        let count = 3 + b.phase as usize * 2;
                        emit(
                            b.aim + (n as f32 - (count - 1) as f32 / 2.) * 0.16,
                            ProjectileKind::Shell,
                            330.,
                            24.,
                            12.,
                            3.5,
                            2.,
                            0,
                        );
                    }
                    b.timer = 1.15;
                }
                BossKind::Prism => {
                    for n in 0..(6 + b.phase as usize * 2) {
                        emit(
                            style.turn(self.tick)
                                + n as f32 * std::f32::consts::TAU
                                    / (6 + b.phase as usize * 2) as f32,
                            ProjectileKind::Piercing,
                            430.,
                            17.,
                            7.,
                            4.2,
                            2.,
                            1,
                        );
                    }
                    b.timer = 1.45;
                }
                BossKind::Foundry => {
                    for n in 0..(2 + b.phase) {
                        emit(
                            b.aim + (n as f32 - 1.) * 0.7,
                            ProjectileKind::Drone,
                            aimed_speed(b.kind),
                            19.,
                            12.,
                            7.,
                            4.,
                            0,
                        );
                    }
                    if b.phase > 1 {
                        emit(b.aim, ProjectileKind::Turret, 80., 19., 17., 6., 4., 0);
                        emit(b.aim, ProjectileKind::Shell, 650., 48., 13., 1.8, 3.2, 0);
                    }
                    b.timer = 2.2;
                }
                BossKind::Tempest => {
                    let count = 3 + b.phase;
                    for n in 0..count {
                        emit(
                            b.aim + (n as f32 - (f32::from(count) - 1.) * 0.5) * 0.45,
                            ProjectileKind::Homing,
                            285.,
                            14.,
                            7.,
                            4.,
                            1.,
                            0,
                        );
                    }
                    if b.tactic != BossTactic::Windup && b.tactic != BossTactic::Charge {
                        b.tactic = BossTactic::Windup;
                        b.tactic_time = 0.7;
                    }
                    b.timer = 1.05;
                }
                BossKind::Bastion => {
                    for n in 0..8 {
                        emit(
                            style.turn(self.tick) + n as f32 * std::f32::consts::TAU / 8.,
                            ProjectileKind::Mine,
                            220.,
                            27.,
                            13.,
                            6.5,
                            2.,
                            0,
                        );
                    }
                    emit(b.aim, ProjectileKind::Explosive, 340., 46., 15., 3., 1., 0);
                    b.timer = 1.85;
                }
                BossKind::Eclipse => {
                    for n in 0..(10 + b.phase as usize * 4) {
                        emit(
                            b.aim
                                + n as f32 * std::f32::consts::TAU
                                    / (10 + b.phase as usize * 4) as f32,
                            ProjectileKind::Fragment,
                            370.,
                            20.,
                            9.,
                            2.5,
                            1.,
                            0,
                        );
                    }
                    b.timer = 2.2;
                    if b.tactic != BossTactic::Windup && b.tactic != BossTactic::Charge {
                        b.tactic = BossTactic::Windup;
                        b.tactic_time = 0.9;
                    }
                }
            }
            // It fires faster the more it is hurt. A smaller swarm is also
            // launched more slowly, so it isn't simply refilled at once.
            b.timer *= match b.phase {
                1 => 1.,
                2 => 0.86,
                _ => 0.72,
            } / if style.drone_limit() > 0 { swarm } else { 1. };
        }
        for (index, attack) in axiom_casts {
            // Cover or a full arena can leave a cast with nowhere to land.
            let _ = self.cast_axiom(crate::creator_boss::AxiomCaster::Boss(index), attack);
        }
        for (pos, id, attack, team) in warnings {
            self.event(EventKind::BossAttack, pos, id, attack, team);
        }
        let room = self
            .config
            .max_projectiles
            .saturating_sub(self.projectiles.len());
        for (owner, team, pos, vel, kind, damage, radius, life, pen, bounces) in
            volleys.into_iter().take(room)
        {
            let mut p =
                self.simple_projectile(owner, team, pos, vel, kind, damage, radius, life, pen);
            p.ricochets = bounces;
            self.projectiles.push(p);
            // A plasma stream is one long shot, not thirty a second.
            if !crate::railgunner::is_beam_round(kind, vel) {
                self.event(EventKind::Shot, pos, owner, kind as u32, team);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boss_brain::intercept_time;
    use crate::world::BOSS_CREDIT_TICKS;
    use tankyard_content::Content;

    #[test]
    fn a_boss_glides_along_a_wall_and_stops_only_when_boxed_in() {
        let bounds = Rect {
            x: 0.,
            y: 0.,
            w: 4000.,
            h: 4000.,
        };
        let wall = Rect {
            x: 1000.,
            y: 0.,
            w: 100.,
            h: 4000.,
        };
        let radius = 80.;
        let pos = V2::new(1000. - radius - 0.5, 2000.);
        let vel = V2::new(60., 40.);
        let (at, after) = boss_glide(pos, vel * DT, vel, radius, bounds, &[wall]);
        assert_eq!(at.x, pos.x, "held off the wall");
        assert!(at.y > pos.y, "keeps moving along it");
        assert_eq!(after.y, vel.y, "at full speed along it");
        assert!(after.x.abs() < vel.x, "and no longer pushing into it");

        // Into a corner of two walls it has nowhere to go and slows.
        let floor = Rect {
            x: 0.,
            y: pos.y + radius + 0.5,
            w: 4000.,
            h: 100.,
        };
        let (at, after) = boss_glide(pos, vel * DT, vel, radius, bounds, &[wall, floor]);
        assert_eq!(at, pos);
        assert!(after.length() < vel.length());

        // At the arena's edge it stops pushing outward.
        let edge = V2::new(radius + 0.2, 2000.);
        let out = V2::new(-90., 30.);
        let (at, after) = boss_glide(edge, out * DT, out, radius, bounds, &[]);
        assert_eq!(at.x, radius);
        assert_eq!(after.x, 0.);
        assert_eq!(after.y, 30.);
    }

    #[test]
    fn steering_commits_to_a_side_of_an_obstacle() {
        let bounds = Rect {
            x: 0.,
            y: 0.,
            w: 4000.,
            h: 4000.,
        };
        // A wall dead ahead, centered, so both ways around score the same.
        let wall = Rect {
            x: 2100.,
            y: 1900.,
            w: 60.,
            h: 200.,
        };
        let (pos, desired) = (V2::new(2000., 2000.), V2::new(120., 0.));
        let up = boss_clear_steering(pos, desired, V2::new(60., -60.), 40., bounds, &[wall]);
        let down = boss_clear_steering(pos, desired, V2::new(60., 60.), 40., bounds, &[wall]);
        assert!(up.y < 0., "already heading up, it goes up: {up:?}");
        assert!(down.y > 0., "already heading down, it goes down: {down:?}");
    }

    #[test]
    fn a_boss_on_the_edge_of_its_range_does_not_flip_tactics() {
        let mut world = arena();
        world.tanks[0].max_health = 1e9;
        world.tanks[0].health = 1e9;
        assert_ne!(world.spawn_boss(BossKind::Guardian), 0);
        let boss = world.bosses[0].id;
        world.bosses[0].arrival = 0.;
        world.bosses[0].attack_timer = 1e6;
        let mut last = None;
        let mut flips = 0;
        for _ in 0..1200 {
            // Hold the pilot on the line between retreating and orbiting.
            let b = world.bosses.iter().find(|b| b.id == boss).unwrap();
            let (at, edge) = (b.pos, 330. * 0.7);
            world.tanks[0].pos = at + V2::new(edge + 1., 0.);
            world.tanks[0].vel = V2::ZERO;
            world.tanks[0].health = 1e9;
            world.projectiles.clear();
            world.update_bosses();
            world.tick += 1;
            let tactic = world.bosses.iter().find(|b| b.id == boss).unwrap().tactic;
            if last.is_some_and(|before| before != tactic) {
                flips += 1;
            }
            last = Some(tactic);
        }
        assert!(flips <= 4, "{flips} changes of tactic in twenty seconds");
    }

    #[test]
    fn a_chasing_boss_stops_at_its_range_and_starts_again_past_the_margin() {
        let mut world = arena();
        world.tanks[0].max_health = 1e9;
        assert_ne!(world.spawn_boss(BossKind::FallenBooster), 0);
        world.bosses[0].arrival = 0.;
        world.bosses[0].attack_timer = 1e6;
        let mut tactic_at = |range: f32| {
            let at = world.bosses[0].pos;
            world.tanks[0].pos = at + V2::new(range, 0.);
            world.tanks[0].vel = V2::ZERO;
            world.tanks[0].health = 1e9;
            world.projectiles.clear();
            world.update_bosses();
            world.bosses[0].tactic
        };
        assert_eq!(tactic_at(400.), BossTactic::Advance);
        // Still closing while it's outside its range.
        assert_eq!(tactic_at(140.), BossTactic::Advance);
        // The edge stops it, exactly as near as it would have come.
        assert_eq!(tactic_at(125.), BossTactic::Orbit);
        // Just past the edge isn't enough to set it off again.
        assert_eq!(tactic_at(150.), BossTactic::Orbit);
        assert_eq!(tactic_at(175.), BossTactic::Advance);
    }

    fn arena() -> World {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                dynamic_arena: false,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Pilot", false);
        world.tanks[0].level = 45;
        world
    }

    #[test]
    fn the_trap_spiral_sweeps_opposite_pairs_outward_and_then_ends() {
        for phase in 0..=4 {
            let steps = TrapSpiral::steps(phase);
            let mut spiral = Some(TrapSpiral {
                aim: 0.4,
                phase,
                tick: 0,
            });
            let mut pairs = Vec::new();
            for tick in 1..=200 {
                let was_active = spiral.is_some();
                if let Some(pair) = TrapSpiral::advance(&mut spiral) {
                    pairs.push((tick, pair));
                }
                if !was_active {
                    assert!(spiral.is_none());
                }
            }
            assert!(spiral.is_none(), "phase {phase} never ended");
            assert_eq!(pairs.len(), usize::from(steps), "phase {phase}");
            let mut last: Option<(f32, f32)> = None;
            for (n, (tick, [a, b])) in pairs.into_iter().enumerate() {
                // A short pause after the mines, then a pair every 5 ticks.
                assert_eq!(tick, 10 + 5 * n);
                let opposite = (b.angle - a.angle - std::f32::consts::PI).abs();
                assert!(opposite < 1e-4, "pair {n} is not opposite");
                assert_eq!(
                    (a.speed, a.kind, b.speed),
                    (b.speed, ProjectileKind::Trap, a.speed)
                );
                if let Some((angle, speed)) = last {
                    assert!(
                        a.angle > angle && a.speed > speed,
                        "pair {n} did not sweep out"
                    );
                }
                last = Some((a.angle, a.speed));
            }
            // Under one and a quarter turns, so the arms never overlap.
            let (angle, speed) = last.unwrap();
            assert!((angle - (0.4 + std::f32::consts::FRAC_PI_2 + 3.8)).abs() < 1e-4);
            assert!((speed - 1340.).abs() < 1e-3);
        }
    }

    #[test]
    fn a_trapper_releases_its_spiral_and_holds_fire_until_it_ends() {
        let mut world = arena();
        world.tanks[0].pos = V2::new(2450., 1300.);
        world.spawn_boss(BossKind::Trapper);
        let boss = &mut world.bosses[0];
        boss.pos = V2::new(1600., 1300.);
        boss.arrival = 0.;
        boss.timer = 0.;
        boss.attack = 0;
        boss.attack_windup = DT * 0.5;
        world.update_bosses();
        let side_mines = world.projectiles.len();
        assert!(world.bosses[0].mind.trap_spiral.is_some());
        let mut traps = 0;
        for _ in 0..120 {
            let before: Vec<_> = world.projectiles.iter().map(|p| p.id).collect();
            world.update_bosses();
            traps += world
                .projectiles
                .iter()
                .filter(|p| !before.contains(&p.id))
                .inspect(|p| {
                    assert_eq!(
                        p.kind,
                        ProjectileKind::Trap,
                        "a primary volley broke into the spiral"
                    )
                })
                .count();
            if world.bosses[0].mind.trap_spiral.is_none() {
                break;
            }
        }
        assert!(side_mines > 0);
        assert_eq!(
            traps,
            2 * usize::from(TrapSpiral::steps(world.bosses[0].phase))
        );
        assert!(
            world.bosses[0].timer > 0.,
            "no recovery gap after the spiral"
        );
    }

    #[test]
    fn a_cornered_trapper_slides_out_along_the_walls() {
        let mut world = arena();
        world.walls.clear();
        let bounds = world.config.bounds();
        world.spawn_boss(BossKind::Trapper);
        let radius = world.bosses[0].radius;
        for corner in [
            V2::new(bounds.x, bounds.y),
            V2::new(bounds.x + bounds.w, bounds.y + bounds.h),
        ] {
            let inward = (bounds.center() - corner).normalized();
            let start = bounds.clamp_circle(corner, radius + 12.);
            let boss = &mut world.bosses[0];
            (boss.pos, boss.vel, boss.leg_time) = (start, V2::ZERO, 0.);
            boss.arrival = 0.;
            // A tank parked on the diagonal blocks the only straight way out.
            world.tanks[0].pos = start + inward * 230.;
            world.tanks[0].vel = V2::ZERO;
            let mut farthest = 0_f32;
            for _ in 0..(5. / DT) as usize {
                world.update_bosses();
                let pos = world.bosses[0].pos;
                assert!(bounds.contains_circle(pos, radius - 1.));
                farthest = farthest.max(pos.distance(start));
            }
            assert!(farthest > 260., "the Trapper stayed pinned: {farthest:.0}");
        }
    }

    #[test]
    fn summoner_drones_have_shorter_acquisition_and_return_to_their_owner() {
        let mut world = arena();
        assert_ne!(world.spawn_boss(BossKind::Summoner), 0);
        world.bosses[0].pos = V2::new(1600., 1300.);
        let boss = &world.bosses[0];
        world.tanks[0].pos = boss.pos + V2::new(810., 0.);
        assert_eq!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.tanks[0].pos = boss.pos + V2::new(850., 0.);
        assert!(
            world
                .boss_drone_goal(boss, 100, boss.pos, 0)
                .distance(boss.pos)
                < 200.
        );
        // A drone near the player cannot extend its boss's pursuit across the map.
        world.tanks[0].pos = boss.pos + V2::new(1100., 0.);
        assert!(
            world
                .boss_drone_goal(boss, 100, boss.pos + V2::new(900., 0.), 0)
                .distance(boss.pos)
                < 200.
        );
        world.tanks[0].pos = boss.pos + V2::new(700., 0.);
        assert!(
            world
                .boss_drone_goal(boss, 100, boss.pos + V2::new(1100., 0.), 0)
                .distance(boss.pos)
                < 200.
        );
    }

    #[test]
    fn boss_drones_observe_cover_protection_and_new_player_grace() {
        let mut world = arena();
        world.spawn_boss(BossKind::Summoner);
        world.bosses[0].pos = V2::new(1600., 1300.);
        world.tanks[0].pos = V2::new(2200., 1300.);
        world.tanks[0].level = 1;
        let boss = &world.bosses[0];
        assert_ne!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.tanks[0].level = 45;
        world.tanks[0].protection = 5.;
        assert_ne!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.tanks[0].protection = 0.;
        world.walls.push(Rect {
            x: 1900.,
            y: 1000.,
            w: 50.,
            h: 600.,
        });
        assert_ne!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.walls.clear();
        assert_eq!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
    }

    #[test]
    fn secondary_warnings_pause_primary_fire_and_leave_a_recovery_gap() {
        for kind in BossKind::ALL {
            let mut world = arena();
            world.tanks[0].pos = V2::new(2450., 1300.);
            world.spawn_boss(kind);
            world.bosses[0].pos = V2::new(1600., 1300.);
            world.bosses[0].arrival = 0.;
            world.bosses[0].timer = 0.;
            world.bosses[0].attack_timer = 0.;
            world.update_bosses();
            assert!(
                world.projectiles.is_empty(),
                "{kind:?} hid its windup behind a primary volley"
            );
            for _ in 0..(world.bosses[0].warning_seconds() / DT) as usize + 2 {
                world.update_bosses();
            }
            assert!(
                !world.projectiles.is_empty(),
                "{kind:?} did not release its secondary"
            );
            // The Trapper's spiral and the three-gun batteries finish their
            // sequence before the recovery window starts.
            for _ in 0..120 {
                if world.bosses[0].mind.trap_spiral.is_none()
                    && world.bosses[0].mind.salvo.is_none()
                {
                    break;
                }
                world.update_bosses();
            }
            assert!(
                world.bosses[0].mind.trap_spiral.is_none() && world.bosses[0].mind.salvo.is_none(),
                "{kind:?} never finished its special"
            );
            let shots = world.projectiles.len();
            for _ in 0..20 {
                world.update_bosses();
            }
            assert_eq!(
                world.projectiles.len(),
                shots,
                "{kind:?} overlapped its recovery gap"
            );
        }
    }

    #[test]
    fn siege_traverses_early_then_holds_its_mark_through_release() {
        let mut world = arena();
        world.spawn_boss(BossKind::SiegeEngine);
        world.bosses[0].pos = V2::new(1600., 1300.);
        world.bosses[0].aim = 0.;
        world.bosses[0].arrival = 0.;
        world.bosses[0].attack_windup = 1.4;
        world.tanks[0].pos = V2::new(1900., 1900.);
        for _ in 0..49 {
            world.update_bosses();
        }
        let locked = world.bosses[0].aim;
        assert!(locked > 0.3);
        world.tanks[0].pos = V2::new(2100., 1000.);
        while world.bosses[0].attack_windup > 0. {
            world.update_bosses();
            assert_eq!(world.bosses[0].aim, locked);
        }
        assert!(
            world
                .projectiles
                .iter()
                .any(|p| p.radius == SIEGE_ROUND_RADIUS && (p.vel.angle() - locked).abs() < 1e-4)
        );
    }

    #[test]
    fn a_jester_plays_every_borrowed_special_in_the_simulation() {
        use crate::creator_boss::Attack;
        for (code, (source, slot)) in
            (0u8..).map_while(|code| crate::encounters::jester_act(code).map(|a| (code, a)))
        {
            let mut world = arena();
            world.spawn_boss(BossKind::Jester);
            let pos = V2::new(1600., 1300.);
            world.tanks[0].pos = V2::new(2450., 1300.);
            let b = &mut world.bosses[0];
            b.pos = pos;
            b.health = b.max_health * 0.5;
            b.aim = 0.;
            b.arrival = 0.;
            b.attack = code;
            b.attack_windup = DT;
            b.attack_timer = 7.;
            world.update_bosses();
            let b = &world.bosses[0];
            match crate::encounters::axiom_act(code) {
                Some(Attack::RiftDash) => assert!(b.pos.distance(pos) > 500.),
                Some(Attack::Reforge) => assert!(b.health / b.max_health > 0.57),
                _ if (source, slot) == (BossKind::Railgunner, 2) => {
                    assert!(b.beaming());
                    assert!(
                        world
                            .projectiles
                            .iter()
                            .any(|p| crate::is_beam_round(p.kind, p.vel))
                    );
                }
                _ => assert!(
                    !world.projectiles.is_empty(),
                    "act {code}: {source:?}/{slot}"
                ),
            }
        }
    }

    #[test]
    fn every_boss_has_more_health_and_a_telegraphed_secondary_attack() {
        for kind in BossKind::ALL {
            let mut world = arena();
            world.tanks[0].pos = V2::new(2450., 1300.);
            world.spawn_boss(kind);
            let boss = &mut world.bosses[0];
            assert!(boss.max_health >= 3000.);
            boss.pos = V2::new(1600., 1300.);
            boss.arrival = 0.;
            boss.timer = 1000.;
            boss.attack_timer = 0.;
            world.update_bosses();
            assert!(world.bosses[0].attack_windup >= kind.windup() - DT * 2.);
            assert!(
                world.projectiles.is_empty(),
                "{kind:?} attacked before its warning"
            );
            assert!(
                world
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::BossAttack))
            );
            for _ in 0..(kind.windup() / DT) as usize + 5 {
                world.update_bosses();
            }
            assert!(
                !world.projectiles.is_empty(),
                "{kind:?} did not release its attack"
            );
            assert!(world.projectiles.len() <= 24);
            assert!(
                world
                    .projectiles
                    .iter()
                    .all(|p| p.vel.finite() && p.owner == world.bosses[0].id)
            );
        }
    }

    fn siege_round(world: &mut World, target: V2, cover: Option<Rect>) -> EntityId {
        world.spawn_boss(BossKind::SiegeEngine);
        let boss = &mut world.bosses[0];
        boss.pos = V2::new(900., 1300.);
        boss.aim = (target - boss.pos).angle();
        boss.arrival = 0.;
        boss.timer = 1000.;
        // Past the longest it waits, so it locks onto this exact line.
        boss.attack_timer = -BOSS_AIM_HOLD;
        let id = boss.id;
        world.update_bosses();
        assert_eq!(
            world.bosses[0].attack_windup,
            BossKind::SiegeEngine.windup()
        );
        world.walls.extend(cover);
        for _ in 0..(BossKind::SiegeEngine.windup() / DT) as usize + 2 {
            world.update_bosses();
            world.bosses[0].pos = V2::new(900., 1300.);
        }
        assert!(
            world.projectiles.iter().any(|p| p.damage >= LETHAL_DAMAGE),
            "the cannon fired"
        );
        id
    }

    #[test]
    fn the_siege_cannon_destroys_any_tank_on_its_line_after_a_long_warning() {
        let mut world = arena();
        world.tanks[0].pos = V2::new(1900., 1300.);
        world.tanks[0].max_health = 5000.;
        world.tanks[0].health = 5000.;
        world.tanks[0].shield = 100.;
        let target = world.tanks[0].pos;
        let id = siege_round(&mut world, target, None);
        let round = world
            .projectiles
            .iter()
            .find(|p| p.owner == id && p.damage >= LETHAL_DAMAGE)
            .unwrap();
        assert!(round.radius >= 20. && round.vel.length() >= 1400.);
        world.tanks[0].shield = 100.;
        for _ in 0..60 {
            world.step();
            if world.tanks[0].deaths > 0 {
                return;
            }
            world.tanks[0].shield = 100.;
        }
        panic!("the round should destroy a shielded tank in one hit");
    }

    #[test]
    fn cover_stops_the_siege_cannon_and_bosses_survive_it() {
        let mut world = arena();
        world.tanks[0].pos = V2::new(2000., 1300.);
        let wall = Rect {
            x: 1400.,
            y: 1100.,
            w: 60.,
            h: 400.,
        };
        let target = world.tanks[0].pos;
        siege_round(&mut world, target, Some(wall));
        for _ in 0..90 {
            world.step();
        }
        assert_eq!(world.tanks[0].deaths, 0, "the wall took it");

        let mut world = arena();
        world.tanks[0].pos = V2::new(300., 300.);
        let target = V2::new(2000., 1300.);
        let id = siege_round(&mut world, target, None);
        let other = world.spawn_boss(BossKind::Sentinel);
        let boss = world.bosses.iter_mut().find(|b| b.id == other).unwrap();
        boss.pos = V2::new(1600., 1300.);
        boss.arrival = 5.;
        let health = boss.health;
        for _ in 0..40 {
            world.step();
        }
        let boss = world.bosses.iter().find(|b| b.id == other).unwrap();
        assert!(boss.health >= health - LETHAL_CAP - 1., "capped on bosses");
        assert!(world.bosses.iter().any(|b| b.id == id));
    }

    #[test]
    fn the_siege_round_respects_spawn_protection_and_leaves_assists_their_credit() {
        let mut world = arena();
        world.tanks[0].pos = V2::new(1900., 1300.);
        let target = world.tanks[0].pos;
        siege_round(&mut world, target, None);
        for _ in 0..50 {
            world.tanks[0].protection = 5.;
            world.step();
        }
        assert_eq!(world.tanks[0].deaths, 0, "protection holds");

        let mut world = arena();
        let friend = world.add_player("Friend", false);
        let victim = world.tanks[0].id;
        world.record_damage(victim, friend, 50.);
        world.damage_tank(0, LETHAL_DAMAGE, 777, V2::ZERO);
        let list = &world.contributions[&victim];
        let boss = list.iter().find(|entry| entry.0 == 777).unwrap().1;
        assert!(boss <= LETHAL_CAP, "{boss}");
        assert!(world.events.iter().all(|e| e.value <= LETHAL_CAP as u32));
    }

    #[test]
    fn a_parried_siege_round_hurts_its_owner_and_never_one_shots_it() {
        let mut world = arena();
        world.config.mode = Mode::Sandbox;
        assert!(world.sandbox_class(world.tanks[0].id, "riposte"));
        world.tanks[0].pos = V2::new(1200., 1300.);
        let parry = world
            .content
            .class("riposte")
            .unwrap()
            .ability
            .parry_window();
        world.tanks[0].shield = parry.unwrap();
        let target = world.tanks[0].pos;
        let id = siege_round(&mut world, target, None);
        let index = world
            .projectiles
            .iter()
            .position(|p| p.owner == id && p.damage >= LETHAL_DAMAGE)
            .unwrap();
        let impact = world.tanks[0].pos;
        let full = world.bosses[0].max_health;
        world.bosses[0].health = full;
        assert!(world.parry_projectile(index, 0, impact));
        assert!(
            world.projectiles[index].lifetime <= 0.,
            "the round is spent"
        );
        for _ in 0..60 {
            world.land_clashes();
        }
        let dealt = full - world.bosses[0].health;
        assert!(
            dealt > full * 0.05 && dealt < full * 0.1,
            "{dealt} of {full}"
        );
    }

    #[test]
    fn a_lead_meets_a_moving_target_where_the_round_gets_there() {
        let bounds = Rect {
            x: 0.,
            y: 0.,
            w: 4000.,
            h: 4000.,
        };
        let (from, at, moving) = (
            V2::new(500., 2000.),
            V2::new(950., 2000.),
            V2::new(0., 180.),
        );
        let point = lead_point(from, (at, moving), 400., 0., bounds, &[]);
        let flight = point.distance(from) / 400.;
        assert!(point.distance(at + moving * flight) < 1., "{point:?}");
        // A target it can never catch just gets shot at where it is.
        let fast = V2::new(900., 0.);
        assert_eq!(intercept_time(V2::new(600., 0.), fast, 400.), None);
        // Cover in front of the lead: it aims at the target instead.
        let wall = Rect {
            x: 700.,
            y: 2090.,
            w: 60.,
            h: 400.,
        };
        assert_eq!(
            lead_point(from, (at, moving), 400., 0., bounds, &[wall]),
            at
        );
    }

    #[test]
    fn a_circling_boss_drives_in_straight_legs() {
        let mut world = arena();
        world.tanks[0].max_health = 1e9;
        world.spawn_boss(BossKind::Tempest);
        let foe = V2::new(1600., 1300.);
        world.bosses[0].pos = foe + V2::new(450., 0.);
        world.bosses[0].arrival = 0.;
        world.bosses[0].timer = 1e6;
        world.bosses[0].attack_timer = 1e6;
        let (mut legs, mut bent) = (0, 0);
        let mut last: Option<(V2, bool)> = None;
        for _ in 0..600 {
            world.tanks[0].pos = foe;
            world.tanks[0].vel = V2::ZERO;
            world.tanks[0].health = 1e9;
            world.update_bosses();
            let boss = &world.bosses[0];
            assert_eq!(boss.tactic, BossTactic::Orbit);
            let on_line = boss
                .vel
                .normalized()
                .dot((boss.leg - boss.pos).normalized())
                > 0.9999;
            if let Some((leg, was_on_line)) = last {
                if leg != boss.leg {
                    legs += 1;
                } else if was_on_line && !on_line {
                    bent += 1;
                }
            }
            last = Some((boss.leg, on_line));
        }
        assert!(legs >= 3, "it keeps moving around the target: {legs}");
        assert_eq!(bent, 0, "once on a leg it holds a straight line");
    }

    #[test]
    fn a_ready_special_waits_for_the_gun_but_not_forever() {
        let mut world = arena();
        world.tanks[0].max_health = 1e9;
        world.tanks[0].pos = V2::new(1600., 1900.);
        world.spawn_boss(BossKind::SiegeEngine);
        let boss = &mut world.bosses[0];
        boss.pos = V2::new(1600., 1300.);
        boss.aim = -std::f32::consts::FRAC_PI_2;
        boss.arrival = 0.;
        boss.timer = 1e6;
        boss.attack_timer = DT;
        world.update_bosses();
        assert_eq!(world.bosses[0].attack_windup, 0., "still pointing away");
        for _ in 0..((BOSS_AIM_HOLD + 0.1) / DT) as usize {
            world.tanks[0].health = 1e9;
            world.update_bosses();
        }
        assert!(world.bosses[0].attack_windup > 0., "it stopped waiting");
    }

    #[test]
    fn bastions_cannon_hurts_badly_but_is_no_siege_gun() {
        let rounds = special_volley(BossKind::Bastion, 0, 0.5, 1);
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].angle, 0.5);
        assert!(rounds[0].damage < LETHAL_DAMAGE);
        assert!(rounds[0].speed < SIEGE_ROUND_SPEED);
        assert!(rounds[0].radius < SIEGE_ROUND_RADIUS);
        assert!(BossKind::Bastion.windup() < BossKind::SiegeEngine.windup());
        assert_eq!(special_volley(BossKind::Bastion, 0, 0.5, 2).len(), 3);
        // Other signature volleys only reach the cannon threshold on the heavy hulls.
        for kind in BossKind::ALL {
            for phase in 1..=3 {
                for round in special_volley(kind, 0, 0., phase) {
                    assert!(
                        round.damage < BASTION_ROUND_DAMAGE
                            || matches!(
                                kind,
                                BossKind::Bastion | BossKind::SiegeEngine | BossKind::Carillon
                            ),
                        "{kind:?}"
                    );
                }
            }
        }

        let mut world = arena();
        world.tanks[0].pos = V2::new(1500., 1300.);
        world.tanks[0].max_health = 1000.;
        world.tanks[0].health = 1000.;
        world.spawn_boss(BossKind::Bastion);
        let boss = &mut world.bosses[0];
        boss.pos = V2::new(900., 1300.);
        boss.arrival = 0.;
        boss.timer = 1e6;
        boss.attack_timer = -BOSS_AIM_HOLD;
        for _ in 0..(BossKind::Bastion.windup() / DT) as usize + 50 {
            world.bosses[0].pos = V2::new(900., 1300.);
            world.bosses[0].timer = 1e6;
            world.step();
        }
        assert_eq!(world.tanks[0].deaths, 0);
        assert!(world.tanks[0].health <= 1000. - BASTION_ROUND_DAMAGE + 40.);
    }

    #[test]
    fn every_boss_keeps_its_footing_through_a_long_messy_fight() {
        // A small cheap generator, so a failure is the same failure next time.
        let mut seed = 0x2F6E_2B17_u32;
        let mut roll = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32
        };
        for (width, height) in [(3200., 2600.), (700., 520.)] {
            for kind in BossKind::ALL {
                let mut world = arena();
                world.config.width = width;
                world.config.height = height;
                let bounds = world.config.bounds();
                world.walls = vec![
                    Rect {
                        x: bounds.x + bounds.w * 0.3,
                        y: bounds.y + bounds.h * 0.2,
                        w: bounds.w * 0.05,
                        h: bounds.h * 0.45,
                    },
                    Rect {
                        x: bounds.x + bounds.w * 0.62,
                        y: bounds.y + bounds.h * 0.55,
                        w: bounds.w * 0.2,
                        h: bounds.h * 0.06,
                    },
                ];
                world.tanks[0].max_health = 1e9;
                if world.spawn_boss(kind) == 0 {
                    // Large hulls can run out of safe space between these walls.
                    assert!(matches!(
                        kind,
                        BossKind::Railgunner | BossKind::Jester | BossKind::Carillon
                    ));
                    assert_eq!((width, height), (700., 520.));
                    assert!(world.bosses.is_empty());
                    continue;
                }
                world.bosses[0].arrival = 0.;
                let top = world.bosses[0].max_health;
                for tick in 0..2400 {
                    let tank = &mut world.tanks[0];
                    tank.health = 1e9;
                    if tick % 37 == 0 {
                        // Teleports, corners and standing on the boss itself.
                        tank.pos = match tick % 4 {
                            0 => world.bosses[0].pos,
                            1 => V2::new(bounds.x, bounds.y + bounds.h),
                            _ => {
                                V2::new(bounds.x + roll() * bounds.w, bounds.y + roll() * bounds.h)
                            }
                        };
                        tank.vel =
                            V2::from_angle(roll() * std::f32::consts::TAU) * (roll() * 2400.);
                    }
                    if tick % 600 == 0 {
                        world.bosses[0].health = top * (1. - tick as f32 / 2600.);
                    }
                    world.update_bosses();
                    let boss = &world.bosses[0];
                    assert!(
                        boss.pos.finite()
                            && boss.vel.finite()
                            && boss.leg.finite()
                            && boss.read.finite()
                            && boss.aim.is_finite(),
                        "{kind:?} at tick {tick} in {width}x{height}"
                    );
                    assert!(
                        boss.pos.x >= bounds.x
                            && boss.pos.y >= bounds.y
                            && boss.pos.x <= bounds.x + bounds.w
                            && boss.pos.y <= bounds.y + bounds.h,
                        "{kind:?} left the arena at tick {tick}: {:?}",
                        boss.pos
                    );
                    assert!(
                        world
                            .projectiles
                            .iter()
                            .all(|p| p.pos.finite() && p.vel.finite())
                    );
                    world.projectiles.clear();
                }
            }
        }
    }

    #[test]
    fn a_bosses_health_follows_the_arena_and_keeps_how_hurt_it_is() {
        let mut world = arena();
        world.tanks[0].pos = V2::new(2450., 1300.);
        world.spawn_boss(BossKind::Sentinel);
        let before = world.bosses[0].max_health;
        world.bosses[0].health = before * 0.5;
        for n in 0..3 {
            let id = world.add_player(&format!("Friend {n}"), false);
            world.set_level_unchecked(id, MAX_LEVEL);
        }
        world.update_bosses();
        let boss = &world.bosses[0];
        assert!(boss.max_health > before * 2.);
        assert!((boss.health / boss.max_health - 0.5).abs() < 0.01);
    }

    #[test]
    fn everyone_who_hurt_a_boss_in_the_last_minute_shares_its_reward() {
        let mut world = arena();
        world.tanks[0].pos = V2::new(2450., 1300.);
        let early = world.tanks[0].id;
        world.spawn_boss(BossKind::Sentinel);
        let boss = world.bosses[0].id;
        world.record_damage(boss, early, 500.);
        world.tick += 1800;
        world.contributions.retain(|_, v| !v.is_empty());
        let before = world.tanks[0].score;
        world.reward_within(boss, 0, 1000, 1000, BOSS_CREDIT_TICKS);
        assert!(world.tanks[0].score > before);
    }

    #[test]
    fn random_events_roll_once_per_second_with_grace_and_a_single_boss_cooldown() {
        let mut world = arena();
        let rng = world.rng;
        for tick in 1..3600 {
            world.tick = tick;
            world.random_boss_event();
        }
        assert!(world.bosses.is_empty());
        assert_eq!(world.rng, rng);
        let mut arrivals = 0;
        for second in 60..10060 {
            world.tick = second * 60;
            world.random_boss_event();
            arrivals += world.bosses.len();
            world.bosses.clear();
        }
        assert!(
            (10..=45).contains(&arrivals),
            "0.25% aggregate roll produced {arrivals} arrivals in 10000 seconds"
        );
        assert_ne!(world.spawn_boss(BossKind::Summoner), 0);
        world.tick += 60;
        world.random_boss_event();
        assert_eq!(world.bosses.len(), 1);
        assert_eq!(world.boss_roll_at, world.tick + 90 * 60);
    }

    #[test]
    fn every_drone_boss_has_a_bounded_swarm_that_pursues_players() {
        for kind in BossKind::ALL
            .into_iter()
            .filter(|kind| kind.drone_limit() > 0)
        {
            let mut world = arena();
            world.config.mode = Mode::Sandbox;
            let player = world.tanks[0].id;
            world.sandbox_invulnerability(player, true);
            world.tanks[0].pos = V2::new(2450., 1300.);
            let id = world.spawn_boss(kind);
            assert_ne!(id, 0);
            world.bosses[0].pos = V2::new(1600., 1300.);
            if kind == BossKind::Jester {
                // A Jester only calls escorts while wearing a swarm mask.
                world.bosses[0].attack = (0..=u8::MAX)
                    .find(|&i| crate::encounters::jester_act(i) == Some((BossKind::Foundry, 0)))
                    .unwrap();
                world.bosses[0].attack_timer = 12.;
            }
            let mut largest = 0;
            for _ in 0..1200 {
                world.step();
                let swarm = world
                    .projectiles
                    .iter()
                    .filter(|p| p.owner == id && p.kind.is_drone())
                    .count();
                assert!(swarm <= kind.drone_limit());
                largest = largest.max(swarm);
            }
            assert!(largest > 0, "{kind:?} launched no swarm");
            assert_eq!(world.bosses[0].target, player);
        }
    }

    #[test]
    fn boss_movement_and_spawn_respect_borders_walls_and_protected_players() {
        let mut world = arena();
        world.config.arena_scale = 0.7;
        world.walls.push(Rect {
            x: 1750.,
            y: 600.,
            w: 100.,
            h: 1400.,
        });
        let id = world.spawn_boss(BossKind::FallenBooster);
        assert_ne!(id, 0);
        let boss = &world.bosses[0];
        assert!(world.position_clear(boss.pos, boss.radius));
        world.tanks[0].protection = 30.;
        for _ in 0..120 {
            world.update_bosses();
        }
        assert!(
            !world
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Shot))
        );
        assert!(
            world
                .config
                .bounds()
                .contains_circle(world.bosses[0].pos, world.bosses[0].radius)
        );
    }

    #[test]
    fn arrival_grace_separates_hulls_without_contact_damage_then_combat_resumes() {
        let mut world = arena();
        world.config.mode = Mode::Sandbox;
        world.spawn_boss(BossKind::Sentinel);
        world.tanks[0].pos = world.bosses[0].pos + V2::new(30., 0.);
        let health = world.bosses[0].health;
        world.step();
        assert_eq!(world.bosses[0].health, health);
        assert_eq!(world.tanks[0].health, world.tanks[0].max_health);
        assert!(
            world.tanks[0].pos.distance(world.bosses[0].pos)
                >= world.tanks[0].radius + world.bosses[0].radius - 0.1
        );
        world.bosses[0].arrival = 0.;
        world.tanks[0].pos = world.bosses[0].pos + V2::new(30., 0.);
        world.step();
        assert!(world.tanks[0].health < world.tanks[0].max_health);
        assert!(world.bosses[0].health < health);
    }
}
