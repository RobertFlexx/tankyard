//! Map art and effects. Particles are reconstructed from their birth time;
//! there is no particle allocation, network traffic or simulation work here.
use crate::glow::{self, Blend};
use crate::graphics::{draw_circle, draw_poly};
use crate::{
    render::{self, View},
    settings::Settings,
    ui::{self, alpha, mix},
};
use macroquad::prelude::*;
use tankyard_client::map_model::WallStyle;
use tankyard_content::{
    BlockStyle, DecorKind, DecorReact, EmitArea, GroupPoses, MapCounters, MapDecor, MapDef,
    MapZone, ParticleEffect, ParticleShape, ZoneKind,
};
use tankyard_core::{V2, WorldConfig};

/// The parts of a map that change during a round, as the art needs them.
#[derive(Clone, Debug, Default)]
pub struct Shown {
    pub hidden: u32,
    pub counters: MapCounters,
    /// Where map logic has moved, turned and faded each group.
    pub poses: GroupPoses,
    /// The tank decor reacts to, in world units: the player's own.
    pub focus: Option<V2>,
    /// Pads and decor drawn faded, as the editor shows layers other than
    /// the one being edited.
    pub fades: Fades,
}
/// How strongly each pad and piece of decor shows, by index, where it is
/// not 1. Empty in a round.
#[derive(Clone, Debug, Default)]
pub struct Fades {
    pub zones: Vec<f32>,
    pub decor: Vec<f32>,
}
impl Fades {
    fn zone(&self, index: usize) -> f32 {
        self.zones.get(index).copied().unwrap_or(1.)
    }
    fn decor(&self, index: usize) -> f32 {
        self.decor.get(index).copied().unwrap_or(1.)
    }
}

pub fn wall_style(style: BlockStyle, map: &str) -> WallStyle {
    match style {
        BlockStyle::Map => WallStyle::for_map(map),
        BlockStyle::Slate => WallStyle::Slate,
        BlockStyle::Garden => WallStyle::Garden,
        BlockStyle::Circuit => WallStyle::Circuit,
        BlockStyle::Bastion => WallStyle::Bastion,
        BlockStyle::Relay => WallStyle::Relay,
        BlockStyle::Orrery => WallStyle::Orrery,
        BlockStyle::Sand => WallStyle::Sand,
    }
}
pub fn color(rgb: [u8; 3]) -> Color {
    Color::from_rgba(rgb[0], rgb[1], rgb[2], 255)
}
pub fn zone_color(kind: ZoneKind) -> Color {
    match kind {
        ZoneKind::Speed => Color::from_rgba(70, 175, 215, 255),
        ZoneKind::Mud => Color::from_rgba(155, 121, 80, 255),
        ZoneKind::Repair => Color::from_rgba(70, 174, 121, 255),
        ZoneKind::Hazard => Color::from_rgba(212, 104, 69, 255),
        ZoneKind::Shelter => Color::from_rgba(141, 133, 204, 255),
        ZoneKind::Conveyor => Color::from_rgba(196, 158, 58, 255),
        ZoneKind::Portal => Color::from_rgba(196, 92, 172, 255),
        ZoneKind::Launch => Color::from_rgba(166, 204, 60, 255),
        ZoneKind::Pull => Color::from_rgba(122, 108, 236, 255),
        ZoneKind::Ice => Color::from_rgba(176, 226, 248, 255),
    }
}
/// `rgb` with `tint` mixed in at `strength`, as Tint triggers colour decor.
pub fn tinted(rgb: [u8; 3], (tint, strength): ([f32; 3], f32)) -> [u8; 3] {
    if strength <= 0. {
        return rgb;
    }
    std::array::from_fn(|i| {
        let own = f32::from(rgb[i]) / 255.;
        ((own + (tint[i] - own) * strength.min(1.)) * 255.).round() as u8
    })
}
pub fn screen_rect(view: &View, config: &WorldConfig, rect: [f32; 4]) -> Rect {
    let r = tankyard_core::map_rect(config, rect);
    let p = view.screen(V2::new(r.x, r.y));
    Rect::new(p.x, p.y, r.w * view.zoom, r.h * view.zoom)
}
fn visible(rect: Rect, margin: f32) -> bool {
    rect.x < screen_width() + margin
        && rect.right() > -margin
        && rect.y < screen_height() + margin
        && rect.bottom() > -margin
}

pub fn floor(map: &MapDef, config: &WorldConfig, shown: &Shown, view: &View, time: f32) {
    let time = if ui::reduced_motion() { 0. } else { time };
    let mut marks_left = 1024;
    for (index, zone) in map.zones.iter().enumerate() {
        if MapDef::group_bit(zone.group) & shown.hidden != 0 {
            continue;
        }
        let rect = shown.poses.shift(zone.rect, zone.group);
        let fade = shown.fades.zone(index);
        if zone.kind == ZoneKind::Portal {
            portal(zone, rect, config, view, time, fade);
            continue;
        }
        let r = screen_rect(view, config, rect);
        if !visible(r, 0.) {
            continue;
        }
        let c = crate::accessibility::palette(zone_color(zone.kind));
        // Launch arrows light up one after another towards where they fling,
        // and a gravity well's arrows in rings closing on its middle.
        let wave = |p: Vec2| match zone.kind {
            ZoneKind::Launch => {
                let (sin, cos) = zone.angle.to_radians().sin_cos();
                let along = ((p - r.center()).dot(vec2(cos, sin)) / spacing_of(view)).max(-64.);
                0.35 + 0.55 * (1. - (time * 1.6 - along * 0.25).rem_euclid(1.))
            }
            ZoneKind::Pull => {
                let out = ((p - r.center()).length() / spacing_of(view)).min(64.);
                0.3 + 0.6 * (1. - (time * 1.2 + out * 0.3).rem_euclid(1.))
            }
            _ => 0.5,
        };
        // Pads that hurt or heal breathe, so they read as live from afar.
        let breath = match zone.kind {
            ZoneKind::Hazard => 0.07 * (time * 3.2).sin(),
            ZoneKind::Repair => 0.04 * (time * 2.).sin(),
            _ => 0.,
        };
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(c, (0.13 + breath) * fade));
        draw_rectangle_lines(
            r.x,
            r.y,
            r.w,
            r.h,
            (2. * view.zoom).max(1.),
            alpha(c, 0.65 * fade),
        );
        pad_corners(r, view.zoom, alpha(c, 0.9 * fade));
        // A restrained repeating stencil makes pads distinguishable without
        // relying on colour. The count is capped even for arena-sized pads.
        let spacing = spacing_of(view);
        let cols = (r.w / spacing).ceil().clamp(1., 20.) as usize;
        let rows = (r.h / spacing).ceil().clamp(1., 20.) as usize;
        // Conveyor marks travel with the belt and wrap inside the pad.
        let travel = if zone.kind == ZoneKind::Conveyor {
            let angle = zone.angle.to_radians();
            let step = vec2(r.w / cols as f32, r.h / rows as f32);
            let shift = (time * zone.strength * 0.8).fract();
            vec2(angle.cos() * step.x, angle.sin() * step.y) * shift
        } else {
            Vec2::ZERO
        };
        for y in 0..rows {
            for x in 0..cols {
                if marks_left == 0 {
                    break;
                }
                let mut p = vec2(
                    r.x + (x as f32 + 0.5) / cols as f32 * r.w,
                    r.y + (y as f32 + 0.5) / rows as f32 * r.h,
                ) + travel;
                p.x = r.x + (p.x - r.x).rem_euclid(r.w.max(1.));
                p.y = r.y + (p.y - r.y).rem_euclid(r.h.max(1.));
                if p.x < -12.
                    || p.y < -12.
                    || p.x > screen_width() + 12.
                    || p.y > screen_height() + 12.
                {
                    continue;
                }
                // Arrows stay readable when zoomed out.
                let least = if zone.kind.aims() { spacing * 0.3 } else { 1. };
                let radius = (10. * view.zoom).min(r.w.min(r.h) * 0.18).max(least);
                // A gravity well's arrows point at its middle.
                let angle = if zone.kind == ZoneKind::Pull {
                    let to = r.center() - p;
                    to.y.atan2(to.x).to_degrees()
                } else {
                    zone.angle
                };
                zone_mark(zone.kind, angle, p, radius, alpha(c, wave(p) * fade));
                marks_left -= 1;
            }
        }
    }
}
/// A portal pad: turning rings over its area and a matching ring at the exit.
fn portal(zone: &MapZone, rect: [f32; 4], config: &WorldConfig, view: &View, time: f32, fade: f32) {
    let r = screen_rect(view, config, rect);
    let exit = view.screen(tankyard_core::map_point(config, zone.exit));
    let exit = vec2(exit.x, exit.y);
    let c = crate::accessibility::palette(zone_color(ZoneKind::Portal));
    let radius = (r.w.min(r.h) * 0.5).max(4.);
    if visible(r, 0.) {
        let p = r.center();
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(c, 0.10 * fade));
        draw_rectangle_lines(
            r.x,
            r.y,
            r.w,
            r.h,
            (2. * view.zoom).max(1.),
            alpha(c, 0.55 * fade),
        );
        for wave in 0..3 {
            let phase = (time * 0.6 + wave as f32 / 3.).fract();
            ring(
                p,
                radius * (0.25 + 0.7 * phase),
                (2.5 * view.zoom).max(1.),
                alpha(c, 0.75 * (1. - phase) * fade),
            );
        }
        draw_circle(p.x, p.y, radius * 0.18, alpha(c, 0.8 * fade));
    }
    let size = (34. * view.zoom).max(6.);
    if exit.x > -size
        && exit.y > -size
        && exit.x < screen_width() + size
        && exit.y < screen_height() + size
    {
        ring(exit, size, (2.5 * view.zoom).max(1.), alpha(c, 0.7 * fade));
        draw_poly_lines(
            exit.x,
            exit.y,
            4,
            size * 0.55,
            45. + time * 40.,
            2.,
            alpha(c, 0.6 * fade),
        );
    }
}

/// Bold brackets on a pad's corners, so its edges read at a glance.
fn pad_corners(r: Rect, zoom: f32, c: Color) {
    let arm = (r.w.min(r.h) * 0.22).min(30. * zoom.max(0.25)).max(3.);
    let thickness = (3.5 * zoom).clamp(1.5, 5.);
    let half = thickness * 0.5;
    for (corner, out) in [
        (vec2(r.x, r.y), vec2(1., 1.)),
        (vec2(r.right(), r.y), vec2(-1., 1.)),
        (vec2(r.right(), r.bottom()), vec2(-1., -1.)),
        (vec2(r.x, r.bottom()), vec2(1., -1.)),
    ] {
        // Two bars that meet at the corner without overlapping: the one
        // along the top or bottom edge covers the corner itself.
        let span = |from: f32, length: f32| (from.min(from + length), length.abs());
        let (x, w) = span(corner.x - out.x * half, out.x * (arm + half));
        draw_rectangle(x, corner.y - half, w, thickness, c);
        let (y, h) = span(corner.y + out.y * half, out.y * (arm - half).max(0.));
        draw_rectangle(corner.x - half, y, thickness, h, c);
    }
}

fn spacing_of(view: &View) -> f32 {
    (100. * view.zoom).max(24.)
}

pub fn zone_mark(kind: ZoneKind, angle: f32, p: Vec2, r: f32, c: Color) {
    match kind {
        ZoneKind::Launch => {
            // A solid arrowhead over a short stem: a hard push, not a belt.
            let (sin, cos) = angle.to_radians().sin_cos();
            let turn = |v: Vec2| p + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            draw_triangle(
                turn(vec2(r * 0.9, 0.)),
                turn(vec2(-r * 0.1, -r * 0.8)),
                turn(vec2(-r * 0.1, r * 0.8)),
                c,
            );
            let (a, b) = (turn(vec2(-r * 0.9, 0.)), turn(vec2(-r * 0.1, 0.)));
            draw_line(a.x, a.y, b.x, b.y, (r * 0.36).max(1.5), c);
        }
        ZoneKind::Speed => {
            for offset in [-0.5, 0.35] {
                let x = p.x + offset * r;
                chevron(
                    vec2(x + r * 0.25, p.y),
                    vec2(x - r * 0.4, p.y - r * 0.65),
                    vec2(x - r * 0.4, p.y + r * 0.65),
                    2.,
                    c,
                );
            }
        }
        ZoneKind::Repair => {
            // A cross in three pieces that meet without overlapping.
            let arm = r * 0.28;
            draw_rectangle(p.x - r, p.y - arm, r * 2., arm * 2., c);
            draw_rectangle(p.x - arm, p.y - r, arm * 2., r - arm, c);
            draw_rectangle(p.x - arm, p.y + arm, arm * 2., r - arm, c);
        }
        ZoneKind::Hazard => {
            draw_triangle(p + vec2(0., -r), p + vec2(r, r), p + vec2(-r, r), c);
            draw_line(p.x, p.y - r * 0.35, p.x, p.y + r * 0.25, 2., ui::bg_color());
        }
        ZoneKind::Shelter => {
            draw_poly_lines(p.x, p.y, 6, r, 30., 2., c);
        }
        ZoneKind::Conveyor => {
            let (sin, cos) = angle.to_radians().sin_cos();
            let turn = |v: Vec2| p + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            for offset in [-0.45, 0.4] {
                let tip = turn(vec2((offset + 0.35) * r, 0.));
                let a = turn(vec2((offset - 0.3) * r, -0.7 * r));
                let b = turn(vec2((offset - 0.3) * r, 0.7 * r));
                chevron(tip, a, b, (r * 0.3).clamp(1.5, 2.5), c);
            }
        }
        ZoneKind::Portal => {
            ring(p, r, 2., c);
            ring(p, r * 0.5, 2., c);
        }
        ZoneKind::Pull => {
            // One chevron, pointing at the well's middle.
            let (sin, cos) = angle.to_radians().sin_cos();
            let turn = |v: Vec2| p + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            let (tip, a, b) = (
                turn(vec2(r * 0.5, 0.)),
                turn(vec2(-r * 0.3, -r * 0.7)),
                turn(vec2(-r * 0.3, r * 0.7)),
            );
            chevron(tip, a, b, (r * 0.3).clamp(1.5, 2.5), c);
        }
        ZoneKind::Ice => {
            // A six-armed crystal.
            for i in 0..3 {
                let (sin, cos) = (i as f32 * 60f32).to_radians().sin_cos();
                let arm = vec2(cos, sin) * r;
                draw_line(p.x - arm.x, p.y - arm.y, p.x + arm.x, p.y + arm.y, 1.5, c);
                for end in [arm, -arm] {
                    let tip = p + end * 0.6;
                    let side = vec2(-end.y, end.x) * 0.25;
                    let back = p + end * 0.85;
                    draw_line(tip.x + side.x, tip.y + side.y, back.x, back.y, 1.2, c);
                    draw_line(tip.x - side.x, tip.y - side.y, back.x, back.y, 1.2, c);
                }
            }
        }
        ZoneKind::Mud => {
            for offset in [-0.5, 0., 0.5] {
                draw_line(
                    p.x - r * 0.7,
                    p.y + r * offset,
                    p.x + r * 0.7,
                    p.y + r * offset,
                    2.,
                    c,
                );
            }
        }
    }
}

/// How map logic and the nearby tank change a piece of decor this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Degrees turned on top of its rotation and spin.
    pub turn: f32,
    /// Opacity, 0 to 1, on top of its own.
    pub fade: f32,
    /// A color mixed in and how strongly.
    pub tint: ([f32; 3], f32),
    /// How much brighter it shines, 0 to 1.
    pub glow: f32,
}
impl Default for Look {
    fn default() -> Self {
        Self {
            turn: 0.,
            fade: 1.,
            tint: ([1.; 3], 0.),
            glow: 0.,
        }
    }
}

/// Where a piece of decor stands this frame, in window coordinates, and how
/// it looks; None when it is hidden, faded out or shrunk to nothing.
fn placed(
    item: &MapDecor,
    config: &WorldConfig,
    shown: &Shown,
    view: &View,
    reduced_motion: bool,
) -> Option<(Rect, f32, Look)> {
    if MapDef::group_bit(item.group) & shown.hidden != 0 {
        return None;
    }
    let poses = &shown.poses;
    let mut fade = poses.opacity(item.group);
    // Reduced motion keeps Scale but holds animations at rest.
    let (wobble, sway, mut scale) = if reduced_motion {
        let scale = poses
            .scale
            .get(usize::from(item.group))
            .copied()
            .unwrap_or(1.);
        ([0., 0.], 0., scale)
    } else {
        (
            poses.wobble(item.group),
            poses.sway(item.group),
            poses.scale(item.group),
        )
    };
    let shifted = poses.shift(item.rect, item.group);
    let mut glow = 0.;
    if item.react != DecorReact::None
        && let Some(focus) = shown.focus
    {
        let area = tankyard_core::map_rect(config, shifted);
        let dx = (area.x - focus.x).max(focus.x - (area.x + area.w)).max(0.);
        let dy = (area.y - focus.y).max(focus.y - (area.y + area.h)).max(0.);
        let near = DecorReact::closeness((dx * dx + dy * dy).sqrt(), item.react_reach());
        match item.react {
            DecorReact::Fade => fade *= 1. - 0.88 * near,
            DecorReact::Appear => fade *= near,
            DecorReact::Grow => scale *= 1. + 0.35 * near,
            DecorReact::Glow => glow = near,
            DecorReact::None => {}
        }
    }
    if fade <= 0.004 || scale <= 0.004 {
        return None;
    }
    let mut r = screen_rect(view, config, shifted);
    let center = r.center() + vec2(wobble[0], wobble[1]) * view.zoom;
    r.w *= scale;
    r.h *= scale;
    r.x = center.x - r.w * 0.5;
    r.y = center.y - r.h * 0.5;
    Some((
        r,
        scale,
        Look {
            turn: poses.turn(item.group) + sway,
            fade,
            tint: poses.tint(item.group),
            glow,
        },
    ))
}

pub fn decorations(
    map: &MapDef,
    config: &WorldConfig,
    shown: &Shown,
    view: &View,
    time: f32,
    above: bool,
    settings: &Settings,
) {
    let time = if settings.reduced_motion {
        0.
    } else {
        time.max(0.)
    };
    let mut remaining = (1024. * settings.effects.clamp(0., 1.)) as usize;
    SHADED_LIGHTS.with(|left| left.set(128));
    for (index, item) in map.decor.iter().enumerate() {
        if item.above != above {
            continue;
        }
        let Some((r, scale, mut look)) = placed(item, config, shown, view, settings.reduced_motion)
        else {
            continue;
        };
        look.fade *= shown.fades.decor(index);
        if item.kind == DecorKind::Emitter {
            if remaining == 0 || settings.reduced_motion {
                continue;
            }
            let mut effect = item.effect.unwrap_or_default();
            effect.color = tinted(effect.color, look.tint);
            effect.end_color = tinted(effect.end_color, look.tint);
            effect.size *= scale;
            effect.end_size *= scale;
            effect.direction += item.rotation + look.turn + time * item.spin;
            let reach = (effect.speed * effect.lifetime
                + effect.gravity.abs() * effect.lifetime.powi(2) * 0.5
                + effect.wobble
                + 120.)
                * view.zoom;
            if !visible(r, reach) {
                continue;
            }
            let pulse = if item.pulse > 0. {
                0.8 + 0.2 * (time * item.pulse * std::f32::consts::TAU).cos()
            } else {
                1.
            };
            particles(
                &effect,
                r,
                view.zoom,
                time,
                index as u32,
                &mut remaining,
                f32::from(item.alpha) / 255. * pulse * look.fade * (1. + look.glow).min(2.),
            );
        } else if visible(r, r.w.max(r.h) * 0.5) {
            decoration(item, r, time, view.zoom, &shown.counters, look);
        }
    }
    // Single pieces drawn elsewhere, such as palette previews, always shade.
    SHADED_LIGHTS.with(|left| left.set(u32::MAX));
}

/// The light map's share of the map: lamps, spotlights and beams, and a
/// faint glow over pads that warn or carry, for when the arena is dark.
pub fn lights(
    map: &MapDef,
    config: &WorldConfig,
    shown: &Shown,
    view: &View,
    time: f32,
    reduced_motion: bool,
    out: &mut Vec<crate::lighting::Light>,
) {
    use crate::lighting::Light;
    let time = if reduced_motion { 0. } else { time.max(0.) };
    for (index, item) in map.decor.iter().enumerate() {
        if out.len() >= crate::lighting::MOST_LIGHTS {
            return;
        }
        // Glowing particles light the floor around where they rise.
        let glowing =
            item.kind == DecorKind::Emitter && item.effect.as_ref().is_some_and(|e| e.glow);
        if !item.kind.lights() && !glowing {
            continue;
        }
        let Some((r, scale, mut look)) = placed(item, config, shown, view, reduced_motion) else {
            continue;
        };
        look.fade *= shown.fades.decor(index);
        if let Some(effect) = item.effect.as_ref().filter(|_| glowing) {
            let spread = (effect.speed * effect.lifetime * 0.5 + effect.size * 3.) * view.zoom;
            crate::lighting::gather(
                out,
                Light::Spot {
                    at: r.center(),
                    radius: (r.w.max(r.h) * 0.5 + spread * scale).max(4.),
                    color: alpha(
                        mix(color(tinted(effect.color, look.tint)), WHITE, 0.3),
                        (f32::from(item.alpha) / 255. * 0.55 * look.fade).min(1.),
                    ),
                },
            );
            continue;
        }
        let pulse = if item.pulse > 0. {
            0.8 + 0.2 * (time * item.pulse * std::f32::consts::TAU).cos()
        } else {
            1.
        };
        let strength = (f32::from(item.alpha) / 255. * pulse * look.fade * flicker(item, time)
            + look.glow * 0.5)
            .min(1.);
        // Light reveals true colors more than it tints them.
        let color = alpha(
            mix(color(tinted(item.color, look.tint)), WHITE, 0.4),
            strength,
        );
        let angle = (item.rotation + look.turn + time * item.spin).to_radians();
        let direction = vec2(angle.cos(), angle.sin());
        if item.kind == DecorKind::Spotlight && out.len() + 1 < crate::lighting::MOST_LIGHTS {
            // The pool where the beam lands, as the visible spotlight draws it.
            let spread = (r.h * 0.5).atan2(r.w.max(1.));
            let land = r.w * 0.7;
            crate::lighting::gather(
                out,
                Light::Spot {
                    at: r.center() - direction * r.w * 0.5 + direction * land,
                    radius: (land * spread.tan() * 0.85).max(2.),
                    color: alpha(color, color.a * 0.35),
                },
            );
        }
        crate::lighting::gather(
            out,
            match item.kind {
                DecorKind::Spotlight => Light::Cone {
                    at: r.center() - direction * r.w * 0.5,
                    angle,
                    spread: (r.h * 0.5).atan2(r.w.max(1.)),
                    reach: r.w * 1.1,
                    color,
                },
                // A tube lights a broad band of floor either side of it.
                DecorKind::Beam => Light::Shaft {
                    from: r.center() - direction * r.w * 0.5,
                    to: r.center() + direction * r.w * 0.5,
                    width: (r.h * 6.).max(110. * view.zoom),
                    color: alpha(color, color.a * 0.85),
                },
                _ => Light::Spot {
                    at: r.center(),
                    radius: r.w.min(r.h) * 0.6,
                    color,
                },
            },
        );
    }
    for zone in &map.zones {
        if out.len() >= crate::lighting::MOST_LIGHTS {
            return;
        }
        let glows = matches!(
            zone.kind,
            ZoneKind::Hazard | ZoneKind::Portal | ZoneKind::Launch | ZoneKind::Repair
        );
        if !glows || MapDef::group_bit(zone.group) & shown.hidden != 0 {
            continue;
        }
        let r = screen_rect(view, config, shown.poses.shift(zone.rect, zone.group));
        crate::lighting::gather(
            out,
            Light::Spot {
                at: r.center(),
                radius: r.w.max(r.h) * 0.75,
                color: alpha(crate::accessibility::palette(zone_color(zone.kind)), 0.35),
            },
        );
    }
}

/// An unturned grid, only its lines on screen and only as long as the
/// screen shows them: a grid as large as the arena costs a screenful.
fn straight_grid(r: Rect, step: f32, thickness: f32, c: Color) {
    let (columns, rows) = ((r.w / step).floor().max(1.), (r.h / step).floor().max(1.));
    let (dx, dy) = (r.w / columns, r.h / rows);
    let (left, right) = (
        r.x.max(-thickness),
        r.right().min(screen_width() + thickness),
    );
    let (top, bottom) = (
        r.y.max(-thickness),
        r.bottom().min(screen_height() + thickness),
    );
    if left > right || top > bottom {
        return;
    }
    let first = ((left - r.x) / dx).ceil().max(0.) as u32;
    let last = (((right - r.x) / dx).floor().min(columns) as u32).min(first + 512);
    for i in first..=last {
        let x = r.x + i as f32 * dx;
        draw_line(x, top, x, bottom, thickness, c);
    }
    let first = ((top - r.y) / dy).ceil().max(0.) as u32;
    let last = (((bottom - r.y) / dy).floor().min(rows) as u32).min(first + 512);
    for i in first..=last {
        let y = r.y + i as f32 * dy;
        draw_line(left, y, right, y, thickness, c);
    }
}

/// One piece of decor in `r`, as map logic and the nearby tank leave it.
pub fn decoration(item: &MapDecor, r: Rect, time: f32, zoom: f32, counters: &[i32], look: Look) {
    let pulse = if item.pulse > 0. {
        0.8 + 0.2 * (time * item.pulse * std::f32::consts::TAU).cos()
    } else {
        1.
    };
    let base = crate::accessibility::palette(color(tinted(item.color, look.tint)));
    let c = alpha(
        mix(base, WHITE, look.glow * 0.45),
        (f32::from(item.alpha) / 255. * pulse * look.fade * (1. + look.glow)).min(1.),
    );
    let p = r.center();
    let angle = (item.rotation + look.turn + time * item.spin).to_radians();
    let rotate = |point: Vec2| {
        p + vec2(
            point.x * angle.cos() - point.y * angle.sin(),
            point.x * angle.sin() + point.y * angle.cos(),
        )
    };
    let corners = || {
        [
            vec2(-r.w, -r.h),
            vec2(r.w, -r.h),
            vec2(r.w, r.h),
            vec2(-r.w, r.h),
        ]
        .map(|v| rotate(v * 0.5))
    };
    match item.kind {
        DecorKind::Tile | DecorKind::Stripes => {
            let vertices = corners();
            draw_triangle(vertices[0], vertices[1], vertices[2], c);
            draw_triangle(vertices[0], vertices[2], vertices[3], c);
            if item.kind == DecorKind::Stripes {
                let span = r.w + r.h * 0.5;
                let n = (span / (16. * zoom).max(8.)).ceil().clamp(1., 48.) as usize;
                for i in 0..n {
                    let [a, b] =
                        stripe_segment(vec2(r.w, r.h), (i as f32 + 0.5) / n as f32).map(rotate);
                    draw_line(
                        a.x,
                        a.y,
                        b.x,
                        b.y,
                        (span / n as f32 * 0.32).max(1.),
                        alpha(BLACK, c.a * 0.45),
                    );
                }
            }
        }
        DecorKind::Light => light(item, p, r.w.min(r.h) * 0.5, c, time),
        DecorKind::Spotlight => {
            let strength = c.a * flicker(item, time);
            let direction = vec2(angle.cos(), angle.sin());
            let at = p - direction * r.w * 0.5;
            let spread = (r.h * 0.5).atan2(r.w.max(1.));
            let hood = (r.h.min(r.w) * 0.08).clamp(2., 16.);
            if glow::begin(Blend::Add) {
                crate::lighting::cone(at, angle, spread, r.w * 1.1, alpha(c, strength * 0.45));
                glow::end();
                // Where the beam lands, a pool; at the lamp, glare.
                if glow::begin(Blend::Glow) {
                    let land = r.w * 0.7;
                    let pool = (land * spread.tan() * 0.85).max(2.);
                    glow::spot(at + direction * land, pool, alpha(c, strength * 0.14));
                    glow::spot(at, hood * 3.2, alpha(mix(c, WHITE, 0.6), strength * 0.55));
                    glow::end();
                }
            } else {
                let side = |s: f32| at + vec2((angle + s).cos(), (angle + s).sin()) * r.w;
                draw_triangle(at, side(-spread), side(spread), alpha(c, strength * 0.18));
            }
            spotlight_lamp(at, direction, hood, c, strength);
        }
        DecorKind::Beam => {
            let strength = c.a * flicker(item, time);
            let direction = vec2(angle.cos(), angle.sin());
            let (from, to) = (p - direction * r.w * 0.5, p + direction * r.w * 0.5);
            let tube = (r.h * 0.35).max(1.);
            let hot = mix(c, WHITE, 0.55);
            // Haze all around the tube, soft at its ends as at its sides,
            // and a tighter glow hugging it.
            if glow::begin(Blend::Add) {
                crate::lighting::shaft(from, to, r.h * 3., alpha(c, strength * 0.2));
                crate::lighting::shaft(from, to, r.h * 1.2, alpha(c, strength * 0.3));
                glow::end();
            } else {
                capsule(from, to, r.h * 0.6, alpha(c, strength * 0.15));
            }
            // The tube: one rounded shape, so nothing in it overlaps.
            capsule(from, to, tube * 0.5, alpha(hot, strength));
            if glow::begin(Blend::Add) {
                // A white-hot line down its middle.
                crate::lighting::shaft(from, to, tube * 0.8, alpha(WHITE, strength * 0.5));
                glow::end();
            }
            // Pulses of brighter light run along it at a steady pace,
            // swelling in from one end and fading out at the other.
            if !ui::reduced_motion() && glow::begin(Blend::Glow) {
                let seed = item.rect[0].to_bits() ^ item.rect[1].to_bits().rotate_left(7);
                let length = r.w / zoom.max(0.01);
                let period = (length / 520.).max(1.2);
                let phase = (time / period + hash(seed)).fract();
                glow::stretched_spot(
                    from.lerp(to, phase),
                    direction * (r.w * 0.09).max(tube * 2.),
                    (r.h * 0.9).max(tube),
                    alpha(
                        mix(c, WHITE, 0.7),
                        strength * 0.6 * (phase * std::f32::consts::PI).sin(),
                    ),
                );
                glow::end();
            }
            if glow::begin(Blend::Glow) {
                for end in [from, to] {
                    glow::spot(end, (r.h * 1.4).max(3.), alpha(hot, strength * 0.45));
                }
                glow::end();
            }
        }
        DecorKind::Grid => {
            let step = (40. * zoom).max(6.);
            let thickness = (1.5 * zoom).max(1.);
            if angle.sin().abs() < 1e-4 && angle.cos() > 0. {
                straight_grid(r, step, thickness, c);
                return;
            }
            let columns = ((r.w / step).floor() as i32).clamp(1, 96);
            let rows = ((r.h / step).floor() as i32).clamp(1, 96);
            for i in 0..=columns {
                let x = -r.w * 0.5 + i as f32 / columns as f32 * r.w;
                let (a, b) = (rotate(vec2(x, -r.h * 0.5)), rotate(vec2(x, r.h * 0.5)));
                draw_line(a.x, a.y, b.x, b.y, thickness, c);
            }
            for i in 0..=rows {
                let y = -r.h * 0.5 + i as f32 / rows as f32 * r.h;
                let (a, b) = (rotate(vec2(-r.w * 0.5, y)), rotate(vec2(r.w * 0.5, y)));
                draw_line(a.x, a.y, b.x, b.y, thickness, c);
            }
        }
        DecorKind::Disc => {
            let sides = (r.w.max(r.h) * 0.5).clamp(12., 48.) as u8;
            if (r.w - r.h).abs() < 0.5 {
                draw_poly(p.x, p.y, sides, r.w * 0.5, 0., c);
            } else {
                // An ellipse, as a fan, so it can turn.
                let n = u16::from(sides);
                let mut vertices = vec![Vertex::new(p.x, p.y, 0., 0., 0., c)];
                for i in 0..=n {
                    let a = f32::from(i) / f32::from(n) * std::f32::consts::TAU;
                    let q = rotate(vec2(a.cos() * r.w * 0.5, a.sin() * r.h * 0.5));
                    vertices.push(Vertex::new(q.x, q.y, 0., 0., 0., c));
                }
                let indices = (1..=n).flat_map(|i| [0, i, i + 1]).collect();
                draw_mesh(&Mesh {
                    vertices,
                    indices,
                    texture: None,
                });
            }
        }
        DecorKind::Triangle => {
            draw_triangle(
                rotate(vec2(0., -r.h * 0.5)),
                rotate(vec2(r.w * 0.5, r.h * 0.5)),
                rotate(vec2(-r.w * 0.5, r.h * 0.5)),
                c,
            );
        }
        DecorKind::Hexagon => {
            let points: [Vec2; 6] = std::array::from_fn(|i| {
                let a = (i as f32 * 60.).to_radians();
                rotate(vec2(a.cos() * r.w * 0.5, a.sin() * r.h * 0.5))
            });
            for i in 1..5 {
                draw_triangle(points[0], points[i], points[i + 1], c);
            }
        }
        DecorKind::Ring => ring(p, r.w.min(r.h) * 0.5, (3. * zoom).max(1.), c),
        DecorKind::Arrow => {
            // One outline in five triangles that share their edges, so a
            // see-through arrow is even all over.
            let stem = (r.h * 0.11).max(0.5);
            let [
                tail_top,
                base_top,
                head_top,
                tip,
                head_bottom,
                base_bottom,
                tail_bottom,
            ] = [
                vec2(-r.w * 0.4, -stem),
                vec2(0., -stem),
                vec2(0., -r.h * 0.5),
                vec2(r.w * 0.5, 0.),
                vec2(0., r.h * 0.5),
                vec2(0., stem),
                vec2(-r.w * 0.4, stem),
            ]
            .map(rotate);
            draw_triangle(tail_top, base_top, base_bottom, c);
            draw_triangle(tail_top, base_bottom, tail_bottom, c);
            draw_triangle(head_top, tip, base_top, c);
            draw_triangle(base_top, tip, base_bottom, c);
            draw_triangle(base_bottom, tip, head_bottom, c);
        }
        DecorKind::Label => {
            let size = (r.h * 0.7).clamp(6., 72.);
            let filled = tankyard_content::fill_counters(&item.text, counters);
            let text = ui::fitted_label(&filled, size, r.w);
            let matrix = Mat4::from_translation(vec3(p.x, p.y, 0.))
                * Mat4::from_rotation_z(angle)
                * Mat4::from_translation(vec3(-p.x, -p.y, 0.));
            unsafe {
                get_internal_gl().quad_gl.push_model_matrix(matrix);
            }
            ui::center_label(&text, p.x, p.y + size * 0.3, size, c);
            unsafe {
                get_internal_gl().quad_gl.pop_model_matrix();
            }
        }
        DecorKind::Emitter => {}
    }
}

/// A ring `thickness` wide around `center`, as one band with no seams, so
/// a see-through ring has no beads where segments would overlap.
pub(crate) fn ring(center: Vec2, radius: f32, thickness: f32, c: Color) {
    const MOST: usize = 96;
    let outer = radius + thickness * 0.5;
    let inner = (radius - thickness * 0.5).max(0.);
    if outer < 0.5 || c.a <= 0.004 {
        return;
    }
    let steps = ((outer * 0.5) as usize).clamp(12, MOST);
    let mut vertices = [crate::lighting::vertex(center, c); 2 * (MOST + 1)];
    for i in 0..=steps {
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        let d = vec2(a.cos(), a.sin());
        vertices[i * 2] = crate::lighting::vertex(center + d * outer, c);
        vertices[i * 2 + 1] = crate::lighting::vertex(center + d * inner, c);
    }
    let mut indices = [0_u16; MOST * 6];
    for i in 0..steps {
        let o = (i * 2) as u16;
        indices[i * 6..i * 6 + 6].copy_from_slice(&[o, o + 2, o + 3, o, o + 3, o + 1]);
    }
    crate::lighting::geometry(&vertices[..2 * (steps + 1)], &indices[..steps * 6]);
}

/// A V `thickness` wide with its point at `tip` and its arms reaching to
/// `a` and `b`, in two pieces that meet at a mitred point without
/// overlapping.
pub(crate) fn chevron(tip: Vec2, a: Vec2, b: Vec2, thickness: f32, c: Color) {
    chevron_with(draw_triangle, tip, a, b, thickness, c);
}

/// [`chevron`], drawn with `draw_triangle`, such as the page-space one
/// widgets use.
pub(crate) fn chevron_with(
    draw_triangle: fn(Vec2, Vec2, Vec2, Color),
    tip: Vec2,
    a: Vec2,
    b: Vec2,
    thickness: f32,
    c: Color,
) {
    let (to_a, to_b) = ((a - tip).normalize_or_zero(), (b - tip).normalize_or_zero());
    let forward = -(to_a + to_b).normalize_or_zero();
    if forward == Vec2::ZERO || to_a == Vec2::ZERO || to_b == Vec2::ZERO {
        return;
    }
    let half = thickness * 0.5;
    // Each arm's sides lie half the thickness out; they meet past the tip
    // by as much more as the V is sharp, within reason.
    let sine = (1. - to_a.dot(-forward).powi(2)).max(0.).sqrt().max(0.3);
    let (outer, inner) = (tip + forward * (half / sine), tip - forward * (half / sine));
    let side = |to: Vec2| {
        let normal = vec2(-to.y, to.x);
        let outward = if normal.dot(forward) >= 0. {
            normal
        } else {
            -normal
        };
        outward * half
    };
    let (out_a, out_b) = (side(to_a), side(to_b));
    draw_triangle(a + out_a, outer, inner, c);
    draw_triangle(a + out_a, inner, a - out_a, c);
    draw_triangle(outer, b + out_b, b - out_b, c);
    draw_triangle(outer, b - out_b, inner, c);
}

/// A filled capsule of `radius` from `a` to `b`: one convex shape, so a
/// see-through tube has no overlapping ends.
fn capsule(a: Vec2, b: Vec2, radius: f32, c: Color) {
    const STEPS: usize = 10;
    if radius < 0.1 || c.a <= 0.004 {
        return;
    }
    let along = b - a;
    let direction = along.try_normalize().unwrap_or(vec2(1., 0.));
    let turn = direction.y.atan2(direction.x);
    let middle = (a + b) * 0.5;
    let mut vertices = [crate::lighting::vertex(middle, c); 1 + 2 * (STEPS + 1)];
    for (end, (center, start)) in [
        (b, turn - std::f32::consts::FRAC_PI_2),
        (a, turn + std::f32::consts::FRAC_PI_2),
    ]
    .into_iter()
    .enumerate()
    {
        for j in 0..=STEPS {
            let angle = start + j as f32 / STEPS as f32 * std::f32::consts::PI;
            vertices[1 + end * (STEPS + 1) + j] =
                crate::lighting::vertex(center + vec2(angle.cos(), angle.sin()) * radius, c);
        }
    }
    let outline = 2 * (STEPS + 1);
    let mut indices = [0_u16; 3 * 2 * (STEPS + 1)];
    for i in 0..outline {
        let next = (i + 1) % outline;
        indices[i * 3..i * 3 + 3].copy_from_slice(&[0, 1 + i as u16, 1 + next as u16]);
    }
    crate::lighting::geometry(&vertices, &indices);
}

/// How much a flickering light dims at `time`: smooth noise, so it wavers
/// like a flame or a tired tube rather than strobing.
fn flicker(item: &MapDecor, time: f32) -> f32 {
    if item.flicker <= 0. || ui::reduced_motion() {
        return 1.;
    }
    let seed = item.rect[0].to_bits() ^ item.rect[1].to_bits().rotate_left(13);
    let t = time * 9.;
    let step = t.floor();
    let f = t - step;
    let f = f * f * (3. - 2. * f);
    let a = hash(seed ^ step as i64 as u32);
    let b = hash(seed ^ (step as i64 + 1) as u32);
    1. - item.flicker * 0.85 * (a + (b - a) * f)
}

/// Clip a line x = intercept + y/2 to the local decor rectangle. Clamping
/// its x endpoints alone makes tall rectangles collapse every stripe onto
/// the same diagonal instead of preserving the pattern's angle and spacing.
fn stripe_segment(size: Vec2, t: f32) -> [Vec2; 2] {
    let intercept = (t - 0.5) * (size.x + size.y * 0.5);
    let low = (-size.y * 0.5).max(-size.x - intercept * 2.);
    let high = (size.y * 0.5).min(size.x - intercept * 2.);
    [
        vec2(intercept + low * 0.5, low),
        vec2(intercept + high * 0.5, high),
    ]
}

thread_local! {
    /// Lights a map layer may still shade. Each shaded light costs its own
    /// draw calls, so past the budget lights draw as rings, which batch, and
    /// a map crowded with lamps can't stall the frame.
    static SHADED_LIGHTS: std::cell::Cell<u32> = const { std::cell::Cell::new(u32::MAX) };
}

/// A lamp: a wash of its color over the floor, light added on top that
/// brightens toward the middle, a hot core and the bulb itself.
fn light(item: &MapDecor, p: Vec2, radius: f32, c: Color, time: f32) {
    let strength = c.a * flicker(item, time);
    if strength <= 0.004 || radius < 0.5 {
        return;
    }
    let hot = mix(c, WHITE, 0.55);
    let budget = SHADED_LIGHTS.with(|left| {
        let n = left.get();
        left.set(n.saturating_sub(1));
        n > 0
    });
    if budget && glow::begin(Blend::Soft) {
        glow::spot(p, radius, alpha(c, strength * 0.42));
        glow::end();
        glow::begin(Blend::Glow);
        glow::spot(p, radius * 0.9, alpha(c, strength * 0.8));
        glow::spot(p, radius * 0.3, alpha(hot, strength));
        glow::end();
        // A glint on the bulb, the way a lens catches it, and a white-hot
        // core added on top so the bulb never reads darker than its glow.
        if glow::begin(Blend::Add) {
            if radius > 10. {
                crate::lighting::glint(
                    p,
                    (radius * 0.32).min(90.),
                    std::f32::consts::FRAC_PI_4,
                    alpha(hot, strength * 0.5),
                );
            }
            glow::spot(p, (radius * 0.08).max(2.), alpha(WHITE, strength * 0.9));
            glow::end();
        }
    } else {
        glow::fallback_spot(p, radius, alpha(c, strength * 0.9));
        draw_circle(p.x, p.y, radius * 0.14, alpha(hot, strength * 0.6));
        draw_circle(p.x, p.y, (radius * 0.05).max(1.2), alpha(WHITE, strength));
    }
}

/// A spotlight's lamp at `at`, shining along `direction`: a housing that
/// flares toward the front, its rim, and a lit lens with a glint.
fn spotlight_lamp(at: Vec2, direction: Vec2, hood: f32, c: Color, strength: f32) {
    let across = vec2(-direction.y, direction.x);
    let (back, front) = (at - direction * hood * 1.9, at + direction * hood * 0.3);
    let shell = alpha(mix(c, BLACK, 0.62), c.a.max(0.7));
    let corners = [
        back - across * hood * 0.7,
        back + across * hood * 0.7,
        front + across * hood * 1.15,
        front - across * hood * 1.15,
    ];
    draw_triangle(corners[0], corners[1], corners[2], shell);
    draw_triangle(corners[0], corners[2], corners[3], shell);
    let (a, b) = (corners[3], corners[2]);
    draw_line(
        a.x,
        a.y,
        b.x,
        b.y,
        (hood * 0.28).max(1.),
        alpha(mix(c, BLACK, 0.3), shell.a),
    );
    draw_circle(at.x, at.y, hood * 0.7, alpha(mix(c, WHITE, 0.85), strength));
    if hood > 3. && glow::begin(Blend::Add) {
        crate::lighting::glint(
            at,
            hood * 2.6,
            0.,
            alpha(mix(c, WHITE, 0.7), strength * 0.45),
        );
        glow::end();
    }
}

fn hash(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846ca68b);
    value ^= value >> 16;
    (value >> 8) as f32 / 16_777_216.
}

/// How opaque a particle `age` seconds old and `fraction` of the way through
/// its life is, before the emitter's opacity: with `fade` it appears over a
/// tenth of a second, holds its strength through most of its life and fades
/// out as it ends.
fn particle_alpha(fade: bool, fraction: f32, age: f32) -> f32 {
    if fade {
        (smooth(age * 10.) * (1. - fraction.clamp(0., 1.).powf(1.5))).clamp(0., 1.)
    } else {
        1.
    }
}

/// Smoothly from 0 to 1 as `t` goes from 0 to 1.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// A color in linear light, where mixing behaves as light does.
fn linear([r, g, b]: [u8; 3]) -> [f32; 3] {
    [r, g, b].map(|c| (f32::from(c) / 255.).powf(2.2))
}

/// The color `t` of the way from `from` to `to`, both in linear light, so a
/// fade from yellow to red passes through orange rather than brown.
fn mix_light(from: [f32; 3], to: [f32; 3], t: f32) -> Color {
    let [r, g, b] = std::array::from_fn(|i| (from[i] + (to[i] - from[i]) * t).powf(1. / 2.2));
    Color::new(r, g, b, 1.)
}

/// Where in `r` a particle starts, from two random fractions.
fn emit_point(area: EmitArea, r: Rect, u: f32, v: f32) -> Vec2 {
    match area {
        EmitArea::Fill => vec2(r.x + u * r.w, r.y + v * r.h),
        EmitArea::Round => {
            let (sin, cos) = (u * std::f32::consts::TAU).sin_cos();
            r.center() + vec2(cos * r.w, sin * r.h) * (v.sqrt() * 0.5)
        }
        EmitArea::Edge => {
            let along = u * 2. * (r.w + r.h);
            if along < r.w {
                vec2(r.x + along, r.y)
            } else if along < r.w + r.h {
                vec2(r.right(), r.y + along - r.w)
            } else if along < 2. * r.w + r.h {
                vec2(r.right() - (along - r.w - r.h), r.bottom())
            } else {
                vec2(r.x, r.bottom() - (along - 2. * r.w - r.h))
            }
        }
        EmitArea::Point => r.center(),
    }
}

/// One particle as it is this frame, in screen pixels.
#[derive(Clone, Copy)]
struct Mote {
    at: Vec2,
    radius: f32,
    /// Where it's heading, in pixels a second, for sparks to streak along.
    velocity: Vec2,
    turn: f32,
    color: Color,
    /// How near the top of its twinkle it is, from 0 to 1.
    glint: f32,
}

thread_local! {
    static MOTES: std::cell::RefCell<Vec<Mote>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// How strong a glowing particle's halo is, against the particle.
const HALO: f32 = 0.34;
/// How wide the spark sprite's head is, against its cell.
const SPARK_HEAD: f32 = 0.32;

pub fn particles(
    effect: &ParticleEffect,
    r: Rect,
    zoom: f32,
    time: f32,
    seed: u32,
    remaining: &mut usize,
    opacity: f32,
) {
    use std::f32::consts::TAU;
    let last = (time * effect.rate).floor() as i64;
    let count = (effect.rate * effect.lifetime).ceil().min(240.) as i64;
    let (from, to) = (linear(effect.color), linear(effect.end_color));
    MOTES.with(|motes| {
        let motes = &mut *motes.borrow_mut();
        motes.clear();
        for birth in (last - count + 1).max(0)..=last {
            if *remaining == 0 {
                break;
            }
            let age = time - birth as f32 / effect.rate;
            if age < 0. || age > effect.lifetime {
                continue;
            }
            let key = seed
                .wrapping_mul(0x9e3779b9)
                .wrapping_add((birth as u32).wrapping_mul(37));
            let random = |n: u32| hash(key.wrapping_add(n));
            let angle = (effect.direction + (random(0) - 0.5) * effect.spread).to_radians();
            let speed = effect.speed * (0.65 + 0.35 * random(1));
            let displacement = if effect.drag > 0.001 {
                -(-effect.drag * age).exp_m1() / effect.drag
            } else {
                age
            };
            let heading = vec2(angle.cos(), angle.sin());
            let mut p = emit_point(effect.area, r, random(2), random(3))
                + heading * speed * displacement * zoom
                + vec2(0., effect.gravity * age * age * 0.5 * zoom);
            let mut velocity =
                heading * speed * (-effect.drag * age).exp() + vec2(0., effect.gravity * age);
            if effect.wobble > 0. {
                // Sway across the direction of travel, starting from the origin.
                let phase = random(5) * TAU;
                let rate = TAU * (0.4 + 0.8 * random(6));
                let sway = (age * rate + phase).sin() - phase.sin();
                let side = vec2(-heading.y, heading.x) * effect.wobble * 0.5;
                p += side * sway * zoom;
                velocity += side * rate * (age * rate + phase).cos();
            }
            if p.x < -120.
                || p.y < -120.
                || p.x > screen_width() + 120.
                || p.y > screen_height() + 120.
            {
                continue;
            }
            let fraction = age / effect.lifetime;
            // Growing particles swell fast and settle, as a puff does; shrinking
            // ones hold their size, then dwindle, as an ember does.
            let eased = if effect.end_size >= effect.size {
                fraction * (2. - fraction)
            } else {
                fraction * fraction
            };
            // Each one a little larger or smaller than the next, and grown in
            // over its first moment rather than popping in.
            let radius = (effect.size + (effect.end_size - effect.size) * eased)
                * zoom
                * (0.82 + 0.36 * random(9))
                * (0.6 + 0.4 * smooth(age / 0.08));
            if radius < 0.3 {
                continue;
            }
            let (shimmer, glint) = if effect.twinkle > 0. {
                let rate = TAU * (1.2 + 2. * random(7));
                let dip = 0.5 + 0.5 * (age * rate + random(8) * TAU).sin();
                (
                    1. - effect.twinkle * dip,
                    effect.twinkle * smooth((0.14 - dip) / 0.14),
                )
            } else {
                (1., 0.)
            };
            let mut color = mix_light(from, to, fraction);
            color.a = particle_alpha(effect.fade, fraction, age) * opacity * shimmer;
            if color.a <= 0.003 {
                continue;
            }
            motes.push(Mote {
                at: p,
                radius,
                velocity: if velocity.length_squared() > 1e-4 {
                    velocity * zoom
                } else {
                    heading
                },
                turn: (random(4) * 360. + effect.spin * age).to_radians(),
                color,
                glint,
            });
            *remaining -= 1;
        }
        draw_motes(effect, motes, zoom);
    });
}

/// A spark's half length along its travel and half width across its head:
/// longer the faster it goes, as the eye smears a quick bright thing.
fn streak(mote: &Mote) -> (Vec2, f32) {
    let direction = mote.velocity.normalize_or_zero();
    let smear = (mote.velocity.length() * 0.03).min(mote.radius * 5.);
    (
        direction * (mote.radius + smear),
        (mote.radius * 0.32).max(0.9),
    )
}

/// The hot middle of a glowing particle: its color, most of the way to white.
fn core(c: Color) -> Color {
    Color::new(
        c.r + (1. - c.r) * 0.6,
        c.g + (1. - c.g) * 0.6,
        c.b + (1. - c.b) * 0.6,
        c.a * 0.85,
    )
}

fn draw_motes(effect: &ParticleEffect, motes: &[Mote], zoom: f32) {
    use crate::sprites::{self, Sprite};
    if motes.is_empty() {
        return;
    }
    let shape = effect.shape;
    let ring_width = (zoom * 2.).max(1.);
    // Light first: a soft halo under each glowing particle.
    if effect.glow {
        if glow::begin(Blend::Glow) {
            for m in motes {
                let halo = alpha(m.color, m.color.a * HALO);
                match shape {
                    ParticleShape::Spark => {
                        let (along, across) = streak(m);
                        glow::stretched_spot(m.at, along * 1.3, across * 5., halo);
                    }
                    // A soft particle is its own glow, and a ring glows along
                    // its line, below.
                    ParticleShape::Soft | ParticleShape::Ring => {}
                    _ => glow::spot(m.at, m.radius * 2.6, halo),
                }
            }
            glow::end();
        }
        if shape == ParticleShape::Ring && glow::begin(Blend::Light) {
            for m in motes {
                let halo = alpha(m.color, m.color.a * HALO * 0.8);
                sprites::ring(m.at, m.radius, 0., (6. * zoom).max(3.), halo);
            }
            glow::end();
        }
    }
    // The particles themselves.
    if shape == ParticleShape::Soft {
        if glow::begin(if effect.glow {
            Blend::Glow
        } else {
            Blend::Soft
        }) {
            for m in motes {
                glow::spot(m.at, m.radius * 2., m.color);
            }
            glow::end();
        } else {
            for m in motes {
                glow::fallback_spot(m.at, m.radius * 1.6, m.color);
            }
        }
    } else {
        let lit = effect.glow && glow::begin(Blend::Light);
        for m in motes {
            match shape {
                ParticleShape::Dot => sprites::draw(
                    Sprite::Disc,
                    m.at,
                    Vec2::splat(m.radius / 0.86),
                    0.,
                    m.color,
                ),
                ParticleShape::Square => sprites::draw(
                    Sprite::Square,
                    m.at,
                    Vec2::splat(m.radius * 0.84),
                    m.turn,
                    m.color,
                ),
                ParticleShape::Triangle => sprites::draw(
                    Sprite::Triangle,
                    m.at,
                    Vec2::splat(m.radius * 1.13),
                    m.turn,
                    m.color,
                ),
                ParticleShape::Star => sprites::draw(
                    Sprite::Star,
                    m.at,
                    Vec2::splat(m.radius * 1.38),
                    m.turn,
                    m.color,
                ),
                ParticleShape::Ring => sprites::ring(m.at, m.radius, ring_width, 0.9, m.color),
                ParticleShape::Spark => {
                    let (along, across) = streak(m);
                    sprites::draw(
                        Sprite::Spark,
                        m.at,
                        vec2(along.length(), across / SPARK_HEAD),
                        along.y.atan2(along.x),
                        m.color,
                    );
                }
                ParticleShape::Soft => {}
            }
        }
        if lit {
            glow::end();
        }
    }
    // The brightest part of glowing particles, and the catch of light at the
    // top of a twinkle.
    if effect.glow && glow::begin(Blend::Light) {
        for m in motes {
            let hot = core(m.color);
            match shape {
                ParticleShape::Dot => {
                    sprites::draw(Sprite::Disc, m.at, Vec2::splat(m.radius * 0.45), 0., hot)
                }
                // A soft particle brightens towards its middle with no edge.
                ParticleShape::Soft => sprites::draw(
                    Sprite::Halo,
                    m.at,
                    Vec2::splat(m.radius),
                    0.,
                    alpha(hot, hot.a * 0.5),
                ),
                ParticleShape::Square => sprites::draw(
                    Sprite::Square,
                    m.at,
                    Vec2::splat(m.radius * 0.4),
                    m.turn,
                    hot,
                ),
                ParticleShape::Triangle => sprites::draw(
                    Sprite::Triangle,
                    m.at,
                    Vec2::splat(m.radius * 0.55),
                    m.turn,
                    hot,
                ),
                ParticleShape::Star => {
                    sprites::draw(Sprite::Star, m.at, Vec2::splat(m.radius * 0.7), m.turn, hot)
                }
                ParticleShape::Ring => {
                    sprites::ring(m.at, m.radius, ring_width * 0.45, 0.6, hot);
                }
                ParticleShape::Spark => {
                    let (along, across) = streak(m);
                    sprites::draw(
                        Sprite::Spark,
                        m.at,
                        vec2(along.length() * 0.85, across / SPARK_HEAD * 0.5),
                        along.y.atan2(along.x),
                        hot,
                    );
                }
            }
            if m.glint > 0.01 && matches!(shape, ParticleShape::Star | ParticleShape::Dot) {
                sprites::draw(
                    Sprite::Glint,
                    m.at,
                    Vec2::splat(m.radius * (2.4 + 1.2 * m.glint)),
                    std::f32::consts::FRAC_PI_4 * 0.25,
                    alpha(core(m.color), m.color.a.min(1.) * m.glint),
                );
            }
        }
        glow::end();
    }
}

/// A cached broad phase for authored block styles. Looking up every wall
/// against every block would be quadratic on a busy custom map.
pub struct BlockStyles {
    rects: Vec<[f32; 4]>,
    cells: Vec<Vec<usize>>,
}
thread_local! {
    static STYLES: std::cell::RefCell<Option<std::rc::Rc<BlockStyles>>> = const { std::cell::RefCell::new(None) };
}
pub fn block_styles(map: &MapDef) -> std::rc::Rc<BlockStyles> {
    STYLES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(index) = &*cache
            && index.rects.len() == map.blocks.len()
            && index
                .rects
                .iter()
                .zip(&map.blocks)
                .all(|(rect, block)| *rect == block.rect)
        {
            return index.clone();
        }
        let rects: Vec<_> = map.blocks.iter().map(|b| b.rect).collect();
        let mut cells = vec![Vec::new(); 64];
        for (index, r) in rects.iter().enumerate() {
            let x0 = (r[0] * 8.).floor().clamp(0., 7.) as usize;
            let y0 = (r[1] * 8.).floor().clamp(0., 7.) as usize;
            let x1 = ((r[0] + r[2]) * 8.).floor().clamp(0., 7.) as usize;
            let y1 = ((r[1] + r[3]) * 8.).floor().clamp(0., 7.) as usize;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    cells[y * 8 + x].push(index);
                }
            }
        }
        let index = std::rc::Rc::new(BlockStyles { rects, cells });
        *cache = Some(index.clone());
        index
    })
}
impl BlockStyles {
    /// A carved collider keeps its source style without painting a full block
    /// over team territory that the simulator reserved as empty.
    pub fn collider_style(
        &self,
        map: &MapDef,
        config: &WorldConfig,
        hidden: u32,
        wall: tankyard_core::Rect,
    ) -> (WallStyle, Option<Color>) {
        let center = wall.center();
        let bounds = config.bounds();
        let nx = (center.x - bounds.x) / bounds.w;
        let ny = (center.y - bounds.y) / bounds.h;
        let x = (nx * 8.).floor().clamp(0., 7.) as usize;
        let y = (ny * 8.).floor().clamp(0., 7.) as usize;
        for &index in &self.cells[y * 8 + x] {
            let block = &map.blocks[index];
            if MapDef::group_bit(block.group) & hidden != 0 {
                continue;
            }
            let [bx, by, bw, bh] = block.rect;
            if nx >= bx && nx <= bx + bw && ny >= by && ny <= by + bh {
                return (
                    wall_style(
                        if block.style == BlockStyle::Map {
                            map.style
                        } else {
                            block.style
                        },
                        &map.id,
                    ),
                    block.color.map(color),
                );
            }
        }
        (wall_style(map.style, &map.id), None)
    }
}

pub fn block(r: Rect, style: WallStyle, tint: Option<Color>, dark: bool, quality: u8) {
    tankyard_client::map_model::wall_geometry(
        r,
        style,
        dark,
        quality,
        &mut render::draw_model_primitive,
    );
    if let Some(tint) = tint {
        let inset = r.w.min(r.h) * 0.12;
        draw_rectangle(
            r.x + inset,
            r.y + inset,
            r.w - 2. * inset,
            r.h - 2. * inset,
            alpha(tint, 0.3),
        );
    }
}

/// Marks a block of a pushable body, so players can tell what gives way: a
/// frame inside its edge with bolts at the corners, braced corner to corner
/// when it is squarish and slatted across when it is long and thin.
pub fn body_marks(r: Rect, dark: bool) {
    let short = r.w.min(r.h);
    if short < 4. {
        return;
    }
    let ink = if dark { WHITE } else { BLACK };
    let c = alpha(ink, 0.22);
    let inset = (short * 0.14).clamp(1.5, 12.);
    let inner = Rect::new(r.x + inset, r.y + inset, r.w - inset * 2., r.h - inset * 2.);
    let thickness = (short * 0.05).clamp(1., 4.);
    draw_rectangle_lines(inner.x, inner.y, inner.w, inner.h, thickness, c);
    let long = r.w.max(r.h);
    if long < short * 2.2 {
        draw_line(
            inner.x,
            inner.y,
            inner.right(),
            inner.bottom(),
            thickness,
            c,
        );
        draw_line(
            inner.right(),
            inner.y,
            inner.x,
            inner.bottom(),
            thickness,
            c,
        );
    } else {
        // Slats leaning across the short side, spaced by it.
        let slats = (long / short).floor().clamp(1., 24.) as usize;
        for i in 0..slats {
            let t = (i as f32 + 0.5) / slats as f32;
            let (a, b) = if r.w >= r.h {
                let x = inner.x + t * inner.w;
                let lean = inner.h * 0.35;
                (vec2(x - lean, inner.bottom()), vec2(x + lean, inner.y))
            } else {
                let y = inner.y + t * inner.h;
                let lean = inner.w * 0.35;
                (vec2(inner.x, y + lean), vec2(inner.right(), y - lean))
            };
            draw_line(a.x, a.y, b.x, b.y, thickness, c);
        }
    }
    let bolt = (short * 0.05).clamp(1., 4.);
    for corner in [
        vec2(inner.x, inner.y),
        vec2(inner.right(), inner.y),
        vec2(inner.right(), inner.bottom()),
        vec2(inner.x, inner.bottom()),
    ] {
        draw_circle(corner.x, corner.y, bolt, alpha(ink, 0.35));
    }
}

#[cfg(test)]
mod tests {
    use super::{particle_alpha, stripe_segment};

    #[test]
    fn stripes_keep_their_angle_and_stay_inside_tall_and_wide_decor() {
        for size in [
            macroquad::prelude::vec2(40., 400.),
            macroquad::prelude::vec2(400., 40.),
            macroquad::prelude::vec2(100., 100.),
        ] {
            let mut previous = None;
            for i in 0..48 {
                let segment = stripe_segment(size, (i as f32 + 0.5) / 48.);
                for p in segment {
                    assert!(p.x.abs() <= size.x * 0.5 + 0.001);
                    assert!(p.y.abs() <= size.y * 0.5 + 0.001);
                }
                let line = segment[1] - segment[0];
                assert!(line.y > 0. && (line.x - line.y * 0.5).abs() < 0.001);
                assert_ne!(
                    previous,
                    Some(segment),
                    "stripes must not collapse together"
                );
                previous = Some(segment);
            }
        }
    }

    #[test]
    fn fading_particles_appear_quickly_and_fade_as_they_age() {
        assert_eq!(particle_alpha(true, 0., 0.), 0.);
        assert!(particle_alpha(true, 0.02, 0.05) < 0.6, "still appearing");
        assert!(
            particle_alpha(true, 0.1, 0.2) > 0.95,
            "in within a tenth of a second"
        );
        // It holds most of its strength through middle age, then goes.
        assert!(particle_alpha(true, 0.5, 0.8) > 0.6);
        // Once in, it only fades.
        let mut previous = 1.;
        for step in 2..=20 {
            let fraction = step as f32 / 20.;
            let alpha = particle_alpha(true, fraction, fraction * 1.6);
            assert!(alpha <= previous, "fades steadily");
            previous = alpha;
        }
        assert_eq!(particle_alpha(true, 1., 1.6), 0.);
        // Without fade, a particle is solid from its first to its last frame.
        assert_eq!(particle_alpha(false, 0., 0.), 1.);
        assert_eq!(particle_alpha(false, 1., 1.6), 1.);
    }
}
