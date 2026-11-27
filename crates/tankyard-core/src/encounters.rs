//! The Jester's repertoire and the Carillon's casting patterns. Warnings and
//! live rounds use the same geometry, including when a Jester borrows it.
use crate::{
    Boss, BossKind, BossRound,
    boss_brain::{Scene, special_fit},
    bosses::round,
};
use serde::{Serializer, ser::SerializeSeq};
use std::f32::consts::{PI, TAU};
use tankyard_content::ProjectileKind::*;

pub const CARD_MUZZLE: f32 = 1.43;
pub const JESTER_RADIUS: f32 = 116.;
pub const CARILLON_RADIUS: f32 = 148.;
pub const JESTER_ODDS: f32 = 1. / 2_000.;
pub const CARILLON_ODDS: f32 = 1. / 48_000.;

use crate::creator_boss::{ATTACK_COUNT, Attack};
use BossKind::*;

/// One mask uses all three published specials of its source. The last twelve
/// cards use Axiom's combat actions, including its dash and repair.
#[derive(Clone, Copy)]
struct Act {
    source: BossKind,
    attack: u8,
    proxy: (BossKind, u8),
    wire_radius: f32,
}
const SOURCES: [BossKind; 16] = [
    Sentinel,
    Prism,
    Foundry,
    Tempest,
    Bastion,
    Eclipse,
    Guardian,
    Summoner,
    Defender,
    FallenOverlord,
    FallenBooster,
    SiegeEngine,
    Skirmisher,
    Trapper,
    Railgunner,
    Carillon,
];
const AXIOM_START: usize = 3 + SOURCES.len() * 3;
const ACTS: [Act; AXIOM_START + ATTACK_COUNT] = repertoire();

// Protocol 35 already distinguishes the Railgunner by hull size. Four reserved
// legacy silhouettes distinguish the mask families without changing field
// meanings or rounding a timer. New clients restore the physical radius.
const fn repertoire() -> [Act; AXIOM_START + ATTACK_COUNT] {
    let base = Act {
        source: Jester,
        attack: 0,
        proxy: (Prism, 1),
        wire_radius: JESTER_RADIUS + 3.,
    };
    let mut acts = [base; AXIOM_START + ATTACK_COUNT];
    acts[1] = Act {
        attack: 1,
        proxy: (Eclipse, 2),
        ..base
    };
    acts[2] = Act {
        attack: 2,
        proxy: (Trapper, 1),
        ..base
    };
    let mut i = 3;
    while i < AXIOM_START {
        let source = SOURCES[(i - 3) / 3];
        let attack = ((i - 3) % 3) as u8;
        let (proxy, wire_radius) = match source {
            Railgunner => (SiegeEngine, JESTER_RADIUS + 1.),
            Carillon => (SiegeEngine, JESTER_RADIUS + 2.),
            _ => (source, JESTER_RADIUS),
        };
        acts[i] = Act {
            source,
            attack,
            proxy: (proxy, attack),
            wire_radius,
        };
        i += 1;
    }
    // Visually related published attacks for clients predating these masks.
    let proxy = [
        (Sentinel, 0),
        (Bastion, 1),
        (Eclipse, 0),
        (Tempest, 1),
        (SiegeEngine, 1),
        (Sentinel, 1),
        (Defender, 0),
        (Defender, 2),
        (Foundry, 0),
        (Tempest, 2),
        (Skirmisher, 0),
        (Foundry, 2),
    ];
    let mut i = 0;
    while i < ATTACK_COUNT {
        acts[AXIOM_START + i] = Act {
            attack: 3 + i as u8,
            proxy: proxy[i],
            ..base
        };
        i += 1;
    }
    acts
}
pub fn jester_act(code: u8) -> Option<(BossKind, u8)> {
    ACTS.get(usize::from(code)).map(|a| (a.source, a.attack))
}
pub fn axiom_act(code: u8) -> Option<Attack> {
    let index = usize::from(code).checked_sub(AXIOM_START)?;
    Attack::ALL.get(index).copied()
}

impl BossKind {
    /// A copied heavy round keeps its parry and impact even after its owner
    /// changes masks. The gun’s bore survives velocity changes and stasis.
    pub fn cannon_source(
        self,
        kind: tankyard_content::ProjectileKind,
        radius: f32,
        velocity: crate::V2,
    ) -> Self {
        if self == Jester && crate::is_beam_round(kind, velocity) {
            return Railgunner;
        }
        if kind != Shell {
            return self;
        }
        match self {
            Jester
                if radius == crate::RAIL_ROUND_RADIUS
                    || radius == crate::railgunner::RAIL_ECHO_RADIUS =>
            {
                Railgunner
            }
            Jester if radius == crate::SIEGE_ROUND_RADIUS => SiegeEngine,
            Jester if radius == crate::BASTION_ROUND_RADIUS => Bastion,
            Jester | Carillon if radius == PEAL_RADIUS => Bastion,
            _ => self,
        }
    }
}
const PEAL_RADIUS: f32 = 20.;

impl Boss {
    pub fn attack_style(&self) -> (BossKind, u8) {
        if self.kind == Jester {
            jester_act(self.attack).unwrap_or((Jester, 0))
        } else {
            (self.kind, self.attack)
        }
    }
    pub fn warning_seconds(&self) -> f32 {
        self.attack_style().0.windup()
    }

    /// Inverse of the snapshot projection below. Reject unmapped combinations
    /// instead of guessing the attack a player should be warned about.
    pub fn restore_encounter(&mut self) -> bool {
        if (JESTER_RADIUS..=JESTER_RADIUS + 3.).contains(&self.radius) {
            let Some(slot) = ACTS
                .iter()
                .position(|a| a.wire_radius == self.radius && a.proxy == (self.kind, self.attack))
            else {
                return false;
            };
            self.radius = JESTER_RADIUS;
            self.kind = Jester;
            self.attack = slot as u8;
        } else if self.kind == SiegeEngine && self.radius == CARILLON_RADIUS {
            self.kind = Carillon;
        } else {
            self.kind = self.kind.from_wire(self.radius);
        }
        true
    }
}

pub(crate) fn serialize_bosses<S: Serializer>(
    bosses: &[Boss],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut seq = serializer.serialize_seq(Some(bosses.len()))?;
    for boss in bosses {
        let proxy = match boss.kind {
            Jester => {
                let act = ACTS
                    .get(usize::from(boss.attack))
                    .ok_or_else(|| serde::ser::Error::custom("unknown Jester act"))?;
                Some((act.proxy.0, act.proxy.1, act.wire_radius))
            }
            Carillon => Some((SiegeEngine, boss.attack, boss.radius)),
            _ => None,
        };
        if let Some((kind, attack, radius)) = proxy {
            let mut view = boss.for_viewer();
            view.kind = kind;
            view.attack = attack;
            view.radius = radius;
            seq.serialize_element(&view)?;
        } else {
            seq.serialize_element(boss)?;
        }
    }
    seq.end()
}

/// Deal one complete repertoire before reshuffling. Distance, movement and
/// nearby opponents weight the remaining acts. Even across the shuffle, the
/// audience never sees the same act twice in a row.
pub(crate) fn choose_act(previous: u8, played: &mut u64, scene: &Scene, roll: f32) -> u8 {
    const FULL_DECK: u64 = u64::MAX >> (64 - ACTS.len());
    if *played == FULL_DECK {
        *played = 0;
    }
    let unavailable = *played | (1u64 << previous);
    let mut weights = [0.; ACTS.len()];
    let mut chosen = 0;
    for (index, act) in ACTS.iter().enumerate() {
        if unavailable & (1u64 << index) == 0 {
            let fit = special_fit(act.source, act.attack).suits(scene);
            let own = if index < 3 {
                6.
            } else if index >= AXIOM_START {
                0.7
            } else {
                1.
            };
            weights[index] = own * (fit.clamp(-1., 2.) * 1.4).exp();
            chosen = index;
        }
    }
    let mut left = roll.clamp(0., 0.999_999) * weights.iter().sum::<f32>();
    for (index, weight) in weights.into_iter().enumerate() {
        if left < weight {
            chosen = index;
            break;
        }
        left -= weight;
    }
    *played |= 1u64 << chosen;
    chosen as u8
}

/// A three-beat release on the bearing locked in during the windup. The
/// middle gun coaches the dodge, then the port and starboard guns answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Battery {
    Cards,
    Hammers,
}
impl Battery {
    pub(crate) fn for_attack(style: BossKind, slot: u8) -> Option<Self> {
        match (style, slot) {
            (Jester, 0) => Some(Self::Cards),
            (Carillon, 0) => Some(Self::Hammers),
            _ => None,
        }
    }
    fn interval(self) -> u8 {
        match self {
            Self::Cards => 8,
            Self::Hammers => 12,
        }
    }
    fn beat(self, aim: f32, index: u8, mut fire: impl FnMut(BossRound)) {
        let side = [0., -1., 1.][usize::from(index)];
        match self {
            Self::Cards => {
                let angle = aim + side * 0.38;
                for card in 0..3 {
                    fire(round(
                        angle + (card as f32 - 1.) * 0.055,
                        Piercing,
                        665. + 155. * card as f32,
                        38.,
                        9.,
                        2.2,
                        3.,
                        1,
                    ));
                }
            }
            Self::Hammers => {
                let angle = aim + side * 0.25;
                fire(round(angle, Shell, 1430., 295., PEAL_RADIUS, 1.75, 11., 0));
                fire(round(angle, Explosive, 815., 88., 14., 2.45, 4., 0));
            }
        }
    }
    fn preview(self, aim: f32, shots: &mut Vec<BossRound>) {
        for beat in 0..3 {
            self.beat(aim, beat, |shot| shots.push(shot));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Salvo {
    battery: Battery,
    aim: f32,
    tick: u8,
}
impl Salvo {
    pub(crate) fn new(battery: Battery, aim: f32) -> Self {
        Self {
            battery,
            aim,
            tick: 0,
        }
    }
    /// The returned beat is emitted through the same wall and arena checks as
    /// other shots. A boss killed between beats never releases the remainder.
    pub(crate) fn advance(current: &mut Option<Self>) -> Option<Self> {
        let salvo = current.as_mut()?;
        let beat = *salvo;
        salvo.tick += 1;
        let interval = beat.battery.interval();
        if !beat.tick.is_multiple_of(interval) {
            return None;
        }
        if beat.tick == 2 * interval {
            *current = None;
        }
        Some(beat)
    }
    pub(crate) fn fire(self, emit: impl FnMut(BossRound)) {
        self.battery
            .beat(self.aim, self.tick / self.battery.interval(), emit);
    }
}

/// Three-card trick, curtain call and jack-in-the-box. The outer cards
/// ricochet, the curtain has two exits, and the box closes from the sides.
pub(crate) fn jester_volley(attack: u8, aim: f32, phase: u8) -> Vec<BossRound> {
    let Some((source, slot)) = jester_act(attack) else {
        return Vec::new();
    };
    if source != Jester {
        return crate::special_volley(source, slot, aim, phase);
    }
    let mut shots = Vec::with_capacity(24);
    match slot {
        0 => Battery::Cards.preview(aim, &mut shots),
        1 => {
            let spokes = 14 + 2 * phase.clamp(1, 3);
            for i in 0..spokes {
                if i % (spokes / 2) != 0 {
                    shots.push(round(
                        aim + TAU * f32::from(i) / f32::from(spokes),
                        Fragment,
                        if i % 2 == 0 { 590. } else { 360. },
                        35.,
                        11.,
                        2.6,
                        2.,
                        0,
                    ));
                }
            }
        }
        2 => {
            for side in [-1., 1.] {
                for j in 0..4 {
                    shots.push(round(
                        aim + side * (0.38 + 0.22 * j as f32),
                        Mine,
                        760. + 80. * j as f32,
                        44.,
                        13.,
                        5.,
                        3.,
                        0,
                    ));
                }
            }
            shots.push(round(aim, Explosive, 700., 62., 15., 2.3, 3., 0));
        }
        _ => {
            if let Some(attack) = axiom_act(attack) {
                let (count, speed, damage, radius, life, kind) = attack.stats();
                for i in 0..count {
                    shots.push(round(
                        attack.shot_angle(aim, i, count),
                        kind,
                        speed,
                        damage,
                        radius,
                        life,
                        attack.penetration(),
                        0,
                    ));
                }
            }
        }
    }
    shots
}

/// A hammer blow down three lanes, a spill from the two ladles, and a
/// casting bed of mines around the furnace with an open service aisle.
pub(crate) fn carillon_volley(attack: u8, aim: f32, phase: u8) -> Vec<BossRound> {
    let mut shots = Vec::with_capacity(32);
    match attack {
        0 => Battery::Hammers.preview(aim, &mut shots),
        1 => {
            for side in [-1., 1.] {
                for i in 0..(4 + phase.clamp(1, 3)) {
                    shots.push(round(
                        aim + side * (0.12 + 0.19 * f32::from(i)),
                        Piercing,
                        710. + 48. * f32::from(i),
                        68.,
                        13.,
                        2.8,
                        5.,
                        1,
                    ));
                }
            }
        }
        _ => {
            let teeth = 12 + 2 * phase.clamp(1, 3);
            for i in 1..teeth {
                let angle = aim + TAU * f32::from(i) / f32::from(teeth);
                shots.push(round(angle, Mine, 830., 67., 15., 5., 3.5, 0));
            }
            for side in [-1., 1.] {
                shots.push(round(
                    aim + side * PI * 0.33,
                    Drone,
                    490.,
                    42.,
                    18.,
                    7.,
                    7.,
                    0,
                ));
            }
        }
    }
    shots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repertoire_keeps_the_original_patterns_and_leaves_the_worm_alone() {
        for kind in BossKind::ALL {
            assert!(ACTS.iter().any(|act| act.source == kind), "{kind:?}");
        }
        assert!(ACTS.iter().all(|a| a.source != Sunderworm));
        for (code, act) in ACTS.iter().enumerate() {
            assert_eq!(
                ACTS.iter().filter(|other| other.proxy == act.proxy && other.wire_radius == act.wire_radius).count(),
                1
            );
            for phase in 1..=3 {
                let shots = jester_volley(code as u8, 0.57, phase);
                assert!(
                    (act.source != Jester || shots.len() <= 32)
                        && (!shots.is_empty()
                            || (act.source == Railgunner && act.attack == 2)
                            || axiom_act(code as u8).is_some_and(|a| a.stats().0 == 0)),
                    "act {code} phase {phase}"
                );
                if act.source != Jester {
                    assert_eq!(
                        shots,
                        crate::special_volley(act.source, act.attack, 0.57, phase)
                    );
                }
            }
        }
        for source in SOURCES {
            for slot in 0..3 {
                assert_eq!(
                    ACTS.iter()
                        .filter(|a| (a.source, a.attack) == (source, slot))
                        .count(),
                    1
                );
            }
        }
        assert_eq!(
            (0..ACTS.len())
                .filter_map(|i| axiom_act(i as u8))
                .collect::<Vec<_>>(),
            Attack::ALL
        );
        let scene = Scene::default();
        let mut drawn = [false; ACTS.len()];
        for n in 0..65_536 {
            let act = choose_act(0, &mut 1, &scene, n as f32 / 65_536.);
            assert_ne!(act, 0);
            drawn[usize::from(act)] = true;
        }
        assert!(drawn[1..].iter().all(|seen| *seen));
    }

    #[test]
    fn jester_deals_whole_decks_without_repeating_across_the_shuffle() {
        let mut mind = crate::boss_brain::Mind::default();
        let mut previous = None;
        for _ in 0..3 {
            let mut seen = [false; ACTS.len()];
            for n in 0..ACTS.len() {
                let next = mind.pick_special(Jester, &Scene::default(), (n % 7) as f32 / 6.);
                assert_ne!(previous, Some(next));
                assert!(!std::mem::replace(&mut seen[usize::from(next)], true));
                previous = Some(next);
            }
        }
    }

    #[test]
    fn all_three_beats_follow_the_warning_and_finish_on_schedule() {
        for (kind, interval) in [(Jester, 8), (Carillon, 12)] {
            let battery = Battery::for_attack(kind, 0).unwrap();
            let mut salvo = Some(Salvo::new(battery, 1.23));
            let mut fired = Vec::new();
            let mut ticks = Vec::new();
            for tick in 0..90 {
                if let Some(beat) = Salvo::advance(&mut salvo) {
                    ticks.push(tick);
                    beat.fire(|r| fired.push(r));
                }
            }
            assert_eq!(fired, crate::special_volley(kind, 0, 1.23, 2));
            assert_eq!(ticks, [0, interval, 2 * interval]);
            assert!(salvo.is_none());
        }
    }

    #[test]
    fn rare_bands_have_distinct_entrances_and_strength_requirements() {
        use crate::{railgunner::natural_boss, sunderworm::WORM_ODDS};
        let choose = |roll, power| natural_boss(roll, 0.5, power);
        assert_eq!(choose(WORM_ODDS * 0.5, 4.), Sunderworm);
        assert_eq!(choose(WORM_ODDS, 3.), Carillon);
        assert_ne!(choose(WORM_ODDS, 2.99), Carillon);
        let jester_start = WORM_ODDS + CARILLON_ODDS;
        assert_eq!(choose(jester_start, 2.5), Jester);
        assert_ne!(choose(jester_start, 2.49), Jester);
        assert_eq!(choose(jester_start + JESTER_ODDS, 4.), Railgunner);
        assert!(
            Sunderworm.toughness() > Carillon.toughness()
                && Carillon.toughness() > Railgunner.toughness()
        );
        assert!(
            BossKind::EVERY
                .into_iter()
                .enumerate()
                .all(|(index, kind)| kind as usize == index)
        );
        assert_eq!(Sunderworm as u8 + 1, 16); // Defeated triggers in published maps.
    }
}
