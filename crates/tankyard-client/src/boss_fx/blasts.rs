//! What boss fire sets off: flashes, shock waves, sparks, smoke and the
//! scorch marks heavy blasts leave on the floor.
use super::fire::ramp;
use super::ribbon::Path;
use super::shots::{band_from_trail, bolt};
use super::{
    BossFx, Burst, Flash, Fuel, SCAR_SECONDS, WAKE_SECONDS, direction, ease_out, perpendicular,
};
use crate::emission::{Emission, Layer};
use crate::render::View;
use crate::ui::{AMBER, RED, alpha, mix};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};
use tankyard_client::boss_model::AXIOM_TEAL;

/// How dark a full-sized scorch mark is.
const SCAR_SHADE: f32 = 0.42;
/// The radius at which a mark counts as a full-sized crater.
const BIG_SCAR: f32 = 90.;

impl BossFx {
    /// The soot heavy blasts leave. Drawn under everything standing on the
    /// floor, so tanks and shapes pass over it.
    pub fn scars(&self, view: &View) {
        for scar in &self.scars {
            if !view.visible(scar.at, scar.radius * 1.2) {
                continue;
            }
            // A cannon's crater stays longest and darkest; a small blast
            // leaves a light mark that's gone sooner.
            let weight = (scar.radius / BIG_SCAR).clamp(0.5, 1.);
            let t = (scar.age / (SCAR_SECONDS * weight)).clamp(0., 1.);
            // Holds, then fades through its last half.
            let fade = (1. - (t - 0.5).max(0.) * 2.).clamp(0., 1.);
            let shade = SCAR_SHADE * weight * fade * fade.min((scar.age / 0.08).min(1.));
            if shade > 0.004 {
                soot(
                    view.screen(scar.at),
                    scar.radius * view.zoom,
                    scar.at.x * 0.13 + scar.at.y * 0.29,
                    shade,
                );
            }
        }
    }

    /// The trails of rounds already gone, fading where they lie. They run
    /// along the floor with the rounds, so they go down before the tanks.
    pub fn wakes(&self, light: &mut Emission, view: &View) {
        if !self.look.live() {
            return;
        }
        let level = self.look.level();
        for wake in &self.wakes {
            let fade = (wake.life / WAKE_SECONDS).clamp(0., 1.);
            let head = view.screen(wake.trail.latest);
            band_from_trail(
                light,
                view,
                &wake.trail,
                &Path::of(&wake.trail, view, head, Vec2::ZERO),
                wake.trail.radius * view.zoom,
                fade * fade * level,
            );
        }
    }

    /// Everything happening apart from the bosses and their rounds: flame,
    /// flashes, shock waves and sparks.
    pub fn aftermath(&self, light: &mut Emission, view: &View) {
        if !self.look.live() {
            return;
        }
        let level = self.look.level();
        self.draw_flames(light, view, level);
        for burst in &self.bursts {
            if view.visible(burst.at, burst.size * 8.) {
                draw_burst(light, view, burst, level);
            }
        }
        self.draw_casts(light, view, level);
        for spark in &self.sparks {
            if !view.visible(spark.pos, 40.) {
                continue;
            }
            let fade = (spark.life / spark.total).clamp(0., 1.);
            let p = view.screen(spark.pos);
            let speed = spark.vel.length();
            let length = (speed * 0.05).clamp(1.5, 26.) * view.zoom;
            let back = vec2(spark.vel.x, spark.vel.y) / speed.max(1.);
            let size = (spark.size * view.zoom * (0.5 + 0.5 * fade)).max(0.7);
            // Cooling as they go: white, then their color, then deep red.
            let tint = if fade > 0.55 {
                light.hot(spark.color, (fade - 0.55) * 1.6)
            } else {
                mix(
                    spark.color,
                    Color::new(0.75, 0.16, 0.08, 1.),
                    1. - fade / 0.55,
                )
            };
            let strength = (fade * 1.25).min(1.) * level.min(1.);
            light.streak(
                Layer::Core,
                p - back * length,
                p,
                size,
                alpha(tint, 0.),
                alpha(tint, strength),
            );
            if light.rich() {
                light.spot(Layer::Wash, p, size * 3.4, alpha(tint, 0.15 * strength));
            }
        }
    }
}

/// A patch of burnt floor: even in the middle, with a ragged, soft rim, so it
/// doesn't read as a dark dot. `seed` keeps each mark's outline its own.
fn soot(p: Vec2, radius: f32, seed: f32, shade: f32) {
    const SIDES: usize = 22;
    let ink = Color::new(0.045, 0.035, 0.035, shade);
    let edge = Color { a: 0., ..ink };
    let mut vertices = [crate::lighting::vertex(p, ink); 1 + SIDES * 2];
    let mut indices = [0u16; SIDES * 9];
    for i in 0..SIDES {
        let turn = i as f32 / SIDES as f32 * TAU;
        let reach = radius * soot_rim(turn, seed);
        let out = vec2(turn.cos(), turn.sin()) * reach;
        vertices[1 + i * 2] = crate::lighting::vertex(p + out * 0.42, ink);
        vertices[2 + i * 2] = crate::lighting::vertex(p + out, edge);
        let next = (i + 1) % SIDES;
        let (solid, rim) = ((1 + i * 2) as u16, (2 + i * 2) as u16);
        let (next_solid, next_rim) = ((1 + next * 2) as u16, (2 + next * 2) as u16);
        indices[i * 9..(i + 1) * 9].copy_from_slice(&[
            0, solid, next_solid, solid, rim, next_solid, rim, next_rim, next_solid,
        ]);
    }
    crate::lighting::geometry(&vertices, &indices);
}

/// How far a scorch mark's rim reaches at `turn`, as a share of its radius.
fn soot_rim(turn: f32, seed: f32) -> f32 {
    0.84 + 0.1 * (turn * 3. + seed).sin() + 0.06 * (turn * 7. - seed * 1.7).cos()
}

fn muzzle(light: &mut Emission, p: Vec2, d: Vec2, burst: &Burst, s: f32, t: f32, level: f32) {
    let fuel = burst.fuel;
    let fade = 1. - t;
    let n = perpendicular(d);
    let reach = s * (1.7 + 2.5 * ease_out(t));
    let c = if fuel == Fuel::Fire {
        ramp(0.26)
    } else {
        burst.color
    };
    light.spot(
        Layer::Wash,
        p + d * s * 0.9,
        s * (2.7 + 1.2 * t),
        alpha(c, 0.46 * fade * fade * level),
    );
    light.oval(
        Layer::Core,
        p + d * reach * 0.46,
        d * reach * 0.62,
        s * (0.66 - 0.34 * t),
        alpha(light.hot(c, 0.92), 0.95 * fade * level),
    );
    match fuel {
        Fuel::Arc => {
            // Forks of electricity leap from the barrel.
            let seed = (t * 5.).floor() * 2.3 + p.x * 0.013;
            for fork in -1..=1 {
                let spread = fork as f32 * 0.55 + (seed + fork as f32).sin() * 0.14;
                let to = p + (d + n * spread).normalize_or_zero() * reach * 1.35;
                bolt(
                    light,
                    p,
                    to,
                    (s * 0.14).max(0.8),
                    alpha(light.hot(c, 0.6), 0.9 * fade * level),
                    seed + fork as f32 * 3.1,
                );
            }
        }
        Fuel::Spectrum => {
            // The flash splits into a fan of color.
            for (side, tint) in [
                (-1., Color::new(1., 0.36, 0.72, 1.)),
                (1., Color::new(0.34, 0.78, 1., 1.)),
            ] {
                let petal = (d + n * side * 0.34).normalize_or_zero();
                light.oval(
                    Layer::Wash,
                    p + petal * reach * 0.55,
                    petal * reach * 0.6,
                    s * 0.46,
                    alpha(tint, 0.62 * fade * level),
                );
            }
        }
        Fuel::Void => {
            // Light drawn inward, the reverse of a flash.
            light.ring(
                Layer::Core,
                p,
                s * (2.5 - 2. * ease_out(t)),
                (s * 0.12).max(0.8),
                alpha(light.hot(c, 0.5), 0.8 * fade * level),
            );
        }
        Fuel::Star => {
            // A four-point star at the barrel's mouth, gold over teal.
            light.glint(
                p + d * s * 0.3,
                s * (2.6 - 0.9 * t),
                d.y.atan2(d.x) + PI / 4.,
                alpha(light.hot(c, 0.95), 0.9 * fade * level),
            );
            light.ring(
                Layer::Core,
                p + d * s * 0.3,
                s * (0.7 + 1.5 * ease_out(t)),
                (s * 0.07).max(0.8),
                alpha(light.hot(AXIOM_TEAL, 0.4), 0.7 * fade * fade * level),
            );
        }
        Fuel::Plasma | Fuel::Fire => {
            for side in [-1., 1.] {
                let petal = (d + n * side * 0.6).normalize_or_zero();
                light.streak(
                    Layer::Core,
                    p,
                    p + petal * reach * 0.62,
                    s * 0.28,
                    alpha(c, 0.),
                    alpha(light.hot(c, 0.6), 0.75 * fade * level),
                );
            }
        }
    }
}

fn draw_burst(light: &mut Emission, view: &View, burst: &Burst, level: f32) {
    let z = view.zoom;
    let p = view.screen(burst.at);
    let t = (burst.age / burst.life).clamp(0., 1.);
    let s = burst.size * z;
    let c = burst.color;
    let fade = 1. - t;
    let level = level.min(1.);
    match burst.flash {
        Flash::Muzzle(d) => muzzle(light, p, d, burst, s, t, level),
        Flash::Hit => {
            let grow = ease_out(t);
            light.spot(
                Layer::Core,
                p,
                s * (1.15 + 0.6 * grow),
                alpha(light.hot(c, 0.92), 0.88 * fade.powi(3) * level),
            );
            light.spot(
                Layer::Wash,
                p,
                s * (3.1 + 1.3 * grow),
                alpha(c, 0.36 * fade * fade * level),
            );
            light.ring(
                Layer::Core,
                p,
                s * (0.8 + 2.3 * grow),
                (s * 0.2 * (1. - t * 0.6)).max(0.8),
                alpha(light.hot(c, 0.5), 0.72 * fade * fade * level),
            );
        }
        Flash::Fizzle => {
            light.spot(
                Layer::Wash,
                p,
                s * (1.5 + t),
                alpha(c, 0.3 * fade * fade * level),
            );
            light.spot(
                Layer::Core,
                p,
                s * (0.7 - 0.3 * t),
                alpha(light.hot(c, 0.6), 0.55 * fade * fade * level),
            );
        }
        Flash::Blast => {
            let grow = ease_out(t);
            light.spot(
                Layer::Wash,
                p,
                s * (4. + 2.6 * grow),
                alpha(mix(c, AMBER, 0.45), 0.55 * fade * fade * level),
            );
            light.spot(
                Layer::Core,
                p,
                s * (1.5 + 0.9 * grow),
                alpha(light.hot(c, 1.), 0.95 * fade.powi(4) * level),
            );
            light.ring(
                Layer::Core,
                p,
                s * (0.9 + 3.4 * grow),
                (s * 0.24 * fade).max(0.8),
                alpha(light.hot(c, 0.6), 0.8 * fade * fade * level),
            );
            light.ring(
                Layer::Wash,
                p,
                s * (0.7 + 2.5 * grow),
                s * 0.7 * fade + 1.,
                alpha(mix(c, RED, 0.3), 0.3 * fade * level),
            );
        }
        Flash::Release => {
            let grow = ease_out(t);
            light.ring(
                Layer::Core,
                p,
                s * (0.95 + 0.9 * grow),
                (s * 0.05 * fade).max(0.9),
                alpha(light.hot(c, 0.7), 0.8 * fade * fade * level),
            );
            light.ring(
                Layer::Wash,
                p,
                s * (0.9 + 0.8 * grow),
                s * 0.22,
                alpha(c, 0.3 * fade * fade * level),
            );
        }
        Flash::Phase => {
            let grow = ease_out(t);
            let late = ease_out((t - 0.12) / 0.88);
            light.spot(
                Layer::Core,
                p,
                s * (1.15 + 0.3 * grow),
                alpha(light.hot(c, 1.), 0.7 * (1. - t * 2.6).max(0.) * level),
            );
            light.spot(
                Layer::Wash,
                p,
                s * (2.6 + 1.6 * grow),
                alpha(c, 0.4 * fade * fade * level),
            );
            light.ring(
                Layer::Core,
                p,
                s * (1. + 2.5 * grow),
                (s * 0.05 * fade).max(1.),
                alpha(light.hot(c, 0.65), 0.85 * fade * fade * level),
            );
            light.ring(
                Layer::Core,
                p,
                s * (0.95 + 1.7 * late),
                (s * 0.03 * fade).max(0.8),
                alpha(light.hot(c, 0.4), 0.6 * fade * fade * level),
            );
            light.ring(
                Layer::Wash,
                p,
                s * (1. + 2.3 * grow),
                s * 0.32 * fade + 1.,
                alpha(c, 0.26 * fade * level),
            );
        }
        Flash::Fall => {
            let blast = ease_out(t / 0.62);
            let second = ease_out((t - 0.1) / 0.75);
            let third = ease_out((t - 0.22) / 0.78);
            // A white-out, gone within the first fifth.
            let white = (1. - t / 0.2).max(0.);
            light.spot(
                Layer::Core,
                p,
                s * (2.2 + 2.6 * blast),
                alpha(light.hot(c, 1.), 0.92 * white * white * level),
            );
            light.spot(
                Layer::Wash,
                p,
                s * (3.6 + 3.4 * blast),
                alpha(mix(c, AMBER, 0.4), 0.56 * fade.powi(3) * level),
            );
            light.spot(
                Layer::Wash,
                p,
                s * (1.6 + 0.9 * blast),
                alpha(
                    mix(mix(c, AMBER, 0.5), Color::new(0.8, 0.15, 0.08, 1.), t),
                    0.6 * fade * fade * level,
                ),
            );
            for (grow, width, strength) in
                [(blast, 0.06, 0.9), (second, 0.04, 0.6), (third, 0.03, 0.4)]
            {
                let ring_fade = (1. - grow).max(0.) * fade;
                light.ring(
                    Layer::Core,
                    p,
                    s * (0.9 + 5.2 * grow),
                    (s * width * (0.4 + ring_fade)).max(1.),
                    alpha(light.hot(c, 0.65), strength * ring_fade.sqrt() * level),
                );
                light.ring(
                    Layer::Wash,
                    p,
                    s * (0.9 + 5. * grow),
                    s * 0.34 * ring_fade + 1.,
                    alpha(c, 0.28 * strength * ring_fade * level),
                );
            }
        }
        Flash::Arrival => {
            let settle = ease_out(t);
            for (index, lead) in [0., 0.14, 0.28].into_iter().enumerate() {
                let u = ((t - lead) / (1. - lead)).clamp(0., 1.);
                let radius = s * (4.2 - 3.1 * ease_out(u));
                let strength = (u * PI).sin();
                light.ring(
                    Layer::Core,
                    p,
                    radius,
                    (s * 0.035).max(1.),
                    alpha(light.hot(c, 0.55), 0.8 * strength * level),
                );
                light.ring(
                    Layer::Wash,
                    p,
                    radius,
                    s * 0.2,
                    alpha(c, 0.2 * strength * level),
                );
                let angle = settle * 2.1 * if index % 2 == 0 { 1. } else { -1. };
                light.arc(
                    Layer::Core,
                    p,
                    radius * 1.09,
                    (s * 0.05).max(1.),
                    angle + index as f32 * 2.1,
                    1.5,
                    alpha(light.hot(c, 0.7), 0.7 * strength * level),
                );
            }
            light.spot(
                Layer::Wash,
                p,
                s * (1.6 + 1.1 * settle),
                alpha(c, 0.3 * (t * PI).sin() * level),
            );
        }
        Flash::Smoke => {
            let grow = ease_out(t);
            let drift = direction(burst.age * 0.6 + burst.size) * s * 0.1 * grow;
            light.spot(
                Layer::Smoke,
                p + drift,
                s * (0.8 + 1.3 * grow),
                alpha(c, 0.34 * (t * PI).sin().powf(0.7) * fade * level),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scorch_marks_rim_stays_ragged_but_inside_its_radius() {
        let mut reaches = Vec::new();
        for seed in [0., 1.7, 42.5, -9.3] {
            for i in 0..360 {
                let reach = soot_rim(i as f32 / 360. * TAU, seed);
                assert!((0.68..=1.).contains(&reach), "{reach}");
                reaches.push(reach);
            }
        }
        let widest = reaches.iter().copied().fold(f32::MIN, f32::max);
        let narrowest = reaches.iter().copied().fold(f32::MAX, f32::min);
        assert!(widest - narrowest > 0.2, "the rim should not be a circle");
    }
}
