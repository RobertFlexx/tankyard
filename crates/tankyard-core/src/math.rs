use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Sub, SubAssign};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}
impl V2 {
    pub const ZERO: Self = Self { x: 0., y: 0. };
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }
    pub fn dot(self, b: Self) -> f32 {
        self.x * b.x + self.y * b.y
    }
    pub fn normalized(self) -> Self {
        let l = self.length();
        if l > 0.00001 { self / l } else { Self::ZERO }
    }
    pub fn clamp_length(self, max: f32) -> Self {
        if self.length_squared() > max * max {
            self.normalized() * max
        } else {
            self
        }
    }
    pub fn distance(self, b: Self) -> f32 {
        (self - b).length()
    }
    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }
    pub fn from_angle(a: f32) -> Self {
        Self::new(a.cos(), a.sin())
    }
    pub fn rotate(self, a: f32) -> Self {
        Self::new(
            self.x * a.cos() - self.y * a.sin(),
            self.x * a.sin() + self.y * a.cos(),
        )
    }
    pub fn lerp(self, b: Self, t: f32) -> Self {
        self + (b - self) * t
    }
    /// This vector moved toward `to` by at most `step`, at a steady rate.
    pub fn toward(self, to: Self, step: f32) -> Self {
        let gap = to - self;
        if gap.length_squared() <= step * step {
            to
        } else {
            self + gap.normalized() * step
        }
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
impl Add for V2 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y)
    }
}
impl Sub for V2 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y)
    }
}
impl Mul<f32> for V2 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}
impl Div<f32> for V2 {
    type Output = Self;
    fn div(self, s: f32) -> Self {
        Self::new(self.x / s, self.y / s)
    }
}
impl AddAssign for V2 {
    fn add_assign(&mut self, b: Self) {
        *self = *self + b;
    }
}
impl SubAssign for V2 {
    fn sub_assign(&mut self, b: Self) {
        *self = *self - b;
    }
}
impl MulAssign<f32> for V2 {
    fn mul_assign(&mut self, s: f32) {
        *self = *self * s;
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
/// `value` kept between `low` and `high`, like `f32::clamp`, but it never
/// panics. `clamp` does when its ends cross, which rounding, a small arena or
/// a big boss can cause, and a panic mid-match takes the game or the whole
/// server down. Crossed ends give the middle, a NaN end is ignored, and a NaN
/// `value` gives `low`.
pub fn within(value: f32, low: f32, high: f32) -> f32 {
    if low > high {
        (low + high) * 0.5
    } else {
        value.max(low).min(high)
    }
}

impl Rect {
    pub fn clamp_circle(self, p: V2, radius: f32) -> V2 {
        let radius = radius.max(0.).min(self.w.min(self.h) * 0.49);
        V2::new(
            within(p.x, self.x + radius, self.x + self.w - radius),
            within(p.y, self.y + radius, self.y + self.h - radius),
        )
    }
    pub fn contains_circle(self, p: V2, radius: f32) -> bool {
        p.finite()
            && p.x >= self.x + radius
            && p.y >= self.y + radius
            && p.x <= self.x + self.w - radius
            && p.y <= self.y + self.h - radius
    }
    pub fn center(self) -> V2 {
        V2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
    pub fn contains(self, p: V2) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
    /// Distance from a point to the rectangle; zero inside it.
    pub fn distance(self, p: V2) -> f32 {
        let dx = (self.x - p.x).max(p.x - (self.x + self.w)).max(0.);
        let dy = (self.y - p.y).max(p.y - (self.y + self.h)).max(0.);
        (dx * dx + dy * dy).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_never_panics_on_crossed_or_broken_bounds() {
        assert_eq!(within(5., 0., 10.), 5.);
        assert_eq!(within(-3., 0., 10.), 0.);
        assert_eq!(within(12., 0., 10.), 10.);
        // Crossed ends, as a big boss in a small arena gives: the middle.
        assert_eq!(within(5., 10., 0.), 5.);
        assert_eq!(within(f32::NAN, 0., 10.), 0.);
        assert_eq!(within(1., f32::NAN, 10.), 1.);
        assert_eq!(within(11., 0., f32::NAN), 11.);
    }

    #[test]
    fn a_circle_wider_than_its_box_is_held_in_the_middle() {
        let room = Rect {
            x: 0.,
            y: 0.,
            w: 100.,
            h: 60.,
        };
        let held = room.clamp_circle(V2::new(500., -500.), 400.);
        assert!(held.finite());
        assert!((0.0..=100.).contains(&held.x) && (0.0..=60.).contains(&held.y));
        let p = room.clamp_circle(V2::new(f32::NAN, 30.), 5.);
        assert!(p.finite());
    }
}
