use crate::{
    BASTION_ROUND_DAMAGE, BASTION_ROUND_RADIUS, BASTION_ROUND_SPEED, BossKind, BossRound,
    bosses::round,
};
use std::f32::consts::PI;
use tankyard_content::ProjectileKind::*;

/// Each half of the steam wave leaves this much room on either side of its axis.
pub const BLOWDOWN_OPENING: f32 = 0.46;
pub const BLOWDOWN_SPEEDS: [f32; 2] = [680., 480.];
pub const BLOWDOWN_INTERVAL: u8 = 24;

/// The later specials of Carillon and Bastion. The first three keep their
/// existing slot numbers because both snapshots and Jester masks use them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeavyAttack {
    Bellfall,
    Blowdown,
    SlagSeekers,
    Crossfire,
    ShieldWall,
}

impl HeavyAttack {
    pub const fn for_slot(kind: BossKind, slot: u8) -> Option<Self> {
        use BossKind::{Bastion, Carillon};
        match (kind, slot) {
            (Carillon, 3) => Some(Self::Bellfall),
            (Carillon, 4) => Some(Self::Blowdown),
            (Carillon, 5) => Some(Self::SlagSeekers),
            (Bastion, 3) => Some(Self::Crossfire),
            (Bastion, 4) => Some(Self::ShieldWall),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Bellfall => "Bellfall",
            Self::Blowdown => "Blowdown",
            Self::SlagSeekers => "Slag seekers",
            Self::Crossfire => "Crossfire",
            Self::ShieldWall => "Shield wall",
        }
    }

    /// The shot the boss leads with as it sets its bearing for this pattern.
    pub(crate) fn lead_speed(self) -> f32 {
        match self {
            Self::Bellfall => 900.,
            Self::Blowdown => BLOWDOWN_SPEEDS[0],
            Self::SlagSeekers => 605.,
            Self::Crossfire => BASTION_ROUND_SPEED,
            Self::ShieldWall => 470.,
        }
    }

    pub(super) fn beats(self) -> u8 {
        match self {
            Self::Bellfall | Self::Crossfire => 3,
            _ => 2,
        }
    }

    pub(super) fn interval(self) -> u8 {
        match self {
            Self::Bellfall => 11,
            Self::Blowdown => BLOWDOWN_INTERVAL,
            Self::SlagSeekers => 18,
            Self::Crossfire => 14,
            Self::ShieldWall => 19,
        }
    }

    pub(super) fn fire(self, aim: f32, phase: u8, beat: u8, mut emit: impl FnMut(BossRound)) {
        let phase = phase.clamp(1, 3);
        let stage = f32::from(phase - 1);
        match self {
            Self::Bellfall => {
                // Each pair closes in on the bearing marked during the windup.
                let offset = [0.56, 0.32, 0.10][usize::from(beat)];
                for side in [-1., 1.] {
                    emit(round(
                        aim + side * offset,
                        Piercing,
                        900. + 125. * f32::from(beat),
                        53. + 4. * stage,
                        12.,
                        1.8,
                        4.,
                        1,
                    ));
                }
                if beat == 2 {
                    emit(round(aim, Explosive, 780., 72., 16., 2.3, 3., 0));
                }
            }
            Self::Blowdown => {
                // A fast steam front and a slower second front. Both leave the
                // same two exits even at low health.
                let count = u32::from(7 + phase);
                let span = PI - BLOWDOWN_OPENING * 2.;
                let speed = BLOWDOWN_SPEEDS[usize::from(beat)];
                let (damage, size) = if beat == 0 { (42., 13.) } else { (49., 17.) };
                for half in [0., PI] {
                    for vent in 0..count {
                        let offset = BLOWDOWN_OPENING + span * vent as f32 / (count - 1) as f32;
                        emit(round(
                            aim + half + offset,
                            Scatter,
                            speed,
                            damage,
                            size,
                            1.22,
                            4.,
                            0,
                        ));
                    }
                }
            }
            Self::SlagSeekers => {
                for side in [-1., 1.] {
                    if beat == 0 {
                        emit(round(aim + side * 0.48, Homing, 470., 54., 15., 3.3, 3., 0));
                        if phase == 3 {
                            emit(round(aim + side * 0.73, Homing, 530., 40., 11., 3., 2., 0));
                        }
                    } else {
                        emit(round(
                            aim + side * 0.21,
                            Explosive,
                            605.,
                            78.,
                            17.,
                            2.4,
                            3.,
                            0,
                        ));
                    }
                }
            }
            Self::Crossfire => {
                // Port, starboard, then the center gun. Each slug can be parried.
                let offset = [-0.36, 0.36, 0.][usize::from(beat)];
                emit(round(
                    aim + offset,
                    Shell,
                    BASTION_ROUND_SPEED,
                    BASTION_ROUND_DAMAGE,
                    BASTION_ROUND_RADIUS,
                    1.7,
                    8.,
                    0,
                ));
                if beat < 2 {
                    emit(round(
                        aim + offset * 1.45,
                        Fragment,
                        520.,
                        23. + 3. * stage,
                        9.,
                        2.,
                        2.,
                        0,
                    ));
                }
            }
            Self::ShieldWall => {
                // Heavy traps fan out ahead of two shells. Both beats leave
                // room in the center, and the traps settle into cover.
                for side in [-1., 1.] {
                    if beat == 0 {
                        for panel in 0..(3 + phase) {
                            let offset = 0.31 + 0.15 * f32::from(panel);
                            emit(round(aim + side * offset, Trap, 470., 35., 18., 4.6, 6., 0));
                        }
                    } else {
                        emit(round(aim + side * 0.28, Shell, 790., 72., 16., 2.1, 4., 0));
                    }
                }
            }
        }
    }
}
