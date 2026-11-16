//! Widgets that scripts and map triggers put on players' screens: text,
//! bars and panels with titles, icons and choices to pick. The editor's
//! preview draws them with the same code, into a smaller frame.
use crate::ui::{self, alpha, fitted_label, label, label_width, mix};
// Page-space shapes, so widgets scroll with an editor page like its text.
use crate::ui::{
    ui_circle as draw_circle, ui_circle_lines as draw_circle_lines, ui_line as draw_line,
    ui_rectangle as draw_rectangle, ui_triangle as draw_triangle,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use tankyard_mods::{HudAlign, HudEnter, HudIcon, HudStyle, HudWidget};

/// Seconds an entrance takes.
const ENTER_SECONDS: f64 = 0.32;
/// Seconds a widget takes to fade once it is gone.
const EXIT_SECONDS: f64 = 0.22;

/// Where a widget's parts land.
#[derive(Clone, Debug)]
struct Layout {
    /// The panel, or the content for plain widgets.
    frame: Rect,
    size: f32,
    title: Option<(Vec2, f32)>,
    text: Option<Vec2>,
    /// The title, text and choices as they fit.
    words: [String; 2],
    choice_words: Vec<String>,
    icon: Option<(Vec2, f32)>,
    bar: Option<Rect>,
    choices: Vec<Rect>,
    corner: f32,
}

/// What the screen showed last frame, for entrances, exits and bars that
/// glide instead of jumping.
#[derive(Default)]
struct Memory {
    seen: HashMap<u64, Seen>,
    frame: u64,
}
struct Seen {
    since: f64,
    frame: u64,
    progress: f32,
    /// The widget as it last showed, for its exit.
    widget: HudWidget,
    gone: Option<f64>,
}
thread_local! {
    static MEMORY: RefCell<Memory> = RefCell::new(Memory::default());
}

/// A widget's identity on screen: where it sits and how. A widget that
/// keeps its place while its text changes is the same widget.
fn identity(widget: &HudWidget) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for word in [
        widget.x.to_bits(),
        widget.y.to_bits(),
        widget.align as u32,
        widget.style as u32,
        widget.menu,
    ] {
        hash = (hash ^ u64::from(word)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn rgb(color: Option<[u8; 3]>, fallback: Color) -> Color {
    color.map_or(fallback, |[r, g, b]| Color::from_rgba(r, g, b, 255))
}
fn accent_of(widget: &HudWidget) -> Color {
    rgb(widget.accent.or(widget.bar_color), ui::accent())
}

/// Lay `widget` out in `screen` at `scale`, `grow` times its size.
fn layout(widget: &HudWidget, screen: Rect, scale: f32, grow: f32) -> Layout {
    let size = widget.size * scale * grow;
    let margin = 10. * scale;
    let room = (screen.w - margin * 2.).max(40. * scale);
    let boxed = widget.style != HudStyle::Plain;
    let pad = if boxed { size * 0.65 } else { 0. };
    let title_size = (size * 0.62).max(9. * scale);
    let icon_size = if widget.icon == HudIcon::None {
        0.
    } else {
        size * 1.1
    };
    let gap = size * 0.4;
    let inner_room = room - pad * 2.;
    let line_text = fitted_label(&widget.text, size, inner_room - icon_size - gap).into_owned();
    let text_width = if line_text.is_empty() {
        0.
    } else {
        label_width(&line_text, size)
    };
    let line_width = text_width
        + if icon_size > 0. {
            icon_size + if text_width > 0. { gap } else { 0. }
        } else {
            0.
        };
    let title_text = fitted_label(&widget.title, title_size, inner_room).into_owned();
    let title_width = if title_text.is_empty() {
        0.
    } else {
        label_width(&title_text, title_size)
    };
    let bar_width = widget
        .progress
        .map_or(0., |_| (widget.width * screen.w).min(inner_room));
    let choice_size = (size * 0.8).max(10. * scale);
    let choice_pad = choice_size * 0.9;
    let choice_words: Vec<String> = widget
        .choices
        .iter()
        .map(|choice| fitted_label(choice, choice_size, inner_room - choice_pad * 2.).into_owned())
        .collect();
    let choice_widths: Vec<f32> = choice_words
        .iter()
        .map(|choice| label_width(choice, choice_size) + choice_pad * 2.)
        .collect();
    let choice_gap = size * 0.4;
    let row = choice_widths.iter().sum::<f32>()
        + choice_gap * choice_widths.len().saturating_sub(1) as f32;
    let stacked = row > inner_room;
    let choices_width = if stacked {
        choice_widths
            .iter()
            .copied()
            .fold(0., f32::max)
            .min(inner_room)
    } else {
        row
    };
    let choice_height = choice_size * 2.;
    let content_width = line_width
        .max(title_width)
        .max(bar_width)
        .max(choices_width)
        .min(inner_room);
    // Heights from the top of the content.
    let mut height = 0.;
    let title_at = (!title_text.is_empty()).then(|| {
        let at = height + title_size * 0.8;
        height += title_size * 1.25;
        at
    });
    let line_at = (line_width > 0.).then(|| {
        let at = height + size * 0.72;
        height += size * 1.05;
        at
    });
    let bar_height = (size * 0.45).max(7. * scale);
    let bar_at = widget.progress.map(|_| {
        let at = height + if height > 0. { size * 0.2 } else { 0. };
        height = at + bar_height;
        at
    });
    let choices_at = (!widget.choices.is_empty()).then(|| {
        let at = height + if height > 0. { size * 0.5 } else { 0. };
        let rows = if stacked { widget.choices.len() } else { 1 };
        height = at + rows as f32 * choice_height + (rows - 1) as f32 * choice_gap;
        at
    });
    let banner = widget.style == HudStyle::Banner;
    let pill = widget.style == HudStyle::Pill;
    let side = if pill { pad + height * 0.25 } else { pad };
    let stripe = if widget.style == HudStyle::Card {
        5. * scale * grow
    } else {
        0.
    };
    let width = if banner {
        screen.w
    } else {
        content_width + side * 2. + stripe
    };
    let full_height = height + pad * 2.;
    let anchor = screen.x + widget.x * screen.w;
    let left = if banner {
        screen.x
    } else {
        let left = match widget.align {
            HudAlign::Left => anchor,
            HudAlign::Center => anchor - width * 0.5,
            HudAlign::Right => anchor - width,
        };
        left.clamp(
            screen.x + margin,
            (screen.x + screen.w - margin - width).max(screen.x + margin),
        )
    };
    // `y` is the middle of a plain widget's first line, and of a panel.
    let middle = screen.y + widget.y * screen.h;
    let first = line_at
        .map(|at| (at, size))
        .or(title_at.map(|at| (at, title_size)));
    let top = match first {
        Some((at, line)) if !boxed => middle + line * 0.35 - at,
        _ => middle - full_height * 0.5,
    }
    .clamp(
        screen.y + margin,
        (screen.y + screen.h - margin - full_height).max(screen.y),
    );
    let frame = Rect::new(left, top, width, full_height);
    // Content runs from `x0` for `content_width`, placed by the alignment
    // inside a banner and centered elsewhere.
    let x0 = if banner {
        match widget.align {
            HudAlign::Left => (anchor).max(screen.x + pad),
            HudAlign::Center => anchor - content_width * 0.5,
            HudAlign::Right => anchor - content_width,
        }
        .clamp(
            screen.x + pad,
            (screen.x + screen.w - pad - content_width).max(screen.x),
        )
    } else {
        left + stripe + side
    };
    let y0 = top + pad;
    let line_left = |line: f32| match (boxed && !banner, widget.align) {
        (true, _) | (false, HudAlign::Center) => x0 + (content_width - line) * 0.5,
        (false, HudAlign::Left) => x0,
        (false, HudAlign::Right) => x0 + content_width - line,
    };
    let title = title_at.map(|at| {
        let width = title_width.min(content_width);
        (vec2(line_left(width), y0 + at), title_size)
    });
    let (text, icon) = match line_at {
        Some(at) => {
            let start = line_left(line_width.min(content_width));
            let icon = (icon_size > 0.).then(|| {
                (
                    vec2(start + icon_size * 0.5, y0 + at - size * 0.3),
                    icon_size,
                )
            });
            let text_x = start + if icon_size > 0. { icon_size + gap } else { 0. };
            ((text_width > 0.).then(|| vec2(text_x, y0 + at)), icon)
        }
        None => (None, None),
    };
    let bar = bar_at.map(|at| {
        let width = bar_width.min(content_width);
        Rect::new(line_left(width), y0 + at, width, bar_height)
    });
    let mut choices = Vec::with_capacity(widget.choices.len());
    if let Some(at) = choices_at {
        if stacked {
            for index in 0..widget.choices.len() {
                choices.push(Rect::new(
                    line_left(choices_width),
                    y0 + at + index as f32 * (choice_height + choice_gap),
                    choices_width,
                    choice_height,
                ));
            }
        } else {
            let mut x = line_left(row);
            for width in &choice_widths {
                choices.push(Rect::new(x, y0 + at, *width, choice_height));
                x += width + choice_gap;
            }
        }
    }
    Layout {
        frame,
        size,
        title,
        text,
        words: [title_text, line_text],
        choice_words,
        icon,
        bar,
        choices,
        corner: if pill {
            full_height * 0.5
        } else if banner {
            0.
        } else {
            (size * 0.45).min(full_height * 0.5)
        },
    }
}

/// A rounded rectangle, with no overlap where a translucent fill would
/// darken.
pub fn rounded(rect: Rect, radius: f32, color: Color) {
    let r = radius.min(rect.w * 0.5).min(rect.h * 0.5).max(0.);
    if r < 0.5 {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);
        return;
    }
    draw_rectangle(rect.x + r, rect.y, rect.w - r * 2., rect.h, color);
    draw_rectangle(rect.x, rect.y + r, r, rect.h - r * 2., color);
    draw_rectangle(rect.x + rect.w - r, rect.y + r, r, rect.h - r * 2., color);
    let steps = ((r * 0.5) as usize).clamp(3, 10);
    for (cx, cy, from) in [
        (rect.x + r, rect.y + r, std::f32::consts::PI),
        (
            rect.x + rect.w - r,
            rect.y + r,
            -std::f32::consts::FRAC_PI_2,
        ),
        (rect.x + rect.w - r, rect.y + rect.h - r, 0.),
        (rect.x + r, rect.y + rect.h - r, std::f32::consts::FRAC_PI_2),
    ] {
        let point = |i: usize| {
            let a = from + std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
            vec2(cx + a.cos() * r, cy + a.sin() * r)
        };
        for i in 0..steps {
            draw_triangle(vec2(cx, cy), point(i), point(i + 1), color);
        }
    }
}

/// A rounded rectangle's outline.
pub fn rounded_lines(rect: Rect, radius: f32, thickness: f32, color: Color) {
    let r = radius.min(rect.w * 0.5).min(rect.h * 0.5).max(0.);
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    draw_line(x + r, y, x + w - r, y, thickness, color);
    draw_line(x + r, y + h, x + w - r, y + h, thickness, color);
    draw_line(x, y + r, x, y + h - r, thickness, color);
    draw_line(x + w, y + r, x + w, y + h - r, thickness, color);
    if r < 0.5 {
        return;
    }
    let steps = ((r * 0.5) as usize).clamp(3, 10);
    for (cx, cy, from) in [
        (x + r, y + r, std::f32::consts::PI),
        (x + w - r, y + r, -std::f32::consts::FRAC_PI_2),
        (x + w - r, y + h - r, 0.),
        (x + r, y + h - r, std::f32::consts::FRAC_PI_2),
    ] {
        let point = |i: usize| {
            let a = from + std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
            vec2(cx + a.cos() * r, cy + a.sin() * r)
        };
        for i in 0..steps {
            let (a, b) = (point(i), point(i + 1));
            draw_line(a.x, a.y, b.x, b.y, thickness, color);
        }
    }
}

/// `icon` centered on `at`, `size` across.
pub fn icon(kind: HudIcon, at: Vec2, size: f32, color: Color) {
    let r = size * 0.5;
    let line = (size * 0.11).max(1.2);
    let ink = alpha(BLACK, color.a * 0.55);
    let polygon = |points: &[Vec2], fill: Color| {
        for i in 1..points.len().saturating_sub(1) {
            draw_triangle(points[0], points[i], points[i + 1], fill);
        }
    };
    match kind {
        HudIcon::None => {}
        HudIcon::Star => {
            let points: Vec<Vec2> = (0..10)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.;
                    let reach = if i % 2 == 0 { r } else { r * 0.45 };
                    at + vec2(a.cos(), a.sin()) * reach
                })
                .collect();
            for i in 0..10 {
                draw_triangle(at, points[i], points[(i + 1) % 10], color);
            }
        }
        HudIcon::Heart => {
            let lobe = r * 0.52;
            draw_circle(at.x - lobe * 0.95, at.y - r * 0.2, lobe, color);
            draw_circle(at.x + lobe * 0.95, at.y - r * 0.2, lobe, color);
            draw_triangle(
                vec2(at.x - r * 0.98, at.y - r * 0.02),
                vec2(at.x + r * 0.98, at.y - r * 0.02),
                vec2(at.x, at.y + r * 0.95),
                color,
            );
        }
        HudIcon::Coin => {
            draw_circle(at.x, at.y, r, color);
            draw_circle_lines(at.x, at.y, r * 0.68, line, ink);
            draw_rectangle(at.x - line * 0.5, at.y - r * 0.4, line, r * 0.8, ink);
        }
        HudIcon::Clock => {
            draw_circle_lines(at.x, at.y, r * 0.88, line * 1.2, color);
            draw_line(at.x, at.y, at.x, at.y - r * 0.6, line, color);
            draw_line(at.x, at.y, at.x + r * 0.45, at.y + r * 0.1, line, color);
        }
        HudIcon::Flag => {
            draw_line(
                at.x - r * 0.7,
                at.y - r,
                at.x - r * 0.7,
                at.y + r,
                line * 1.2,
                color,
            );
            polygon(
                &[
                    vec2(at.x - r * 0.6, at.y - r),
                    vec2(at.x + r * 0.9, at.y - r * 0.55),
                    vec2(at.x - r * 0.6, at.y - r * 0.05),
                ],
                color,
            );
        }
        HudIcon::Bolt => {
            // Two wedges, since the bolt's outline is not convex.
            draw_triangle(
                vec2(at.x + r * 0.35, at.y - r),
                vec2(at.x - r * 0.6, at.y + r * 0.14),
                vec2(at.x + r * 0.18, at.y + r * 0.14),
                color,
            );
            draw_triangle(
                vec2(at.x - r * 0.18, at.y - r * 0.14),
                vec2(at.x + r * 0.6, at.y - r * 0.14),
                vec2(at.x - r * 0.35, at.y + r),
                color,
            );
        }
        HudIcon::Shield => polygon(
            &[
                vec2(at.x, at.y + r),
                vec2(at.x - r * 0.85, at.y + r * 0.1),
                vec2(at.x - r * 0.85, at.y - r * 0.75),
                vec2(at.x, at.y - r),
                vec2(at.x + r * 0.85, at.y - r * 0.75),
                vec2(at.x + r * 0.85, at.y + r * 0.1),
            ],
            color,
        ),
        HudIcon::Key => {
            draw_circle_lines(at.x - r * 0.45, at.y, r * 0.4, line * 1.2, color);
            draw_line(at.x - r * 0.05, at.y, at.x + r, at.y, line * 1.2, color);
            draw_line(
                at.x + r * 0.7,
                at.y,
                at.x + r * 0.7,
                at.y + r * 0.4,
                line,
                color,
            );
            draw_line(
                at.x + r * 0.4,
                at.y,
                at.x + r * 0.4,
                at.y + r * 0.3,
                line,
                color,
            );
        }
        HudIcon::Gem => {
            polygon(
                &[
                    vec2(at.x, at.y + r),
                    vec2(at.x - r, at.y - r * 0.2),
                    vec2(at.x - r * 0.5, at.y - r * 0.8),
                    vec2(at.x + r * 0.5, at.y - r * 0.8),
                    vec2(at.x + r, at.y - r * 0.2),
                ],
                color,
            );
            draw_line(
                at.x - r,
                at.y - r * 0.2,
                at.x + r,
                at.y - r * 0.2,
                line * 0.7,
                ink,
            );
        }
        HudIcon::Trophy => {
            polygon(
                &[
                    vec2(at.x - r * 0.65, at.y - r * 0.85),
                    vec2(at.x + r * 0.65, at.y - r * 0.85),
                    vec2(at.x + r * 0.4, at.y + r * 0.05),
                    vec2(at.x - r * 0.4, at.y + r * 0.05),
                ],
                color,
            );
            draw_circle_lines(at.x - r * 0.7, at.y - r * 0.45, r * 0.25, line, color);
            draw_circle_lines(at.x + r * 0.7, at.y - r * 0.45, r * 0.25, line, color);
            draw_rectangle(at.x - line * 0.6, at.y, line * 1.2, r * 0.55, color);
            draw_rectangle(at.x - r * 0.45, at.y + r * 0.55, r * 0.9, r * 0.3, color);
        }
        HudIcon::Target => {
            draw_circle_lines(at.x, at.y, r * 0.85, line, color);
            draw_circle_lines(at.x, at.y, r * 0.45, line, color);
            draw_circle(at.x, at.y, r * 0.15, color);
        }
        HudIcon::Info | HudIcon::Alert => {
            if kind == HudIcon::Info {
                draw_circle(at.x, at.y, r, color);
            } else {
                draw_triangle(
                    vec2(at.x, at.y - r),
                    vec2(at.x - r * 1.05, at.y + r * 0.85),
                    vec2(at.x + r * 1.05, at.y + r * 0.85),
                    color,
                );
            }
            let (top, bottom) = if kind == HudIcon::Info {
                (at.y - r * 0.05, at.y + r * 0.55)
            } else {
                (at.y - r * 0.4, at.y + r * 0.3)
            };
            let dot = if kind == HudIcon::Info {
                at.y - r * 0.45
            } else {
                at.y + r * 0.58
            };
            draw_line(at.x, top, at.x, bottom, line * 1.3, ink);
            draw_circle(at.x, dot, line * 0.8, ink);
        }
    }
}

/// How a widget enters: opacity, size and nudge, `t` from 0 to 1.
fn entrance(enter: HudEnter, t: f32, x: f32, scale: f32) -> (f32, f32, Vec2) {
    let eased = 1. - (1. - t).powi(3);
    match enter {
        HudEnter::None => (1., 1., Vec2::ZERO),
        HudEnter::Fade => (eased, 1., Vec2::ZERO),
        HudEnter::Pop => {
            let c = 1.70158;
            let back = 1. + (c + 1.) * (t - 1.).powi(3) + c * (t - 1.).powi(2);
            (eased, 0.6 + 0.4 * back, Vec2::ZERO)
        }
        HudEnter::Slide => {
            let side = if x < 0.5 { -1. } else { 1. };
            (eased, 1., vec2(side * (1. - eased) * 80. * scale, 0.))
        }
        HudEnter::Drop => (eased, 1., vec2(0., -(1. - eased) * 50. * scale)),
    }
}

/// Draw one widget; returns the choice under a fresh click, if any.
#[allow(clippy::too_many_arguments)]
fn paint(
    widget: &HudWidget,
    screen: Rect,
    scale: f32,
    opacity: f32,
    grow: f32,
    nudge: Vec2,
    progress: f32,
    time: f64,
    pointer: Option<Vec2>,
) -> Option<u8> {
    let mut l = layout(widget, screen, scale, grow);
    if grow != 1. {
        // Grow about the resting widget's center.
        let rest = layout(widget, screen, scale, 1.);
        let shift = rest.frame.center() - l.frame.center();
        offset(&mut l, shift);
    }
    offset(&mut l, nudge);
    let pulse = if widget.pulse && !ui::reduced_motion() {
        0.5 + 0.5 * (time * 4.).sin() as f32
    } else {
        0.
    };
    let accent = accent_of(widget);
    let text_color = rgb(widget.color, ui::text_color());
    let fade = |color: Color, amount: f32| alpha(color, color.a * amount * opacity);
    let panel = rgb(widget.panel, ui::surface_color());
    let frame = l.frame;
    match widget.style {
        HudStyle::Plain => {}
        HudStyle::Banner => {
            draw_rectangle(
                frame.x,
                frame.y,
                frame.w,
                frame.h,
                fade(panel, widget.opacity),
            );
            let edge = (2. * scale).max(1.);
            for y in [frame.y, frame.y + frame.h - edge] {
                draw_rectangle(frame.x, y, frame.w, edge, fade(accent, 0.75 + pulse * 0.25));
            }
        }
        _ => {
            let shadow = Rect::new(frame.x, frame.y + 3. * scale, frame.w, frame.h);
            rounded(
                shadow,
                l.corner,
                fade(BLACK, 0.22 * widget.opacity.max(0.3)),
            );
            rounded(frame, l.corner, fade(panel, widget.opacity));
            if widget.style == HudStyle::Card {
                let stripe = Rect::new(frame.x, frame.y, 5. * scale * grow, frame.h);
                rounded(stripe, l.corner.min(stripe.w * 0.5), fade(accent, 1.));
            }
            rounded_lines(
                frame,
                l.corner,
                (1.5 * scale).max(1.) * (1. + pulse * 0.6),
                fade(accent, 0.45 + pulse * 0.45),
            );
        }
    }
    let shadowed = |value: &str, at: Vec2, size: f32, color: Color| {
        if widget.style == HudStyle::Plain && !crate::accessibility::contrast() {
            let off = (size * 0.06).max(1.);
            label(value, at.x + off, at.y + off, size, fade(BLACK, 0.55));
        }
        label(value, at.x, at.y, size, fade(color, 1.));
    };
    if let Some((at, size)) = l.title {
        shadowed(&l.words[0], at, size, mix(accent, text_color, 0.35));
    }
    if let Some((at, size)) = l.icon {
        icon(widget.icon, at, size, fade(accent, 1.));
    }
    if let Some(at) = l.text {
        shadowed(&l.words[1], at, l.size, text_color);
    }
    if let Some(bar) = l.bar {
        let fill = rgb(widget.bar_color, accent);
        if widget.segments > 1 {
            let n = usize::from(widget.segments);
            let gap = (bar.h * 0.35).max(2. * scale);
            let piece = (bar.w - gap * (n - 1) as f32) / n as f32;
            let filled = progress * n as f32;
            for i in 0..n {
                let r = Rect::new(
                    bar.x + i as f32 * (piece + gap),
                    bar.y,
                    piece.max(1.),
                    bar.h,
                );
                rounded(r, bar.h * 0.3, fade(Color::new(0.2, 0.22, 0.25, 1.), 0.85));
                let part = (filled - i as f32).clamp(0., 1.);
                if part > 0. {
                    rounded(
                        Rect::new(r.x, r.y, r.w * part, r.h),
                        bar.h * 0.3,
                        fade(fill, 1.),
                    );
                }
            }
        } else {
            ui::capsule(bar, fade(Color::new(0.2, 0.22, 0.25, 1.), 0.9));
            let edge = (bar.h * 0.18).clamp(1., 3.);
            let inner = Rect::new(
                bar.x + edge,
                bar.y + edge,
                bar.w - edge * 2.,
                bar.h - edge * 2.,
            );
            let width = inner.w * progress.clamp(0., 1.);
            if width > 0.5 {
                ui::capsule(
                    Rect::new(inner.x, inner.y, width.max(inner.h).min(inner.w), inner.h),
                    fade(fill, 1.),
                );
            }
        }
    }
    let mut picked = None;
    for (index, (rect, choice)) in l.choices.iter().zip(&l.choice_words).enumerate() {
        let hovered = pointer.is_some_and(|p| rect.contains(p));
        let base = if widget.style == HudStyle::Plain {
            mix(ui::surface_color(), panel, 0.3)
        } else {
            mix(panel, WHITE, if ui::is_dark() { 0.08 } else { 0.4 })
        };
        let fill = if hovered {
            mix(base, accent, 0.45)
        } else {
            base
        };
        let r = rect.h * 0.5;
        rounded(*rect, r, fade(fill, 0.95));
        rounded_lines(
            *rect,
            r,
            (1.5 * scale).max(1.),
            fade(accent, if hovered { 1. } else { 0.6 }),
        );
        let size = rect.h * 0.5;
        ui::center_label(
            choice,
            rect.x + rect.w * 0.5,
            rect.y + rect.h * 0.5 + size * 0.33,
            size,
            fade(text_color, 1.),
        );
        if hovered && is_mouse_button_pressed(MouseButton::Left) {
            picked = Some(index as u8);
        }
    }
    picked
}

fn offset(l: &mut Layout, by: Vec2) {
    if by == Vec2::ZERO {
        return;
    }
    l.frame = l.frame.offset(by);
    if let Some(at) = &mut l.text {
        *at += by;
    }
    if let Some((at, _)) = &mut l.title {
        *at += by;
    }
    if let Some((at, _)) = &mut l.icon {
        *at += by;
    }
    if let Some(bar) = &mut l.bar {
        *bar = bar.offset(by);
    }
    for rect in &mut l.choices {
        *rect = rect.offset(by);
    }
}

/// Draw the widgets this player sees, with entrances and exits. Returns a
/// pick: the menu and the choice.
pub fn draw(widgets: &[HudWidget], s: f32) -> Option<(u32, u8)> {
    let screen = Rect::new(0., 0., screen_width(), screen_height());
    let now = get_time();
    let calm = ui::reduced_motion();
    let pointer =
        (crate::ui_layout::pointer_visible() && ui::interactive()).then(crate::ui_layout::pointer);
    let mut picked = None;
    MEMORY.with(|memory| {
        let mut memory = memory.borrow_mut();
        memory.frame += 1;
        let frame = memory.frame;
        let dt = get_frame_time().min(0.1);
        for widget in widgets {
            let key = identity(widget);
            let target = widget.progress.unwrap_or(0.);
            let seen = memory.seen.entry(key).or_insert_with(|| Seen {
                since: now,
                frame,
                progress: target,
                widget: widget.clone(),
                gone: None,
            });
            if seen.gone.is_some() {
                // Back before its exit ended: show it again at once.
                seen.gone = None;
                seen.since = now - ENTER_SECONDS;
            }
            seen.frame = frame;
            if calm {
                seen.progress = target;
            } else {
                seen.progress += (target - seen.progress) * (1. - (-dt * 12.).exp());
            }
            if seen.widget != *widget {
                seen.widget = widget.clone();
            }
            let t = if calm {
                1.
            } else {
                ((now - seen.since) / ENTER_SECONDS).clamp(0., 1.) as f32
            };
            let (opacity, grow, nudge) = entrance(widget.enter, t, widget.x, s);
            let pick = paint(
                widget,
                screen,
                s,
                opacity,
                grow,
                nudge,
                seen.progress,
                now,
                if t >= 1. { pointer } else { None },
            );
            if let Some(option) = pick
                && widget.menu != 0
            {
                picked = Some((widget.menu, option));
            }
        }
        // Widgets that went away fade out where they were.
        memory.seen.retain(|_, seen| {
            if seen.frame == frame {
                return true;
            }
            if calm {
                return false;
            }
            let gone = *seen.gone.get_or_insert(now);
            let t = ((now - gone) / EXIT_SECONDS) as f32;
            if t >= 1. {
                return false;
            }
            let ghost = HudWidget {
                choices: Vec::new(),
                ..seen.widget.clone()
            };
            if ghost.valid() {
                paint(
                    &ghost,
                    screen,
                    s,
                    1. - t,
                    1.,
                    Vec2::ZERO,
                    seen.progress,
                    now,
                    None,
                );
            }
            true
        });
    });
    picked
}

/// Whether the pointer is over a widget a player can pick from, so a click
/// there picks instead of firing.
pub fn captures_pointer(widgets: &[HudWidget], s: f32) -> bool {
    if !crate::ui_layout::pointer_visible() {
        return false;
    }
    let pointer = crate::ui_layout::pointer();
    let screen = Rect::new(0., 0., screen_width(), screen_height());
    widgets
        .iter()
        .filter(|widget| !widget.choices.is_empty())
        .any(|widget| layout(widget, screen, s, 1.).frame.contains(pointer))
}

/// The choice of `widget` at `point` when drawn into `frame` at `scale`.
pub fn choice_at(widget: &HudWidget, frame: Rect, scale: f32, point: Vec2) -> Option<u8> {
    layout(widget, frame, scale, 1.)
        .choices
        .iter()
        .position(|rect| rect.contains(point))
        .map(|index| index as u8)
}

/// Whether `point` is on `widget` when drawn into `frame` at `scale`.
pub fn covers(widget: &HudWidget, frame: Rect, scale: f32, point: Vec2) -> bool {
    layout(widget, frame, scale, 1.).frame.contains(point)
}

/// Draw `widget` into `frame`, a small picture of the screen, `t` seconds
/// after it appeared, its choices lit under `pointer`; for the editor.
pub fn preview(
    widget: &HudWidget,
    frame: Rect,
    scale: f32,
    t: f32,
    progress: f32,
    pointer: Option<Vec2>,
) {
    let enter = (t as f64 / ENTER_SECONDS).clamp(0., 1.) as f32;
    let (opacity, grow, nudge) = if ui::reduced_motion() {
        (1., 1., Vec2::ZERO)
    } else {
        entrance(widget.enter, enter, widget.x, scale)
    };
    paint(
        widget,
        frame,
        scale,
        opacity,
        grow,
        nudge,
        widget.progress.unwrap_or(progress),
        get_time(),
        pointer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_keep_their_identity_while_their_text_changes() {
        let a = HudWidget {
            text: "3 left".into(),
            ..HudWidget::default()
        };
        let b = HudWidget {
            text: "2 left".into(),
            ..a.clone()
        };
        assert_eq!(identity(&a), identity(&b));
        let moved = HudWidget {
            y: 0.6,
            ..a.clone()
        };
        assert_ne!(identity(&a), identity(&moved));
    }

    #[test]
    fn entrances_settle_where_the_widget_rests() {
        for enter in HudEnter::ALL {
            let (opacity, grow, nudge) = entrance(enter, 1., 0.2, 1.);
            assert!((opacity - 1.).abs() < 1e-4, "{enter:?}");
            assert!((grow - 1.).abs() < 1e-4, "{enter:?}");
            assert!(nudge.length() < 1e-3, "{enter:?}");
        }
        let (opacity, _, nudge) = entrance(HudEnter::Slide, 0., 0.2, 1.);
        assert!(
            opacity < 0.01 && nudge.x < 0.,
            "slides in from the nearer side"
        );
    }
}
