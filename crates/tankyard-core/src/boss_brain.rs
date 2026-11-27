//! How a boss makes up its mind: who to go after, which of its specials to
//! bring, and where a moving tank will be when a round gets there.
//!
//! Every tank a boss could fight is scored by a small two-layer network. The
//! first layer is the same for every boss and turns what it sees into seven
//! drives: hunger for a wounded tank close by, anger at whoever is hurting
//! it, greed for a crowd, how easily it can swing round, respect for a strong
//! tank at full health, a tank sitting still, and unease about piling onto a
//! tank other bosses already chase. The second layer is the boss's own
//! temperament: how much each drive counts for it.
//!
//! It then draws among the scores instead of always taking the best, so two
//! fights with the same boss don't play out the same way, and it sticks with
//! a choice for a while instead of flitting between two tanks that score
//! about the same. Every draw comes from the world's own random source, so a
//! recorded match replays exactly.
use crate::*;

/// How many attackers a boss keeps a grudge against at once.
const GRUDGES: usize = 6;
/// Seconds for a grudge to cool to about a third.
const COOLING: f32 = 7.;
/// How often a boss weighs its targets again, give or take.
const THINK: (f32, f32) = (0.75, 1.7);
/// How many drives the first layer has.
const DRIVES: usize = 7;
/// The sharpest a boss's acceleration read may be, in units a second
/// squared. A tank jinking every tick shouldn't send its aim into the walls.
const MOST_SWERVE: f32 = 900.;
/// How quickly that read follows the target.
const SWERVE_RATE: f32 = 5.;
/// The longest a lead looks ahead, in seconds.
const MOST_LEAD: f32 = 1.4;

/// One tank it has a grudge against.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Grudge {
    id: EntityId,
    /// The damage the arena's records held for this tank when last read.
    seen: f32,
    /// What it has done lately, cooling over time.
    heat: f32,
}

/// What a boss remembers between ticks. Only the host thinks, so this never
/// travels in a snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mind {
    pub(crate) gun: u8,
    pub(crate) salvo: Option<crate::encounters::Salvo>,
    jester_played: u64,
    pub(crate) trap_spiral: Option<crate::bosses::TrapSpiral>,
    grudges: [Grudge; GRUDGES],
    /// Last damage-record tick read, so an evicted old attacker cannot be
    /// reinserted as fresh damage every frame in a large fight.
    read_at: Option<u64>,
    /// Seconds until it weighs its targets again.
    think: f32,
    /// Seconds it has spent on its current target.
    focus: f32,
    /// The last tanks it let go of, and seconds since.
    past: [(EntityId, f32); 3],
    /// The specials it chose, newest first, as attack + 1 (0 for none yet).
    recent: [u8; 2],
    /// How its target's motion has been changing.
    pub(crate) swerve: V2,
    /// Its target's motion last tick.
    last: V2,
    /// Seconds since it last saw its target, and where that was.
    hidden: f32,
    seen_at: V2,
}

/// What one boss sees of one tank, each part from 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Read {
    pub id: EntityId,
    /// 1 against the hull, 0 at the edge of its sight.
    pub near: f32,
    /// How much this tank has hurt it lately.
    pub threat: f32,
    /// How hurt the tank is.
    pub weak: f32,
    /// 1 for the tank it is already after.
    pub focus: f32,
    /// How many other tanks stand close to this one.
    pub crowd: f32,
    /// Its level.
    pub strong: f32,
    /// 1 straight down the gun, 0 right behind.
    pub front: f32,
    /// How long since this boss last went after it.
    pub fresh: f32,
    /// How many other bosses are already after it.
    pub hunted: f32,
    /// How fast it is moving.
    pub quick: f32,
}

fn sigmoid(x: f32) -> f32 {
    1. / (1. + (-x).exp())
}

/// The first layer: what any boss feels about a tank.
fn drives(r: &Read) -> [f32; DRIVES] {
    [
        // Hunger: a wounded tank close by, most of all when it's both.
        sigmoid(2.6 * r.near + 3. * r.weak + 3. * r.near * r.weak - 3.4),
        // Anger: whoever is hurting it.
        sigmoid(7. * r.threat - 2.2),
        // Greed: a cluster it could hit at once.
        sigmoid(4. * r.crowd + 1.4 * r.near - 2.8),
        // Ease: in front of the gun, and not chased lately.
        sigmoid(2.4 * r.front + 2.2 * r.fresh - 2.4),
        // Respect: a strong tank at full health.
        sigmoid(4.2 * r.strong * (1. - r.weak) - 2.6),
        // A sitting target: slow, and near enough to reach.
        sigmoid(3.4 * (1. - r.quick) + 0.8 * r.near - 2.8),
        // Unease: other bosses are already on it.
        sigmoid(4. * r.hunted - 1.8),
    ]
}

/// The second layer, one per boss: how much each drive counts for it, how
/// sharply it prefers its best choice, and how much better another tank must
/// look before it lets go of the one it has.
struct Temperament {
    weights: [f32; DRIVES],
    sharpness: f32,
    loyalty: f32,
}

fn temperament(kind: BossKind) -> Temperament {
    use BossKind::*;
    //               hunger anger greed ease respect still unease
    let (weights, sharpness, loyalty) = match kind {
        // Rams and dashers go for whatever is weakest and closest.
        FallenBooster => ([2.6, 1.0, 0.2, 1.2, -0.4, 0.4, -0.6], 5., 0.5),
        // Picks off stragglers and leaves a crowd alone.
        Skirmisher => ([2.8, 0.6, -0.9, 0.8, -0.9, 0.4, -1.1], 5.5, 0.4),
        Tempest => ([1.6, 1.0, 1.1, 1.0, 0.0, 0.0, -0.6], 4., 0.5),
        // Gunners swing to what's in front and answer whoever stings them.
        Sentinel => ([1.2, 1.7, 0.7, 1.5, 0.2, 0.9, -0.6], 4., 0.6),
        Jester => ([1.4, 1.1, 1.5, 1.2, 0.4, 0.7, -0.9], 4.1, 0.4),
        Carillon => ([1.2, 1.8, 2.3, 0.8, 0.9, 1.1, -0.4], 4.4, 0.7),
        Prism | Eclipse => ([1.0, 1.3, 1.7, 0.7, 0.1, 0.3, -0.4], 3.6, 0.5),
        // Swarms go where the crowd is.
        Guardian | Summoner | FallenOverlord | Foundry => {
            ([0.8, 1.4, 2.0, 0.6, 0.0, 0.2, -0.3], 3.4, 0.5)
        }
        // Cannons wait for a strong tank to stand still, and commit to it.
        SiegeEngine => ([0.4, 1.0, 0.2, 1.4, 1.3, 2.3, -0.8], 4.6, 0.9),
        Bastion => ([0.6, 1.4, 0.8, 1.2, 0.7, 1.6, -0.7], 4.2, 0.8),
        Defender => ([0.8, 1.9, 1.2, 0.8, 0.3, 0.4, -0.5], 4., 0.6),
        Trapper => ([1.1, 1.4, 1.0, 1.0, 0.2, -0.4, -0.5], 4., 0.6),
        // It hunts by the shaking ground: whoever is close, hurting it, or
        // standing still long enough to be found.
        Sunderworm => ([2., 1.7, 1.3, 0.4, 0.6, 1.5, -0.2], 3., 0.6),
        // The Railgunner keeps its own sniper's choice.
        Railgunner => ([0.4, 1.0, 0.2, 1.4, 1.3, 2.3, -0.8], 4.6, 0.9),
    };
    Temperament {
        weights,
        sharpness,
        loyalty,
    }
}

/// How much a boss of `kind` in `phase` wants the tank it reads as `read`.
/// Wounded bosses grow hungrier and less patient.
pub(crate) fn appeal(kind: BossKind, phase: u8, read: &Read) -> f32 {
    let temper = temperament(kind);
    let drives = drives(read);
    let rage = f32::from(phase.clamp(1, 3) - 1) * 0.45;
    let mut score: f32 = drives
        .iter()
        .zip(temper.weights)
        .map(|(drive, weight)| drive * weight)
        .sum();
    score += drives[0] * rage;
    score + read.focus * temper.loyalty
}

/// Picks one of `reads` with odds by how much the boss wants each: a weighted
/// draw where `roll` is in [0, 1). None when there's nothing to choose.
pub(crate) fn draw(kind: BossKind, phase: u8, reads: &[Read], roll: f32) -> Option<EntityId> {
    let sharpness = temperament(kind).sharpness + f32::from(phase.clamp(1, 3) - 1) * 0.6;
    let scores = reads.iter().map(|read| appeal(kind, phase, read));
    let top = scores.clone().fold(f32::NEG_INFINITY, f32::max);
    if !top.is_finite() {
        return None;
    }
    let weights = scores.map(|score| ((score - top) * sharpness).exp());
    let total: f32 = weights.clone().sum();
    let mut left = roll.clamp(0., 0.999_999) * total;
    for (read, weight) in reads.iter().zip(weights) {
        if left < weight {
            return Some(read.id);
        }
        left -= weight;
    }
    reads.last().map(|read| read.id)
}

impl Mind {
    /// Reads the arena's damage records for this boss and lets old grudges
    /// cool. `records` is what each attacker has done to it in all.
    pub(crate) fn feel(&mut self, records: &[(EntityId, f32, u64)], dt: f32) {
        let cool = (-dt / COOLING).exp();
        for grudge in &mut self.grudges {
            grudge.heat *= cool;
        }
        let read_at = self.read_at;
        for &(id, total, tick) in records {
            if let Some(grudge) = self.grudges.iter_mut().find(|g| g.id == id) {
                // A record can be cleared and begun again; never count a drop.
                grudge.heat += (total - grudge.seen).max(0.);
                grudge.seen = total;
            } else if read_at.is_none_or(|before| tick > before)
                && let Some(slot) = self
                    .grudges
                    .iter_mut()
                    .min_by(|a, b| a.heat.total_cmp(&b.heat))
            {
                *slot = Grudge {
                    id,
                    seen: total,
                    heat: total,
                };
            }
        }
        self.read_at = records
            .iter()
            .map(|record| record.2)
            .chain(self.read_at)
            .max();
        for (_, since) in &mut self.past {
            *since += dt;
        }
        self.think -= dt;
        self.focus += dt;
        self.hidden += dt;
    }
    /// It can see its target, at `at`.
    pub(crate) fn saw(&mut self, at: V2) {
        self.hidden = 0.;
        self.seen_at = at;
    }
    /// Seconds since it last saw its target.
    pub(crate) fn hidden(&self) -> f32 {
        self.hidden
    }
    /// Where it last saw its target.
    pub(crate) fn seen_at(&self) -> V2 {
        self.seen_at
    }
    /// How much `id` has hurt it lately, against `scale`: 0 for nothing and
    /// toward 1 for a lot.
    pub(crate) fn threat(&self, id: EntityId, scale: f32) -> f32 {
        let heat = self
            .grudges
            .iter()
            .find(|g| g.id == id)
            .map_or(0., |g| g.heat);
        heat / (heat + scale.max(1.))
    }
    /// How long since it last went after `id`, from 0 for just now to 1 for
    /// a long while or never.
    pub(crate) fn fresh(&self, id: EntityId, current: EntityId) -> f32 {
        if id == current {
            return 0.;
        }
        self.past
            .iter()
            .find(|(past, _)| *past == id && id != 0)
            .map_or(1., |(_, since)| (since / 20.).min(1.))
    }
    /// Whether it's time to weigh its targets again.
    pub(crate) fn due(&self) -> bool {
        self.think <= 0.
    }
    /// It has chosen `id`; `roll` sets when it will think again.
    pub(crate) fn chose(&mut self, id: EntityId, current: EntityId, roll: f32) {
        self.think = THINK.0 + (THINK.1 - THINK.0) * roll.clamp(0., 1.);
        if id != current {
            if current != 0 {
                self.past.rotate_right(1);
                self.past[0] = (current, 0.);
            }
            self.focus = 0.;
            self.last = V2::ZERO;
            self.swerve = V2::ZERO;
        }
    }
    /// It lost its target and has to look again now.
    pub(crate) fn lost(&mut self) {
        self.think = 0.;
    }
    /// Follow how the target's motion changes, for leading curved paths.
    pub(crate) fn watch(&mut self, moving: V2, dt: f32) {
        if self.last != V2::ZERO || moving != V2::ZERO {
            let change = ((moving - self.last) / dt.max(1e-4)).clamp_length(MOST_SWERVE);
            self.swerve = self.swerve.lerp(change, 1. - (-dt * SWERVE_RATE).exp());
        }
        self.last = moving;
    }
    /// The special it brings next out of the three it has, from what each
    /// is good for and what it used last. `roll` is in [0, 1).
    pub(crate) fn pick_special(&mut self, kind: BossKind, scene: &Scene, roll: f32) -> u8 {
        // Every boss opens with its signature.
        if self.recent == [0, 0] {
            self.recent = [1, 0];
            if kind == BossKind::Jester {
                self.jester_played = 1;
            }
            return 0;
        }
        if kind == BossKind::Jester {
            let chosen = crate::encounters::choose_act(
                self.recent[0] - 1,
                &mut self.jester_played,
                scene,
                roll,
            );
            self.recent = [chosen + 1, self.recent[0]];
            return chosen;
        }
        let mut weights = [0_f32; 3];
        for (attack, weight) in weights.iter_mut().enumerate() {
            let fit = special_fit(kind, attack as u8);
            let mut score = fit.suits(scene);
            if self.recent[0] == attack as u8 + 1 {
                score -= 1.4;
            } else if self.recent[1] == attack as u8 + 1 {
                score -= 0.5;
            }
            *weight = (score * 2.4).exp();
        }
        let total: f32 = weights.iter().sum();
        let mut left = roll.clamp(0., 0.999_999) * total;
        let mut chosen = 2;
        for (attack, weight) in weights.iter().enumerate() {
            if left < *weight {
                chosen = attack as u8;
                break;
            }
            left -= weight;
        }
        self.recent = [chosen + 1, self.recent[0]];
        chosen
    }
}

/// What a boss can see of the fight when it chooses a special.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Scene {
    /// How far its target is.
    pub range: f32,
    /// How many hostile tanks stand within reach of it.
    pub foes: f32,
    /// How fast its target moves across its line of fire, units a second.
    pub across: f32,
    /// How hurt it is, from 0 to 1.
    pub hurt: f32,
}

/// What one special is good for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fit {
    /// The range it works best at.
    pub range: (f32, f32),
    /// How much it is worth against a crowd instead of one tank.
    pub crowd: f32,
    /// How well it catches a tank on the move.
    pub moving: f32,
    /// How much more it is worth to a badly hurt boss.
    pub desperate: f32,
}

impl Fit {
    pub(crate) fn suits(&self, scene: &Scene) -> f32 {
        let (near, far) = self.range;
        let outside = if scene.range < near {
            (near - scene.range) / near.max(1.)
        } else if scene.range > far {
            (scene.range - far) / far.max(1.)
        } else {
            0.
        };
        let crowd = ((scene.foes - 1.).max(0.) / 3.).min(1.);
        let moving = (scene.across / 260.).min(1.);
        1. - outside.min(1.5) + crowd * self.crowd + moving * self.moving
            - (1. - moving) * self.moving * 0.3
            + scene.hurt * self.desperate
    }
}

const fn fit(range: (f32, f32), crowd: f32, moving: f32, desperate: f32) -> Fit {
    Fit {
        range,
        crowd,
        moving,
        desperate,
    }
}

/// What each of a boss's three specials is good for. These match the
/// patterns in `special_volley`.
pub(crate) fn special_fit(kind: BossKind, attack: u8) -> Fit {
    use BossKind::*;
    match (kind, attack.min(2)) {
        (SiegeEngine, 0) => fit((450., 1400.), 0., -0.6, 0.),
        (SiegeEngine, 1) => fit((300., 900.), 0.5, 0.5, 0.3),
        (SiegeEngine, _) => fit((250., 800.), 0.8, 0.2, 0.8),
        (Bastion, 0) => fit((350., 1100.), 0., -0.3, 0.),
        (Bastion, 1) => fit((200., 700.), 0.9, 0.2, 0.3),
        (Bastion, _) => fit((250., 900.), 0.4, 0.6, 0.6),
        (Skirmisher, 0) => fit((200., 650.), 0., 0.6, 0.),
        (Skirmisher, 1) => fit((150., 500.), 0.4, 0.2, 0.4),
        (Skirmisher, _) => fit((250., 800.), 0.2, 0.8, 0.6),
        (Trapper, 0) => fit((250., 750.), 0.3, 0.6, 0.),
        (Trapper, 1) => fit((150., 550.), 0.8, 0.2, 0.3),
        (Trapper, _) => fit((300., 900.), 0.2, 0.9, 0.6),
        (Guardian | Summoner | FallenOverlord, 0) => fit((250., 800.), 0.5, 0.3, 0.),
        (Guardian | Summoner | FallenOverlord, 1) => fit((150., 600.), 1., 0., 0.4),
        (Guardian | Summoner | FallenOverlord, _) => fit((300., 900.), 0.2, 0.8, 0.6),
        (Defender, 0) => fit((150., 600.), 1., 0., 0.),
        (Defender, 1) => fit((300., 900.), 0., 0.4, 0.3),
        (Defender, _) => fit((200., 700.), 0.6, 0.5, 0.7),
        (FallenBooster | Tempest, 0) => fit((250., 800.), 0.2, 0.9, 0.),
        (FallenBooster | Tempest, 1) => fit((100., 500.), 0.7, 0.3, 0.4),
        (FallenBooster | Tempest, _) => fit((300., 900.), 0.4, 0.7, 0.7),
        (Sentinel, 0) => fit((300., 1000.), 0.3, 0.2, 0.),
        (Sentinel, 1) => fit((400., 1200.), 0., -0.2, 0.2),
        (Sentinel, _) => fit((200., 800.), 0.7, 0.6, 0.6),
        (Prism | Eclipse, 0) => fit((150., 700.), 1., 0.1, 0.),
        (Prism | Eclipse, 1) => fit((250., 900.), 0.3, 0.9, 0.3),
        (Prism | Eclipse, _) => fit((150., 700.), 0.8, 0.4, 0.8),
        (Foundry, 0) => fit((250., 800.), 0.5, 0.3, 0.),
        (Foundry, 1) => fit((150., 600.), 1., 0., 0.3),
        (Foundry, _) => fit((300., 900.), 0.3, 0.6, 0.7),
        (Carillon, 0) => fit((440., 1450.), 0.2, -0.4, 0.),
        (Carillon, 1) => fit((200., 920.), 0.9, 0.8, 0.3),
        (Carillon, _) => fit((80., 620.), 1., 0.4, 0.8),
        (Jester, 0) => fit((300., 1000.), 0.6, 0.7, 0.),
        (Jester, 1) => fit((120., 700.), 1., 0.4, 0.3),
        (Jester, _) => fit((120., 560.), 0.5, 0.2, 0.8),
        (Railgunner | Sunderworm, _) => fit((0., 2000.), 0., 0., 0.),
    }
}

/// Seconds until a round at `speed` meets a target `gap` away moving at
/// `moving`, if it ever can.
pub(crate) fn intercept_time(gap: V2, moving: V2, speed: f32) -> Option<f32> {
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

/// Where a target at `at`, moving at `moving` and turning by `swerve`, will
/// be when a round at `speed` fired `warned` seconds from now reaches it.
/// The turn counts for half, and less the further ahead it looks: people
/// rarely keep a turn up for long.
pub(crate) fn curved_lead(from: V2, at: V2, moving: V2, swerve: V2, speed: f32, warned: f32) -> V2 {
    let bend = |t: f32| {
        let kept = t.min(0.8);
        moving * t + swerve * (0.25 * kept * kept)
    };
    let later = at + bend(warned);
    let mut flight = intercept_time(later - from, moving, speed)
        .unwrap_or(0.)
        .min(MOST_LEAD);
    // A few rounds of refining settle on the curve.
    for _ in 0..3 {
        let point = at + bend(warned + flight);
        flight = (point.distance(from) / speed.max(1.)).min(MOST_LEAD);
    }
    at + bend(warned + flight)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(id: EntityId) -> Read {
        Read {
            id,
            near: 0.5,
            threat: 0.,
            weak: 0.,
            focus: 0.,
            crowd: 0.,
            strong: 0.5,
            front: 0.5,
            fresh: 1.,
            hunted: 0.,
            quick: 0.5,
        }
    }

    #[test]
    fn a_wounded_tank_close_by_draws_a_hunter_and_a_crowd_draws_a_swarm() {
        let wounded = Read {
            near: 0.9,
            weak: 0.8,
            ..read(1)
        };
        let crowd = Read {
            crowd: 1.,
            ..read(2)
        };
        let booster = |r: &Read| appeal(BossKind::FallenBooster, 1, r);
        let summoner = |r: &Read| appeal(BossKind::Summoner, 1, r);
        assert!(booster(&wounded) > booster(&crowd));
        assert!(summoner(&crowd) > summoner(&read(3)));
        // A cannon wants a tank standing still.
        let still = Read {
            quick: 0.,
            ..read(4)
        };
        let darting = Read {
            quick: 1.,
            ..read(5)
        };
        assert!(
            appeal(BossKind::SiegeEngine, 1, &still) > appeal(BossKind::SiegeEngine, 1, &darting)
        );
        // Whoever is hurting it gets its attention.
        let attacker = Read {
            threat: 0.8,
            ..read(6)
        };
        assert!(appeal(BossKind::Defender, 1, &attacker) > appeal(BossKind::Defender, 1, &read(7)));
    }

    #[test]
    fn draws_vary_but_lean_toward_the_best_choice() {
        let reads = [
            Read {
                near: 0.95,
                weak: 0.7,
                ..read(1)
            },
            read(2),
            read(3),
        ];
        let mut counts = [0; 3];
        for step in 0..1000 {
            let roll = step as f32 / 1000.;
            let id = draw(BossKind::Sentinel, 1, &reads, roll).unwrap();
            counts[id as usize - 1] += 1;
        }
        assert!(counts[0] > counts[1] && counts[0] > counts[2], "{counts:?}");
        assert!(counts[1] > 0 && counts[2] > 0, "never varies: {counts:?}");
        assert_eq!(draw(BossKind::Sentinel, 1, &[], 0.5), None);
    }

    #[test]
    fn grudges_build_from_damage_and_cool_off() {
        let mut mind = Mind::default();
        mind.feel(&[(9, 400., 1)], DT);
        assert!(mind.threat(9, 400.) > 0.45);
        // Nothing new: it cools.
        let hot = mind.threat(9, 400.);
        for _ in 0..600 {
            mind.feel(&[(9, 400., 1)], DT);
        }
        assert!(mind.threat(9, 400.) < hot * 0.6);
        // More damage stokes it again; a cleared record doesn't count back.
        mind.feel(&[(9, 1400., 2)], DT);
        assert!(mind.threat(9, 400.) > hot);
        let after = mind.threat(9, 400.);
        mind.feel(&[(9, 10., 3)], DT);
        assert!(mind.threat(9, 400.) <= after);
        assert_eq!(mind.threat(77, 400.), 0.);
    }

    #[test]
    fn old_records_do_not_keep_reheating_evicted_attackers() {
        let mut mind = Mind::default();
        let records: Vec<_> = (1..=12).map(|id| (id, id as f32 * 100., 1)).collect();
        mind.feel(&records, DT);
        let before: f32 = mind.grudges.iter().map(|g| g.heat).sum();
        for _ in 0..600 {
            mind.feel(&records, DT);
        }
        let after: f32 = mind.grudges.iter().map(|g| g.heat).sum();
        assert!(
            after < before * 0.3,
            "stale records kept reheating: {before} to {after}"
        );
        mind.feel(&[(100, 3000., 2)], DT);
        assert!(mind.threat(100, 100.) > 0.9);
    }

    #[test]
    fn it_thinks_again_now_and_then_and_remembers_who_it_let_go() {
        let mut mind = Mind::default();
        assert!(mind.due());
        mind.chose(4, 0, 0.5);
        assert!(!mind.due());
        mind.chose(5, 4, 0.5);
        assert_eq!(mind.fresh(4, 5), 0.);
        for _ in 0..(25. / DT) as usize {
            mind.feel(&[], DT);
        }
        assert!(mind.due());
        assert_eq!(mind.fresh(4, 5), 1.);
        assert_eq!(mind.fresh(5, 5), 0.);
        mind.lost();
        assert!(mind.due());
    }

    #[test]
    fn specials_suit_the_fight_and_rarely_repeat() {
        let mut mind = Mind::default();
        let crowd = Scene {
            range: 300.,
            foes: 4.,
            across: 50.,
            hurt: 0.,
        };
        let mut uses = [0; 3];
        let mut repeats = 0;
        let mut last = 9;
        for step in 0..300 {
            let attack =
                mind.pick_special(BossKind::Defender, &crowd, (step * 37 % 300) as f32 / 300.);
            uses[usize::from(attack)] += 1;
            repeats += usize::from(attack == last);
            last = attack;
        }
        // Its ring is the thing for a crowd, but it mixes the others in.
        assert!(uses[0] > uses[1], "{uses:?}");
        assert!(uses.iter().all(|&n| n > 0), "{uses:?}");
        assert!(repeats < 60, "{repeats} repeats");
    }

    #[test]
    fn a_curved_lead_meets_a_turning_target_closer_than_a_straight_one() {
        // A target circling the gun: its true path bends.
        let from = V2::new(0., 0.);
        let speed = 500.;
        let radius = 400.;
        let spin = 0.9_f32;
        let at = V2::new(radius, 0.);
        let moving = V2::new(0., radius * spin);
        let swerve = V2::new(-radius * spin * spin, 0.);
        let truth = |t: f32| V2::new((spin * t).cos(), (spin * t).sin()) * radius;
        let miss = |point: V2| {
            let flight = point.distance(from) / speed;
            point.distance(truth(flight))
        };
        let straight = curved_lead(from, at, moving, V2::ZERO, speed, 0.);
        let curved = curved_lead(from, at, moving, swerve, speed, 0.);
        assert!(
            miss(curved) < miss(straight),
            "{} vs {}",
            miss(curved),
            miss(straight)
        );
        // Standing still, it aims right at the target.
        assert!(curved_lead(from, at, V2::ZERO, V2::ZERO, speed, 0.4).distance(at) < 0.01);
    }

    #[test]
    fn a_read_of_the_targets_turning_is_bounded() {
        let mut mind = Mind::default();
        mind.watch(V2::new(0., 0.), DT);
        for n in 0..120 {
            let sign = if n % 2 == 0 { 1. } else { -1. };
            mind.watch(V2::new(400. * sign, 0.), DT);
            assert!(mind.swerve.length() <= MOST_SWERVE + 0.01);
        }
    }
}
