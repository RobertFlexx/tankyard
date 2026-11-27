//! The light a boss carries: its core, the pool it casts, an edge of light
//! that follows its real armor, and the burning of a wounded hull.
use super::fire::ramp;
use super::ribbon::{FINE, PLAIN, Path, Section, Sway, swell};
use super::{BossFx, Fuel, direction};
use crate::emission::{Emission, Layer, Stop};
use crate::render::View;
use crate::ui::{alpha, mix};
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
        let (source, slot) = boss.attack_style();
        let special = tankyard_core::encounters::HeavyAttack::for_slot(source, slot);
        let color = match boss.kind {
            tankyard_core::BossKind::Jester => {
                if source == boss.kind || special.is_some() {
                    tankyard_client::boss_model::silk(time * 0.055 + 0.85)
                } else {
                    tankyard_client::boss_model::boss_color(source)
                }
            }
            tankyard_core::BossKind::Carillon => tankyard_client::boss_model::FURNACE,
            _ => shown.color,
        };
        let color = if boss.kind != tankyard_core::BossKind::Jester {
            special.map_or(color, |attack| {
                mix(color, super::warnings::heavy_tint(attack), charge)
            })
        } else {
            color
        };
        let calm = self.look.calm;
        let pulse = if calm {
            0.5
        } else if boss.phase >= 3 {
            0.5 + 0.5 * (time * 4.8).sin()
        } else {
            0.5 + 0.5 * (time * 1.6).sin()
        };
        if boss.kind == tankyard_core::BossKind::Jester && self.look.live() {
            let tips = tankyard_client::boss_model::bell_tips(
                p,
                r,
                shown.aim,
                shown.bells.map(|b| b.angle),
            );
            for (i, at) in tips.into_iter().enumerate() {
                let dye = tankyard_client::boss_model::silk(time * 0.055 + i as f32 / 3. + 0.17);
                light.spot(
                    Layer::Wash,
                    at,
                    r * (0.32 + 0.17 * charge),
                    alpha(dye, (0.3 + charge * 0.4) * level),
                );
            }
        }
        if boss.kind == tankyard_core::BossKind::Carillon && self.look.live() {
            // Furnace draft climbs up the two stacks, then strikes the nozzle
            // collars. The same local offsets place the ironwork on the hull.
            let forward = direction(shown.aim);
            for side in [-1., 1.] {
                let stack = p + forward.rotate(vec2(-0.75, side * 0.38)) * r;
                let heat = (0.38 + 0.16 * pulse + charge * 0.43) * level;
                light.spot(Layer::Wash, stack, r * 0.34, alpha(ramp(0.63), heat * 0.65));
                light.spot(Layer::Core, stack, r * 0.086, alpha(ramp(0.12), heat));
            }
            if charge > 0.03 {
                for lane in [-0.25, 0., 0.25] {
                    let gun = direction(shown.aim + lane);
                    let lip = p + gun * r * 1.48;
                    light.streak(
                        Layer::Core,
                        lip - gun.perp() * r * 0.095,
                        lip + gun.perp() * r * 0.095,
                        (r * 0.013).max(0.7),
                        alpha(ramp(0.32), charge),
                        alpha(ramp(0.12), charge),
                    );
                }
            }
        }
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

impl BossFx {
    /// What its drive leaves in the air: two wisps of hot gas that stay where
    /// they were laid, so they show every turn it really made, spread as they
    /// cool, and keep going for a moment after it stops.
    pub fn exhaust(&self, light: &mut Emission, view: &View, boss: &Boss) {
        if !self.look.live() {
            return;
        }
        let Some(shown) = self.bosses.get(&boss.id) else {
            return;
        };
        let level = self.look.level().min(1.);
        let nozzle = (boss.radius * view.zoom * 0.085).max(1.6);
        let sections = if light.rich() { FINE } else { PLAIN };
        for wisp in &shown.exhaust {
            if wisp.count < 2 || !view.visible(wisp.latest, 420.) {
                continue;
            }
            let path = Path::of(wisp, view, view.screen(wisp.latest), Vec2::ZERO);
            let length = path.length();
            if length < 4. {
                continue;
            }
            let base = wisp.color;
            let (hot, deep) = if wisp.fuel == Fuel::Fire {
                (ramp(0.1), ramp(0.72))
            } else {
                (
                    light.hot(base, 0.55),
                    Color::new(base.r * 0.45, base.g * 0.45, base.b * 0.45, 1.),
                )
            };
            let sway = Sway::of(wisp.fuel, nozzle * 1.7, wisp.clock + wisp.seed);
            let line = path.line(0., length, sections, sway);
            let life = wisp.linger;
            // How old the gas is, and how far it has opened out of the nozzle.
            let state = |at: f32| ((path.age(at) / life).clamp(0., 1.), swell(at, nozzle * 2.2));
            line.band(light, Layer::Wash, |_, at| {
                let (u, joined) = state(at);
                Section {
                    half: nozzle * (2. + 3.8 * u.sqrt()),
                    color: alpha(
                        mix(base, deep, u),
                        0.34 * joined * (1. - u).powf(1.6) * level,
                    ),
                    side: 0.,
                }
            });
            line.band(light, Layer::Core, |_, at| {
                let (u, joined) = state(at);
                Section {
                    half: nozzle * (0.5 + 0.35 * u),
                    color: alpha(
                        mix(hot, base, (u * 1.6).min(1.)),
                        0.5 * joined * (1. - u).powi(3) * level,
                    ),
                    side: 0.,
                }
            });
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
