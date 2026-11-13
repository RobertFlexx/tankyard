//! Easing curves and the group motions map logic plays with them. The server
//! moves cover with these and clients draw the same motion from the same
//! numbers, so a moving wall needs no per-frame network traffic.
use crate::MAP_GROUPS;
use serde::{Deserialize, Serialize};

/// The shape of a motion over time, after Robert Penner's easing equations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    Linear,
    #[default]
    Sine,
    Quad,
    Cubic,
    Quart,
    Quint,
    Expo,
    Circ,
    /// Pulls back a little before setting off, or overshoots before settling.
    Back,
    /// Springs past the end and wobbles into place.
    Elastic,
    /// Lands and bounces like a dropped ball.
    Bounce,
}
impl Curve {
    pub const ALL: [Self; 11] = [
        Self::Linear,
        Self::Sine,
        Self::Quad,
        Self::Cubic,
        Self::Quart,
        Self::Quint,
        Self::Expo,
        Self::Circ,
        Self::Back,
        Self::Elastic,
        Self::Bounce,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Sine => "Sine",
            Self::Quad => "Quad",
            Self::Cubic => "Cubic",
            Self::Quart => "Quart",
            Self::Quint => "Quint",
            Self::Expo => "Expo",
            Self::Circ => "Circ",
            Self::Back => "Back",
            Self::Elastic => "Elastic",
            Self::Bounce => "Bounce",
        }
    }
    /// The curve eased in: slow at the start, `t` and the result both 0 to 1
    /// (Back and Elastic leave that range on the way).
    fn ease_in(self, t: f32) -> f32 {
        use std::f32::consts::{FRAC_PI_2, TAU};
        match self {
            Self::Linear => t,
            Self::Sine => 1. - (t * FRAC_PI_2).cos(),
            Self::Quad => t * t,
            Self::Cubic => t * t * t,
            Self::Quart => t * t * t * t,
            Self::Quint => t * t * t * t * t,
            Self::Expo => {
                if t <= 0. {
                    0.
                } else {
                    (2f32).powf(10. * t - 10.)
                }
            }
            Self::Circ => 1. - (1. - t * t).max(0.).sqrt(),
            Self::Back => {
                const OVERSHOOT: f32 = 1.70158;
                (OVERSHOOT + 1.) * t * t * t - OVERSHOOT * t * t
            }
            Self::Elastic => {
                if t <= 0. || t >= 1. {
                    t.clamp(0., 1.)
                } else {
                    -(2f32).powf(10. * t - 10.) * ((t * 10. - 10.75) * TAU / 3.).sin()
                }
            }
            Self::Bounce => 1. - bounce_out(1. - t),
        }
    }
}
fn bounce_out(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1. / D {
        N * t * t
    } else if t < 2. / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984375
    }
}

/// Which end of a motion the curve shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EaseMode {
    /// Starts slowly.
    In,
    /// Ends slowly.
    Out,
    /// Starts and ends slowly.
    #[default]
    InOut,
    /// Fast at both ends, slow in the middle.
    OutIn,
}
impl EaseMode {
    pub const ALL: [Self; 4] = [Self::In, Self::Out, Self::InOut, Self::OutIn];
    pub fn name(self) -> &'static str {
        match self {
            Self::In => "Ease in",
            Self::Out => "Ease out",
            Self::InOut => "In and out",
            Self::OutIn => "Out and in",
        }
    }
}

/// A curve and the end it shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Easing {
    pub curve: Curve,
    pub mode: EaseMode,
}
impl Easing {
    pub const LINEAR: Self = Self {
        curve: Curve::Linear,
        mode: EaseMode::InOut,
    };
    /// Progress at `t` from 0 to 1, where 0 is the start and 1 the end.
    pub fn apply(self, t: f32) -> f32 {
        if !t.is_finite() {
            return 1.;
        }
        let t = t.clamp(0., 1.);
        let curve = self.curve;
        let out = |t: f32| 1. - curve.ease_in(1. - t);
        let value = match self.mode {
            EaseMode::In => curve.ease_in(t),
            EaseMode::Out => out(t),
            EaseMode::InOut => {
                if t < 0.5 {
                    curve.ease_in(t * 2.) * 0.5
                } else {
                    0.5 + out(t * 2. - 1.) * 0.5
                }
            }
            EaseMode::OutIn => {
                if t < 0.5 {
                    out(t * 2.) * 0.5
                } else {
                    0.5 + curve.ease_in(t * 2. - 1.) * 0.5
                }
            }
        };
        if value.is_finite() { value } else { t }
    }
}

/// What a group motion changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TweenChannel {
    /// Where the group's cover, pads, decorations and trigger areas sit,
    /// as an offset from where the map places them.
    #[default]
    Move,
    /// How far the group's decorations are turned, in degrees.
    Turn,
    /// How opaque the group's decorations are, 0 to 1.
    Fade,
}
impl TweenChannel {
    /// The value a group has before anything changes it.
    pub fn rest(self) -> [f32; 2] {
        match self {
            Self::Move | Self::Turn => [0., 0.],
            Self::Fade => [1., 0.],
        }
    }
}

/// One group's motion on one channel, in round seconds. Holding its end
/// value after it finishes, it also records where a group has come to rest.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tween {
    pub group: u8,
    pub channel: TweenChannel,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub start: f32,
    pub length: f32,
    pub easing: Easing,
}
impl Tween {
    pub fn value(&self, now: f32) -> [f32; 2] {
        if self.length <= 0. || now >= self.start + self.length {
            return self.to;
        }
        let k = self.easing.apply((now - self.start) / self.length);
        [
            self.from[0] + (self.to[0] - self.from[0]) * k,
            self.from[1] + (self.to[1] - self.from[1]) * k,
        ]
    }
    pub fn moving(&self, now: f32) -> bool {
        self.length > 0. && now < self.start + self.length && self.from != self.to
    }
    /// Whether every field holds a value a map could have produced, so a
    /// snapshot cannot hand a client a motion it would trip over.
    pub fn valid(&self) -> bool {
        (1..=MAP_GROUPS).contains(&self.group)
            && self
                .from
                .iter()
                .chain(&self.to)
                .all(|v| v.is_finite() && v.abs() <= TWEEN_LIMIT)
            && self.start.is_finite()
            && (0. ..=TWEEN_SECONDS).contains(&self.length)
    }
}

/// Longest motion, in seconds.
pub const TWEEN_SECONDS: f32 = 600.;
/// Largest magnitude a tween value may reach: a move offset stays within
/// a couple of arena widths and a turn within many full circles.
pub const TWEEN_LIMIT: f32 = 1.0e5;
/// Most motions a round keeps: one per group and channel.
pub const TWEEN_SLOTS: usize = MAP_GROUPS as usize * 3;

/// Every group's current offset, turn and opacity, found once per frame or
/// tick so each object looks its group up instead of searching tweens.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPoses {
    /// Map-fraction offsets, by group; group 0 never moves.
    pub offset: [[f32; 2]; MAP_GROUPS as usize + 1],
    /// Degrees.
    pub turn: [f32; MAP_GROUPS as usize + 1],
    pub opacity: [f32; MAP_GROUPS as usize + 1],
    /// Whether any group sits away from where the map placed it.
    pub moved: bool,
}
impl Default for GroupPoses {
    fn default() -> Self {
        Self {
            offset: [[0.; 2]; MAP_GROUPS as usize + 1],
            turn: [0.; MAP_GROUPS as usize + 1],
            opacity: [1.; MAP_GROUPS as usize + 1],
            moved: false,
        }
    }
}
impl GroupPoses {
    pub fn at(tweens: &[Tween], now: f32) -> Self {
        let mut poses = Self::default();
        for tween in tweens {
            let group = usize::from(tween.group);
            if group == 0 || group > usize::from(MAP_GROUPS) {
                continue;
            }
            let value = tween.value(now);
            match tween.channel {
                TweenChannel::Move => {
                    poses.offset[group] = value;
                    poses.moved |= value != [0., 0.];
                }
                TweenChannel::Turn => poses.turn[group] = value[0],
                TweenChannel::Fade => poses.opacity[group] = value[0].clamp(0., 1.),
            }
        }
        poses
    }
    pub fn offset(&self, group: u8) -> [f32; 2] {
        self.offset
            .get(usize::from(group))
            .copied()
            .unwrap_or_default()
    }
    /// `rect` moved with its group.
    pub fn shift(&self, rect: [f32; 4], group: u8) -> [f32; 4] {
        let [dx, dy] = self.offset(group);
        [rect[0] + dx, rect[1] + dy, rect[2], rect[3]]
    }
    pub fn turn(&self, group: u8) -> f32 {
        self.turn.get(usize::from(group)).copied().unwrap_or(0.)
    }
    pub fn opacity(&self, group: u8) -> f32 {
        self.opacity.get(usize::from(group)).copied().unwrap_or(1.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for curve in Curve::ALL {
            for mode in EaseMode::ALL {
                let easing = Easing { curve, mode };
                assert!(easing.apply(0.).abs() < 1e-4, "{curve:?} {mode:?} start");
                assert!(
                    (easing.apply(1.) - 1.).abs() < 1e-4,
                    "{curve:?} {mode:?} end"
                );
                if matches!(mode, EaseMode::InOut | EaseMode::OutIn) {
                    assert!(
                        (easing.apply(0.5) - 0.5).abs() < 1e-4,
                        "{curve:?} {mode:?} middle"
                    );
                }
                for step in 0..=100 {
                    let value = easing.apply(step as f32 / 100.);
                    assert!(value.is_finite() && (-0.5..=1.5).contains(&value));
                }
            }
        }
        assert_eq!(Easing::default().apply(f32::NAN), 1.);
        assert_eq!(Easing::LINEAR.apply(0.25), 0.25);
    }

    #[test]
    fn tweens_hold_their_end_and_poses_follow_groups() {
        let tween = Tween {
            group: 3,
            channel: TweenChannel::Move,
            from: [0., 0.],
            to: [0.2, -0.1],
            start: 10.,
            length: 2.,
            easing: Easing::LINEAR,
        };
        assert_eq!(tween.value(9.), [0., 0.]);
        assert_eq!(tween.value(11.), [0.1, -0.05]);
        assert_eq!(tween.value(50.), [0.2, -0.1]);
        assert!(tween.moving(11.) && !tween.moving(12.));
        let fade = Tween {
            channel: TweenChannel::Fade,
            from: [1., 0.],
            to: [0., 0.],
            ..tween
        };
        let poses = GroupPoses::at(&[tween, fade], 12.);
        assert!(poses.moved);
        assert_eq!(poses.shift([0.5, 0.5, 0.1, 0.1], 3), [0.7, 0.4, 0.1, 0.1]);
        assert_eq!(poses.offset(0), [0., 0.]);
        assert_eq!(poses.opacity(3), 0.);
        assert_eq!(poses.opacity(4), 1.);
        assert!(tween.valid());
        assert!(
            !Tween {
                to: [f32::NAN, 0.],
                ..tween
            }
            .valid()
        );
    }
}
