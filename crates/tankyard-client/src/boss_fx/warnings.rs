//! The warning before a special attack. Players dodge by these, so they are
//! drawn whatever the Effects setting says, and hold still with motion
//! reduced. Each uses the boss's own color, kept saturated so it reads
//! against the floor, except the Siege Engine's cannon, which burns.
use super::fire::ramp;
use super::{BossFx, direction, ease_out, perpendicular};
use crate::emission::{Emission, Layer, Stop};
use crate::render::View;
use crate::ui::{AMBER, RED, alpha};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};
use tankyard_core::{Boss, BossTactic, LETHAL_DAMAGE, Rect, V2, special_volley};

/// How far along its barrel the Siege Engine's round leaves, in hull radii.
const SIEGE_MUZZLE: f32 = 1.62;

/// A color kept saturated, because light added to a cool floor pulls a pale
/// tint toward grey.
fn signal(tint: Color) -> Color {
    let top = tint.r.max(tint.g).max(tint.b).max(0.001);
    let low = tint.r.min(tint.g).min(tint.b) * 0.82;
    if top - low < 0.001 {
        return Color::new(1., 1., 1., tint.a);
    }
    let push = |channel: f32| ((channel - low) / (top - low)).clamp(0., 1.);
    Color::new(push(tint.r), push(tint.g), push(tint.b), tint.a)
}

impl BossFx {
    /// The lanes its rounds will take, charging up, and a ring closing on
    /// the hull. The Siege Engine's cannon shows its whole line, to the
    /// first cover that stops it.
    pub fn warning(
        &self,
        light: &mut Emission,
        view: &View,
        boss: &Boss,
        at: V2,
        walls: impl Iterator<Item = Rect> + Clone,
        time: f32,
    ) {
        let z = view.zoom;
        let p = view.screen(at);
        let r = boss.radius * z;
        let progress = (1. - boss.attack_windup / boss.kind.windup()).clamp(0., 1.);
        // Its last fifth tightens and brightens: the moment to dodge.
        let armed = ((progress - 0.8) / 0.2).clamp(0., 1.);
        let flow = if self.look.calm { 0. } else { time };
        let tint = signal(
            self.bosses
                .get(&boss.id)
                .map_or(AMBER, |presence| presence.color),
        );
        let stage = (progress, armed);
        let rounds = special_volley(boss.kind, boss.attack, boss.aim, boss.phase);
        let circle = rounds.len() >= 8;
        for (index, round) in rounds.iter().enumerate() {
            let d = direction(round.angle);
            let start = p + d * (r + 6. * z);
            if round.damage >= LETHAL_DAMAGE {
                let reach = round.speed * round.life;
                let from = at + V2::new(d.x, d.y) * boss.radius;
                let to = at + V2::new(d.x, d.y) * reach;
                let stop = walls
                    .clone()
                    .filter_map(|wall| {
                        tankyard_core::physics::sweep_rect(from, to, round.radius, wall)
                    })
                    .map(|(t, _)| t)
                    .fold(1., f32::min);
                let end = view.screen(from.lerp(to, stop));
                cannon_line(
                    light,
                    p + d * r * SIEGE_MUZZLE,
                    end,
                    round.radius * z,
                    stage,
                    flow,
                );
                continue;
            }
            if circle {
                spoke(light, start, d, (round.radius * z).max(2.), tint, stage, z);
            } else {
                let length = (round.speed * round.life.min(1.3)).min(760.) * z;
                lane(
                    light,
                    start,
                    d,
                    length,
                    (round.radius * z).max(1.5),
                    tint,
                    stage,
                    flow + index as f32 * 0.37,
                );
            }
        }
        if circle {
            light.ring(
                Layer::Wash,
                p,
                r + (40. + 70. * progress) * z,
                (3. + 3. * armed) * z,
                alpha(tint, 0.06 + 0.15 * progress),
            );
        }
        closing_ring(light, p, r, z, tint, stage, flow);
    }

    /// The dash a quick boss winds up for: chevrons running ahead of it,
    /// and speed lines streaming off it while it charges.
    pub fn dash(&self, light: &mut Emission, view: &View, boss: &Boss, at: V2, time: f32) {
        if boss.tactic != BossTactic::Windup && boss.tactic != BossTactic::Charge {
            return;
        }
        let z = view.zoom;
        let p = view.screen(at);
        let r = boss.radius * z;
        let d = direction(boss.aim);
        let side = perpendicular(d);
        let flow = if self.look.calm { 0. } else { time };
        let color = self
            .bosses
            .get(&boss.id)
            .map_or(RED, |presence| signal(presence.color));
        if boss.tactic == BossTactic::Windup {
            for n in 0..4 {
                let along = r * (1.3 + n as f32 * 0.7 + (flow * 2.2).fract() * 0.7);
                let tip = p + d * along;
                let width = r * 0.45;
                let fade = 0.82 - n as f32 * 0.18;
                let tint = alpha(color, fade);
                let core = alpha(light.hot(color, 0.55), fade);
                for wing in [side, -side] {
                    let back = tip - d * width * 0.7 + wing * width;
                    light.streak(Layer::Wash, back, tip, (5.5 * z).max(2.), tint, tint);
                    light.streak(Layer::Core, back, tip, (2. * z).max(1.), core, core);
                }
            }
            light.ring(
                Layer::Core,
                p,
                r * 1.25,
                (2.2 * z).max(1.),
                alpha(color, 0.7),
            );
            return;
        }
        if !self.look.live() {
            return;
        }
        for n in 0..7 {
            let offset = (n as f32 - 3.) / 3.;
            let shimmer = 0.75 + 0.25 * (flow * 21. + n as f32 * 1.7).sin();
            let from = p + side * offset * r * 0.92 - d * r * (0.7 + 0.25 * (1. - offset.abs()));
            let to = from - d * r * (1.1 + 0.9 * (1. - offset.abs())) * shimmer;
            light.streak(
                Layer::Wash,
                to,
                from,
                (4.5 * z).max(1.5),
                alpha(color, 0.),
                alpha(color, 0.34 * self.look.level()),
            );
        }
    }
}

/// One lane of a fan: a lit corridor as wide as the round, with soft edges,
/// a fuse burning out along it as the attack charges, and pulses showing the
/// way the rounds will fly.
#[allow(clippy::too_many_arguments)]
fn lane(
    light: &mut Emission,
    start: Vec2,
    d: Vec2,
    length: f32,
    half: f32,
    tint: Color,
    (progress, armed): (f32, f32),
    flow: f32,
) {
    let end = start + d * length;
    let n = perpendicular(d);
    let along = |u: f32| start + d * length * u;
    let wide = half * 1.7;
    let fill = 0.17 + 0.25 * progress + 0.12 * armed;
    light.band(
        Layer::Wash,
        &[
            Stop {
                at: start,
                half: wide * 0.55,
                color: alpha(tint, 0.),
            },
            Stop {
                at: along(0.05),
                half: wide,
                color: alpha(tint, fill),
            },
            Stop {
                at: along(0.5),
                half: wide,
                color: alpha(tint, fill * 0.62),
            },
            Stop {
                at: end,
                half: wide * 0.72,
                color: alpha(tint, 0.),
            },
        ],
    );
    // Faint edges mark exactly how wide the danger is.
    let rail = (half * 0.14).clamp(0.7, 1.8);
    let edge = alpha(light.hot(tint, 0.14), 0.16 + 0.26 * progress + 0.26 * armed);
    for side in [-1., 1.] {
        let offset = n * side * half;
        light.band(
            Layer::Core,
            &[
                Stop {
                    at: start + offset,
                    half: rail * 0.6,
                    color: alpha(edge, 0.),
                },
                Stop {
                    at: along(0.08) + offset,
                    half: rail,
                    color: edge,
                },
                Stop {
                    at: along(0.6) + offset,
                    half: rail,
                    color: alpha(edge, edge.a * 0.6),
                },
                Stop {
                    at: end + offset,
                    half: rail * 0.6,
                    color: alpha(edge, 0.),
                },
            ],
        );
    }
    // The fuse: quick at first, slowing as it nears the end of the lane.
    let burn = ease_out(progress);
    if burn > 0.01 {
        let head = along(burn);
        let width = (half * (0.34 + 0.26 * armed)).clamp(1., 8.);
        let fuse = alpha(light.hot(tint, 0.62), 0.55 + 0.45 * progress);
        light.streak(Layer::Core, start, head, width, alpha(tint, 0.), fuse);
        light.spot(
            Layer::Wash,
            head,
            (half * 2.2).clamp(6., 34.),
            alpha(tint, 0.3 + 0.3 * armed),
        );
        light.spot(
            Layer::Core,
            head,
            (half * 0.95).clamp(2.5, 15.),
            alpha(light.hot(tint, 0.85), 0.7 + 0.3 * armed),
        );
    }
    for pulse in 0..3 {
        let u = (flow * 0.85 + pulse as f32 / 3.).fract();
        light.oval(
            Layer::Core,
            along(burn * u),
            d * (half * 1.6).clamp(3., 14.),
            (half * 0.28).max(0.8),
            alpha(light.hot(tint, 0.35), 0.42 * (u * PI).sin() * progress),
        );
    }
}

/// A spoke of a ring attack, so the gaps between its rounds show.
fn spoke(
    light: &mut Emission,
    start: Vec2,
    d: Vec2,
    half: f32,
    tint: Color,
    (progress, armed): (f32, f32),
    z: f32,
) {
    let end = start + d * (34. + 70. * ease_out(progress)) * z;
    let strength = 0.4 + 0.45 * progress;
    light.streak(
        Layer::Wash,
        start,
        end,
        half * 1.6,
        alpha(tint, 0.1),
        alpha(tint, 0.5 * strength),
    );
    light.streak(
        Layer::Core,
        start,
        end,
        (half * (0.36 + 0.2 * armed)).max(1.),
        alpha(tint, 0.1),
        alpha(light.hot(tint, 0.55), strength),
    );
    light.spot(
        Layer::Core,
        end,
        (half * 0.9).max(2.5),
        alpha(light.hot(tint, 0.8), 0.45 + 0.5 * armed),
    );
}

/// The Siege Engine's line: a burning corridor as wide as its round, with
/// hard edges, a spine that narrows and flares just before it fires, fire
/// gathering in the muzzle, and a sight where cover will stop it.
fn cannon_line(
    light: &mut Emission,
    muzzle: Vec2,
    end: Vec2,
    radius: f32,
    (progress, armed): (f32, f32),
    flow: f32,
) {
    let along = end - muzzle;
    if along.length_squared() < 1. {
        return;
    }
    let d = along.normalize();
    let n = perpendicular(d);
    let half = radius.max(2.);
    let deep = Color::new(1., 0.2, 0.05, 1.);
    let burn = ramp(0.3);
    let flare = if armed > 0. {
        0.84 + 0.16 * (flow * 14.).sin()
    } else {
        1.
    };
    light.beam(
        Layer::Wash,
        muzzle,
        end,
        half * 1.9,
        alpha(deep, (0.1 + 0.22 * progress) * flare),
    );
    light.beam(
        Layer::Wash,
        muzzle,
        end,
        half,
        alpha(deep, 0.14 + 0.24 * progress),
    );
    for side in [-1., 1.] {
        light.beam(
            Layer::Core,
            muzzle + n * side * half,
            end + n * side * half,
            (half * 0.08).clamp(1., 2.6),
            alpha(light.hot(burn, 0.1), (0.42 + 0.42 * progress) * flare),
        );
    }
    light.beam(
        Layer::Core,
        muzzle,
        end,
        (half * (0.3 - 0.16 * armed)).max(1.5),
        alpha(
            light.hot(burn, 0.3 + 0.6 * armed),
            (0.42 + 0.5 * progress) * flare,
        ),
    );
    // Heat running down the line toward where it will land.
    for pulse in 0..4 {
        let u = (flow * 1.3 + pulse as f32 / 4.).fract();
        light.oval(
            Layer::Core,
            muzzle + along * u,
            d * half * 1.5,
            half * 0.32,
            alpha(light.hot(burn, 0.35), 0.5 * (u * PI).sin() * progress),
        );
    }
    // Fire gathering in the muzzle, drawn in from around it.
    let gather = progress * progress;
    light.spot(
        Layer::Wash,
        muzzle,
        half * (1.6 + 3.1 * gather),
        alpha(deep, 0.2 + 0.5 * gather),
    );
    light.spot(
        Layer::Wash,
        muzzle,
        half * (0.9 + 1.5 * gather),
        alpha(burn, 0.3 + 0.55 * gather),
    );
    light.spot(
        Layer::Core,
        muzzle,
        half * (0.42 + 0.9 * gather),
        alpha(light.hot(ramp(0.1), gather), 0.5 + 0.5 * gather),
    );
    if flow != 0. {
        for mote in 0..10 {
            let seed = mote as f32 / 10.;
            let u = (flow * 1.6 + seed * 3.7).fract();
            let angle = seed * TAU * 1.618 + flow * 0.8;
            let at = muzzle + direction(angle) * half * (3.6 - 3.3 * u * u);
            light.spot(
                Layer::Core,
                at,
                (half * 0.09).max(1.1) * (0.7 + 0.8 * u),
                alpha(
                    ramp(0.28 - 0.2 * u),
                    (u * PI).sin() * (0.3 + 0.7 * progress),
                ),
            );
        }
    }
    // Where cover stops it.
    let sight = half * 1.05;
    let turn = flow * 1.4;
    for quarter in 0..4 {
        light.arc(
            Layer::Core,
            end,
            sight * (1.32 - 0.3 * progress),
            (half * 0.09).max(1.),
            turn + quarter as f32 * PI * 0.5 + 0.2,
            1.1,
            alpha(light.hot(burn, 0.2), (0.55 + 0.4 * progress) * flare),
        );
    }
    light.spot(
        Layer::Core,
        end,
        half * (0.34 + 0.2 * armed),
        alpha(light.hot(burn, 0.6), 0.4 + 0.5 * armed),
    );
    light.spot(
        Layer::Wash,
        end,
        half * 2.4,
        alpha(deep, 0.16 + 0.22 * progress),
    );
}

/// Arcs closing on the hull as a special charges, and motes drawn into it.
fn closing_ring(
    light: &mut Emission,
    p: Vec2,
    r: f32,
    z: f32,
    tint: Color,
    (progress, armed): (f32, f32),
    flow: f32,
) {
    let radius = r * (1.72 - 0.55 * ease_out(progress));
    let thin = (1.8 * z).max(1.);
    let hot = light.hot(tint, 0.45 + 0.5 * armed);
    for third in 0..3 {
        let start = flow * 1.6 + third as f32 * TAU / 3.;
        light.arc(
            Layer::Wash,
            p,
            radius,
            thin * 3.6,
            start,
            TAU / 3. - 0.32,
            alpha(tint, 0.16 + 0.26 * progress),
        );
        light.arc(
            Layer::Core,
            p,
            radius,
            thin,
            start,
            TAU / 3. - 0.32,
            alpha(hot, 0.5 + 0.45 * progress),
        );
    }
    if flow != 0. {
        for mote in 0..12 {
            let seed = mote as f32 / 12.;
            let u = (flow * 0.95 + seed * 3.1).fract();
            let angle = seed * TAU + flow * 1.1 + u * 2.4;
            let at = p + direction(angle) * r * (2.05 - 1.72 * u * u);
            light.spot(
                Layer::Core,
                at,
                (2.6 * z).max(1.2) * (0.7 + 0.7 * u),
                alpha(hot, (u * PI).sin() * (0.25 + 0.7 * progress)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_colors_stay_saturated_and_keep_their_hue() {
        let amber = signal(Color::new(0.96, 0.69, 0.25, 1.));
        assert!(amber.r > 0.99 && amber.g > 0.5 && amber.b < 0.15);
        let cyan = signal(Color::new(0.18, 0.8, 0.9, 1.));
        assert!(cyan.b > 0.99 && cyan.g > 0.7 && cyan.r < 0.1);
        // A grey boss still gets a clear warning, and alpha is left alone.
        let grey = signal(Color::new(0.75, 0.75, 0.75, 0.4));
        assert_eq!((grey.r, grey.g, grey.b, grey.a), (1., 1., 1., 0.4));
        let black = signal(Color::new(0., 0., 0., 1.));
        assert!(black.r.is_finite() && black.g.is_finite() && black.b.is_finite());
    }
}
