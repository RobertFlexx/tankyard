//! The light a boss carries: its core, the pool it casts, an edge of light
//! that follows its real armor, and the burning of a wounded hull.
use super::{BossFx, direction};
use crate::emission::{Emission, Layer, Stop};
use crate::ui::alpha;
use macroquad::prelude::*;
use std::f32::consts::TAU;
use tankyard_core::Boss;

/// A boss's armor shape, as `boss_model::armor` gives it.
#[derive(Clone, Copy, Debug)]
pub struct Armor {
    /// Its sides; under three is a circle.
    pub sides: u8,
    /// Its size against the boss's radius.
    pub size: f32,
    pub angle: f32,
}

impl Armor {
    /// The point `u` of the way around its outline, from its first corner.
    fn point(self, at: Vec2, radius: f32, u: f32) -> Vec2 {
        let u = u.rem_euclid(1.);
        if self.sides < 3 {
            return at + direction(self.angle + u * TAU) * radius;
        }
        let sides = f32::from(self.sides);
        let step = u * sides;
        let edge = step.floor();
        let corner = |n: f32| at + direction(self.angle + n * TAU / sides) * radius;
        corner(edge).lerp(corner(edge + 1.), step - edge)
    }
    fn trace(self, light: &mut Emission, layer: Layer, at: Vec2, radius: f32, half: f32, c: Color) {
        if self.sides < 3 {
            light.ring(layer, at, radius, half, c);
        } else {
            light.outline(layer, at, self.sides, radius, self.angle, half, c);
        }
    }
    /// A short run of light along the outline, from `head` back by `length`.
    #[allow(clippy::too_many_arguments)]
    fn sweep(
        self,
        light: &mut Emission,
        layer: Layer,
        at: Vec2,
        radius: f32,
        head: f32,
        length: f32,
        half: f32,
        c: Color,
    ) {
        const POINTS: usize = 12;
        let mut stops = [Stop {
            at,
            half: 0.,
            color: alpha(c, 0.),
        }; POINTS];
        for (index, stop) in stops.iter_mut().enumerate() {
            let t = index as f32 / (POINTS - 1) as f32;
            *stop = Stop {
                at: self.point(at, radius, head - length * (1. - t)),
                half: half * (0.35 + 0.65 * t),
                color: alpha(c, c.a * t * t * if index + 1 == POINTS { 0. } else { 1. }),
            };
        }
        light.band(layer, &stops);
    }
}

impl BossFx {
    /// Its core, its pool of light, the edge of its armor, the heat of a
    /// wounded hull and the flash of being hit.
    #[allow(clippy::too_many_arguments)]
    pub fn glow(
        &self,
        light: &mut Emission,
        boss: &Boss,
        p: Vec2,
        core: Vec2,
        r: f32,
        time: f32,
        armor: Armor,
    ) {
        let Some(shown) = self.bosses.get(&boss.id) else {
            return;
        };
        let charge = shown.charge;
        let level = self.look.level();
        let color = shown.color;
        let calm = self.look.calm;
        let pulse = if calm {
            0.5
        } else if boss.phase >= 3 {
            0.5 + 0.5 * (time * 4.8).sin()
        } else {
            0.5 + 0.5 * (time * 1.6).sin()
        };
        // Charging always shows; it warns of the attack.
        light.spot(
            Layer::Wash,
            p,
            r * (2.1 + 0.5 * charge),
            alpha(color, (0.05 + 0.02 * pulse) * level + 0.1 * charge),
        );
        light.spot(
            Layer::Wash,
            core,
            r * (0.64 + 0.07 * pulse + 0.4 * charge),
            alpha(
                light.hot(color, 0.28 + 0.4 * charge),
                (0.3 + 0.08 * pulse) * level.max(0.45) + 0.22 * charge,
            ),
        );
        light.spot(
            Layer::Core,
            core,
            r * (0.2 + 0.025 * pulse + 0.14 * charge),
            alpha(
                light.hot(color, 0.72 + 0.28 * charge),
                (0.62 + 0.1 * pulse) * level.max(0.45) + 0.35 * charge,
            ),
        );
        if charge > 0.3 {
            let up = (charge - 0.3) / 0.7;
            light.glint(
                core,
                r * (0.55 + 1.05 * up),
                if calm { 0. } else { time * 0.7 },
                alpha(light.hot(color, 0.75), 0.5 * up),
            );
        }
        // An edge of light along the real armor gives the hull depth, and
        // brightens with its charge or when it's struck.
        let outline = r * armor.size;
        let lit = (0.1 + 0.04 * pulse) * level + 0.3 * charge + 0.42 * shown.heat * level;
        armor.trace(
            light,
            Layer::Wash,
            p,
            outline,
            (r * 0.1).max(1.5),
            alpha(color, lit * 0.7),
        );
        armor.trace(
            light,
            Layer::Core,
            p,
            outline,
            (r * 0.014).max(0.7),
            alpha(light.hot(color, 0.4), (lit * 1.5).min(0.9)),
        );
        if charge > 0.02 && !calm {
            // Two runs of light chase each other around the hull.
            let head = time * (0.42 + 0.5 * charge);
            for offset in [0., 0.5] {
                armor.sweep(
                    light,
                    Layer::Core,
                    p,
                    outline,
                    head + offset,
                    0.24,
                    (r * 0.03).max(1.),
                    alpha(light.hot(color, 0.75), 0.4 + 0.6 * charge),
                );
            }
        }
        if level <= 0. {
            return;
        }
        if boss.phase >= 2 {
            let (heat, strength, patches) = if boss.phase >= 3 {
                (Color::new(1., 0.3, 0.14, 1.), 0.34 + 0.16 * pulse, 7)
            } else {
                (Color::new(1., 0.6, 0.2, 1.), 0.26, 4)
            };
            light.spot(
                Layer::Wash,
                p,
                r * 1.5,
                alpha(heat, strength * 0.32 * level),
            );
            // Burning plates, spaced by the golden angle so they never line
            // up, each flaring and dying down on its own.
            let seed = (boss.id % 97) as f32;
            for patch in 0..patches {
                let n = patch as f32 + seed;
                let angle = n * 2.399_963;
                let reach = r * (0.34 + 0.5 * (n * 0.618_034).fract());
                let flare = if calm {
                    0.6
                } else {
                    0.42 + 0.58 * (0.5 + 0.5 * (time * (1.7 + n % 3.) + n * 1.9).sin())
                };
                let at = p + direction(angle) * reach;
                light.spot(
                    Layer::Wash,
                    at,
                    r * (0.34 + 0.12 * flare),
                    alpha(heat, (strength + 0.24) * flare * level),
                );
                light.spot(
                    Layer::Core,
                    at,
                    r * 0.11,
                    alpha(light.hot(heat, 0.5), (strength + 0.4) * flare * level),
                );
            }
        }
        if shown.heat > 0.02 {
            // Being hit lights the hull from within.
            light.spot(
                Layer::Wash,
                p,
                r * 1.55,
                alpha(light.hot(color, 0.45), 0.34 * shown.heat * level),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn the_outline_walks_the_real_corners_of_the_armor() {
        let square = Armor {
            sides: 4,
            size: 1.,
            angle: 0.,
        };
        let at = vec2(10., 20.);
        for (u, corner) in [
            (0., vec2(110., 20.)),
            (0.25, vec2(10., 120.)),
            (0.5, vec2(-90., 20.)),
            (0.75, vec2(10., -80.)),
            (1., vec2(110., 20.)),
        ] {
            assert!(square.point(at, 100., u).distance(corner) < 0.01, "{u}");
        }
        // Halfway along an edge is inside the circle, as a flat side is.
        let middle = square.point(at, 100., 0.125);
        assert!((middle.distance(at) - 100. * (PI / 4.).cos()).abs() < 0.01);
        let round = Armor {
            sides: 0,
            size: 1.,
            angle: 1.,
        };
        for step in 0..20 {
            let point = round.point(at, 50., step as f32 / 20.);
            assert!((point.distance(at) - 50.).abs() < 0.01);
        }
    }
}
