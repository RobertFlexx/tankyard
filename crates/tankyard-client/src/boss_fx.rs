//! How bosses' attacks look: the warning before a special, the core that
//! charges for it, and the rounds they fire.
use crate::render::View;
use crate::ui::{AMBER, RED, alpha, mix};
use macroquad::prelude::*;
use tankyard_content::ProjectileKind;
use tankyard_core::{Boss, BossTactic, LETHAL_DAMAGE, Rect, V2, special_volley};

/// The warning before a special attack: the lanes its rounds will take,
/// filling as it charges, and a ring closing on the hull. The Siege
/// Engine's cannon shows its whole line, to the first cover that stops it.
pub fn warning(
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
    let rounds = special_volley(boss.kind, boss.attack, boss.aim, boss.phase);
    let ring = rounds.len() >= 8;
    for round in &rounds {
        let d = vec2(round.angle.cos(), round.angle.sin());
        let start = p + d * (r + 6. * z);
        if round.damage >= LETHAL_DAMAGE {
            let reach = round.speed * round.life;
            let from = at + V2::new(d.x, d.y) * boss.radius;
            let to = at + V2::new(d.x, d.y) * reach;
            let stop = walls
                .clone()
                .filter_map(|wall| tankyard_core::physics::sweep_rect(from, to, round.radius, wall))
                .map(|(t, _)| t)
                .fold(1., f32::min);
            let end = view.screen(from.lerp(to, stop));
            cannon_line(start, end, round.radius * z, progress, time);
            continue;
        }
        let color = if round.kind.is_drone() { RED } else { AMBER };
        if ring {
            // A ring: short spokes on each lane, so its gap shows.
            let end = start + d * (34. + 70. * progress) * z;
            draw_line(
                start.x,
                start.y,
                end.x,
                end.y,
                (round.radius * 1.1 * z).max(2.),
                alpha(color, 0.3 + 0.45 * progress),
            );
            continue;
        }
        // A fan: each lane as wide as its round, its line burning outward
        // as the attack charges.
        let length = (round.speed * round.life.min(1.3)).min(760.) * z;
        let end = start + d * length;
        draw_line(
            start.x,
            start.y,
            end.x,
            end.y,
            (round.radius * 2. * z).max(2.),
            alpha(color, 0.06 + 0.1 * progress),
        );
        let lit = start + d * length * progress;
        draw_line(
            start.x,
            start.y,
            lit.x,
            lit.y,
            (2.5 * z).max(1.),
            alpha(color, 0.45 + 0.4 * progress),
        );
    }
    draw_circle_lines(
        p.x,
        p.y,
        r * (1.6 - progress * 0.45),
        (2.5 * z).max(1.),
        alpha(WHITE, 0.25 + 0.45 * progress),
    );
    draw_circle_lines(p.x, p.y, r * 1.1, (1.5 * z).max(1.), alpha(AMBER, 0.6));
}

/// The Siege Engine's line: a red band as wide as its round, edged, with a
/// burning core that flickers just before it fires.
fn cannon_line(start: Vec2, end: Vec2, radius: f32, progress: f32, time: f32) {
    let width = (radius * 2.).max(4.);
    draw_line(
        start.x,
        start.y,
        end.x,
        end.y,
        width * 1.3,
        alpha(RED, 0.05 + 0.08 * progress),
    );
    draw_line(
        start.x,
        start.y,
        end.x,
        end.y,
        width,
        alpha(RED, 0.08 + 0.14 * progress),
    );
    let along = end - start;
    if along.length_squared() > 1. {
        let side = vec2(-along.y, along.x).normalize() * width * 0.5;
        for edge in [side, -side] {
            draw_line(
                start.x + edge.x,
                start.y + edge.y,
                end.x + edge.x,
                end.y + edge.y,
                1.5,
                alpha(RED, 0.35 + 0.4 * progress),
            );
        }
    }
    let flicker = if progress > 0.75 && (time * 28.).sin() > 0. {
        1.
    } else {
        0.55
    };
    draw_line(
        start.x,
        start.y,
        end.x,
        end.y,
        (width * 0.12).max(2.),
        alpha(mix(RED, WHITE, 0.35), (0.35 + 0.5 * progress) * flicker),
    );
    // Where cover stops it.
    draw_circle_lines(
        end.x,
        end.y,
        width * 0.6,
        2.,
        alpha(RED, 0.4 + 0.4 * progress),
    );
}

/// The dash a quick boss winds up for: chevrons running ahead of it.
pub fn dash(view: &View, boss: &Boss, at: V2, time: f32) {
    if boss.tactic != BossTactic::Windup {
        return;
    }
    let z = view.zoom;
    let p = view.screen(at);
    let r = boss.radius * z;
    let d = vec2(boss.aim.cos(), boss.aim.sin());
    let side = vec2(-d.y, d.x);
    for n in 0..4 {
        let along = r * (1.3 + n as f32 * 0.7 + (time * 2.2).fract() * 0.7);
        let tip = p + d * along;
        let width = r * 0.45;
        let fade = 0.55 - n as f32 * 0.12;
        for wing in [side, -side] {
            let back = tip - d * width * 0.7 + wing * width;
            draw_line(
                tip.x,
                tip.y,
                back.x,
                back.y,
                (4. * z).max(1.5),
                alpha(RED, fade),
            );
        }
    }
    draw_circle_lines(p.x, p.y, r * 1.25, 2., alpha(RED, 0.55));
}

/// The bright core every boss carries, glowing brighter as a special
/// charges and pulsing when it is nearly beaten.
pub fn core_glow(core: Vec2, r: f32, color: Color, charge: f32, phase: u8, time: f32) {
    let pulse = if phase >= 3 {
        0.5 + 0.5 * (time * 5.).sin()
    } else {
        0.5 + 0.5 * (time * 1.6).sin()
    };
    let strength = 0.22 + 0.1 * pulse + 0.5 * charge;
    let radius = r * (0.42 + 0.08 * pulse + 0.45 * charge);
    let tint = mix(color, WHITE, 0.35 + 0.4 * charge);
    if crate::glow::begin(crate::glow::Blend::Glow) {
        crate::glow::spot(core, radius, alpha(tint, strength));
        crate::glow::end();
    }
}

/// A glow to draw after the rounds, all in one pass.
pub struct ShotGlow {
    center: Vec2,
    along: Vec2,
    across: f32,
    color: Color,
}

/// One boss round, shaped by kind and lit, at `p` with radius `r`, flying
/// along `heading`. Drones, traps and mines keep the ordinary looks.
#[allow(clippy::too_many_arguments)]
pub fn shot(
    p: Vec2,
    r: f32,
    heading: Vec2,
    kind: ProjectileKind,
    damage: f32,
    color: Color,
    opacity: f32,
    age: f32,
    z: f32,
    glows: &mut Vec<ShotGlow>,
) {
    let d = heading.try_normalize().unwrap_or(vec2(1., 0.));
    let n = vec2(-d.y, d.x);
    let outline = alpha(tankyard_client::tank_model::OUTLINE, opacity);
    let body = alpha(color, opacity);
    let hot = alpha(mix(color, WHITE, 0.6), opacity);
    let border = 2. * z;
    if damage >= LETHAL_DAMAGE {
        // The Siege Engine's round: a long white-hot slug and its wake.
        let tail = p - d * r * 3.2;
        draw_line(tail.x, tail.y, p.x, p.y, r * 2. + border * 2., outline);
        draw_circle(p.x, p.y, r + border, outline);
        draw_line(
            tail.x,
            tail.y,
            p.x,
            p.y,
            r * 2.,
            alpha(mix(color, RED, 0.5), opacity),
        );
        draw_circle(p.x, p.y, r, alpha(mix(color, RED, 0.5), opacity));
        draw_line(tail.x, tail.y, p.x, p.y, r * 0.8, hot);
        draw_circle(p.x, p.y, r * 0.55, alpha(WHITE, opacity));
        glows.push(ShotGlow {
            center: p - d * r * 4.,
            along: d * r * 6.,
            across: r * 1.6,
            color: alpha(mix(RED, AMBER, 0.4), 0.55 * opacity),
        });
        glows.push(ShotGlow {
            center: p,
            along: d * r * 2.2,
            across: r * 2.,
            color: alpha(WHITE, 0.45 * opacity),
        });
        return;
    }
    match kind {
        ProjectileKind::Piercing => {
            // A lance.
            let tip = p + d * r * 2.3;
            let tail = p - d * r * 1.7;
            let grow = border * 1.6;
            let lance = |grow: f32, color: Color| {
                draw_triangle(
                    tip + d * grow,
                    p + n * (r * 0.8 + grow),
                    tail - d * grow,
                    color,
                );
                draw_triangle(
                    tip + d * grow,
                    p - n * (r * 0.8 + grow),
                    tail - d * grow,
                    color,
                );
            };
            lance(grow, outline);
            lance(0., body);
            draw_line(tail.x, tail.y, tip.x, tip.y, (r * 0.3).max(1.), hot);
        }
        ProjectileKind::Homing => {
            // A dart that turns toward its target.
            let tip = p + d * r * 1.7;
            let left = p - d * r + n * r;
            let right = p - d * r - n * r;
            let grow = border * 1.5;
            draw_triangle(
                tip + d * grow,
                left + (left - p).normalize_or_zero() * grow,
                right + (right - p).normalize_or_zero() * grow,
                outline,
            );
            draw_triangle(tip, left, right, body);
            draw_circle(p.x, p.y, r * 0.38, hot);
        }
        ProjectileKind::Fragment => {
            let tip = p + d * r * 1.5;
            let left = p - d * r * 0.8 + n * r * 0.75;
            let right = p - d * r * 0.8 - n * r * 0.75;
            draw_triangle(
                tip + d * border,
                left - d * border + n * border,
                right - d * border - n * border,
                outline,
            );
            draw_triangle(tip, left, right, hot);
        }
        ProjectileKind::Explosive => {
            // A charge with a burning fuse ring.
            draw_circle(p.x, p.y, r + border, outline);
            draw_circle(p.x, p.y, r, body);
            draw_circle(p.x, p.y, r * 0.5, hot);
            let fuse = r * (1.4 + 0.15 * (age * 18.).sin());
            draw_circle_lines(
                p.x,
                p.y,
                fuse,
                (1.5 * z).max(1.),
                alpha(mix(color, AMBER, 0.5), 0.7 * opacity),
            );
        }
        _ => {
            // Shells and bullets: a round with a hot heart and a short wake.
            for (back, size, fade) in [(1.3, 0.8, 0.3), (2.4, 0.6, 0.15)] {
                let wake = p - d * r * back;
                draw_circle(wake.x, wake.y, r * size, alpha(color, opacity * fade));
            }
            draw_circle(p.x, p.y, r + border, outline);
            draw_circle(p.x, p.y, r, body);
            draw_circle(p.x, p.y, r * 0.45, hot);
        }
    }
    glows.push(ShotGlow {
        center: p,
        along: d * r * 2.,
        across: r * 1.7,
        color: alpha(mix(color, WHITE, 0.3), 0.35 * opacity),
    });
}

/// The glows gathered from `shot`, in one pass.
pub fn shot_glows(glows: &mut Vec<ShotGlow>) {
    if !glows.is_empty() && crate::glow::begin(crate::glow::Blend::Glow) {
        for glow in glows.iter() {
            crate::glow::stretched_spot(glow.center, glow.along, glow.across, glow.color);
        }
        crate::glow::end();
    }
    glows.clear();
}
