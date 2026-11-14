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

/// A motion's value. Each channel uses as many of the four numbers as it
/// needs and leaves the rest at zero.
pub type TweenValue = [f32; 4];

/// What a group motion changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TweenChannel {
    /// Where the group's cover, pads, decorations and trigger areas sit,
    /// as an offset from where the map places them: `[x, y]`.
    #[default]
    Move,
    /// How far the group's decorations are turned, in degrees: `[degrees]`.
    Turn,
    /// How opaque the group's decorations are, 0 to 1: `[opacity]`.
    Fade,
    // Newer channels go last: the wire format numbers variants in order.
    /// A colour mixed into the group's decorations: `[red, green, blue,
    /// strength]`, each 0 to 1. Strength 0 leaves their own colours.
    Tint,
    /// How large the group's decorations are drawn: `[factor]`, 1 as placed.
    Scale,
    /// A looping animation: `[size, cycles per second, phase]`. See
    /// [`AnimateStyle`] for what size means for each.
    Bob,
    Sway,
    Throb,
    Shake,
    Orbit,
}
impl TweenChannel {
    pub const ALL: [Self; 10] = [
        Self::Move,
        Self::Turn,
        Self::Fade,
        Self::Tint,
        Self::Scale,
        Self::Bob,
        Self::Sway,
        Self::Throb,
        Self::Shake,
        Self::Orbit,
    ];
    /// The value a group has before anything changes it.
    pub fn rest(self) -> TweenValue {
        match self {
            Self::Fade | Self::Scale => [1., 0., 0., 0.],
            Self::Tint => [1., 1., 1., 0.],
            _ => [0.; 4],
        }
    }
    /// The animation this channel plays, if it is one.
    pub fn style(self) -> Option<AnimateStyle> {
        AnimateStyle::ALL
            .into_iter()
            .find(|style| style.channel() == self)
    }
}

/// A looping animation an Animate trigger plays on a group's decorations.
/// Animations only change how decorations look; cover never moves with them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimateStyle {
    /// Floats up and down by `size` world units.
    #[default]
    Bob,
    /// Rocks back and forth by `size` degrees.
    Sway,
    /// Grows and shrinks by `size` percent.
    Throb,
    /// Trembles by up to `size` world units.
    Shake,
    /// Circles `size` world units from where it rests.
    Orbit,
}
impl AnimateStyle {
    pub const ALL: [Self; 5] = [Self::Bob, Self::Sway, Self::Throb, Self::Shake, Self::Orbit];
    pub fn name(self) -> &'static str {
        match self {
            Self::Bob => "Bob",
            Self::Sway => "Sway",
            Self::Throb => "Throb",
            Self::Shake => "Shake",
            Self::Orbit => "Orbit",
        }
    }
    pub fn channel(self) -> TweenChannel {
        match self {
            Self::Bob => TweenChannel::Bob,
            Self::Sway => TweenChannel::Sway,
            Self::Throb => TweenChannel::Throb,
            Self::Shake => TweenChannel::Shake,
            Self::Orbit => TweenChannel::Orbit,
        }
    }
    /// What `size` counts, for the editor.
    pub fn unit(self) -> &'static str {
        match self {
            Self::Sway => "°",
            Self::Throb => "%",
            _ => " units",
        }
    }
    /// The largest size a map may ask for.
    pub fn limit(self) -> i32 {
        match self {
            Self::Sway => 180,
            Self::Throb => 100,
            _ => ANIMATE_REACH,
        }
    }
    /// The speed a new Animate trigger starts with, in cycles a second.
    pub fn default_speed(self) -> f32 {
        match self {
            Self::Shake => 8.,
            Self::Orbit => 0.5,
            _ => 1.,
        }
    }
}

/// Largest Bob, Shake or Orbit, in world units.
pub const ANIMATE_REACH: i32 = 400;
/// Fastest animation, in cycles a second.
pub const ANIMATE_SPEED: f32 = 12.;
/// Largest Scale, as a factor.
pub const SCALE_LIMIT: f32 = 4.;

/// One group's motion on one channel, in round seconds. Holding its end
/// value after it finishes, it also records where a group has come to rest.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tween {
    pub group: u8,
    pub channel: TweenChannel,
    pub from: TweenValue,
    pub to: TweenValue,
    pub start: f32,
    pub length: f32,
    pub easing: Easing,
}
impl Tween {
    pub fn value(&self, now: f32) -> TweenValue {
        if self.length <= 0. || now >= self.start + self.length {
            return self.to;
        }
        let k = self.easing.apply((now - self.start) / self.length);
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * k)
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
pub const TWEEN_SLOTS: usize = MAP_GROUPS as usize * TweenChannel::ALL.len();

/// Where an animation of `style` is at `phase` cycles, scaled by `size`:
/// an offset in world units, a turn in degrees and a change of scale.
fn animation(style: AnimateStyle, size: f32, phase: f32) -> ([f32; 2], f32, f32) {
    use std::f32::consts::TAU;
    let wave = (phase * TAU).sin();
    match style {
        AnimateStyle::Bob => ([0., -size * wave], 0., 0.),
        AnimateStyle::Sway => ([0., 0.], size * wave, 0.),
        AnimateStyle::Throb => ([0., 0.], 0., size / 100. * wave),
        AnimateStyle::Orbit => ([size * (phase * TAU).cos(), size * wave], 0., 0.),
        AnimateStyle::Shake => (
            [size * noise(phase, 0x9e37), size * noise(phase, 0x7f4a)],
            0.,
            0.,
        ),
    }
}

/// Smooth value noise from -1 to 1, one new value per whole `t`.
fn noise(t: f32, seed: u32) -> f32 {
    let hash = |n: i64| {
        let mut x = (n as u32) ^ seed.wrapping_mul(0x2c1b_3c6d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x297a_2d39);
        x ^= x >> 12;
        x as f32 / u32::MAX as f32 * 2. - 1.
    };
    let step = t.floor();
    let f = t - step;
    let f = f * f * (3. - 2. * f);
    let a = hash(step as i64);
    a + (hash(step as i64 + 1) - a) * f
}

const SLOTS: usize = MAP_GROUPS as usize + 1;

/// Every group's current offset, turn, opacity, tint, scale and animation,
/// found once per frame or tick so each object looks its group up instead
/// of searching tweens.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPoses {
    /// Map-fraction offsets, by group; group 0 never moves.
    pub offset: [[f32; 2]; SLOTS],
    /// Degrees.
    pub turn: [f32; SLOTS],
    pub opacity: [f32; SLOTS],
    /// Red, green, blue and strength, each 0 to 1.
    pub tint: [[f32; 4]; SLOTS],
    pub scale: [f32; SLOTS],
    /// Animated decorations' offsets in world units, turns in degrees and
    /// changes of scale, as of the time these poses were found.
    pub wobble: [[f32; 2]; SLOTS],
    pub sway: [f32; SLOTS],
    pub throb: [f32; SLOTS],
    /// Whether any group sits away from where the map placed it.
    pub moved: bool,
}
impl Default for GroupPoses {
    fn default() -> Self {
        Self {
            offset: [[0.; 2]; SLOTS],
            turn: [0.; SLOTS],
            opacity: [1.; SLOTS],
            tint: [[1., 1., 1., 0.]; SLOTS],
            scale: [1.; SLOTS],
            wobble: [[0.; 2]; SLOTS],
            sway: [0.; SLOTS],
            throb: [0.; SLOTS],
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
                    poses.offset[group] = [value[0], value[1]];
                    poses.moved |= value[0] != 0. || value[1] != 0.;
                }
                TweenChannel::Turn => poses.turn[group] = value[0],
                TweenChannel::Fade => poses.opacity[group] = value[0].clamp(0., 1.),
                TweenChannel::Tint => poses.tint[group] = value.map(|v| v.clamp(0., 1.)),
                TweenChannel::Scale => poses.scale[group] = value[0].clamp(0., SCALE_LIMIT),
                channel => {
                    let Some(style) = channel.style() else {
                        continue;
                    };
                    let [size, speed, phase, _] = value;
                    let ([x, y], turn, grow) = animation(style, size, now * speed + phase);
                    let wobble = &mut poses.wobble[group];
                    *wobble = [wobble[0] + x, wobble[1] + y];
                    poses.sway[group] += turn;
                    poses.throb[group] += grow;
                }
            }
        }
        poses
    }
    /// The same poses with every animation at rest, for reduced motion.
    pub fn still(mut self) -> Self {
        self.wobble = [[0.; 2]; SLOTS];
        self.sway = [0.; SLOTS];
        self.throb = [0.; SLOTS];
        self
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
    /// The colour mixed into the group and how strongly, 0 to 1.
    pub fn tint(&self, group: u8) -> ([f32; 3], f32) {
        let [r, g, b, strength] = self
            .tint
            .get(usize::from(group))
            .copied()
            .unwrap_or([1., 1., 1., 0.]);
        ([r, g, b], strength)
    }
    /// How large the group's decorations are drawn, Scale and Throb together.
    pub fn scale(&self, group: u8) -> f32 {
        let group = usize::from(group);
        let scale = self.scale.get(group).copied().unwrap_or(1.);
        let throb = self.throb.get(group).copied().unwrap_or(0.);
        (scale * (1. + throb)).clamp(0., SCALE_LIMIT * 2.)
    }
    /// An animated decoration's extra offset, in world units.
    pub fn wobble(&self, group: u8) -> [f32; 2] {
        self.wobble
            .get(usize::from(group))
            .copied()
            .unwrap_or_default()
    }
    /// An animated decoration's extra turn, in degrees.
    pub fn sway(&self, group: u8) -> f32 {
        self.sway.get(usize::from(group)).copied().unwrap_or(0.)
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
            from: [0.; 4],
            to: [0.2, -0.1, 0., 0.],
            start: 10.,
            length: 2.,
            easing: Easing::LINEAR,
        };
        assert_eq!(tween.value(9.), [0.; 4]);
        assert_eq!(tween.value(11.), [0.1, -0.05, 0., 0.]);
        assert_eq!(tween.value(50.), [0.2, -0.1, 0., 0.]);
        assert!(tween.moving(11.) && !tween.moving(12.));
        let fade = Tween {
            channel: TweenChannel::Fade,
            from: [1., 0., 0., 0.],
            to: [0.; 4],
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
                to: [f32::NAN, 0., 0., 0.],
                ..tween
            }
            .valid()
        );
    }

    #[test]
    fn tints_scales_and_animations_pose_decorations_without_moving_cover() {
        let at = |channel, to: TweenValue| Tween {
            group: 2,
            channel,
            from: channel.rest(),
            to,
            start: 0.,
            length: 0.,
            easing: Easing::LINEAR,
        };
        let poses = GroupPoses::at(
            &[
                at(TweenChannel::Tint, [1., 0., 0., 0.5]),
                at(TweenChannel::Scale, [2., 0., 0., 0.]),
                // A quarter cycle in: bob at its height, sway at its widest.
                at(TweenChannel::Bob, [10., 1., 0., 0.]),
                at(TweenChannel::Sway, [15., 1., 0., 0.]),
                at(TweenChannel::Throb, [50., 1., 0., 0.]),
            ],
            0.25,
        );
        assert_eq!(poses.tint(2), ([1., 0., 0.], 0.5));
        assert_eq!(poses.tint(3), ([1., 1., 1.], 0.));
        assert!((poses.wobble(2)[1] + 10.).abs() < 1e-4);
        assert!((poses.sway(2) - 15.).abs() < 1e-4);
        assert!((poses.scale(2) - 3.).abs() < 1e-4);
        assert!(!poses.moved, "animations never move cover");
        let still = poses.clone().still();
        assert_eq!(still.wobble(2), [0., 0.]);
        assert_eq!(still.scale(2), 2.);
        assert_eq!(still.tint(2), poses.tint(2));
        // Shake stays within its size and changes smoothly.
        let mut last: Option<[f32; 2]> = None;
        for step in 0..400 {
            let poses = GroupPoses::at(
                &[at(TweenChannel::Shake, [12., 8., 0., 0.])],
                step as f32 / 120.,
            );
            let [x, y] = poses.wobble(2);
            assert!(x.abs() <= 12.001 && y.abs() <= 12.001);
            if let Some([lx, ly]) = last {
                assert!((x - lx).abs() < 4. && (y - ly).abs() < 4.);
            }
            last = Some([x, y]);
        }
        for channel in TweenChannel::ALL {
            assert_eq!(
                channel.style().map(AnimateStyle::channel),
                channel.style().map(|_| channel)
            );
        }
        assert_eq!(TWEEN_SLOTS, 32 * 10);
    }
}
