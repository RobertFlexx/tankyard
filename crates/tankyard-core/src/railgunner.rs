//! The Railgunner: a sniper on the biggest hull of any regular boss, and a
//! very rare sight in the wild. It picks off whoever stands furthest away,
//! with a cannon that outdoes the Siege Engine's and a stream of plasma, and
//! its long barrel is no use at all against anyone standing next to it.
//!
//! On the wire it is a Siege Engine of its own size ([`BossKind::from_wire`]),
//! with its beam and cooling carried in the boss's tactic, so a client from
//! an earlier 2.0 release reads it as a very large Siege Engine and stays in
//! the match.
use crate::bosses::{
    LINED_UP, LINING_UP, Volley, boss_clear_steering, boss_glide, lead_point, special_volley,
};
use crate::physics::sweep_rect;
use crate::*;
use std::f32::consts::PI;
use tankyard_content::ProjectileKind;

/// Its hull radius, the largest of any regular boss. No other boss has this
/// size, which is how a Railgunner is told apart on arrival.
pub const RAIL_RADIUS: f32 = 132.;
/// How far ahead of the hull's middle its rounds leave, in hull radii.
pub const RAIL_MUZZLE: f32 = 2.35;
/// How fast the rail cannon's round flies, well past the Siege Engine's.
pub const RAIL_ROUND_SPEED: f32 = 2450.;
/// How wide the rail cannon's round is.
pub const RAIL_ROUND_RADIUS: f32 = 30.;
const RAIL_ROUND_LIFE: f32 = 1.5;
/// What the wounded Railgunner's second, quicker shot deals.
pub const RAIL_ECHO_DAMAGE: f32 = 170.;
pub(crate) const RAIL_ECHO_RADIUS: f32 = 22.;
/// Seconds between the first shot and the echo.
const RAIL_ECHO_DELAY: f32 = 0.6;
/// How fast the plasma stream's rounds fly.
pub const BEAM_ROUND_SPEED: f32 = 2100.;
/// How wide one round of the stream is.
pub const BEAM_ROUND_RADIUS: f32 = 11.;
/// What one round of the stream deals. It lands thirty of them a second.
pub const BEAM_ROUND_DAMAGE: f32 = 15.;
const BEAM_ROUND_LIFE: f32 = 1.05;
/// Seconds between the stream's rounds: every other tick.
const BEAM_CADENCE: f32 = 2. * DT;
const COIL_ROUND_SPEED: f32 = 1700.;
const COIL_ROUND_DAMAGE: f32 = 46.;
const COIL_ROUND_RADIUS: f32 = 9.;
const COIL_RELOAD: f32 = 1.15;
const FLAK_SPEED: f32 = 620.;
const FLAK_DAMAGE: f32 = 9.;
const FLAK_RELOAD: f32 = 0.62;
/// How far it can see a target, from the middle of its hull.
pub const RAIL_SIGHT: f32 = 2400.;
/// Inside this range its big guns won't bear on a target.
pub const RAIL_CLOSE: f32 = 440.;
/// How long before firing its aim stops following: the moment to move.
pub const RAIL_LOCK: f32 = 0.55;
/// How far it likes to be from what it is shooting.
const STANDOFF: f32 = 1150.;
const DRIVE: f32 = 34.;
/// How much better a new target must score before it changes its mind.
const KEEP_MARK: f32 = 1.12;
const READ_RATE: f32 = 16.;
const TURN_RATE: f32 = 1.15;
/// How fast the stream can swing, by phase. Slow enough that someone close
/// and moving can always outrun it.
const BEAM_TURN: [f32; 3] = [0.62, 0.72, 0.84];
const BEAM_SECONDS: [f32; 3] = [3.4, 4.2, 5.2];
const VENT_SECONDS: [f32; 3] = [2.4, 2.2, 1.9];
/// How much harder it is hit while its vents are open.
pub const RAIL_EXPOSED: f32 = 1.4;
const HOP_RANGE: f32 = 300.;
const HOP_SPEED: f32 = 470.;
const HOP_SECONDS: f32 = 0.55;
const HOP_WAIT: [f32; 3] = [9., 8., 6.5];
/// The chance a random boss event brings the Railgunner: with one event every
/// few minutes, about once in a long day of play.
pub const RAILGUNNER_ODDS: f32 = 1. / 80.;
/// How strong the arena must be, in full-level tanks, for it to turn up on
/// its own, so it never drops on a lobby of beginners.
pub const RAILGUNNER_STRENGTH: f32 = 2.;

/// What a Railgunner is in the middle of. Only the host runs it, so it never
/// travels in a snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RailWork {
    /// Seconds of plasma beam left.
    pub beam: f32,
    /// Seconds until the stream's next round.
    pub cadence: f32,
    /// Seconds left with its vents open, not firing and easier to hurt.
    pub vent: f32,
    /// Seconds left of a quick hop away, and until it can hop again.
    pub hop: f32,
    pub hop_wait: f32,
    /// The way it is hopping.
    pub way: V2,
    /// Seconds until the echo of a wounded rail shot.
    pub echo: f32,
}

impl Boss {
    /// Whether this is a Railgunner with its plasma beam running.
    pub fn beaming(&self) -> bool {
        self.attack_style().0 == BossKind::Railgunner
            && self.tactic == BossTactic::Charge
            && self.tactic_time > 0.
    }
    /// Whether this is a Railgunner cooling down with its vents open.
    pub fn venting(&self) -> bool {
        self.attack_style().0 == BossKind::Railgunner
            && self.tactic == BossTactic::Windup
            && self.tactic_time > 0.
    }
    /// What it takes of a hit, as a share.
    pub fn damage_taken(&self) -> f32 {
        if self.venting() { RAIL_EXPOSED } else { 1. }
    }
    /// A parry has broken its beam: the stream stops dead and the vents open
    /// for `extra` seconds longer than a beam that ran its course.
    pub(crate) fn break_beam(&mut self, extra: f32) {
        let stage = usize::from(self.phase.clamp(1, 3) - 1);
        self.rail.beam = 0.;
        self.rail.cadence = 0.;
        self.rail.vent = VENT_SECONDS[stage] + extra;
        self.tactic = BossTactic::Windup;
        self.tactic_time = self.rail.vent;
    }
}

impl BossKind {
    /// The kind a snapshot read as `self` really is, for a hull of `radius`.
    /// The Railgunner goes out as a Siege Engine of a size no other boss has.
    pub fn from_wire(self, radius: f32) -> Self {
        if self != Self::SiegeEngine {
            self
        } else if radius == RAIL_RADIUS {
            Self::Railgunner
        } else {
            self
        }
    }
}

/// Whether a round flying at `vel` is part of a plasma stream. Its owner
/// still has to be a Railgunner.
pub fn is_beam_round(kind: ProjectileKind, vel: V2) -> bool {
    kind == ProjectileKind::Piercing && vel.length() >= BEAM_ROUND_SPEED * 0.99
}

/// The first roll selects an eligible rare encounter. The second picks
/// uniformly from the ordinary roster when the first misses.
pub(crate) fn natural_boss(rare: f32, pick: f32, strength: f32) -> BossKind {
    if let Some(kind) = crate::sunderworm::rare_boss(rare, strength) {
        return kind;
    }
    // Disjoint bands preserve the exact odds for each eligible encounter.
    let start = crate::sunderworm::WORM_ODDS;
    let end = start + crate::encounters::CARILLON_ODDS;
    if (start..end).contains(&rare) && strength >= 3. {
        return BossKind::Carillon;
    }
    if (end..end + crate::encounters::JESTER_ODDS).contains(&rare) && strength >= 2.5 {
        return BossKind::Jester;
    }
    if rare < RAILGUNNER_ODDS && strength >= RAILGUNNER_STRENGTH {
        return BossKind::Railgunner;
    }
    let count = BossKind::COMMON.len();
    BossKind::COMMON[((pick * count as f32) as usize).min(count - 1)]
}

/// The rounds of its special attacks. The beam is not a volley: it starts when
/// the warning ends and streams for as long as it can stay cool.
pub(crate) fn rail_volley(attack: u8, aim: f32, phase: u8) -> Vec<BossRound> {
    use ProjectileKind::*;
    let mut rounds = Vec::with_capacity(5);
    let round = |angle, kind, speed, damage, radius, life, penetration| BossRound {
        angle,
        kind,
        speed,
        damage,
        radius,
        life,
        penetration,
        bounces: 0,
    };
    match attack {
        1 => {
            rounds.push(round(
                aim,
                Shell,
                RAIL_ROUND_SPEED,
                LETHAL_DAMAGE,
                RAIL_ROUND_RADIUS,
                RAIL_ROUND_LIFE,
                40.,
            ));
            if phase >= 2 {
                for side in [-1., 1.] {
                    rounds.push(round(
                        aim + side * 0.045,
                        Piercing,
                        1500.,
                        70.,
                        10.,
                        1.6,
                        2.,
                    ));
                }
            }
        }
        2 => {}
        _ => {
            for n in -2..=2 {
                rounds.push(round(
                    aim + n as f32 * 0.055,
                    Piercing,
                    1650.,
                    40.,
                    10.,
                    1.7,
                    2.,
                ));
            }
        }
    }
    rounds
}

/// Where a round of `radius` leaves the barrel: at the muzzle, or as far along
/// it as cover and the arena's edge allow.
fn muzzle(at: V2, aim: f32, reach: f32, radius: f32, bounds: Rect, walls: &[Rect]) -> Option<V2> {
    let tip = at + V2::from_angle(aim) * reach;
    let clear = walls
        .iter()
        .filter_map(|&wall| sweep_rect(at, tip, radius, wall).map(|(t, _)| t))
        .fold(1_f32, f32::min);
    let mut origin = at.lerp(tip, (clear - 0.02).max(0.));
    for _ in 0..8 {
        if bounds.contains_circle(origin, radius) {
            return Some(origin);
        }
        origin = at.lerp(origin, 0.7);
    }
    None
}

/// Who it's aiming at.
#[derive(Clone, Copy)]
struct Mark {
    id: EntityId,
    pos: V2,
    vel: V2,
    distance: f32,
}

impl World {
    /// The furthest tank it has a clear line to. It settles on one, saves
    /// beginners for last, and takes someone under its barrel only when
    /// there's nobody else to shoot.
    fn rail_mark(&self, i: usize) -> Option<Mark> {
        let b = &self.bosses[i];
        let mut best: Option<(f32, Mark)> = None;
        for t in &self.tanks {
            if !self.can_hunt(b.team, t) {
                continue;
            }
            let distance = t.pos.distance(b.pos);
            if distance > RAIL_SIGHT {
                continue;
            }
            let mut score = distance.max(1.);
            if t.id == b.target {
                score *= KEEP_MARK;
            }
            if distance < RAIL_CLOSE {
                score *= 0.05;
            }
            if t.level < 15 && t.id != b.last_attacker {
                score *= 0.25;
            }
            if best.as_ref().is_some_and(|(top, _)| *top >= score)
                || self
                    .walls
                    .iter()
                    .any(|&wall| sweep_rect(b.pos, t.pos, 5., wall).is_some())
            {
                continue;
            }
            best = Some((
                score,
                Mark {
                    id: t.id,
                    pos: t.pos,
                    vel: t.vel,
                    distance,
                },
            ));
        }
        best.map(|(_, mark)| mark)
    }

    /// How far off the nearest visible tank is. A tank behind cover cannot
    /// force a hop or steer the hull away from its last sighting.
    fn rail_nearest(&self, i: usize) -> Option<(f32, V2)> {
        let b = &self.bosses[i];
        self.tanks
            .iter()
            .filter(|t| {
                self.can_hunt(b.team, t)
                    && (t.pos - b.pos).length_squared()
                        < (HOP_RANGE + b.radius).max(RAIL_CLOSE + 60.).powi(2)
                    && self
                        .walls
                        .iter()
                        .all(|&wall| sweep_rect(b.pos, t.pos, 5., wall).is_none())
            })
            .map(|t| (t.pos.distance(b.pos), t.pos))
            .min_by(|a, c| a.0.total_cmp(&c.0))
    }

    /// One tick of a Railgunner.
    pub(crate) fn update_railgunner(
        &mut self,
        i: usize,
        volleys: &mut Vec<Volley>,
        warnings: &mut Vec<(V2, EntityId, u32, u8)>,
    ) {
        let mark = self.rail_mark(i);
        let near = self.rail_nearest(i);
        let boss = &self.bosses[i];
        let remembered = boss.target != 0
            && self
                .tanks
                .iter()
                .any(|t| t.id == boss.target && self.can_hunt(boss.team, t));
        let bounds = self.config.bounds();
        let walls = &self.walls;
        let tick = self.tick;
        let b = &mut self.bosses[i];
        b.arrival = (b.arrival - DT).max(0.);
        if b.kind != BossKind::Jester || b.attack_windup <= 0. {
            b.phase = if b.health < b.max_health * BOSS_PHASES[1] {
                3
            } else if b.health < b.max_health * BOSS_PHASES[0] {
                2
            } else {
                1
            };
        }
        let stage = usize::from(b.phase.clamp(1, 3) - 1);
        let mut search = None;
        if let Some(mark) = mark {
            b.read = if mark.id == b.target {
                b.read.lerp(mark.vel, 1. - (-DT * READ_RATE).exp())
            } else {
                mark.vel
            };
            b.target = mark.id;
            b.goal = mark.pos;
            b.mind.saw(mark.pos, mark.vel, b.pos);
        } else {
            let had_target = b.target != 0;
            if !remembered {
                b.mind.forget();
            }
            search = b.search_goal(&self.config, walls, &mut self.boss_routes);
            if search.is_none()
                && (had_target
                    || b.pos.distance(b.goal) < 130.
                    || !bounds.contains_circle(b.goal, b.radius + 60.))
            {
                let angle = tick as f32 * 0.002 + b.id as f32 * 1.618;
                b.goal = bounds.clamp_circle(
                    bounds.center() + V2::from_angle(angle) * (bounds.w.min(bounds.h) * 0.3),
                    b.radius + 80.,
                );
            }
        }
        let beaming = b.rail.beam > 0.;
        b.rail.hop = (b.rail.hop - DT).max(0.);
        b.rail.hop_wait = (b.rail.hop_wait - DT).max(0.);
        b.rail.vent = (b.rail.vent - DT).max(0.);
        b.rail.beam = (b.rail.beam - DT).max(0.);
        if beaming && b.rail.beam <= 0. {
            b.rail.vent = VENT_SECONDS[stage];
        }
        let charging = b.attack_windup > 0.;
        // Its guns follow until the last moment before a shot, then hold.
        let locked = charging && b.attack_windup < RAIL_LOCK;
        let mut lined_up = true;
        if !locked && b.rail.vent <= 0. {
            let (shot, warned) = if b.rail.beam > 0. {
                (BEAM_ROUND_SPEED, 0.)
            } else if charging {
                (RAIL_ROUND_SPEED, b.attack_windup)
            } else if b.attack_timer < LINING_UP {
                (RAIL_ROUND_SPEED, b.warning_seconds())
            } else {
                (COIL_ROUND_SPEED, 0.)
            };
            let look = if charging || b.rail.beam > 0. {
                b.goal
            } else {
                search.unwrap_or(b.goal)
            };
            let point = mark.map_or(look, |mark| {
                lead_point(b.pos, (mark.pos, b.read), shot, warned, bounds, walls)
            });
            let difference =
                ((point - b.pos).angle() - b.aim + PI).rem_euclid(std::f32::consts::TAU) - PI;
            lined_up = difference.abs() < LINED_UP;
            let rate = if b.rail.beam > 0. {
                BEAM_TURN[stage]
            } else {
                TURN_RATE
            };
            b.aim += difference.clamp(-rate * DT, rate * DT);
        }
        // Anyone at its door makes it hop back, when it's free to.
        let planted = b.arrival > 0. || charging || b.rail.beam > 0. || b.rail.vent > 0.;
        if let Some((distance, from)) = near
            && distance < HOP_RANGE + b.radius
            && b.rail.hop_wait <= 0.
            && !planted
        {
            b.rail.hop = HOP_SECONDS;
            b.rail.hop_wait = HOP_WAIT[stage];
            let away = b.pos - from;
            b.rail.way = if away.length_squared() > 1. {
                away.normalized()
            } else {
                V2::from_angle(b.aim + PI)
            };
        }
        let drive = DRIVE * (1. + 0.12 * stage as f32);
        let desired = if planted {
            V2::ZERO
        } else if b.rail.hop > 0. {
            b.rail.way * HOP_SPEED
        } else if let Some((distance, from)) = near
            && distance < RAIL_CLOSE
        {
            b.tactic = BossTactic::Retreat;
            (b.pos - from).normalized() * drive * 1.6
        } else if let Some(mark) = mark {
            let run = (mark.pos - b.pos).normalized();
            if mark.distance > STANDOFF + 180. {
                b.tactic = BossTactic::Advance;
                run * drive
            } else if mark.distance < STANDOFF - 180. {
                b.tactic = BossTactic::Retreat;
                run * -drive
            } else {
                b.tactic = BossTactic::Orbit;
                let side = if b.id.is_multiple_of(2) { 1. } else { -1. };
                V2::new(-run.y, run.x) * side * drive * 0.5
            }
        } else if let Some(point) = search {
            b.tactic = BossTactic::Advance;
            (point - b.pos).clamp_length(drive * 0.75) / 0.75
        } else {
            b.tactic = BossTactic::Patrol;
            (b.goal - b.pos).normalized() * drive * 0.6
        };
        let desired = if desired == V2::ZERO {
            desired
        } else {
            boss_clear_steering(b.pos, desired, b.vel, b.radius, bounds, walls)
        };
        let push = if b.rail.hop > 0. { 2400. } else { drive * 3.2 };
        b.vel = b.vel.toward(desired, push * DT);
        (b.pos, b.vel) = boss_glide(b.pos, b.vel * DT, b.vel, b.radius, bounds, walls);
        // Clients draw the beam and the vents from its tactic.
        if b.rail.beam > 0. {
            b.tactic = BossTactic::Charge;
            b.tactic_time = b.rail.beam;
        } else if b.rail.vent > 0. {
            b.tactic = BossTactic::Windup;
            b.tactic_time = b.rail.vent;
        } else {
            if matches!(b.tactic, BossTactic::Charge | BossTactic::Windup) {
                b.tactic = BossTactic::Patrol;
            }
            b.tactic_time = 0.;
        }
        b.timer -= DT;
        let previous = b.attack_windup;
        b.attack_windup = (b.attack_windup - DT).max(0.);
        let release = previous > 0. && b.attack_windup <= 0.;
        if b.arrival <= 0.
            && (mark.is_some() || (b.kind == BossKind::Jester && near.is_some()))
            && previous <= 0.
            && b.rail.beam <= 0.
            && b.rail.vent <= 0.
            && b.rail.hop <= 0.
        {
            b.attack_timer -= DT;
            // The mimic's director picks the next mask on the main boss tick.
            if b.kind == BossKind::Railgunner
                && b.attack_timer <= 0.
                && (lined_up || b.attack_timer < -BOSS_AIM_HOLD)
            {
                b.attack = (b.attack + 1) % 3;
                b.attack_windup = b.kind.windup();
                b.attack_timer = 8.5 - f32::from(b.phase) * 1.2;
                warnings.push((b.pos, b.id, u32::from(b.attack), b.team));
            }
        }
        let (id, team, at, hull, aim) = (b.id, b.team, b.pos, b.radius, b.aim);
        let muzzle_reach = hull
            * if b.kind == BossKind::Jester {
                crate::encounters::CARD_MUZZLE
            } else {
                RAIL_MUZZLE
            };
        let barrel =
            |angle: f32, radius: f32| muzzle(at, angle, muzzle_reach, radius, bounds, walls);
        let mut fire = |origin: V2, vel: V2, kind, damage, radius, life, pen| {
            volleys.push((id, team, origin, vel, kind, damage, radius, life, pen, 0));
        };
        if release {
            if b.attack_style().1 == 2 {
                b.rail.beam = BEAM_SECONDS[stage];
                b.rail.cadence = 0.;
                b.tactic = BossTactic::Charge;
                b.tactic_time = b.rail.beam;
            } else {
                for round in special_volley(b.kind, b.attack, aim, b.phase) {
                    if let Some(origin) = barrel(round.angle, round.radius) {
                        fire(
                            origin,
                            V2::from_angle(round.angle) * round.speed,
                            round.kind,
                            round.damage,
                            round.radius,
                            round.life,
                            round.penetration,
                        );
                    }
                }
                if b.attack_style().1 == 1 && b.phase >= 3 {
                    b.rail.echo = RAIL_ECHO_DELAY;
                }
            }
            b.timer = b.timer.max(0.55);
        }
        if b.rail.echo > 0. {
            b.rail.echo -= DT;
            if b.rail.echo <= 0.
                && let Some(origin) = barrel(aim, RAIL_ECHO_RADIUS)
            {
                fire(
                    origin,
                    V2::from_angle(aim) * RAIL_ROUND_SPEED * 0.88,
                    ProjectileKind::Shell,
                    RAIL_ECHO_DAMAGE,
                    RAIL_ECHO_RADIUS,
                    RAIL_ROUND_LIFE,
                    8.,
                );
            }
        }
        if b.rail.beam > 0. {
            b.rail.cadence -= DT;
            if b.rail.cadence <= 0. {
                b.rail.cadence += BEAM_CADENCE;
                if let Some(origin) = barrel(aim, BEAM_ROUND_RADIUS) {
                    fire(
                        origin,
                        V2::from_angle(aim) * BEAM_ROUND_SPEED,
                        ProjectileKind::Piercing,
                        BEAM_ROUND_DAMAGE,
                        BEAM_ROUND_RADIUS,
                        BEAM_ROUND_LIFE,
                        3.,
                    );
                }
            }
        }
        if b.timer > 0.
            || b.arrival > 0.
            || release
            || b.attack_windup > 0.
            || b.rail.beam > 0.
            || b.rail.vent > 0.
        {
            return;
        }
        let quicker = [1., 0.9, 0.8][stage];
        if let Some(mark) = mark
            && mark.distance >= RAIL_CLOSE
        {
            if let Some(origin) = barrel(aim, COIL_ROUND_RADIUS) {
                fire(
                    origin,
                    V2::from_angle(aim) * COIL_ROUND_SPEED,
                    ProjectileKind::Piercing,
                    COIL_ROUND_DAMAGE,
                    COIL_ROUND_RADIUS,
                    1.6,
                    1.4,
                );
            }
            b.timer = COIL_RELOAD * quicker;
        } else if let Some((distance, from)) = near
            && distance < RAIL_CLOSE + 60.
        {
            // All it has for close work: two little guns on its flanks.
            let across = V2::from_angle(aim + PI * 0.5);
            for side in [-1., 1.] {
                let origin = at + across * side * hull * 0.72;
                if !bounds.contains_circle(origin, 6.)
                    || walls
                        .iter()
                        .any(|&wall| sweep_rect(at, origin, 6., wall).is_some())
                {
                    continue;
                }
                fire(
                    origin,
                    (from - origin).normalized() * FLAK_SPEED,
                    ProjectileKind::Bullet,
                    FLAK_DAMAGE,
                    6.,
                    0.8,
                    1.,
                );
            }
            b.timer = FLAK_RELOAD;
        }
    }
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

    fn railgunner(world: &mut World, at: V2) -> usize {
        // Put it there by hand, since it won't land on top of a tank.
        let saved: Vec<_> = world.tanks.iter().map(|t| t.pos).collect();
        for tank in &mut world.tanks {
            tank.pos = V2::new(100., 100.);
        }
        assert_ne!(world.spawn_boss(BossKind::Railgunner), 0);
        for (tank, pos) in world.tanks.iter_mut().zip(saved) {
            tank.pos = pos;
        }
        let index = world.bosses.len() - 1;
        let boss = &mut world.bosses[index];
        boss.pos = at;
        boss.arrival = 0.;
        boss.timer = 30.;
        boss.attack_timer = 30.;
        index
    }

    fn keep_alive(world: &mut World) {
        for tank in &mut world.tanks {
            tank.health = tank.max_health;
            tank.vel = V2::ZERO;
        }
    }

    #[test]
    fn it_is_the_biggest_regular_boss_and_outguns_the_siege_engine() {
        for kind in BossKind::COMMON {
            assert!(kind.radius() < BossKind::Railgunner.radius(), "{kind:?}");
            assert!(kind.toughness() < BossKind::Railgunner.toughness());
        }
        const {
            assert!(RAIL_ROUND_SPEED > SIEGE_ROUND_SPEED * 1.4);
            assert!(RAIL_ROUND_SPEED < SIEGE_ROUND_SPEED * 1.8);
            assert!(RAIL_ROUND_RADIUS > SIEGE_ROUND_RADIUS);
        }
        let rounds = rail_volley(1, 0., 1);
        assert_eq!(rounds[0].damage, LETHAL_DAMAGE);
    }

    #[test]
    fn it_marks_the_furthest_tank_it_can_see_and_stays_on_it() {
        let mut world = world();
        let close = tank_at(&mut world, "Close", V2::new(2300., 1500.));
        let far = tank_at(&mut world, "Far", V2::new(3300., 1500.));
        let beyond = tank_at(&mut world, "Beyond", V2::new(1500., 1500.));
        let index = railgunner(&mut world, V2::new(1500., 1500.));
        // The one right next to it is the last it would choose.
        world.tanks.iter_mut().find(|t| t.id == beyond).unwrap().pos = V2::new(1600., 1500.);
        assert_eq!(world.rail_mark(index).map(|mark| mark.id), Some(far));
        world.bosses[index].target = far;
        // A slightly longer shot doesn't pull it off its mark.
        world.tanks.iter_mut().find(|t| t.id == close).unwrap().pos = V2::new(3400., 1500.);
        assert_eq!(world.rail_mark(index).map(|mark| mark.id), Some(far));
        // Out of sight of it, it takes the next.
        world
            .tanks
            .iter_mut()
            .find(|t| t.id == far)
            .unwrap()
            .protection = 3.;
        assert_eq!(world.rail_mark(index).map(|mark| mark.id), Some(close));
    }

    #[test]
    fn beginners_are_saved_for_last_unless_they_start_it() {
        let mut world = world();
        let veteran = tank_at(&mut world, "Veteran", V2::new(2200., 1500.));
        let beginner = tank_at(&mut world, "Beginner", V2::new(3300., 1500.));
        world
            .tanks
            .iter_mut()
            .find(|t| t.id == beginner)
            .unwrap()
            .level = 3;
        let index = railgunner(&mut world, V2::new(1500., 1500.));
        assert_eq!(world.rail_mark(index).map(|m| m.id), Some(veteran));
        world.bosses[index].last_attacker = beginner;
        assert_eq!(world.rail_mark(index).map(|m| m.id), Some(beginner));
    }

    #[test]
    fn a_tank_under_the_barrel_is_nearly_safe_from_it() {
        let mut world = world();
        let id = tank_at(&mut world, "Close", V2::new(1500., 1500.));
        let index = railgunner(&mut world, V2::new(1500., 1800.));
        world.bosses[index].timer = 0.;
        world.bosses[index].attack_timer = 0.;
        // Stay beside the hull through a whole cycle of its guns.
        let mut taken = 0.;
        for _ in 0..(60 * 20) {
            let boss = world.bosses[index].pos;
            let tank = world.tanks.iter_mut().find(|t| t.id == id).unwrap();
            tank.pos = boss + V2::new(0., -190.);
            tank.vel = V2::ZERO;
            let before = tank.health;
            world.step();
            let tank = world.tanks.iter_mut().find(|t| t.id == id).unwrap();
            taken += (before - tank.health).max(0.);
            tank.health = tank.max_health;
        }
        assert!(
            taken / 20. < 40.,
            "took {taken} over 20 s at point-blank range"
        );
    }

    #[test]
    fn its_cannon_destroys_a_distant_tank_that_stands_still() {
        let mut world = world();
        let id = tank_at(&mut world, "Far", V2::new(3000., 1500.));
        let index = railgunner(&mut world, V2::new(1200., 1500.));
        world.bosses[index].aim = 0.;
        world.bosses[index].attack_timer = 0.;
        world.bosses[index].attack = 0;
        let mut hit = false;
        for _ in 0..240 {
            world.step();
            if !world.tanks.iter().find(|t| t.id == id).unwrap().alive {
                hit = true;
                break;
            }
        }
        assert!(hit, "the rail shot should have landed");
    }

    #[test]
    fn it_leads_a_target_moving_steadily_across_its_line() {
        let mut world = world();
        let id = tank_at(&mut world, "Runner", V2::new(2900., 1000.));
        let index = railgunner(&mut world, V2::new(1200., 1500.));
        world.bosses[index].attack = 0;
        world.bosses[index].attack_timer = 0.8;
        let moving = V2::new(0., 120.);
        let mut landed = false;
        for _ in 0..300 {
            let tank = world.tanks.iter_mut().find(|t| t.id == id).unwrap();
            tank.vel = moving;
            if !tank.alive {
                landed = true;
                break;
            }
            tank.input = PlayerInput::default();
            world.step();
        }
        assert!(landed, "a steady target should be hit by the lead");
    }

    #[test]
    fn the_beam_runs_then_the_vents_open_and_it_is_easier_to_hurt() {
        let mut world = world();
        tank_at(&mut world, "Far", V2::new(3000., 1500.));
        let index = railgunner(&mut world, V2::new(1200., 1500.));
        world.bosses[index].aim = 0.;
        world.bosses[index].attack = 1;
        world.bosses[index].attack_timer = 0.;
        let (mut beamed, mut vented, mut rounds_out) = (0, 0, 0);
        for _ in 0..(60 * 9) {
            keep_alive(&mut world);
            world.step();
            let boss = &world.bosses[index];
            beamed += i32::from(boss.beaming());
            vented += i32::from(boss.venting());
            assert_eq!(boss.damage_taken() > 1., boss.venting());
            rounds_out = rounds_out.max(
                world
                    .projectiles
                    .iter()
                    .filter(|p| p.owner == boss.id && is_beam_round(p.kind, p.vel))
                    .count(),
            );
        }
        let (beam, vent) = (BEAM_SECONDS[0], VENT_SECONDS[0]);
        assert!((beamed as f32 * DT - beam).abs() < 0.1, "{beamed}");
        assert!((vented as f32 * DT - vent).abs() < 0.1, "{vented}");
        assert!(rounds_out > 20, "a real stream: {rounds_out}");
    }

    #[test]
    fn the_stream_stays_out_of_the_event_log() {
        let mut world = world();
        tank_at(&mut world, "Far", V2::new(3000., 1500.));
        let index = railgunner(&mut world, V2::new(1200., 1500.));
        world.bosses[index].rail.beam = 2.;
        let id = world.bosses[index].id;
        for _ in 0..30 {
            keep_alive(&mut world);
            world.step();
            assert!(
                !world
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::Shot) && e.subject == id)
            );
        }
    }

    #[test]
    fn a_blocked_barrel_fires_from_where_it_is_clear_or_not_at_all() {
        let bounds = Rect {
            x: 0.,
            y: 0.,
            w: 1000.,
            h: 1000.,
        };
        let at = V2::new(500., 500.);
        let wall = Rect {
            x: 620.,
            y: 0.,
            w: 40.,
            h: 1000.,
        };
        let origin = muzzle(at, 0., 100. * RAIL_MUZZLE, 10., bounds, &[wall]).unwrap();
        assert!(origin.x > at.x && origin.x < 620. - 9.);
        let edge = muzzle(
            V2::new(900., 500.),
            0.,
            100. * RAIL_MUZZLE,
            10.,
            bounds,
            &[],
        )
        .unwrap();
        assert!(bounds.contains_circle(edge, 10.));
    }

    #[test]
    fn it_shows_up_rarely_and_never_for_a_lobby_of_beginners() {
        // The very bottom of the roll belongs to something rarer still.
        let rare = RAILGUNNER_ODDS * 0.5;
        assert_eq!(natural_boss(0., 0.5, 8.), BossKind::Sunderworm);
        assert_eq!(natural_boss(rare, 0.5, 8.), BossKind::Railgunner);
        assert_ne!(natural_boss(rare, 0.5, 0.9), BossKind::Railgunner);
        assert_ne!(natural_boss(RAILGUNNER_ODDS, 0.5, 8.), BossKind::Railgunner);
        for n in 0..=100 {
            let kind = natural_boss(0.9, n as f32 / 100., 8.);
            assert!(BossKind::COMMON.contains(&kind));
        }
        const { assert!(RAILGUNNER_ODDS < 0.02) };
    }

    #[test]
    fn only_its_own_size_reads_back_as_a_railgunner() {
        assert_eq!(
            BossKind::SiegeEngine.from_wire(RAIL_RADIUS),
            BossKind::Railgunner
        );
        assert_eq!(
            BossKind::SiegeEngine.from_wire(BossKind::SiegeEngine.radius()),
            BossKind::SiegeEngine
        );
        assert_eq!(BossKind::Prism.from_wire(RAIL_RADIUS), BossKind::Prism);
    }

    #[test]
    fn a_long_messy_fight_keeps_it_finite_and_in_bounds() {
        let mut world = world();
        let bounds = world.config.bounds();
        let ids: Vec<_> = (0..6)
            .map(|n| {
                tank_at(
                    &mut world,
                    &format!("T{n}"),
                    V2::new(600. + n as f32 * 420., 500. + n as f32 * 220.),
                )
            })
            .collect();
        let index = railgunner(&mut world, V2::new(1800., 1400.));
        world.bosses[index].attack_timer = 0.;
        for tick in 0..(60 * 60) {
            for (n, id) in ids.iter().enumerate() {
                let tank = world.tanks.iter_mut().find(|t| t.id == *id).unwrap();
                if !tank.alive {
                    tank.alive = true;
                    tank.health = tank.max_health;
                }
                tank.protection = 0.;
                let swirl = tick as f32 * 0.021 + n as f32;
                tank.vel = V2::from_angle(swirl) * 220.;
            }
            world.bosses[index].health =
                world.bosses[index].max_health * (1. - tick as f32 / 3700.).max(0.05);
            world.step();
            let boss = &world.bosses[index];
            assert!(boss.pos.x.is_finite() && boss.pos.y.is_finite());
            assert!(boss.aim.is_finite());
            assert!(bounds.contains_circle(boss.pos, boss.radius - 1.));
            assert!((0.0..=BossKind::Railgunner.windup()).contains(&boss.attack_windup));
            assert!((-BOSS_AIM_HOLD..=30.).contains(&boss.attack_timer));
            assert!(boss.attack <= 2);
        }
    }
}
