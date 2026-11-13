//! Map art and effects. Particles are reconstructed from their birth time;
//! there is no particle allocation, network traffic or simulation work here.
use crate::glow::{self, Blend};
use crate::{
    render::{self, View},
    settings::Settings,
    ui::{self, alpha, mix},
};
use macroquad::prelude::*;
use tankyard_client::map_model::WallStyle;
use tankyard_content::{
    BlockStyle, DecorKind, EmitArea, GroupPoses, MAP_COUNTERS, MapDecor, MapDef, MapZone,
    ParticleEffect, ParticleShape, ZoneKind,
};
use tankyard_core::{V2, WorldConfig};

/// The parts of a map that change during a round, as the art needs them.
#[derive(Clone, Debug, Default)]
pub struct Shown {
    pub hidden: u32,
    pub counters: [i32; MAP_COUNTERS as usize],
    /// Where map logic has moved, turned and faded each group.
    pub poses: GroupPoses,
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
    }
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
    for zone in &map.zones {
        if MapDef::group_bit(zone.group) & shown.hidden != 0 {
            continue;
        }
        let rect = shown.poses.shift(zone.rect, zone.group);
        if zone.kind == ZoneKind::Portal {
            portal(zone, rect, config, view, time);
            continue;
        }
        let r = screen_rect(view, config, rect);
        if !visible(r, 0.) {
            continue;
        }
        let c = crate::accessibility::palette(zone_color(zone.kind));
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(c, 0.13));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, (2. * view.zoom).max(1.), alpha(c, 0.65));
        // A restrained repeating stencil makes pads distinguishable without
        // relying on colour. The count is capped even for arena-sized pads.
        let spacing = (100. * view.zoom).max(24.);
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
                // Conveyor arrows stay readable when zoomed out.
                let least = if zone.kind == ZoneKind::Conveyor {
                    spacing * 0.3
                } else {
                    1.
                };
                let radius = (10. * view.zoom).min(r.w.min(r.h) * 0.18).max(least);
                zone_mark(zone.kind, zone.angle, p, radius, alpha(c, 0.5));
                marks_left -= 1;
            }
        }
    }
}
/// A portal pad: turning rings over its area and a matching ring at the exit.
fn portal(zone: &MapZone, rect: [f32; 4], config: &WorldConfig, view: &View, time: f32) {
    let r = screen_rect(view, config, rect);
    let exit = view.screen(tankyard_core::map_point(config, zone.exit));
    let exit = vec2(exit.x, exit.y);
    let c = crate::accessibility::palette(zone_color(ZoneKind::Portal));
    let radius = (r.w.min(r.h) * 0.5).max(4.);
    if visible(r, 0.) {
        let p = r.center();
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(c, 0.10));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, (2. * view.zoom).max(1.), alpha(c, 0.55));
        for ring in 0..3 {
            let phase = (time * 0.6 + ring as f32 / 3.).fract();
            draw_circle_lines(
                p.x,
                p.y,
                radius * (0.25 + 0.7 * phase),
                (2.5 * view.zoom).max(1.),
                alpha(c, 0.75 * (1. - phase)),
            );
        }
        draw_circle(p.x, p.y, radius * 0.18, alpha(c, 0.8));
    }
    let size = (34. * view.zoom).max(6.);
    if exit.x > -size
        && exit.y > -size
        && exit.x < screen_width() + size
        && exit.y < screen_height() + size
    {
        draw_circle_lines(
            exit.x,
            exit.y,
            size,
            (2.5 * view.zoom).max(1.),
            alpha(c, 0.7),
        );
        draw_poly_lines(
            exit.x,
            exit.y,
            4,
            size * 0.55,
            45. + time * 40.,
            2.,
            alpha(c, 0.6),
        );
    }
}

pub fn zone_mark(kind: ZoneKind, angle: f32, p: Vec2, r: f32, c: Color) {
    match kind {
        ZoneKind::Speed => {
            for offset in [-0.5, 0.35] {
                let x = p.x + offset * r;
                draw_line(x - r * 0.4, p.y - r * 0.65, x + r * 0.25, p.y, 2., c);
                draw_line(x + r * 0.25, p.y, x - r * 0.4, p.y + r * 0.65, 2., c);
            }
        }
        ZoneKind::Repair => {
            draw_rectangle(p.x - r, p.y - r * 0.28, r * 2., r * 0.56, c);
            draw_rectangle(p.x - r * 0.28, p.y - r, r * 0.56, r * 2., c);
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
                let thickness = (r * 0.3).clamp(1.5, 2.5);
                draw_line(a.x, a.y, tip.x, tip.y, thickness, c);
                draw_line(tip.x, tip.y, b.x, b.y, thickness, c);
            }
        }
        ZoneKind::Portal => {
            draw_circle_lines(p.x, p.y, r, 2., c);
            draw_circle_lines(p.x, p.y, r * 0.5, 2., c);
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
        if item.above != above || MapDef::group_bit(item.group) & shown.hidden != 0 {
            continue;
        }
        let fade = shown.poses.opacity(item.group);
        if fade <= 0.004 {
            continue;
        }
        let turn = shown.poses.turn(item.group);
        let r = screen_rect(view, config, shown.poses.shift(item.rect, item.group));
        if item.kind == DecorKind::Emitter {
            if remaining == 0 || settings.reduced_motion {
                continue;
            }
            let mut effect = item.effect.unwrap_or_default();
            effect.direction += item.rotation + turn + time * item.spin;
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
                f32::from(item.alpha) / 255. * pulse * fade,
            );
        } else if visible(r, r.w.max(r.h) * 0.5) {
            decoration(item, r, time, view.zoom, &shown.counters, turn, fade);
        }
    }
    // Single pieces drawn elsewhere, such as palette previews, always shade.
    SHADED_LIGHTS.with(|left| left.set(u32::MAX));
}

/// One piece of decor in `r`, turned a further `turn` degrees and faded to
/// `fade` by map logic.
pub fn decoration(
    item: &MapDecor,
    r: Rect,
    time: f32,
    zoom: f32,
    counters: &[i32; MAP_COUNTERS as usize],
    turn: f32,
    fade: f32,
) {
    let pulse = if item.pulse > 0. {
        0.8 + 0.2 * (time * item.pulse * std::f32::consts::TAU).cos()
    } else {
        1.
    };
    let c = alpha(
        crate::accessibility::palette(color(item.color)),
        f32::from(item.alpha) / 255. * pulse * fade,
    );
    let p = r.center();
    let angle = (item.rotation + turn + time * item.spin).to_radians();
    let rotate = |point: Vec2| {
        p + vec2(
            point.x * angle.cos() - point.y * angle.sin(),
            point.x * angle.sin() + point.y * angle.cos(),
        )
    };
    match item.kind {
        DecorKind::Tile | DecorKind::Stripes => {
            let vertices = [
                vec2(-r.w, -r.h),
                vec2(r.w, -r.h),
                vec2(r.w, r.h),
                vec2(-r.w, r.h),
            ]
            .map(|v| rotate(v * 0.5));
            draw_triangle(vertices[0], vertices[1], vertices[2], c);
            draw_triangle(vertices[0], vertices[2], vertices[3], c);
            if item.kind == DecorKind::Stripes {
                let n = (r.w / (16. * zoom).max(8.)).ceil().clamp(1., 48.) as usize;
                for i in 0..n {
                    let x = -r.w * 0.5 + (i as f32 + 0.5) / n as f32 * r.w;
                    let a = rotate(vec2((x - r.h * 0.25).max(-r.w * 0.5), -r.h * 0.5));
                    let b = rotate(vec2((x + r.h * 0.25).min(r.w * 0.5), r.h * 0.5));
                    draw_line(
                        a.x,
                        a.y,
                        b.x,
                        b.y,
                        (r.w / n as f32 * 0.36).max(1.),
                        alpha(BLACK, c.a * 0.45),
                    );
                }
            }
        }
        DecorKind::Light => light(item, p, r.w.min(r.h) * 0.5, c, time),
        DecorKind::Ring => draw_circle_lines(p.x, p.y, r.w.min(r.h) * 0.5, (3. * zoom).max(1.), c),
        DecorKind::Arrow => {
            let a = rotate(vec2(-r.w * 0.4, 0.));
            let b = rotate(vec2(r.w * 0.1, 0.));
            draw_line(a.x, a.y, b.x, b.y, (r.h * 0.22).max(1.), c);
            draw_triangle(
                rotate(vec2(r.w * 0.5, 0.)),
                rotate(vec2(0., -r.h * 0.5)),
                rotate(vec2(0., r.h * 0.5)),
                c,
            );
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
    } else {
        glow::fallback_spot(p, radius, alpha(c, strength * 0.9));
        draw_circle(p.x, p.y, radius * 0.14, alpha(hot, strength * 0.6));
    }
    draw_circle(
        p.x,
        p.y,
        (radius * 0.05).max(1.2),
        alpha(mix(c, WHITE, 0.8), strength),
    );
}

fn hash(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846ca68b);
    value ^= value >> 16;
    (value >> 8) as f32 / 16_777_216.
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

fn star(p: Vec2, radius: f32, rotation: f32, c: Color) {
    let point = |i: usize, scale: f32| {
        let a = (rotation - 90. + i as f32 * 36.).to_radians();
        p + vec2(a.cos(), a.sin()) * radius * scale
    };
    for i in 0..5 {
        let tip = point(i * 2, 1.);
        draw_triangle(p, point(i * 2 + 9, 0.42), tip, c);
        draw_triangle(p, tip, point(i * 2 + 1, 0.42), c);
    }
}

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
    let blend = match (effect.shape, effect.glow) {
        (ParticleShape::Soft, true) => Some(Blend::Glow),
        (ParticleShape::Soft, false) => Some(Blend::Soft),
        (_, true) => Some(Blend::Add),
        (_, false) => None,
    };
    let shaded = blend.is_some_and(glow::begin);
    let last = (time * effect.rate).floor() as i64;
    let count = (effect.rate * effect.lifetime).ceil().min(240.) as i64;
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
        if effect.wobble > 0. {
            // Sway across the direction of travel, starting from the origin.
            let phase = random(5) * TAU;
            let rate = TAU * (0.4 + 0.8 * random(6));
            let sway = (age * rate + phase).sin() - phase.sin();
            p += vec2(-heading.y, heading.x) * effect.wobble * 0.5 * sway * zoom;
        }
        if p.x < -120. || p.y < -120. || p.x > screen_width() + 120. || p.y > screen_height() + 120.
        {
            continue;
        }
        let fraction = age / effect.lifetime;
        let radius = (effect.size + (effect.end_size - effect.size) * fraction) * zoom;
        if radius < 0.4 {
            continue;
        }
        let shimmer = if effect.twinkle > 0. {
            let rate = TAU * (1.2 + 2. * random(7));
            1. - effect.twinkle * (0.5 + 0.5 * (age * rate + random(8) * TAU).sin())
        } else {
            1.
        };
        let c = alpha(
            alpha(
                mix(color(effect.color), color(effect.end_color), fraction),
                if effect.fade {
                    (1. - fraction).min(age * 10.)
                } else {
                    1.
                },
            ),
            opacity * shimmer,
        );
        let rotation = random(4) * 360. + effect.spin * age;
        match effect.shape {
            ParticleShape::Dot => draw_circle(p.x, p.y, radius, c),
            ParticleShape::Soft if shaded => glow::spot(p, radius * 2., c),
            ParticleShape::Soft => {
                draw_circle(p.x, p.y, radius * 1.4, alpha(c, c.a * 0.3));
                draw_circle(p.x, p.y, radius * 0.7, alpha(c, c.a * 0.5));
            }
            ParticleShape::Square => draw_poly(p.x, p.y, 4, radius, rotation + 45., c),
            ParticleShape::Triangle => draw_poly(p.x, p.y, 3, radius, rotation, c),
            ParticleShape::Star => star(p, radius * 1.3, rotation, c),
            ParticleShape::Ring => draw_circle_lines(p.x, p.y, radius, (zoom * 2.).max(1.), c),
            ParticleShape::Spark => {
                let d = heading * radius;
                draw_line(
                    p.x - d.x,
                    p.y - d.y,
                    p.x + d.x,
                    p.y + d.y,
                    (radius * 0.3).max(1.),
                    c,
                );
            }
        }
        *remaining -= 1;
    }
    if shaded {
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
