//! Discrete difficulties and continuous skill blending.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Beginner,
    #[default]
    Normal,
    Advanced,
    Expert,
}

impl Difficulty {
    pub const ALL: [Self; 4] = [Self::Beginner, Self::Normal, Self::Advanced, Self::Expert];

    pub fn name(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Normal => "Normal",
            Self::Advanced => "Advanced",
            Self::Expert => "Expert",
        }
    }

    pub(super) fn reaction_ticks(self) -> u64 {
        match self {
            Self::Beginner => 20,
            Self::Normal => 12,
            Self::Advanced => 8,
            Self::Expert => 7,
        }
    }

    pub(super) fn perception_radius(self) -> f32 {
        match self {
            Self::Beginner => 420.,
            Self::Normal => 500.,
            Self::Advanced => 560.,
            Self::Expert => 620.,
        }
    }

    pub(super) fn prediction(self) -> f32 {
        match self {
            Self::Beginner => 0.15,
            Self::Normal => 0.55,
            Self::Advanced => 0.8,
            Self::Expert => 0.9,
        }
    }

    pub(super) fn aim_error(self) -> f32 {
        match self {
            Self::Beginner => 0.12,
            Self::Normal => 0.075,
            Self::Advanced => 0.045,
            Self::Expert => 0.03,
        }
    }

    pub(super) fn dodge_strength(self) -> f32 {
        match self {
            Self::Beginner => 0.2,
            Self::Normal => 0.5,
            Self::Advanced => 0.75,
            Self::Expert => 0.9,
        }
    }

    pub(super) fn retreat_health(self) -> f32 {
        match self {
            Self::Beginner => 0.2,
            Self::Normal => 0.32,
            Self::Advanced => 0.42,
            Self::Expert => 0.38,
        }
    }

    pub(super) fn dodge_period(self) -> f32 {
        match self {
            Self::Beginner => 12.,
            Self::Normal => 6.,
            Self::Advanced => 5.,
            Self::Expert => 4.,
        }
    }

    pub(super) fn miss_chance(self) -> f32 {
        match self {
            Self::Beginner => 0.48,
            Self::Normal => 0.28,
            Self::Advanced => 0.18,
            Self::Expert => 0.10,
        }
    }

    pub(super) fn awareness(self) -> f32 {
        match self {
            Self::Beginner => 0.15,
            Self::Normal => 0.6,
            Self::Advanced => 0.9,
            Self::Expert => 1.,
        }
    }

    /// How much enemy pressure a bot takes before backing off; beginners
    /// never count.
    pub(super) fn pressure_limit(self) -> f32 {
        match self {
            Self::Beginner => f32::INFINITY,
            Self::Normal => 2.2,
            Self::Advanced => 1.9,
            Self::Expert => 1.7,
        }
    }

    /// Where this difficulty sits on the continuous skill scale.
    pub fn level(self) -> f32 {
        match self {
            Self::Beginner => 0.,
            Self::Normal => 1.,
            Self::Advanced => 2.,
            Self::Expert => 3.,
        }
    }
}

/// A point between the difficulty levels: 1.0 plays exactly like Normal,
/// 1.5 halfway to Advanced. Lets bots get better gradually.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Skill(pub(super) f32);

impl Skill {
    pub(super) fn blend(self, value: impl Fn(Difficulty) -> f32) -> f32 {
        let level = self.0.clamp(0., 3.);
        let low = (level.floor() as usize).min(2);
        let t = level - low as f32;
        let (a, b) = (value(Difficulty::ALL[low]), value(Difficulty::ALL[low + 1]));
        if t == 0. {
            a
        } else if t == 1. {
            b
        } else {
            a + (b - a) * t
        }
    }

    pub(super) fn reaction_ticks(self) -> u64 {
        self.blend(|d| d.reaction_ticks() as f32).round() as u64
    }

    pub(super) fn perception_radius(self) -> f32 {
        self.blend(Difficulty::perception_radius)
    }

    pub(super) fn prediction(self) -> f32 {
        self.blend(Difficulty::prediction)
    }

    pub(super) fn aim_error(self) -> f32 {
        self.blend(Difficulty::aim_error)
    }

    pub(super) fn dodge_strength(self) -> f32 {
        self.blend(Difficulty::dodge_strength)
    }

    pub(super) fn retreat_health(self) -> f32 {
        self.blend(Difficulty::retreat_health)
    }

    pub(super) fn dodge_period(self) -> u64 {
        self.blend(Difficulty::dodge_period).round() as u64
    }

    pub(super) fn miss_chance(self) -> f32 {
        self.blend(Difficulty::miss_chance)
    }

    pub(super) fn awareness(self) -> f32 {
        self.blend(Difficulty::awareness)
    }

    pub(super) fn pressure_limit(self) -> f32 {
        // Blend the inverse, so "never" eases into a real limit.
        1. / self.blend(|d| 1. / d.pressure_limit())
    }
}

impl From<Difficulty> for Skill {
    fn from(difficulty: Difficulty) -> Self {
        Self(difficulty.level())
    }
}
