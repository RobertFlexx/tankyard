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
const LINING_UP: f32 = 0.6;
/// How close its aim must be, in radians, before it locks a special.
const LINED_UP: f32 = 0.1;
/// The longest a ready special waits for the gun to come round, in seconds.
const MOST_HOLD: f32 = 1.2;
/// How far around its target one leg takes a circling boss, in radians.
const LEG_TURN: f32 = 0.62;
/// About how far a boss drives in a straight line before it picks a new one.
const LEG_LENGTH: f32 = 200.;

fn boss_clear_steering(
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
        if !bounds.contains_circle(next, radius + 8.)
            || walls
                .iter()
                .any(|&w| sweep_rect(pos, next, radius + 4., w).is_some())
        {
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
fn boss_glide(pos: V2, step: V2, vel: V2, radius: f32, bounds: Rect, walls: &[Rect]) -> (V2, V2) {
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

/// Seconds until a round at `speed` meets a target `gap` away moving at
/// `moving`, if it ever can.
fn intercept_time(gap: V2, moving: V2, speed: f32) -> Option<f32> {
    let a = moving.length_squared() - speed * speed;
    let b = 2. * gap.dot(moving);
    let c = gap.length_squared();
    if a.abs() < 0.001 {
        return (b < 0.).then(|| -c / b);
    }
    let room = b * b - 4. * a * c;
    if room < 0. {
        return None;
    }
    let root = room.sqrt();
    let (one, two) = ((-b - root) / (2. * a), (-b + root) / (2. * a));
    let (near, far) = (one.min(two), one.max(two));
    [near, far].into_iter().find(|&time| time > 0.)
}

/// Where to point a gun at `from` so a round at `speed`, fired `warned`
/// seconds from now, meets a target at `at`. If cover stands between the gun
/// and that spot, it aims at the target itself.
fn lead_point(
    from: V2,
    (at, moving): (V2, V2),
    speed: f32,
    warned: f32,
    bounds: Rect,
    walls: &[Rect],
) -> V2 {
    let later = at + moving * warned;
    let flight = intercept_time(later - from, moving, speed)
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

/// How fast the round a boss aims with flies.
fn aimed_speed(kind: BossKind) -> f32 {
    match kind {
        BossKind::SiegeEngine => 390.,
        BossKind::Skirmisher => 450.,
        BossKind::Trapper => 240.,
        BossKind::Guardian => 270.,
        BossKind::Summoner => 235.,
        BossKind::FallenOverlord => 305.,
        BossKind::Defender | BossKind::Prism => 410.,
        BossKind::FallenBooster => 460.,
        BossKind::Sentinel => 330.,
        BossKind::Foundry => 215.,
        BossKind::Tempest => 285.,
        BossKind::Bastion => 340.,
        BossKind::Eclipse => 370.,
    }
}

/// The same for the rounds of its special attack.
fn special_speed(kind: BossKind) -> f32 {
    match kind {
        BossKind::SiegeEngine => SIEGE_ROUND_SPEED,
        BossKind::Bastion => BASTION_ROUND_SPEED,
        BossKind::Sentinel => 560.,
        BossKind::Skirmisher => 420.,
        _ => aimed_speed(kind),
    }
}

/// The end of a boss's next straight leg: on the line to `foe` when it's
/// closing in or backing off, or a chord around it when circling, keeping the
/// way it's already going unless that way is blocked.
fn next_leg(b: &Boss, tactic: BossTactic, foe: V2, ring: f32, bounds: Rect, walls: &[Rect]) -> V2 {
    let away = b.pos - foe;
    let out = if away.length_squared() > 1. {
        away.normalized()
    } else {
        V2::from_angle(b.aim + std::f32::consts::PI)
    };
    if tactic != BossTactic::Orbit {
        return bounds.clamp_circle(foe + out * ring, b.radius + 8.);
    }
    let spin = out.x * b.vel.y - out.y * b.vel.x;
    let way = if spin.abs() > 1. {
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
const fn round(
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

/// The rounds of a boss's special attack: the `attack`th it has made,
/// aimed at `aim`, in `phase`. Its warning shows these same lanes. Each
/// complements the boss's normal fire, every ring leaves a turning gap, and
/// every round meets walls, parries and the round limit as any other.
pub fn special_volley(kind: BossKind, attack: u8, aim: f32, phase: u8) -> Vec<BossRound> {
    use ProjectileKind::*;
    use std::f32::consts::{FRAC_PI_2, TAU};
    let turn = f32::from(attack) * 0.33;
    let mut rounds = Vec::with_capacity(24);
    match kind {
        BossKind::SiegeEngine => {
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
        BossKind::Skirmisher => {
            for n in [-2., -1., 1., 2.] {
                rounds.push(round(aim + n * 0.18, Bullet, 420., 18., 7., 2.4, 1.2, 0));
            }
        }
        BossKind::Trapper => {
            // Two side pockets leave the whole forward lane open.
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
        BossKind::Guardian | BossKind::Summoner | BossKind::FallenOverlord => {
            let shot = if kind == BossKind::FallenOverlord {
                Homing
            } else {
                Bullet
            };
            for n in 0..12 {
                let angle = aim + turn + (n as f32 - 5.5) * 0.24;
                rounds.push(round(angle, shot, 340., 18., 8., 2.8, 1.3, 0));
            }
        }
        BossKind::Bastion => {
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
        BossKind::Defender => {
            for n in 1..12 {
                rounds.push(round(
                    aim + turn + n as f32 * TAU / 12.,
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
        BossKind::FallenBooster | BossKind::Tempest => {
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
        BossKind::Sentinel => {
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
        BossKind::Prism | BossKind::Eclipse => {
            let shot = if attack == 1 { Homing } else { Piercing };
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
        BossKind::Foundry => {
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
                Some((from_drone, tank.pos))
            })
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map_or(guard, |(_, pos)| pos)
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
            let index = (self.random() * BossKind::ALL.len() as f32) as usize;
            self.spawn_boss(BossKind::ALL[index.min(BossKind::ALL.len() - 1)]);
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
        11_000.
            * (1. + f32::from(self.match_state.boss_wave.max(1) - 1) * 0.14)
            * kind.toughness()
            * (0.3 + 0.7 * strength)
            * crowd
    }

    /// Returns zero if the map cannot accommodate the boss safely.
    pub fn spawn_boss(&mut self, kind: BossKind) -> EntityId {
        self.spawn_boss_from(kind, None)
    }
    /// A boss arriving as close to `near` as is safe, as map triggers ask.
    pub fn spawn_boss_near(&mut self, kind: BossKind, near: V2) -> EntityId {
        self.spawn_boss_from(kind, Some(near))
    }
    fn spawn_boss_from(&mut self, kind: BossKind, near: Option<V2>) -> EntityId {
        if self.bosses.len() >= 16 {
            return 0;
        }
        let bounds = self.config.bounds();
        let radius = kind.radius();
        let mut best = None;
        let mut safety = f32::NEG_INFINITY;
        let inside = |p: V2| {
            V2::new(
                p.x.clamp(bounds.x + radius + 30., bounds.x + bounds.w - radius - 30.),
                p.y.clamp(bounds.y + radius + 30., bounds.y + bounds.h - radius - 30.),
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
            arrival: 2.,
            attack_timer: 7.,
            attack: 0,
            attack_windup: 0.,
            dodge: V2::ZERO,
            read: V2::ZERO,
            leg: pos,
            leg_time: 0.,
        });
        self.event(EventKind::BossSpawn, pos, id, kind as u32, 0);
        id
    }
    pub(crate) fn update_bosses(&mut self) {
        if self.bosses.is_empty() {
            return;
        }
        let mut volleys = Vec::new();
        let mut warnings = Vec::new();
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
            let full = self.boss_health_at(boss.kind, strength);
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
            let owned = self
                .projectiles
                .iter()
                .filter(|p| {
                    p.owner == b.id && p.kind.is_drone() && p.lifetime > 0. && p.penetration > 0.
                })
                .count();
            let target = self
                .tanks
                .iter()
                .filter(|t| {
                    t.tangible()
                        && t.health > 0.
                        && self.hostile(b.team, t.team)
                        && t.protection <= 0.
                        && !self.config.base_protected(t.team, t.pos, t.radius)
                        && t.pos.distance(b.pos) < 1600.
                        && (t.level >= 15
                            || b.kind.drone_limit() == 0
                            || t.id == b.last_attacker
                            || t.pos.distance(b.pos) < 420.)
                        && self
                            .walls
                            .iter()
                            .all(|&w| sweep_rect(b.pos, t.pos, 5., w).is_none())
                })
                .min_by(|a, c| {
                    let cost = |t: &Tank| {
                        t.pos.distance(b.pos)
                            * if t.id == b.target { 0.75 } else { 1. }
                            * if t.id == b.last_attacker { 0.65 } else { 1. }
                            * (0.75 + t.health / t.max_health.max(1.) * 0.25)
                    };
                    cost(a).total_cmp(&cost(c))
                })
                .map(|t| (t.id, t.pos, t.vel));
            let bounds = self.config.bounds();
            let b = &mut self.bosses[i];
            b.arrival = (b.arrival - DT).max(0.);
            b.phase = if b.health < b.max_health * BOSS_PHASES[1] {
                3
            } else if b.health < b.max_health * BOSS_PHASES[0] {
                2
            } else {
                1
            };
            if let Some((id, position, moving)) = target {
                b.read = if id == b.target {
                    b.read.lerp(moving, 1. - (-DT * READ_RATE).exp())
                } else {
                    moving
                };
                b.target = id;
                b.goal = position;
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
            let to = b.goal - b.pos;
            let mut lined_up = true;
            if b.tactic != BossTactic::Windup
                && b.tactic != BossTactic::Charge
                && b.attack_windup <= 0.
            {
                // Just before a special it aims for that shot instead, at
                // where the target should be once the warning is over.
                let (shot, warned) = if b.attack_timer < LINING_UP {
                    (special_speed(b.kind), b.kind.windup() * 0.45)
                } else {
                    (aimed_speed(b.kind), 0.)
                };
                let point = target.map_or(b.goal, |(_, position, _)| {
                    lead_point(b.pos, (position, b.read), shot, warned, bounds, &self.walls)
                });
                let difference = ((point - b.pos).angle() - b.aim + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                lined_up = difference.abs() < LINED_UP;
                let turn_rate = match b.kind {
                    BossKind::SiegeEngine => 1.,
                    BossKind::Bastion => 1.7,
                    _ => 3.2,
                };
                b.aim += difference.clamp(-turn_rate * DT, turn_rate * DT);
            }
            b.tactic_time = (b.tactic_time - DT).max(0.);
            b.leg_time = (b.leg_time - DT).max(0.);
            let speed = if b.mothership {
                28.
            } else {
                match b.kind {
                    BossKind::Tempest => 125.,
                    BossKind::Eclipse => 95.,
                    BossKind::FallenBooster => 180.,
                    BossKind::Guardian => 90.,
                    BossKind::Summoner => 45.,
                    BossKind::FallenOverlord => 65.,
                    BossKind::SiegeEngine => 25.,
                    BossKind::Skirmisher => 145.,
                    BossKind::Trapper => 62.,
                    BossKind::Bastion => 38.,
                    BossKind::Foundry | BossKind::Defender => 32.,
                    _ => 55.,
                }
            };
            let preferred = match b.kind {
                BossKind::FallenBooster => 0.,
                BossKind::Guardian => 330.,
                BossKind::Summoner | BossKind::FallenOverlord | BossKind::Foundry => 540.,
                BossKind::Bastion | BossKind::Defender => 650.,
                BossKind::SiegeEngine => 720.,
                BossKind::Skirmisher => 420.,
                BossKind::Trapper => 580.,
                _ => 450.,
            };
            let drone_limit = (b.kind.drone_limit() as f32 * swarm).ceil() as usize;
            let depleted = drone_limit > 0 && owned < drone_limit / 3;
            let range = to.length();
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
                    b.leg = next_leg(b, tactic, foe, ring, bounds, &self.walls);
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
            {
                desired += b.dodge * speed * 0.7;
            }
            desired = boss_clear_steering(b.pos, desired, b.vel, b.radius, bounds, &self.walls);
            if b.arrival > 0. || b.attack_windup > 0. {
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
            if b.arrival <= 0. && target.is_some() && previous_windup <= 0. {
                b.attack_timer -= DT;
                // A ready special waits a moment for the gun to come round,
                // so its warning points at something.
                if b.attack_timer <= 0. && (lined_up || b.attack_timer < -MOST_HOLD) {
                    b.attack = (b.attack + 1) % 3;
                    b.attack_windup = b.kind.windup();
                    b.attack_timer = 9.5 - f32::from(b.phase) * 1.5;
                    warnings.push((b.pos, b.id, b.attack as u32, b.team));
                }
            }
            // Let the warning and secondary pattern read clearly. A primary
            // volley cannot land on the same frame as the special attack.
            let primary = primary_due && b.attack_windup <= 0. && !secondary;
            if secondary {
                b.timer = b.timer.max(0.4);
            }
            if !secondary && (target.is_none() || !primary) {
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
            if secondary {
                for round in special_volley(b.kind, b.attack, b.aim, b.phase) {
                    emit(
                        round.angle,
                        round.kind,
                        round.speed,
                        round.damage,
                        round.radius,
                        round.life,
                        round.penetration,
                        round.bounces,
                    );
                }
                // The quick ones dash in behind their volley.
                match b.kind {
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
            match b.kind {
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
                            b.aim + n * 0.55,
                            ProjectileKind::Trap,
                            240.,
                            21.,
                            11.,
                            4.,
                            2.5,
                            0,
                        );
                    }
                    b.timer = 1.35;
                }
                BossKind::Guardian => {
                    for n in 0..4 {
                        emit(
                            b.aim + std::f32::consts::PI + (n as f32 - 1.5) * 0.18,
                            ProjectileKind::Drone,
                            270.,
                            18.,
                            15.,
                            10.,
                            3.5,
                            0,
                        );
                    }
                    b.timer = 1.1;
                }
                BossKind::Summoner => {
                    for n in 0..4 {
                        emit(
                            b.aim + n as f32 * std::f32::consts::FRAC_PI_2,
                            ProjectileKind::Drone,
                            235.,
                            20.,
                            19.,
                            11.,
                            3.,
                            0,
                        );
                    }
                    b.timer = 2.3;
                }
                BossKind::FallenOverlord => {
                    for n in 0..4 {
                        emit(
                            b.aim + n as f32 * std::f32::consts::FRAC_PI_2,
                            ProjectileKind::Drone,
                            305.,
                            27.,
                            17.,
                            10.,
                            3.5,
                            0,
                        );
                    }
                    b.timer = 2.4;
                }
                BossKind::Defender => {
                    for n in 0..3 {
                        // Out of the launchers on its turning sides.
                        let angle = b.kind.turn(self.tick)
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
                            b.kind.turn(self.tick)
                                + n as f32 * std::f32::consts::TAU
                                    / (6 + b.phase as usize * 2) as f32,
                            ProjectileKind::Piercing,
                            410.,
                            16.,
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
                            215.,
                            19.,
                            12.,
                            7.,
                            4.,
                            0,
                        );
                    }
                    if b.phase > 1 {
                        emit(b.aim, ProjectileKind::Turret, 80., 19., 17., 6., 4., 0);
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
                            b.kind.turn(self.tick) + n as f32 * std::f32::consts::TAU / 8.,
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
                2 => 0.9,
                _ => 0.8,
            } / if b.kind.drone_limit() > 0 { swarm } else { 1. };
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
            self.event(EventKind::Shot, pos, owner, kind as u32, team);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            for _ in 0..(kind.windup() / DT) as usize + 2 {
                world.update_bosses();
            }
            assert!(
                !world.projectiles.is_empty(),
                "{kind:?} did not release its secondary"
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
        boss.attack_timer = -MOST_HOLD;
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
    fn a_parried_siege_round_returns_as_a_heavy_shot_not_a_one_shot() {
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
        assert!(world.parry_projectile(index, 0, impact));
        let returned = &world.projectiles[index];
        assert_eq!(returned.owner, world.tanks[0].id);
        assert!(returned.damage <= 240., "{}", returned.damage);
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
        for _ in 0..((MOST_HOLD + 0.1) / DT) as usize {
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
        // Nothing else in any boss's kit can be mistaken for it.
        for kind in BossKind::ALL {
            for phase in 1..=3 {
                for round in special_volley(kind, 0, 0., phase) {
                    assert!(
                        round.damage < BASTION_ROUND_DAMAGE
                            || matches!(kind, BossKind::Bastion | BossKind::SiegeEngine),
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
        boss.attack_timer = -MOST_HOLD;
        for _ in 0..(BossKind::Bastion.windup() / DT) as usize + 50 {
            world.bosses[0].pos = V2::new(900., 1300.);
            world.bosses[0].timer = 1e6;
            world.step();
        }
        assert_eq!(world.tanks[0].deaths, 0);
        assert!(world.tanks[0].health <= 1000. - BASTION_ROUND_DAMAGE + 40.);
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
