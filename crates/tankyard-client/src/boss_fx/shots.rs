//! Boss rounds in flight. Each has a solid body, so it can be seen and dodged
//! whatever the settings, with its heart, halo and trail drawn in light, and
//! each boss's guns burn in their own way.
use super::fire::{ramp, steel};
use super::ribbon::{FINE, PLAIN, Path, Section, Sway, keyed, swell};
use super::{BossFx, Fuel, Round, Trail, perpendicular};
use crate::emission::{Emission, Layer, Stop, heated};
use crate::render::{View, polygon};
use crate::ui::{AMBER, alpha, mix};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};
use tankyard_client::tank_model::OUTLINE;
use tankyard_content::ProjectileKind;
use tankyard_core::{BASTION_ROUND_DAMAGE, LETHAL_DAMAGE};

impl BossFx {
    fn fuel_of(&self, round: &Round) -> Fuel {
        self.bosses
            .get(&round.owner)
            .map_or(Fuel::Plasma, |presence| presence.fuel)
    }
    /// A clock for a round's flicker, different for each round and still
    /// with motion reduced.
    pub(super) fn flicker(&self, round: &Round) -> f32 {
        if self.look.calm {
            0.
        } else {
            round.age + (round.id % 97) as f32 * 0.37
        }
    }

    /// One boss round.
    pub fn round(&self, light: &mut Emission, view: &View, round: &Round) {
        if self.is_star_round(round.owner, round.id) {
            self.star_round(light, view, round);
            return;
        }
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
        // The Railgunner's cannon, and the echo that follows it.
        if fuel == Fuel::Ion && kind == ProjectileKind::Shell {
            self.rail_round(light, round);
            return;
        }
        let path = self.path_of(view, round);
        if damage >= LETHAL_DAMAGE {
            fireball(light, round, &path, clock, level);
            return;
        }
        if damage >= BASTION_ROUND_DAMAGE {
            self.trail(light, view, round, &path, 0.8);
            slug(light, round, &path, clock, level);
            return;
        }
        self.trail(light, view, round, &path, 1.);
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
            flourish(light, round, &path, fuel, clock, level);
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
                let path = self.path_of(view, round);
                self.trail(light, view, round, &path, 0.62);
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

    /// The road a round has flown. One with no trail, as with effects off or
    /// just after launch, gets a straight run behind it, never further back
    /// than it has flown, so nothing pokes out through the back of the gun.
    pub(super) fn path_of(&self, view: &View, round: &Round) -> Path {
        let shortest = round.r * 0.8;
        if let Some(trail) = self.trails.get(&round.id) {
            let path = Path::of(trail, view, round.p, round.heading);
            if path.length() >= shortest {
                return path;
            }
        }
        let flown = (round.age * round.heading.length() * round.z).max(shortest);
        let way = -round.heading.try_normalize().unwrap_or(vec2(1., 0.));
        Path::straight(round.p, way, flown.min(round.r * 8.5))
    }

    /// A round's bright trail, from where it has been up to its body.
    pub(super) fn trail(
        &self,
        light: &mut Emission,
        view: &View,
        round: &Round,
        path: &Path,
        width: f32,
    ) {
        if !self.look.live() {
            return;
        }
        if let Some(trail) = self.trails.get(&round.id) {
            band_from_trail(
                light,
                view,
                trail,
                path,
                round.r * width,
                round.opacity * self.look.level(),
            );
        }
    }
}

/// Bastion's cannon round: a steel slug with a white-hot nose, dragging a
/// wedge of blue light down the line it has really flown, with two lines of
/// shock running off its shoulders and a pair of bright threads winding
/// around each other behind it.
fn slug(light: &mut Emission, round: &Round, path: &Path, clock: f32, level: f32) {
    let Round {
        p,
        r,
        heading,
        color,
        opacity,
        z,
        ..
    } = *round;
    let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
    let o = opacity * level.min(1.);
    let pale = light.pale_floor();
    let paint = |t: f32| steel(if pale { t.max(0.3) } else { t });
    let rim = (1.8 * z).max(1.);
    // The solid body, so it shows on any floor and at any setting.
    polygon(p, 0, r + rim, 0., alpha(mix(color, OUTLINE, 0.8), opacity));
    polygon(p, 0, r, 0., alpha(mix(color, steel(0.3), 0.6), opacity));
    polygon(p, 0, r * 0.68, 0., alpha(paint(0.12), opacity));
    let reach = path.length().min(r * 7.).max(r * 0.8);
    let lead = r * 0.9;
    let sections = if light.rich() { FINE } else { PLAIN };
    let moving = clock != 0.;
    let sway = if moving {
        Sway::of(Fuel::Plasma, r, clock)
    } else {
        Sway::STILL
    };
    let line = path.line(lead, reach, sections, sway);
    light.spot(Layer::Wash, p, r * 3.8, alpha(paint(0.55), 0.34 * o));
    let wash = [
        (-r * 0.9, r * 0.1, 0.3, 0.),
        (-r * 0.2, r * 1.05, 0.35, 0.6),
        (reach * 0.4, r * 0.8, 0.5, 0.42),
        (reach, r * 0.08, 0.9, 0.),
    ];
    line.band(light, Layer::Wash, |_, at| {
        let (half, heat, strength) = keyed(&wash, at);
        Section {
            half,
            color: alpha(paint(heat), strength * o),
            side: 0.,
        }
    });
    let heart = [
        (-r * 0.74, r * 0.08, 0.05, 0.),
        (-r * 0.1, r * 0.62, 0.04, 0.96),
        (reach * 0.34, r * 0.3, 0.2, 0.62),
        (reach * 0.8, r * 0.04, 0.4, 0.),
        (reach, r * 0.02, 0.4, 0.),
    ];
    line.band(light, Layer::Core, |_, at| {
        let (half, heat, strength) = keyed(&heart, at);
        Section {
            half,
            color: alpha(paint(heat), strength * o),
            side: 0.,
        }
    });
    let shock = heated(paint(0.2), 0.4, pale);
    let thin = (r * 0.06).max(0.9);
    for side in [-1., 1.] {
        line.band(light, Layer::Core, |_, at| {
            let u = (at / (reach * 0.72)).clamp(0., 1.);
            // Comes up from nothing at the shoulder, and thins away.
            let strength = 0.8 * swell(at + r * 0.15, r * 0.7) * (1. - u);
            Section {
                half: thin * (1. - 0.5 * u),
                color: alpha(shock, strength * o),
                side: side * r * (0.96 + 0.34 * u),
            }
        });
    }
    if moving && light.rich() {
        twist(
            light,
            &line,
            path,
            (reach, r),
            clock,
            alpha(heated(paint(0.1), 0.5, pale), o),
        );
    }
    light.spot(
        Layer::Core,
        p + d * r * 0.35,
        r * 0.62,
        alpha(heated(paint(0.05), 1., pale), o),
    );
}

/// Two bright threads winding around each other down a heavy round's wake,
/// brighter on the near side of each turn. The winding is fixed to the air, so
/// the round draws it out rather than dragging it along.
fn twist(
    light: &mut Emission,
    line: &super::ribbon::Line,
    path: &Path,
    (reach, r): (f32, f32),
    clock: f32,
    color: Color,
) {
    for strand in [0., PI] {
        line.band(light, Layer::Core, |_, at| {
            let u = (at / reach.max(1.)).clamp(0., 1.);
            let turn = path.in_air(at) / (r * 1.5) - clock * 1.7 + strand;
            let near = 0.45 + 0.55 * turn.cos().max(0.);
            let body = swell(at, r * 1.2) * (1. - u).powf(1.3);
            Section {
                half: (r * 0.09).max(0.8) * (0.5 + 0.5 * near),
                color: alpha(color, color.a * 0.6 * near * body),
                side: turn.sin() * r * 0.62 * (1. - u * 0.6),
            }
        });
    }
}

/// The Siege Engine's round: a white-hot heart in a teardrop of fire that
/// bends with its flight, with tongues of flame licking back along the line
/// it has flown. The stream it leaves in the air is made of real puffs of
/// flame, added as it flies.
fn fireball(light: &mut Emission, round: &Round, path: &Path, clock: f32, level: f32) {
    let Round {
        p,
        r,
        heading,
        color,
        opacity,
        z,
        ..
    } = *round;
    let o = opacity * level.min(1.);
    let pale = light.pale_floor();
    let heat = |t: f32| ramp(if pale { t.max(0.22) } else { t });
    let rim = (1.8 * z).max(1.);
    // The solid body, so it shows on any floor and at any setting.
    polygon(p, 0, r + rim, 0., alpha(mix(color, OUTLINE, 0.8), opacity));
    polygon(p, 0, r, 0., alpha(Color::new(1., 0.42, 0.1, 1.), opacity));
    let reach = path.length().min(r * 8.5).max(r * 0.8);
    let lead = r * 0.95;
    let sections = if light.rich() { FINE } else { PLAIN };
    let moving = clock != 0.;
    let sway = if moving {
        Sway::of(Fuel::Fire, r, clock)
    } else {
        Sway::STILL
    };
    let line = path.line(lead, reach, sections, sway);
    let (behind, _) = path.at(r, &mut 0);
    light.spot(Layer::Wash, behind, r * 5., alpha(heat(0.5), 0.3 * o));
    let wash = [
        (-r * 0.95, r * 0.12, 0.3, 0.),
        (-r * 0.3, r * 1.08, 0.3, 0.62),
        (reach * 0.26, r * 1.1, 0.36, 0.66),
        (reach * 0.6, r * 0.66, 0.58, 0.42),
        (reach, r * 0.1, 0.84, 0.),
    ];
    line.band(light, Layer::Wash, |_, at| {
        let (half, t, strength) = keyed(&wash, at);
        Section {
            half,
            color: alpha(heat(t), strength * o),
            side: 0.,
        }
    });
    let heart = [
        (-r * 0.78, r * 0.1, 0.1, 0.),
        (-r * 0.12, r * 0.76, 0.05, 0.96),
        (reach * 0.28, r * 0.58, 0.14, 0.84),
        (reach * 0.62, r * 0.26, 0.3, 0.5),
        (reach * 0.86, r * 0.05, 0.42, 0.),
        (reach, r * 0.03, 0.42, 0.),
    ];
    line.band(light, Layer::Core, |_, at| {
        let (half, t, strength) = keyed(&heart, at);
        // The hot middle weaves a little inside the flame around it.
        let weave = if moving {
            (path.in_air(at) / (r * 1.7) - clock * 3.4).sin()
                * r
                * 0.12
                * (at / reach).clamp(0., 1.)
        } else {
            0.
        };
        Section {
            half,
            color: alpha(heat(t), strength * o),
            side: weave,
        }
    });
    if moving && light.rich() {
        twist(
            light,
            &line,
            path,
            (reach, r),
            clock,
            alpha(heat(0.12), 0.8 * o),
        );
    }
    // Tongues of flame, each one peeling off the road the round has flown.
    for lick in 0..7 {
        let phase = lick as f32 * 2.399_963;
        let side = lick as f32 / 3. - 1.;
        let sway = (clock * (9. + lick as f32) + phase).sin();
        let length = r * (0.75 + 0.3 * (clock * 13. + phase * 2.).sin());
        let back = r * (0.3 + 0.28 * phase.sin().abs()) + length * 0.72;
        let (at, way) = path.at(back.min(path.length()), &mut 0);
        let across = perpendicular(way);
        light.oval(
            Layer::Wash,
            at + across * (side * r * (0.92 + 0.1 * sway) + sway * r * 0.1),
            way * length,
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
fn flourish(light: &mut Emission, round: &Round, path: &Path, fuel: Fuel, clock: f32, level: f32) {
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
        Fuel::Plasma | Fuel::Star | Fuel::Ion | Fuel::Flesh => {}
        Fuel::Fire => {
            // Burning fuel: flame licking off the back of the round.
            for lick in 0..3 {
                let phase = lick as f32 * 2.1;
                let sway = (clock * 12. + phase).sin();
                let length = r * (0.9 + 0.3 * (clock * 15. + phase).sin());
                let (at, way) = path.at((r * 0.6 + length).min(path.length()), &mut 0);
                light.oval(
                    Layer::Wash,
                    at + perpendicular(way) * (lick as f32 - 1.) * r * (0.5 + 0.1 * sway),
                    way * length,
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

/// A line of electricity from `from` to `to`, pinned at both ends and crooked
/// at three scales at once, so it looks grown rather than drawn, in a soft glow
/// of its own.
pub(super) fn bolt(light: &mut Emission, from: Vec2, to: Vec2, half: f32, color: Color, seed: f32) {
    const POINTS: usize = 11;
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
        let crook = (u * 6.1 + seed).sin()
            + 0.5 * (u * 13.7 + seed * 1.7).sin()
            + 0.26 * (u * 29.3 - seed * 2.3).sin();
        let jag = (u * PI).sin().sqrt() * crook * length * 0.085;
        *stop = Stop {
            at: from + along * u + n * jag,
            half: half * (1. - u * 0.72),
            color: alpha(color, color.a * (1. - u * u * 0.85)),
        };
    }
    let mut glow = stops;
    for stop in &mut glow {
        stop.half = stop.half * 3.4 + 0.6;
        stop.color.a *= 0.24;
    }
    light.band(Layer::Wash, &glow);
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

/// Draw `trail` as a ribbon of light along `path`: hottest just behind the
/// round, cooling and thinning to nothing at its tail.
pub(super) fn band_from_trail(
    light: &mut Emission,
    view: &View,
    trail: &Trail,
    path: &Path,
    r: f32,
    strength: f32,
) {
    let reach = path.length().min(trail.reach() * view.zoom);
    if reach < 1. || strength <= 0.01 {
        return;
    }
    let (color, deep) = if trail.fuel == Fuel::Fire {
        (ramp(0.24), ramp(0.74))
    } else {
        let c = trail.color;
        (c, Color::new(c.r * 0.52, c.g * 0.52, c.b * 0.52, 1.))
    };
    let pale = light.pale_floor();
    let rich = light.rich();
    let sections = if rich { FINE } else { PLAIN };
    let clock = trail.clock + trail.seed;
    let line = path.line(0., reach, sections, Sway::of(trail.fuel, r, clock));
    // It opens over about the round's own width, behind its body.
    let start = (r * 0.8 / reach).clamp(0.05, 0.32);
    // Thicker and thinner patches of gas, which stay where they were left.
    let dense = |at: f32| {
        let place = path.in_air(at) / (r * 2.4).max(5.) + trail.seed;
        0.8 + 0.2 * (0.6 * place.sin() + 0.4 * (place * 2.3 + clock * 1.1).sin())
    };
    line.band(light, Layer::Wash, |t, at| {
        let (body, open) = (1. - t, swell(t, start));
        Section {
            // Spreading as it ages, so it fades out wide rather than to a point.
            half: r * (1.06 * body + 0.6 * t + 0.14) * (0.42 + 0.58 * open),
            color: alpha(
                mix(color, deep, t),
                0.5 * open * body.powf(1.25) * strength * dense(at),
            ),
            side: 0.,
        }
    });
    line.band(light, Layer::Core, |t, at| {
        let (body, open) = (1. - t, swell(t, start));
        Section {
            half: r * (0.52 * body + 0.05) * (0.42 + 0.58 * open),
            color: alpha(
                heated(mix(color, deep, t), 0.5 * body, pale),
                0.85 * open * body.powf(1.5) * strength * dense(at),
            ),
            // The bright thread wanders a little inside its glow.
            side: (path.in_air(at) / (r * 2.1).max(4.) - clock * 2.).sin() * r * 0.1 * t,
        }
    });
    if !rich {
        return;
    }
    // Two wisps peeling away from it as the gas comes apart.
    for (side, start_at) in [(-1., 0.7), (1., 2.9)] {
        line.band(light, Layer::Wash, |t, at| {
            let place = path.in_air(at) / (r * 1.9).max(4.) + start_at + trail.seed;
            Section {
                half: r * (0.1 + 0.16 * t),
                color: alpha(
                    mix(color, deep, t * 0.7),
                    0.36 * swell(t, start * 1.6)
                        * (1. - t).powf(1.6)
                        * strength
                        * (0.55 + 0.45 * place.sin()),
                ),
                side: side
                    * r
                    * (0.3 + 0.75 * t.powf(0.8))
                    * (0.72 + 0.28 * (place * 1.7 - clock * 1.2).sin()),
            }
        });
    }
}
