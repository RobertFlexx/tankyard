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
impl Rect {
    pub fn center(self) -> V2 {
        V2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
    pub fn contains(self, p: V2) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
}
