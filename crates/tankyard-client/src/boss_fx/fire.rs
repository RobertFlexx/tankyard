//! Flame: soft puffs that start white-hot and cool through yellow and orange
//! to a deep red before they go out. A round on fire leaves a stream of them
//! where it has been, so the burning stays behind it in the air.
use super::{BossFx, direction, ease_out, perpendicular};
use crate::emission::{Emission, Layer};
use crate::render::View;
use crate::ui::alpha;
use macroquad::prelude::*;
use std::f32::consts::TAU;
use tankyard_core::V2;

/// Most puffs alive at once.
pub(super) const MOST_PUFFS: usize = 960;

/// One puff of burning gas.
#[derive(Clone, Copy, Debug)]
pub(super) struct Puff {
    pos: V2,
    vel: V2,
    age: f32,
    life: f32,
    size: f32,
    drag: f32,
    /// Sets its sway, so no two move alike.
    seed: f32,
    burn: Burn,
}

/// How a puff burns: how bright it is, and how far down the color ramp it's
/// born. Light adds up, so where many puffs overlap each must give less, or
/// together they clip to a flat white and lose the color of fire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Burn {
    gain: f32,
    heat: f32,
}

/// A few puffs, well apart.
pub(super) const OPEN: Burn = Burn { gain: 1., heat: 0. };
/// A stream, each puff lying over the last.
pub(super) const STREAM: Burn = Burn {
    gain: 0.6,
    heat: 0.07,
};
/// A burst of dozens at once.
pub(super) const DENSE: Burn = Burn {
    gain: 0.36,
    heat: 0.14,
};

/// A burst of puffs.
#[derive(Clone, Copy, Debug)]
pub(super) struct Plume {
    pub at: V2,
    /// How far from `at` they may start.
    pub scatter: f32,
    /// The way they fly and how far to either side; None for all around.
    pub heading: Option<(f32, f32)>,
    pub speed: (f32, f32),
    pub size: (f32, f32),
    pub life: (f32, f32),
    pub count: usize,
    pub burn: Burn,
}

/// The color of Bastion's cannon `t` of the way from its heart to its edge. It
/// runs hot without burning: white, ice, then a deep steel blue.
pub(super) fn steel(t: f32) -> Color {
    let t = if t.is_finite() { t.clamp(0., 1.) } else { 1. };
    let white = Color::new(0.93, 0.98, 1., 1.);
    let ice = Color::new(0.46, 0.78, 1., 1.);
    let deep = Color::new(0.1, 0.34, 0.92, 1.);
    if t < 0.3 {
        crate::ui::mix(white, ice, t / 0.3)
    } else {
        crate::ui::mix(ice, deep, (t - 0.3) / 0.7)
    }
}

/// The color of burning gas `t` of the way through its life: white, yellow,
/// orange, then a red that dies away.
pub(super) fn ramp(t: f32) -> Color {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0., [1., 0.97, 0.86]),
        (0.12, [1., 0.8, 0.36]),
        (0.32, [1., 0.5, 0.09]),
        (0.58, [0.92, 0.22, 0.035]),
        (0.82, [0.52, 0.07, 0.025]),
        (1., [0.2, 0.03, 0.02]),
    ];
    let t = if t.is_finite() { t.clamp(0., 1.) } else { 1. };
    let upper = STOPS
        .iter()
        .position(|&(at, _)| at >= t)
        .unwrap_or(STOPS.len() - 1)
        .max(1);
    let (from, a) = STOPS[upper - 1];
    let (to, b) = STOPS[upper];
    let u = (t - from) / (to - from);
    Color::new(
        a[0] + (b[0] - a[0]) * u,
        a[1] + (b[1] - a[1]) * u,
        a[2] + (b[2] - a[2]) * u,
        1.,
    )
}

impl BossFx {
    pub(super) fn plume(&mut self, plume: Plume) {
        for _ in 0..plume.count {
            if self.flames.len() >= MOST_PUFFS {
                return;
            }
            let angle = match plume.heading {
                Some((heading, spread)) => heading + (self.rand() * 2. - 1.) * spread,
                None => self.rand() * TAU,
            };
            let offset = V2::from_angle(self.rand() * TAU) * plume.scatter * self.rand().sqrt();
            let speed = self.between(plume.speed);
            let size = self.between(plume.size);
            let life = self.between(plume.life);
            let drag = 3.2 + self.rand() * 2.6;
            let seed = self.rand();
            self.flames.push(Puff {
                pos: plume.at + offset,
                vel: V2::from_angle(angle) * speed,
                age: 0.,
                life,
                size,
                drag,
                seed,
                burn: plume.burn,
            });
        }
    }

    pub(super) fn run_flames(&mut self, dt: f32) {
        for puff in &mut self.flames {
            puff.age += dt;
            puff.pos += puff.vel * dt;
            puff.vel *= (-puff.drag * dt).exp();
        }
        self.flames.retain(|puff| puff.age < puff.life);
    }

    pub(super) fn draw_flames(&self, light: &mut Emission, view: &View, level: f32) {
        // White wouldn't show on a pale floor, so flame starts at yellow there.
        let start: f32 = if light.pale_floor() { 0.2 } else { 0. };
        let level = level.min(1.);
        let z = view.zoom;
        for puff in &self.flames {
            if !view.visible(puff.pos, puff.size * 3.) {
                continue;
            }
            let t = (puff.age / puff.life).clamp(0., 1.);
            let strength = (t / 0.08).min(1.) * (1. - t).powf(1.4) * level * 0.56 * puff.burn.gain;
            if strength < 0.01 {
                continue;
            }
            let radius = puff.size * (0.45 + 0.9 * ease_out(t)) * z;
            let speed = puff.vel.length();
            let heading = if speed > 1. {
                vec2(puff.vel.x, puff.vel.y) / speed
            } else {
                direction(puff.seed * TAU)
            };
            // Fast gas is drawn out along its path, and everything sways a
            // little, so a stream reads as flame rather than a spray of dots.
            let stretch = 1. + (speed / 360.).min(1.7);
            let sway = (puff.seed * 31. + puff.age * 15.).sin() * radius * 0.16;
            let p = view.screen(puff.pos) + perpendicular(heading) * sway;
            let born = start.max(puff.burn.heat);
            let heat = born + t * (1. - born);
            let layer = if t < 0.16 { Layer::Core } else { Layer::Wash };
            light.oval(
                layer,
                p,
                heading * radius * stretch,
                radius,
                alpha(ramp(heat), strength),
            );
            if light.rich() {
                // A shadow under the flame: fire looks hottest against dark,
                // and adding orange to a grey floor alone only looks dusty.
                light.spot(
                    Layer::Smoke,
                    p,
                    radius * 1.4,
                    Color::new(0.05, 0.025, 0.02, 0.26 * strength),
                );
                if t < 0.2 {
                    light.spot(
                        Layer::Core,
                        p,
                        radius * 0.38,
                        alpha(ramp(heat * 0.45), strength * 0.7),
                    );
                } else if t > 0.5 {
                    light.spot(
                        Layer::Smoke,
                        p,
                        radius * 1.25,
                        Color::new(0.05, 0.035, 0.035, 0.2 * (1. - t) * level),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flame_cools_from_white_through_orange_to_a_dying_red() {
        let hot = ramp(0.);
        assert!(hot.r > 0.95 && hot.g > 0.9 && hot.b > 0.8);
        let orange = ramp(0.32);
        assert!(orange.r > 0.95 && orange.g > 0.4 && orange.b < 0.3);
        let dying = ramp(1.);
        assert!(dying.r < 0.3 && dying.g < 0.1);
        // Each channel only ever falls, so no flicker of the wrong color.
        let mut last = ramp(0.);
        for step in 1..=100 {
            let next = ramp(step as f32 / 100.);
            assert!(next.r <= last.r + 1e-4 && next.g <= last.g + 1e-4);
            assert!(next.b <= last.b + 1e-4);
            last = next;
        }
        for bad in [f32::NAN, f32::INFINITY, -3., 9.] {
            let color = ramp(bad);
            assert!(color.r.is_finite() && color.g.is_finite() && color.b.is_finite());
        }
    }

    #[test]
    fn puffs_are_capped_burn_out_and_slow_down() {
        let mut fx = BossFx::default();
        let plume = Plume {
            at: V2::ZERO,
            scatter: 10.,
            heading: Some((0., 0.3)),
            speed: (100., 200.),
            size: (4., 8.),
            life: (0.2, 0.4),
            count: MOST_PUFFS * 2,
            burn: OPEN,
        };
        fx.plume(plume);
        assert_eq!(fx.flames.len(), MOST_PUFFS);
        let fastest = |fx: &BossFx| {
            fx.flames
                .iter()
                .map(|puff| puff.vel.length())
                .fold(0., f32::max)
        };
        let before = fastest(&fx);
        fx.run_flames(0.05);
        assert!(fastest(&fx) < before);
        for _ in 0..12 {
            fx.run_flames(0.05);
        }
        assert!(fx.flames.is_empty());
    }
}
