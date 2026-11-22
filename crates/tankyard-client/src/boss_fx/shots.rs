//! Boss rounds in flight. Each has a solid body, so it can be seen and dodged
//! whatever the settings, with its heart, halo and trail drawn in light, and
//! each boss's guns burn in their own way.
use super::fire::ramp;
use super::{BossFx, Fuel, Round, TRAIL_POINTS, Trail, perpendicular};
use crate::emission::{Emission, Layer, Stop};
use crate::render::{View, polygon};
use crate::ui::{AMBER, alpha, mix};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};
use tankyard_client::tank_model::OUTLINE;
use tankyard_content::ProjectileKind;
use tankyard_core::LETHAL_DAMAGE;

impl BossFx {
    fn fuel_of(&self, round: &Round) -> Fuel {
        self.bosses
            .get(&round.owner)
            .map_or(Fuel::Plasma, |presence| presence.fuel)
    }
    /// A clock for a round's flicker, different for each round and still
    /// with motion reduced.
    fn flicker(&self, round: &Round) -> f32 {
        if self.look.calm {
            0.
        } else {
            round.age + (round.id % 97) as f32 * 0.37
        }
    }

    /// One boss round.
    pub fn round(&self, light: &mut Emission, view: &View, round: &Round) {
        let Round {
            p,
            r,
            heading,
            kind,
            damage,
            color,
            opacity,
            age,
            z,
            ..
        } = *round;
        let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
        let n = perpendicular(d);
        let level = self.look.level().max(0.55);
        let rim = (1.6 * z).max(1.);
        let edge = alpha(mix(color, OUTLINE, 0.78), opacity);
        let body = alpha(color, opacity);
        let hot = |light: &Emission, amount: f32, strength: f32| {
            alpha(light.hot(color, amount), strength * opacity * level)
        };
        let fuel = self.fuel_of(round);
        let clock = self.flicker(round);
        if damage >= LETHAL_DAMAGE {
            fireball(light, round, clock, level);
            return;
        }
        self.trail(light, view, round, 1.);
        match kind {
            ProjectileKind::Piercing => {
                // A lance, with a hot spine running its length.
                let tip = p + d * r * 2.4;
                let tail = p - d * r * 1.8;
                let grow = rim * 1.5;
                let lance = |grow: f32, tint: Color| {
                    draw_triangle(
                        tip + d * grow,
                        p + n * (r * 0.78 + grow),
                        tail - d * grow,
                        tint,
                    );
                    draw_triangle(
                        tip + d * grow,
                        p - n * (r * 0.78 + grow),
                        tail - d * grow,
                        tint,
                    );
                };
                lance(grow, edge);
                lance(0., body);
                light.oval(Layer::Wash, p, d * r * 3.4, r * 1.5, hot(light, 0.18, 0.34));
                light.beam(
                    Layer::Core,
                    tail,
                    tip + d * r * 0.3,
                    (r * 0.42).max(1.2),
                    hot(light, 0.9, 0.95),
                );
            }
            ProjectileKind::Homing => {
                // A dart: swept fins, and a bright engine at its tail.
                let tip = p + d * r * 1.75;
                let left = p - d * r + n * r;
                let right = p - d * r - n * r;
                let notch = p - d * r * 0.42;
                let grow = rim * 1.4;
                draw_triangle(
                    tip + d * grow,
                    left + (left - p).normalize_or_zero() * grow,
                    notch - d * grow,
                    edge,
                );
                draw_triangle(
                    tip + d * grow,
                    right + (right - p).normalize_or_zero() * grow,
                    notch - d * grow,
                    edge,
                );
                draw_triangle(tip, left, notch, body);
                draw_triangle(tip, right, notch, body);
                let engine = p - d * r * 0.72;
                light.spot(Layer::Wash, engine, r * 2.5, hot(light, 0.2, 0.42));
                light.spot(Layer::Core, engine, r * 0.92, hot(light, 0.95, 1.));
                light.oval(
                    Layer::Core,
                    p + d * r * 0.5,
                    d * r * 0.95,
                    (r * 0.2).max(0.8),
                    hot(light, 0.7, 0.6),
                );
            }
            ProjectileKind::Fragment => {
                let tip = p + d * r * 1.55;
                let left = p - d * r * 0.85 + n * r * 0.78;
                let right = p - d * r * 0.85 - n * r * 0.78;
                draw_triangle(
                    tip + d * rim,
                    left - d * rim + n * rim,
                    right - d * rim - n * rim,
                    edge,
                );
                draw_triangle(tip, left, right, body);
                light.oval(
                    Layer::Wash,
                    p - d * r * 0.2,
                    d * r * 2.4,
                    r * 1.35,
                    hot(light, 0.15, 0.32),
                );
                light.oval(
                    Layer::Core,
                    p + d * r * 0.1,
                    d * r * 1.15,
                    (r * 0.3).max(0.9),
                    hot(light, 0.9, 0.9),
                );
            }
            ProjectileKind::Explosive => {
                // A charge with a burning fuse that races as it nears the end.
                polygon(p, 0, r + rim, 0., edge);
                polygon(p, 0, r, 0., body);
                polygon(
                    p,
                    0,
                    r * 0.62,
                    0.,
                    alpha(mix(color, OUTLINE, 0.28), opacity),
                );
                let beat = 0.5 + 0.5 * (age * (11. + age * 5.)).sin();
                light.spot(Layer::Wash, p, r * 2.9, hot(light, 0.2, 0.3 + 0.2 * beat));
                light.spot(
                    Layer::Core,
                    p,
                    r * (0.48 + 0.16 * beat),
                    hot(light, 1., 0.95),
                );
                let spin = if self.look.calm { 0. } else { age * 7.5 };
                for side in [0., PI] {
                    light.arc(
                        Layer::Core,
                        p,
                        r * 1.42,
                        (r * 0.2).max(1.1),
                        spin + side,
                        1.9,
                        alpha(light.hot(mix(color, AMBER, 0.55), 0.3), 0.9 * opacity),
                    );
                }
            }
            _ => {
                // Shells and bullets: a ball with a hot heart.
                polygon(p, 0, r + rim, 0., edge);
                polygon(p, 0, r, 0., body);
                polygon(p, 0, r * 0.72, 0., alpha(mix(color, WHITE, 0.16), opacity));
                light.oval(
                    Layer::Wash,
                    p - d * r * 0.4,
                    d * r * 3.1,
                    r * 2.25,
                    hot(light, 0.08, 0.42),
                );
                light.spot(Layer::Core, p, r * 0.98, hot(light, 0.95, 0.82));
            }
        }
        if self.look.live() {
            flourish(light, round, fuel, clock, level);
        }
    }

    /// The light on a boss's drone, trap or mine.
    pub fn craft(&self, light: &mut Emission, view: &View, round: &Round) {
        let Round {
            p,
            r,
            heading,
            kind,
            color,
            opacity,
            age,
            ..
        } = *round;
        let level = self.look.level().max(0.5);
        let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
        let beat = if self.look.calm {
            0.5
        } else {
            0.5 + 0.5 * (age * 4.4).sin()
        };
        match kind {
            ProjectileKind::Drone => {
                self.trail(light, view, round, 0.62);
                let engine = p - d * r * 0.62;
                light.spot(
                    Layer::Wash,
                    engine,
                    r * 1.7,
                    alpha(color, 0.26 * opacity * level),
                );
                light.spot(
                    Layer::Core,
                    engine,
                    r * 0.55,
                    alpha(light.hot(color, 0.85), 0.85 * opacity * level),
                );
            }
            _ => {
                let core = alpha(light.hot(color, 0.8), (0.5 + 0.4 * beat) * opacity * level);
                light.spot(
                    Layer::Wash,
                    p,
                    r * (1.8 + 0.3 * beat),
                    alpha(color, (0.14 + 0.1 * beat) * opacity * level),
                );
                light.spot(Layer::Core, p, r * 0.44, core);
                if kind == ProjectileKind::Mine {
                    light.ring(
                        Layer::Core,
                        p,
                        r * (1.28 + 0.1 * beat),
                        (r * 0.09).max(0.8),
                        alpha(
                            light.hot(color, 0.35),
                            (0.25 + 0.3 * beat) * opacity * level,
                        ),
                    );
                }
            }
        }
    }

    /// A round's bright trail, from where it has been up to its body.
    fn trail(&self, light: &mut Emission, view: &View, round: &Round, width: f32) {
        if !self.look.live() {
            return;
        }
        if let Some(trail) = self.trails.get(&round.id) {
            band_from_trail(
                light,
                view,
                trail,
                round.p,
                round.r * width,
                round.opacity * self.look.level(),
            );
        }
    }
}

/// The Siege Engine's round: a white-hot heart in a teardrop of fire, with
/// tongues of flame licking back off it. The stream it leaves in the air is
/// made of real puffs of flame, added as it flies.
fn fireball(light: &mut Emission, round: &Round, clock: f32, level: f32) {
    let Round {
        p,
        r,
        heading,
        color,
        opacity,
        age,
        z,
        ..
    } = *round;
    let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
    let n = perpendicular(d);
    let o = opacity * level.min(1.);
    let pale = light.pale_floor();
    let heat = |t: f32| ramp(if pale { t.max(0.22) } else { t });
    let rim = (1.8 * z).max(1.);
    // The solid body, so it shows on any floor and at any setting.
    polygon(p, 0, r + rim, 0., alpha(mix(color, OUTLINE, 0.8), opacity));
    polygon(p, 0, r, 0., alpha(Color::new(1., 0.42, 0.1, 1.), opacity));
    // Never trail further back than it has really flown, so at launch the
    // fire doesn't stick out through the back of the gun.
    let flown = (age * heading.length() * z).max(r * 0.8);
    let reach = flown.min(r * 8.5);
    let point = |back: f32, half: f32, t: f32, strength: f32| Stop {
        at: p - d * back,
        half,
        color: alpha(heat(t), strength * o),
    };
    light.spot(Layer::Wash, p - d * r, r * 5., alpha(heat(0.5), 0.3 * o));
    light.band(
        Layer::Wash,
        &[
            point(-r * 0.95, r * 0.12, 0.3, 0.),
            point(-r * 0.3, r * 1.08, 0.3, 0.62),
            point(reach * 0.26, r * 1.1, 0.36, 0.66),
            point(reach * 0.6, r * 0.66, 0.58, 0.42),
            point(reach, r * 0.1, 0.84, 0.),
        ],
    );
    light.band(
        Layer::Core,
        &[
            point(-r * 0.78, r * 0.1, 0.1, 0.),
            point(-r * 0.12, r * 0.76, 0.05, 0.96),
            point(reach * 0.28, r * 0.58, 0.14, 0.84),
            point(reach * 0.62, r * 0.26, 0.3, 0.5),
            point(reach * 0.86, r * 0.05, 0.42, 0.),
        ],
    );
    // Tongues of flame licking back from around the head.
    for lick in 0..7 {
        let phase = lick as f32 * 2.399_963;
        let side = lick as f32 / 3. - 1.;
        let sway = (clock * (9. + lick as f32) + phase).sin();
        let length = r * (0.75 + 0.3 * (clock * 13. + phase * 2.).sin());
        let at = p - d * r * (0.3 + 0.28 * phase.sin().abs()) + n * side * r * (0.92 + 0.1 * sway);
        light.oval(
            Layer::Wash,
            at - d * length * 0.72 + n * sway * r * 0.1,
            -d * length,
            r * 0.28,
            alpha(heat(0.28 + 0.04 * lick as f32), 0.66 * o),
        );
    }
    light.spot(Layer::Wash, p, r * 3.2, alpha(heat(0.3), 0.52 * o));
    light.spot(Layer::Core, p, r * 1.6, alpha(heat(0.06), 0.96 * o));
    light.spot(Layer::Core, p, r * 0.92, alpha(heat(0.), o));
    light.glint(
        p,
        r * 3.1,
        heading.y.atan2(heading.x) + clock * 0.4,
        alpha(heat(0.05), 0.46 * o),
    );
}

/// The touch that marks whose guns a round came from.
fn flourish(light: &mut Emission, round: &Round, fuel: Fuel, clock: f32, level: f32) {
    let Round {
        p,
        r,
        heading,
        color,
        opacity,
        ..
    } = *round;
    let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
    let n = perpendicular(d);
    let o = opacity * level.min(1.);
    match fuel {
        Fuel::Plasma => {}
        Fuel::Fire => {
            // Burning fuel: flame licking off the back of the round.
            for lick in 0..3 {
                let phase = lick as f32 * 2.1;
                let sway = (clock * 12. + phase).sin();
                let length = r * (0.9 + 0.3 * (clock * 15. + phase).sin());
                light.oval(
                    Layer::Wash,
                    p - d * (r * 0.6 + length) + n * (lick as f32 - 1.) * r * (0.5 + 0.1 * sway),
                    -d * length,
                    r * 0.34,
                    alpha(ramp(0.3 + 0.08 * lick as f32), 0.6 * o),
                );
            }
        }
        Fuel::Arc => {
            // Electricity: small forks jumping off the round.
            let beat = (clock * 18.).floor();
            for fork in 0..2 {
                let seed = beat * 1.37 + fork as f32 * 5.1;
                let angle = seed * 2.399_963;
                let from = p + super::direction(angle) * r * 0.75;
                let to = p + super::direction(angle + 0.5) * r * (1.9 + 0.6 * seed.sin().abs());
                bolt(
                    light,
                    from,
                    to,
                    (r * 0.11).max(0.7),
                    alpha(light.hot(color, 0.6), 0.85 * o),
                    seed,
                );
            }
        }
        Fuel::Spectrum => {
            // Split light: a rose and a blue ghost either side of the round.
            let drift = r * (0.42 + 0.08 * (clock * 5.).sin());
            for (side, tint) in [
                (-1., Color::new(1., 0.36, 0.72, 1.)),
                (1., Color::new(0.34, 0.78, 1., 1.)),
            ] {
                light.oval(
                    Layer::Wash,
                    p + n * side * drift - d * r * 0.25,
                    d * r * 1.7,
                    r * 0.72,
                    alpha(tint, 0.42 * o),
                );
            }
            light.glint(
                p,
                r * 2.1,
                heading.y.atan2(heading.x) + PI / 4.,
                alpha(light.hot(color, 0.8), 0.46 * o),
            );
        }
        Fuel::Void => {
            // Dark at the heart, with light bending around its rim.
            polygon(
                p,
                0,
                r * 0.56,
                0.,
                alpha(Color::new(0.04, 0.02, 0.09, 1.), opacity),
            );
            let spin = clock * 4.;
            for side in [0., PI] {
                light.arc(
                    Layer::Core,
                    p,
                    r * 0.84,
                    (r * 0.13).max(0.8),
                    spin + side,
                    TAU * 0.36,
                    alpha(light.hot(color, 0.6), 0.95 * o),
                );
            }
        }
    }
}

/// A jagged line of electricity from `from` to `to`.
pub(super) fn bolt(light: &mut Emission, from: Vec2, to: Vec2, half: f32, color: Color, seed: f32) {
    const POINTS: usize = 6;
    let along = to - from;
    let length = along.length();
    if length < 1. {
        return;
    }
    let n = perpendicular(along / length);
    let mut stops = [Stop {
        at: from,
        half,
        color,
    }; POINTS];
    for (index, stop) in stops.iter_mut().enumerate() {
        let u = index as f32 / (POINTS - 1) as f32;
        let jag = if index == 0 || index + 1 == POINTS {
            0.
        } else {
            (seed * (index as f32 * 7.3 + 1.)).sin() * length * 0.17
        };
        *stop = Stop {
            at: from + along * u + n * jag,
            half: half * (1. - u * 0.6),
            color: alpha(color, color.a * (1. - u * 0.75)),
        };
    }
    light.band(Layer::Core, &stops);
}

/// The light on an ordinary tank's round: a small halo and a warm heart, so
/// shots read as bright without changing their shape.
pub fn tracer(light: &mut Emission, round: &Round, level: f32) {
    let Round {
        p,
        r,
        heading,
        color,
        opacity,
        ..
    } = *round;
    let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
    let strength = opacity * level.min(1.);
    light.oval(
        Layer::Wash,
        p - d * r * 0.5,
        d * r * 2.5,
        r * 1.65,
        alpha(color, 0.2 * strength),
    );
    light.spot(
        Layer::Core,
        p,
        r * 0.66,
        alpha(light.hot(color, 0.8), 0.34 * strength),
    );
}

/// Draw `trail` as a tapered ribbon ending at `head`.
pub(super) fn band_from_trail(
    light: &mut Emission,
    view: &View,
    trail: &Trail,
    head: Vec2,
    r: f32,
    strength: f32,
) {
    let count = usize::from(trail.count);
    if count < 2 || strength <= 0.01 {
        return;
    }
    let color = if trail.fuel == Fuel::Fire {
        ramp(0.3)
    } else {
        trail.color
    };
    let deep = if trail.fuel == Fuel::Fire {
        ramp(0.7)
    } else {
        mix(color, Color::new(0.5, 0.1, 0.1, 1.), 0.5)
    };
    let clear = Stop {
        at: head,
        half: 0.,
        color: alpha(color, 0.),
    };
    let mut wash = [clear; TRAIL_POINTS + 1];
    let mut heart = [clear; TRAIL_POINTS + 1];
    for age in 0..count {
        // The oldest point first, so the band runs tail to head.
        let index = count - 1 - age;
        let u = (age as f32 + 1.) / count as f32;
        let at = view.screen(trail.point(age));
        let at = if age == 0 { at.lerp(head, 0.4) } else { at };
        let body = 1. - u;
        let tint = mix(color, deep, u);
        wash[index] = Stop {
            at,
            half: r * (1.3 * body + 0.14),
            color: alpha(tint, 0.5 * body.powf(1.25) * strength),
        };
        heart[index] = Stop {
            at,
            half: r * (0.52 * body + 0.05),
            color: alpha(
                light.hot(tint, 0.42 * body),
                0.85 * body.powf(1.5) * strength,
            ),
        };
    }
    wash[count] = Stop {
        at: head,
        half: r * 1.35,
        color: alpha(color, 0.54 * strength),
    };
    heart[count] = Stop {
        at: head,
        half: r * 0.56,
        color: alpha(light.hot(color, 0.6), 0.85 * strength),
    };
    light.band(Layer::Wash, &wash[..=count]);
    light.band(Layer::Core, &heart[..=count]);
}
