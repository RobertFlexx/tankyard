//! The looks a widget can take: panel outlines, fills, borders, lettering,
//! bars and buttons. Map triggers and scripts mix these freely, so one
//! arena's interface need not look like another's.
use crate::ui::{alpha, label, mix};
// Page-space shapes, so widgets scroll with an editor page like its text.
use crate::ui::{ui_circle as draw_circle, ui_line as draw_line, ui_rectangle as draw_rectangle};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use tankyard_content::{HudBar, HudBorder, HudButtons, HudFill, HudLettering, HudShape};

/// How far a slanted panel leans, a tag's point reaches and a ribbon's
/// notches cut in, as a share of the panel's height.
const SLANT: f32 = 0.24;
const TAG: f32 = 0.4;
const NOTCH: f32 = 0.26;

thread_local! {
    // draw_mesh copies the geometry into macroquad's batch before returning.
    // Reuse this storage for panel fills and the many clipped pattern pieces.
    static FILL_MESH: RefCell<Mesh> = const { RefCell::new(Mesh {
        vertices: Vec::new(), indices: Vec::new(), texture: None,
    }) };
}

/// Room a shape needs beside the content, on the left and the right, for a
/// panel `height` tall.
pub fn shape_room(shape: HudShape, height: f32) -> (f32, f32) {
    match shape {
        HudShape::Slant => (height * SLANT, height * SLANT),
        HudShape::Tag => (0., height * TAG),
        HudShape::Ribbon => (height * NOTCH, height * NOTCH),
        HudShape::Rounded | HudShape::Square | HudShape::Chamfer => (0., 0.),
    }
}

/// A panel's outline, clockwise on screen from the top left.
pub struct Outline {
    pub points: Vec<Vec2>,
    /// The corners that bracket borders mark.
    pub corners: Vec<Vec2>,
    /// A convex region inside the outline that patterns are cut to.
    pub core: Vec<Vec2>,
}

pub fn outline(rect: Rect, shape: HudShape, corner: f32) -> Outline {
    let (x, y, w, h) = (rect.x, rect.y, rect.w.max(1.), rect.h.max(1.));
    let square = vec![
        vec2(x, y),
        vec2(x + w, y),
        vec2(x + w, y + h),
        vec2(x, y + h),
    ];
    let sharp = |points: Vec<Vec2>| Outline {
        corners: points.clone(),
        core: points.clone(),
        points,
    };
    match shape {
        HudShape::Rounded => {
            let points = rounded_points(rect, corner.min(w * 0.5).min(h * 0.5).max(0.));
            Outline {
                core: points.clone(),
                points,
                corners: square,
            }
        }
        HudShape::Square => sharp(square),
        HudShape::Chamfer => {
            let c = corner.max(3.).min(w * 0.3).min(h * 0.4);
            sharp(vec![
                vec2(x + c, y),
                vec2(x + w - c, y),
                vec2(x + w, y + c),
                vec2(x + w, y + h - c),
                vec2(x + w - c, y + h),
                vec2(x + c, y + h),
                vec2(x, y + h - c),
                vec2(x, y + c),
            ])
        }
        HudShape::Slant => {
            let lean = (h * SLANT).min(w * 0.3);
            sharp(vec![
                vec2(x + lean, y),
                vec2(x + w, y),
                vec2(x + w - lean, y + h),
                vec2(x, y + h),
            ])
        }
        HudShape::Tag => {
            let point = (h * TAG).min(w * 0.4);
            sharp(vec![
                vec2(x, y),
                vec2(x + w - point, y),
                vec2(x + w, y + h * 0.5),
                vec2(x + w - point, y + h),
                vec2(x, y + h),
            ])
        }
        HudShape::Ribbon => {
            let notch = (h * NOTCH).min(w * 0.25);
            let points = vec![
                vec2(x, y),
                vec2(x + w, y),
                vec2(x + w - notch, y + h * 0.5),
                vec2(x + w, y + h),
                vec2(x, y + h),
                vec2(x + notch, y + h * 0.5),
            ];
            Outline {
                corners: points.clone(),
                points,
                core: vec![
                    vec2(x + notch, y),
                    vec2(x + w - notch, y),
                    vec2(x + w - notch, y + h),
                    vec2(x + notch, y + h),
                ],
            }
        }
    }
}

/// A band `width` wide down a panel's left edge, following its shape, in
/// convex pieces.
pub fn side_band(rect: Rect, shape: HudShape, corner: f32, width: f32) -> Vec<Vec<Vec2>> {
    let (x, y, w, h) = (rect.x, rect.y, rect.w.max(1.), rect.h.max(1.));
    match shape {
        HudShape::Slant => {
            let lean = (h * SLANT).min(w * 0.3);
            vec![vec![
                vec2(x + lean, y),
                vec2(x + lean + width, y),
                vec2(x + width, y + h),
                vec2(x, y + h),
            ]]
        }
        HudShape::Ribbon => {
            let notch = (h * NOTCH).min(w * 0.25);
            let middle = y + h * 0.5;
            vec![
                vec![
                    vec2(x, y),
                    vec2(x + width, y),
                    vec2(x + notch + width, middle),
                    vec2(x + notch, middle),
                ],
                vec![
                    vec2(x + notch, middle),
                    vec2(x + notch + width, middle),
                    vec2(x + width, y + h),
                    vec2(x, y + h),
                ],
            ]
        }
        _ => {
            let band = [
                vec2(x, y),
                vec2(x + width, y),
                vec2(x + width, y + h),
                vec2(x, y + h),
            ];
            vec![clip(&band, &outline(rect, shape, corner).core)]
        }
    }
}

fn rounded_points(rect: Rect, r: f32) -> Vec<Vec2> {
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    if r < 0.5 {
        return vec![
            vec2(x, y),
            vec2(x + w, y),
            vec2(x + w, y + h),
            vec2(x, y + h),
        ];
    }
    let steps = ((r * 0.5) as usize).clamp(3, 10);
    let mut points = Vec::with_capacity((steps + 1) * 4);
    for (cx, cy, from) in [
        (x + r, y + r, PI),
        (x + w - r, y + r, -FRAC_PI_2),
        (x + w - r, y + h - r, 0.),
        (x + r, y + h - r, FRAC_PI_2),
    ] {
        for i in 0..=steps {
            let a = from + FRAC_PI_2 * i as f32 / steps as f32;
            points.push(vec2(cx + a.cos() * r, cy + a.sin() * r));
        }
    }
    points
}

/// Fill `points`, convex or star-shaped about their middle, shading from
/// `top` at the highest point to `bottom` at the lowest.
pub fn fill(points: &[Vec2], top: Color, bottom: Color) {
    if points.len() < 3 || points.len() > 4000 || (top.a <= 0.002 && bottom.a <= 0.002) {
        return;
    }
    let (low, high) = points.iter().fold((f32::MAX, f32::MIN), |(low, high), p| {
        (low.min(p.y), high.max(p.y))
    });
    let span = (high - low).max(1e-3);
    let shade = |p: Vec2| mix(top, bottom, (p.y - low) / span);
    // Meshes skip the page-space wrappers, so place the points here.
    let vertex = |p: Vec2| {
        let at = crate::ui_layout::position(p);
        Vertex::new(at.x, at.y, 0., 0., 0., shade(p))
    };
    let middle = points.iter().copied().sum::<Vec2>() / points.len() as f32;
    FILL_MESH.with(|storage| {
        let mut mesh = storage.borrow_mut();
        mesh.vertices.clear();
        mesh.indices.clear();
        mesh.vertices.push(vertex(middle));
        mesh.vertices.extend(points.iter().map(|p| vertex(*p)));
        let n = points.len() as u16;
        mesh.indices
            .extend((1..=n).flat_map(|i| [0, i, if i == n { 1 } else { i + 1 }]));
        draw_mesh(&mesh);
    });
}

fn signed_area(points: &[Vec2]) -> f32 {
    (0..points.len())
        .map(|i| points[i].perp_dot(points[(i + 1) % points.len()]))
        .sum::<f32>()
        * 0.5
}

/// `subject` cut to the inside of the convex polygon `region`.
pub fn clip(subject: &[Vec2], region: &[Vec2]) -> Vec<Vec2> {
    let mut out = subject.to_vec();
    let turn = signed_area(region).signum();
    for i in 0..region.len() {
        if out.len() < 3 {
            return Vec::new();
        }
        let (a, b) = (region[i], region[(i + 1) % region.len()]);
        let edge = b - a;
        let inside = |p: Vec2| edge.perp_dot(p - a) * turn >= 0.;
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (p_in, q_in) = (inside(p), inside(q));
            if p_in {
                out.push(p);
            }
            let across = edge.perp_dot(q - p);
            if p_in != q_in && across.abs() > 1e-6 {
                out.push(p + (q - p) * (edge.perp_dot(a - p) / across));
            }
        }
    }
    if out.len() < 3 { Vec::new() } else { out }
}

fn contains(region: &[Vec2], p: Vec2) -> bool {
    let turn = signed_area(region).signum();
    (0..region.len()).all(|i| {
        let (a, b) = (region[i], region[(i + 1) % region.len()]);
        (b - a).perp_dot(p - a) * turn >= 0.
    })
}

/// Positions from `from` up to `to`, `step` apart: at most a few hundred,
/// and none when the numbers make no sense.
fn ticks(from: f32, to: f32, step: f32) -> impl Iterator<Item = f32> {
    let count = if step > 0. && to > from {
        ((to - from) / step).ceil().min(512.) as usize
    } else {
        0
    };
    (0..count).map(move |i| from + i as f32 * step)
}

/// Stripes, a grid or dots in `color` across the convex `region`, which
/// lies within `bounds`. `drift` slides stripes along.
pub fn pattern(kind: HudFill, region: &[Vec2], bounds: Rect, color: Color, scale: f32, drift: f32) {
    if region.len() < 3 || color.a <= 0.002 {
        return;
    }
    let scale = scale.max(0.25);
    let band = |corners: [Vec2; 4]| fill(&clip(&corners, region), color, color);
    match kind {
        HudFill::Stripes => {
            let step = 13. * scale;
            let width = 5.5 * scale;
            let lean = bounds.h;
            let from = bounds.x - step + drift.rem_euclid(step);
            for x in ticks(from, bounds.right() + lean, step) {
                band([
                    vec2(x, bounds.y),
                    vec2(x + width, bounds.y),
                    vec2(x + width - lean, bounds.bottom()),
                    vec2(x - lean, bounds.bottom()),
                ]);
            }
        }
        HudFill::Grid => {
            let step = 9. * scale;
            let line = scale.max(1.);
            for x in ticks(bounds.x + step * 0.5, bounds.right(), step) {
                band([
                    vec2(x, bounds.y),
                    vec2(x + line, bounds.y),
                    vec2(x + line, bounds.bottom()),
                    vec2(x, bounds.bottom()),
                ]);
            }
            for y in ticks(bounds.y + step * 0.5, bounds.bottom(), step) {
                band([
                    vec2(bounds.x, y),
                    vec2(bounds.right(), y),
                    vec2(bounds.right(), y + line),
                    vec2(bounds.x, y + line),
                ]);
            }
        }
        HudFill::Dots => {
            let step = 8. * scale;
            let radius = 1.3 * scale;
            for (row, y) in ticks(bounds.y + step * 0.5, bounds.bottom(), step).enumerate() {
                let from = bounds.x + step * if row % 2 == 0 { 0.5 } else { 1. };
                for x in ticks(from, bounds.right(), step) {
                    if contains(region, vec2(x, y)) {
                        draw_circle(x, y, radius, color);
                    }
                }
            }
        }
        HudFill::Solid | HudFill::Gradient | HudFill::Glass => {}
    }
}

/// A line along `points`, closed back to the first.
fn stroke(points: &[Vec2], thickness: f32, color: Color, joints: bool) {
    for i in 0..points.len() {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        draw_line(a.x, a.y, b.x, b.y, thickness, color);
        if joints {
            draw_circle(a.x, a.y, thickness * 0.5, color);
        }
    }
}

/// The pieces of a dashed line around the closed outline `points`. Dashes
/// keep their rhythm around corners: the k-th starts k periods along the
/// whole outline. Each is counted by its number rather than by stepping a
/// position, since tiny steps can round away on a long outline.
fn dash_spans(points: &[Vec2], dash: f32, gap: f32) -> Vec<(Vec2, Vec2)> {
    let dash = dash.max(0.5);
    let period = dash + gap.max(0.5);
    let mut spans = Vec::new();
    let mut walked = 0.;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        let length = a.distance(b);
        if !(1e-3..1e5).contains(&length) {
            continue;
        }
        let way = (b - a) / length;
        let first = (walked / period).floor() as i64;
        let last = ((walked + length) / period).ceil() as i64;
        for k in first..last {
            let start = (k as f32 * period - walked).max(0.);
            let end = (k as f32 * period + dash - walked).min(length);
            if end > start {
                spans.push((a + way * start, a + way * end));
            }
        }
        walked += length;
    }
    spans
}

fn dashed(points: &[Vec2], thickness: f32, dash: f32, gap: f32, color: Color) {
    for (p, q) in dash_spans(points, dash, gap) {
        draw_line(p.x, p.y, q.x, q.y, thickness, color);
    }
}

fn brackets(corners: &[Vec2], thickness: f32, arm: f32, color: Color) {
    let n = corners.len();
    for i in 0..n {
        let at = corners[i];
        for other in [corners[(i + n - 1) % n], corners[(i + 1) % n]] {
            let span = other.distance(at);
            if span < 1e-3 {
                continue;
            }
            let end = at + (other - at) / span * arm.min(span * 0.4);
            draw_line(at.x, at.y, end.x, end.y, thickness, color);
        }
        draw_circle(at.x, at.y, thickness * 0.5, color);
    }
}

/// The edge of a panel over `rect`.
pub fn border(rect: Rect, shape: HudShape, corner: f32, kind: HudBorder, width: f32, color: Color) {
    let shape_outline = outline(rect, shape, corner);
    let smooth = shape == HudShape::Rounded && corner >= 0.5;
    match kind {
        HudBorder::None => {}
        HudBorder::Line => stroke(&shape_outline.points, width, color, !smooth),
        HudBorder::Thick => stroke(&shape_outline.points, width * 2.4, color, !smooth),
        HudBorder::Double => {
            stroke(&shape_outline.points, width, color, !smooth);
            let gap = width * 2.6 + 1.;
            let inner = Rect::new(
                rect.x + gap,
                rect.y + gap,
                (rect.w - gap * 2.).max(1.),
                (rect.h - gap * 2.).max(1.),
            );
            stroke(
                &outline(inner, shape, (corner - gap).max(0.)).points,
                width * 0.8,
                alpha(color, color.a * 0.8),
                !smooth,
            );
        }
        HudBorder::Dashed => dashed(
            &shape_outline.points,
            width,
            width * 4.5 + 2.,
            width * 3. + 2.,
            color,
        ),
        HudBorder::Corners => brackets(
            &shape_outline.corners,
            width * 1.8,
            (rect.h * 0.3).clamp(6., 18.),
            color,
        ),
    }
}

/// What a widget's words stand out with, once Auto is settled.
pub fn settle(lettering: HudLettering, plain: bool, glow: bool, contrast: bool) -> HudLettering {
    match lettering {
        HudLettering::Auto if plain && glow => HudLettering::Glow,
        HudLettering::Auto if plain && !contrast => HudLettering::Shadow,
        HudLettering::Auto => HudLettering::Clean,
        other => other,
    }
}

/// `value` at `at` with `lettering` behind it. `ink` darkens outlines and
/// shadows, and `accent` colors glows and painted blocks.
pub fn words(
    lettering: HudLettering,
    value: &str,
    at: Vec2,
    size: f32,
    color: Color,
    accent: Color,
    ink: Color,
) {
    let around = |reach: f32, tint: Color| {
        for k in 0..8 {
            let a = k as f32 * FRAC_PI_4;
            label(
                value,
                at.x + a.cos() * reach,
                at.y + a.sin() * reach,
                size,
                tint,
            );
        }
    };
    match lettering {
        HudLettering::Auto | HudLettering::Clean => {}
        HudLettering::Shadow => {
            let off = (size * 0.06).max(1.);
            label(
                value,
                at.x + off,
                at.y + off,
                size,
                alpha(ink, ink.a * 0.55),
            );
        }
        HudLettering::Block => {
            let off = (size * 0.09).max(1.5);
            let block = alpha(mix(accent, BLACK, 0.4), color.a);
            for step in [0.5, 1.] {
                label(value, at.x + off * step, at.y + off * step, size, block);
            }
        }
        HudLettering::Outline => around((size * 0.07).max(1.2), alpha(ink, ink.a * 0.9)),
        HudLettering::Glow => around((size * 0.08).max(1.5), alpha(accent, accent.a * 0.22)),
    }
    label(value, at.x, at.y, size, color);
}

/// A bar `progress` full in `rect`, split into `segments` when above one.
#[allow(clippy::too_many_arguments)]
pub fn bar(
    style: HudBar,
    rect: Rect,
    progress: f32,
    segments: u8,
    fill_color: Color,
    track: Color,
    scale: f32,
    drift: f32,
) {
    let progress = progress.clamp(0., 1.);
    if style == HudBar::Pips {
        let count = if segments > 1 {
            usize::from(segments)
        } else {
            10
        };
        let gap = (rect.h * 0.35).max(2. * scale);
        let size = rect
            .h
            .min((rect.w - gap * (count - 1) as f32) / count as f32)
            .max(2.);
        let spacing = if count > 1 {
            (rect.w - size) / (count - 1) as f32
        } else {
            0.
        };
        let filled = progress * count as f32;
        for i in 0..count {
            let at = vec2(
                rect.x + size * 0.5 + spacing * i as f32,
                rect.y + rect.h * 0.5,
            );
            draw_circle(at.x, at.y, size * 0.5, track);
            let part = (filled - i as f32).clamp(0., 1.);
            if part > 0. {
                draw_circle(
                    at.x,
                    at.y,
                    size * 0.5 * (0.45 + 0.55 * part),
                    alpha(fill_color, fill_color.a * (0.4 + 0.6 * part)),
                );
            }
        }
        return;
    }
    let round = matches!(style, HudBar::Capsule | HudBar::Striped | HudBar::Glow);
    let piece = |r: Rect, color: Color| {
        if round {
            crate::ui::capsule(r, color);
        } else {
            draw_rectangle(r.x, r.y, r.w, r.h, color);
        }
    };
    if segments > 1 {
        let n = usize::from(segments);
        let gap = (rect.h * 0.35).max(2. * scale);
        let width = ((rect.w - gap * (n - 1) as f32) / n as f32).max(1.);
        let filled = progress * n as f32;
        for i in 0..n {
            let r = Rect::new(rect.x + i as f32 * (width + gap), rect.y, width, rect.h);
            if round {
                crate::hud_widgets::rounded(r, rect.h * 0.3, track);
            } else {
                draw_rectangle(r.x, r.y, r.w, r.h, track);
            }
            let part = (filled - i as f32).clamp(0., 1.);
            if part > 0. {
                let full = Rect::new(r.x, r.y, r.w * part, r.h);
                if round {
                    crate::hud_widgets::rounded(full, rect.h * 0.3, fill_color);
                } else {
                    draw_rectangle(full.x, full.y, full.w, full.h, fill_color);
                }
            }
        }
        return;
    }
    piece(rect, track);
    let edge = if style == HudBar::Thin {
        0.
    } else {
        (rect.h * 0.18).clamp(1., 3.)
    };
    let inner = Rect::new(
        rect.x + edge,
        rect.y + edge,
        (rect.w - edge * 2.).max(0.),
        (rect.h - edge * 2.).max(0.),
    );
    let width = inner.w * progress;
    if width <= 0.5 {
        return;
    }
    let full = Rect::new(
        inner.x,
        inner.y,
        if round {
            width.max(inner.h).min(inner.w)
        } else {
            width
        },
        inner.h,
    );
    if style == HudBar::Glow {
        for (grow, share) in [(3., 0.12), (2., 0.18), (1., 0.26)] {
            let spread = grow * scale;
            crate::ui::capsule(
                Rect::new(
                    full.x - spread,
                    full.y - spread,
                    full.w + spread * 2.,
                    full.h + spread * 2.,
                ),
                alpha(fill_color, fill_color.a * share),
            );
        }
    }
    piece(full, fill_color);
    match style {
        HudBar::Striped => {
            let region = [
                vec2(full.x, full.y),
                vec2(full.right(), full.y),
                vec2(full.right(), full.bottom()),
                vec2(full.x, full.bottom()),
            ];
            pattern(
                HudFill::Stripes,
                &region,
                full,
                alpha(WHITE, fill_color.a * 0.22),
                (full.h / 9.).clamp(0.4, 2.),
                drift,
            );
        }
        HudBar::Glow => {
            let shine = Rect::new(full.x, full.y, full.w, (full.h * 0.4).max(1.));
            crate::ui::capsule(shine, alpha(WHITE, fill_color.a * 0.3));
        }
        _ => {}
    }
}

/// A choice button. `base` fills it, `accent` edges it and lights it when
/// `hovered`; `corner` rounds a box.
#[allow(clippy::too_many_arguments)]
pub fn choice(
    style: HudButtons,
    rect: Rect,
    words_text: &str,
    hovered: bool,
    base: Color,
    accent: Color,
    text_color: Color,
    corner: f32,
    scale: f32,
) {
    let line = (1.5 * scale).max(1.);
    let size = rect.h * 0.5;
    let mut ink = text_color;
    match style {
        HudButtons::Capsule | HudButtons::Box => {
            let r = if style == HudButtons::Capsule {
                rect.h * 0.5
            } else {
                corner.min(rect.h * 0.3)
            };
            let fill = if hovered {
                mix(base, accent, 0.45)
            } else {
                base
            };
            crate::hud_widgets::rounded(rect, r, fill);
            crate::hud_widgets::rounded_lines(
                rect,
                r,
                line,
                alpha(accent, accent.a * if hovered { 1. } else { 0.6 }),
            );
        }
        HudButtons::Outline => {
            let r = rect.h * 0.22;
            if hovered {
                crate::hud_widgets::rounded(rect, r, alpha(accent, accent.a * 0.35));
            }
            crate::hud_widgets::rounded_lines(rect, r, line * 1.3, accent);
        }
        HudButtons::Underline => {
            let thick = if hovered { line * 2.4 } else { line };
            let inset = rect.w * if hovered { 0.08 } else { 0.18 };
            draw_rectangle(
                rect.x + inset,
                rect.bottom() - thick - 2. * scale,
                rect.w - inset * 2.,
                thick,
                alpha(accent, accent.a * if hovered { 1. } else { 0.7 }),
            );
            if hovered {
                ink = mix(text_color, accent, 0.6);
            }
        }
    }
    crate::ui::center_label(
        words_text,
        rect.x + rect.w * 0.5,
        rect.y + rect.h * 0.5 + size * 0.33,
        size,
        ink,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(points: &[Vec2]) -> f32 {
        signed_area(points).abs()
    }

    #[test]
    fn every_shape_outlines_its_rect_clockwise_and_keeps_inside_it() {
        let rect = Rect::new(10., 20., 200., 60.);
        for shape in HudShape::ALL {
            let shaped = outline(rect, shape, 12.);
            assert!(signed_area(&shaped.points) > 0., "{shape:?} runs clockwise");
            assert!(area(&shaped.points) <= rect.w * rect.h + 1e-2);
            assert!(area(&shaped.points) > rect.w * rect.h * 0.6, "{shape:?}");
            for p in shaped.points.iter().chain(&shaped.core) {
                assert!(
                    rect.contains(*p)
                        || (p.x - rect.right()).abs() < 1e-3
                        || (p.y - rect.bottom()).abs() < 1e-3
                );
            }
        }
    }

    #[test]
    fn clipping_keeps_only_what_lies_inside() {
        let square = [vec2(0., 0.), vec2(10., 0.), vec2(10., 10.), vec2(0., 10.)];
        let half_out = [vec2(5., -5.), vec2(15., -5.), vec2(15., 5.), vec2(5., 5.)];
        assert!((area(&clip(&half_out, &square)) - 25.).abs() < 1e-3);
        let outside = [vec2(20., 20.), vec2(30., 20.), vec2(30., 30.)];
        assert!(clip(&outside, &square).is_empty());
        // Counter-clockwise regions cut the same way.
        let mut backwards = square;
        backwards.reverse();
        assert!((area(&clip(&half_out, &backwards)) - 25.).abs() < 1e-3);
    }

    #[test]
    fn dashes_end_on_long_outlines_and_keep_their_rhythm() {
        // A long outline whose running length rounds badly in f32 used to
        // be walked in tiny steps that could stop moving.
        let wide = outline(Rect::new(0., 0., 1919.37, 333.33), HudShape::Chamfer, 9.).points;
        let spans = dash_spans(&wide, 8.75, 6.5);
        let perimeter: f32 = (0..wide.len())
            .map(|i| wide[i].distance(wide[(i + 1) % wide.len()]))
            .sum();
        let drawn: f32 = spans.iter().map(|(p, q)| p.distance(*q)).sum();
        let share = drawn / perimeter;
        assert!((share - 8.75 / 15.25).abs() < 0.02, "{share}");
        assert!(spans.len() < (perimeter / 15.25) as usize + wide.len() + 2);
        // Nonsense sizes draw nothing rather than hang.
        assert!(dash_spans(&[vec2(0., 0.), vec2(f32::NAN, 1.), vec2(2., 2.)], 4., 3.).len() <= 2);
        assert!(dash_spans(&[], 4., 3.).is_empty());
    }

    #[test]
    fn ticks_stop_at_the_end_and_refuse_nonsense() {
        assert_eq!(ticks(0., 10., 3.).collect::<Vec<_>>(), [0., 3., 6., 9.]);
        assert_eq!(ticks(0., 1e9, 1.).count(), 512);
        assert_eq!(ticks(0., 10., 0.).count(), 0);
        assert_eq!(ticks(0., f32::NAN, 1.).count(), 0);
        assert_eq!(ticks(5., 1., 1.).count(), 0);
    }

    #[test]
    fn shapes_with_points_and_notches_ask_for_room() {
        assert_eq!(shape_room(HudShape::Rounded, 50.), (0., 0.));
        let (left, right) = shape_room(HudShape::Tag, 50.);
        assert!(left == 0. && right > 10.);
        let (left, right) = shape_room(HudShape::Ribbon, 50.);
        assert!(left > 0. && left == right);
    }

    #[test]
    fn auto_lettering_keeps_the_old_look() {
        use HudLettering::*;
        assert_eq!(settle(Auto, true, false, false), Shadow);
        assert_eq!(settle(Auto, true, true, false), Glow);
        assert_eq!(settle(Auto, true, false, true), Clean);
        assert_eq!(settle(Auto, false, true, false), Clean);
        assert_eq!(settle(Block, false, false, false), Block);
    }
}
