//! Widgets that scripts and map triggers put on players' screens: text,
//! bars and panels with titles, icons and choices to pick, on the screen
//! or floating over a spot in the arena or a tank. The editor's preview
//! draws them with the same code, into a smaller frame.
use crate::ui::{self, alpha, fitted_label, label_width, mix};
// Page-space shapes, so widgets scroll with an editor page like its text.
use crate::hud_skin;
use crate::ui::{
    ui_circle as draw_circle, ui_circle_lines as draw_circle_lines, ui_line as draw_line,
    ui_rectangle as draw_rectangle, ui_triangle as draw_triangle,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use tankyard_mods::{
    HudAlign, HudBar, HudBorder, HudEnter, HudFill, HudIcon, HudLayout, HudPin, HudShape, HudStyle,
    HudWidget,
};

/// Seconds an entrance takes.
const ENTER_SECONDS: f64 = 0.32;
/// Seconds a widget takes to fade once it is gone.
const EXIT_SECONDS: f64 = 0.22;

/// Where a pinned widget's pin is on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pinned {
    /// The point it floats over.
    pub at: Vec2,
    /// How far above `at` it floats, such as a tank's radius on screen;
    /// None centers it on `at`.
    pub above: Option<f32>,
}

/// Finds where pinned widgets go: what a pin is over on screen, or None
/// while there is nothing there to show the widget over.
pub type Place<'a> = dyn Fn(HudPin) -> Option<Pinned> + 'a;

/// Where `widget` goes: `Some(None)` on the screen, `Some(Some(_))` over
/// what it is pinned to, or None while that is gone.
fn anchor(widget: &HudWidget, place: &Place) -> Option<Option<Pinned>> {
    if widget.pinned() {
        place(widget.pin).map(Some)
    } else {
        Some(None)
    }
}

/// How choices are arranged once laid out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arrange {
    Row,
    Column,
    Grid,
}

/// Where a widget's parts land.
#[derive(Clone, Debug)]
struct Layout {
    scale: f32,
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
    /// A board's cells, and how many pixels a grid unit takes.
    cells: Vec<Rect>,
    board_scale: f32,
    corner: f32,
    /// Where a pointer leaves the frame and which way it points, while a
    /// widget that points the way has its pin out of view.
    toward: Option<(Vec2, Vec2)>,
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
    /// One for each [`Layer`].
    static MEMORY: RefCell<[Memory; 2]> = RefCell::new(Default::default());
}

/// Which widgets a call draws: those pinned over the arena, which sit under
/// the HUD like the arena itself, or those placed on the screen, over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Arena,
    Screen,
}

/// A new match must not inherit another match's widget animations.
pub fn clear() {
    MEMORY.with(|memory| *memory.borrow_mut() = Default::default());
}

/// A widget's identity on screen: where it sits and how. A widget that
/// keeps its place while its text changes is the same widget.
fn identity(widget: &HudWidget) -> u64 {
    // Assigned by the server for both menus and labels. Position and text
    // can change without restarting the entrance or leaving fading copies.
    if widget.menu != 0 {
        return u64::from(widget.menu) | (1 << 63);
    }
    let (pin, group, a, b) = match widget.pin {
        HudPin::Screen => (0, 0, 0, 0),
        HudPin::World([x, y]) => (1, 0, x.to_bits(), y.to_bits()),
        HudPin::Tank(id) => (2, 0, id as u32, (id >> 32) as u32),
        HudPin::Group { id, at: [x, y] } => (3, u32::from(id), x.to_bits(), y.to_bits()),
    };
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for word in [
        widget.x.to_bits(),
        widget.y.to_bits(),
        widget.align as u32,
        widget.style as u32,
        widget.menu,
        pin,
        group,
        a,
        b,
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

/// Reserve the normal HUD's side rails and top/bottom controls. On small
/// screens the inset stays proportional instead of inverting the rectangle.
fn marker_bounds(screen: Rect, scale: f32) -> Rect {
    let side = (300. * scale).min(screen.w * 0.32);
    let top = (160. * scale).min(screen.h * 0.25);
    let bottom = (140. * scale).min(screen.h * 0.25);
    Rect::new(
        screen.x + side,
        screen.y + top,
        screen.w - side * 2.,
        screen.h - top - bottom,
    )
}

/// Lay `widget` out in `screen` at `scale`, `grow` times its size, over
/// `pinned` when it floats over something.
fn layout(
    widget: &HudWidget,
    screen: Rect,
    scale: f32,
    grow: f32,
    pinned: Option<Pinned>,
) -> Layout {
    let mut fit = scale;
    let mut width_limit = screen.w;
    let inset = (pinned.is_some() && widget.point).then(|| marker_bounds(screen, scale));
    let mut laid = layout_at(widget, screen, fit, grow, pinned, width_limit, inset);
    // A long column must remain usable at large UI scales and in previews.
    // Keep the authored layout whenever it fits; reduce only oversized widgets.
    for _ in 0..6 {
        let margin = 20. * fit;
        let height = inset.map_or((screen.h - margin).max(1.), |rect| rect.h);
        let width = inset.map_or((screen.w - margin).max(1.), |rect| rect.w);
        let vertical = height / laid.frame.h.max(1.);
        let horizontal = width / laid.frame.w.max(1.);
        if vertical >= 0.999 && horizontal >= 0.999 {
            break;
        }
        fit *= vertical.clamp(0.01, 1.);
        width_limit *= horizontal.clamp(0.01, 1.);
        laid = layout_at(widget, screen, fit, grow, pinned, width_limit, inset);
    }
    laid
}

fn layout_at(
    widget: &HudWidget,
    screen: Rect,
    scale: f32,
    grow: f32,
    pinned: Option<Pinned>,
    width_limit: f32,
    inset: Option<Rect>,
) -> Layout {
    let size = widget.size * scale * grow;
    let margin = 10. * scale;
    let room = (screen.w - margin * 2.).min(width_limit).max(1.);
    let boxed = widget.style != HudStyle::Plain;
    let pad = if boxed { size * 0.65 } else { 0. };
    let title_size = (size * 0.62).max(9. * scale);
    let icon_size = if widget.icon == HudIcon::None {
        0.
    } else {
        size * 1.1
    };
    let gap = size * 0.4;
    let inner_room = (room - pad * 2.).max(1.);
    let caps = |words: &str| {
        if widget.caps {
            words.to_uppercase()
        } else {
            words.to_owned()
        }
    };
    let (shown_text, shown_title) = (caps(&widget.text), caps(&widget.title));
    let shown_choices: Vec<String> = widget.choices.iter().map(|choice| caps(choice)).collect();
    let line_text = fitted_label(&shown_text, size, inner_room - icon_size - gap).into_owned();
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
    let title_text = fitted_label(&shown_title, title_size, inner_room).into_owned();
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
    let fit = |room: f32| -> Vec<String> {
        shown_choices
            .iter()
            .map(|choice| fitted_label(choice, choice_size, room - choice_pad * 2.).into_owned())
            .collect()
    };
    let mut choice_words = fit(inner_room);
    let choice_widths: Vec<f32> = choice_words
        .iter()
        .map(|choice| label_width(choice, choice_size) + choice_pad * 2.)
        .collect();
    let choice_gap = size * 0.4;
    let count = widget.choices.len();
    let row = choice_widths.iter().sum::<f32>() + choice_gap * count.saturating_sub(1) as f32;
    let widest = choice_widths.iter().copied().fold(0., f32::max);
    // Two to a row needs room for two buttons that still read.
    let half = (inner_room - choice_gap) * 0.5;
    let arrange = match widget.layout {
        HudLayout::Auto if row <= inner_room => Arrange::Row,
        HudLayout::Grid if count > 1 && half >= choice_pad * 2. + choice_size * 2. => Arrange::Grid,
        _ => Arrange::Column,
    };
    let cell = match arrange {
        Arrange::Row => 0.,
        Arrange::Column => widest.min(inner_room),
        Arrange::Grid => widest.min(half),
    };
    if arrange == Arrange::Grid && cell < widest {
        choice_words = fit(cell);
    }
    let choices_width = match arrange {
        Arrange::Row => row,
        Arrange::Column => cell,
        Arrange::Grid => cell * 2. + choice_gap,
    };
    let choice_height = choice_size * 2.;
    // A board keeps its proportions, shrinking to the room there is.
    let board_extent = widget.board.as_ref().map(|board| board.extent());
    let board_scale = board_extent.map_or(0., |[w, _]| (scale * grow).min(inner_room / w.max(1.)));
    let [board_width, board_height] =
        board_extent.map_or([0., 0.], |[w, h]| [w * board_scale, h * board_scale]);
    let content_width = line_width
        .max(title_width)
        .max(bar_width)
        .max(choices_width)
        .max(board_width)
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
    let bar_height = match widget.bar_style {
        HudBar::Thin => (size * 0.16).max(3. * scale),
        HudBar::Pips => (size * 0.55).max(8. * scale),
        _ => (size * 0.45).max(7. * scale),
    };
    let bar_at = widget.progress.map(|_| {
        let at = height + if height > 0. { size * 0.2 } else { 0. };
        height = at + bar_height;
        at
    });
    let choices_at = (count > 0).then(|| {
        let at = height + if height > 0. { size * 0.5 } else { 0. };
        let rows = match arrange {
            Arrange::Row => 1,
            Arrange::Column => count,
            Arrange::Grid => count.div_ceil(2),
        };
        height = at + rows as f32 * choice_height + (rows - 1) as f32 * choice_gap;
        at
    });
    let board_at = board_extent.map(|_| {
        let at = height + if height > 0. { size * 0.45 } else { 0. };
        height = at + board_height;
        at
    });
    // A banner runs across the screen, so pinned it is a panel.
    let banner = widget.style == HudStyle::Banner && pinned.is_none();
    let pill = widget.style == HudStyle::Pill;
    let side = if pill { pad + height * 0.25 } else { pad };
    let stripe = if widget.style == HudStyle::Card {
        5. * scale * grow
    } else {
        0.
    };
    let full_height = height + pad * 2.;
    // Points, notches and leaning sides need room clear of the words.
    let (room_left, room_right) = if boxed && !banner && !pill {
        crate::hud_skin::shape_room(widget.shape, full_height)
    } else {
        (0., 0.)
    };
    let width = if banner {
        screen.w
    } else {
        content_width + side * 2. + stripe + room_left + room_right
    };
    let anchor = pinned.map_or(screen.x + widget.x * screen.w, |pinned| pinned.at.x);
    // Screen widgets stay on screen. A pinned widget moves with the arena;
    // one that points the way stays in the middle of the screen, clear of
    // the HUD in its corners, with an arrow toward its pin whenever that
    // holds it away from it.
    let bounds = match pinned {
        None => Some(Rect::new(
            screen.x + margin,
            screen.y + margin,
            screen.w - margin * 2.,
            screen.h - margin * 2.,
        )),
        Some(_) if widget.point => inset,
        Some(_) => None,
    };
    // Something too big to fit keeps its top left corner in.
    let hold =
        |value: f32, span: f32, low: f32, high: f32| value.clamp(low, (high - span).max(low));
    let rest_left = match widget.align {
        HudAlign::Left => anchor,
        HudAlign::Center => anchor - width * 0.5,
        HudAlign::Right => anchor - width,
    };
    let left = if banner {
        screen.x
    } else {
        bounds.map_or(rest_left, |b| hold(rest_left, width, b.x, b.right()))
    };
    // `y` is the middle of a plain widget's first line, and of a panel.
    let middle = screen.y + widget.y * screen.h;
    let first = line_at
        .map(|at| (at, size))
        .or(title_at.map(|at| (at, title_size)));
    let rest_top = match (pinned, first) {
        (
            Some(Pinned {
                at,
                above: Some(lift),
            }),
            _,
        ) => at.y - lift - 8. * scale - full_height,
        (Some(Pinned { at, above: None }), _) => at.y - full_height * 0.5,
        (None, Some((at, line))) if !boxed => middle + line * 0.35 - at,
        (None, _) => middle - full_height * 0.5,
    };
    let top = bounds.map_or(rest_top, |b| hold(rest_top, full_height, b.y, b.bottom()));
    let frame = Rect::new(left, top, width, full_height);
    let held = (left - rest_left).abs() > 0.5 || (top - rest_top).abs() > 0.5;
    let toward = pinned
        .filter(|_| widget.point && held && !banner)
        .and_then(|pinned| edge_toward(frame, pinned.at));
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
        left + stripe + side + room_left
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
    let mut choices = Vec::with_capacity(count);
    if let Some(at) = choices_at {
        let (x, y) = (line_left(choices_width), y0 + at);
        let down = |line: usize| y + line as f32 * (choice_height + choice_gap);
        match arrange {
            Arrange::Row => {
                let mut x = x;
                for width in &choice_widths {
                    choices.push(Rect::new(x, y, *width, choice_height));
                    x += width + choice_gap;
                }
            }
            Arrange::Column => {
                for index in 0..count {
                    choices.push(Rect::new(x, down(index), cell, choice_height));
                }
            }
            Arrange::Grid => {
                for index in 0..count {
                    // A last choice on its own sits in the middle.
                    let left = if index + 1 == count && count % 2 == 1 {
                        x + (choices_width - cell) * 0.5
                    } else {
                        x + (index % 2) as f32 * (cell + choice_gap)
                    };
                    choices.push(Rect::new(left, down(index / 2), cell, choice_height));
                }
            }
        }
    }
    let cells = match (&widget.board, board_at) {
        (Some(board), Some(at)) => {
            crate::hud_board::cells(board, vec2(line_left(board_width), y0 + at), board_scale)
        }
        _ => Vec::new(),
    };
    Layout {
        scale,
        frame,
        size,
        title,
        text,
        words: [title_text, line_text],
        choice_words,
        icon,
        bar,
        choices,
        cells,
        board_scale,
        corner: if pill {
            full_height * 0.5
        } else if banner {
            0.
        } else {
            (size * 0.45).min(full_height * 0.5)
        },
        toward,
    }
}

/// Where a line from the middle of `frame` toward `at` leaves it, and
/// which way it runs.
fn edge_toward(frame: Rect, at: Vec2) -> Option<(Vec2, Vec2)> {
    let center = frame.center();
    let way = (at - center).try_normalize()?;
    let reach = |span: f32, along: f32| {
        if along.abs() > 1e-4 {
            span * 0.5 / along.abs()
        } else {
            f32::INFINITY
        }
    };
    let out = reach(frame.w, way.x).min(reach(frame.h, way.y));
    Some((center + way * out, way))
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
        HudIcon::Crown => {
            // A band with three points, in pieces that meet edge to edge.
            draw_rectangle(at.x - r * 0.9, at.y, r * 1.8, r * 0.62, color);
            for (from, tip, to) in [
                (-0.9, vec2(-0.9, -0.62), -0.4),
                (-0.4, vec2(0., -0.82), 0.4),
                (0.4, vec2(0.9, -0.62), 0.9),
            ] {
                draw_triangle(
                    vec2(at.x + r * from, at.y),
                    at + tip * r,
                    vec2(at.x + r * to, at.y),
                    color,
                );
            }
            for x in [-0.45, 0., 0.45] {
                draw_circle(at.x + r * x, at.y + r * 0.31, r * 0.11, ink);
            }
        }
        HudIcon::Skull => {
            // One rounded outline: the top of the head, then the jaw.
            let head = vec2(at.x, at.y - r * 0.12);
            let radius = r * 0.8;
            let mut points: Vec<Vec2> = (0..=12)
                .map(|i| {
                    let a = (160. + 220. * i as f32 / 12.).to_radians();
                    head + vec2(a.cos(), a.sin()) * radius
                })
                .collect();
            points.push(vec2(at.x + r * 0.42, at.y + r * 0.88));
            points.push(vec2(at.x - r * 0.42, at.y + r * 0.88));
            polygon(&points, color);
            for side in [-1., 1.] {
                draw_circle(
                    head.x + side * radius * 0.36,
                    head.y + radius * 0.12,
                    radius * 0.24,
                    ink,
                );
            }
            draw_triangle(
                vec2(at.x, head.y + radius * 0.42),
                vec2(at.x - radius * 0.12, head.y + radius * 0.64),
                vec2(at.x + radius * 0.12, head.y + radius * 0.64),
                ink,
            );
        }
        HudIcon::Lock => {
            let body = Rect::new(at.x - r * 0.72, at.y - r * 0.2, r * 1.44, r * 0.95);
            rounded(body, r * 0.14, color);
            // The shackle: half a ring standing on the body.
            let center = vec2(at.x, body.y);
            let (outer, inner) = (r * 0.52, r * 0.52 - line * 1.3);
            let steps = 10;
            let point = |i: usize, reach: f32| {
                let a = std::f32::consts::PI * (1. + i as f32 / steps as f32);
                center + vec2(a.cos(), a.sin()) * reach
            };
            for i in 0..steps {
                draw_triangle(
                    point(i, outer),
                    point(i + 1, outer),
                    point(i + 1, inner),
                    color,
                );
                draw_triangle(point(i, outer), point(i + 1, inner), point(i, inner), color);
            }
            // A keyhole: a round top and a slot that meet along a chord.
            let hole = vec2(at.x, at.y + r * 0.18);
            let size = r * 0.14;
            let round: Vec<Vec2> = (0..=10)
                .map(|i| {
                    let a = (120. + 300. * i as f32 / 10.).to_radians();
                    hole + vec2(a.cos(), a.sin()) * size
                })
                .collect();
            polygon(&round, ink);
            let (right, left) = (round[10], round[0]);
            polygon(
                &[
                    left,
                    right,
                    vec2(hole.x + size * 0.75, hole.y + size * 2.2),
                    vec2(hole.x - size * 0.75, hole.y + size * 2.2),
                ],
                ink,
            );
        }
        HudIcon::Check => crate::scenery::chevron_with(
            draw_triangle,
            vec2(at.x - r * 0.25, at.y + r * 0.55),
            vec2(at.x - r * 0.8, at.y),
            vec2(at.x + r * 0.85, at.y - r * 0.65),
            line * 1.7,
            color,
        ),
        HudIcon::Cross => {
            // A plus turned an eighth: a middle square and four arms.
            let (half, reach) = (line * 0.85, r * 0.78);
            let turn = |p: Vec2| at + vec2(p.x - p.y, p.x + p.y) * std::f32::consts::FRAC_1_SQRT_2;
            let quad = |corners: [Vec2; 4]| {
                let [a, b, c, d] = corners.map(turn);
                draw_triangle(a, b, c, color);
                draw_triangle(a, c, d, color);
            };
            quad([
                vec2(-half, -half),
                vec2(half, -half),
                vec2(half, half),
                vec2(-half, half),
            ]);
            quad([
                vec2(half, -half),
                vec2(reach, -half),
                vec2(reach, half),
                vec2(half, half),
            ]);
            quad([
                vec2(-reach, -half),
                vec2(-half, -half),
                vec2(-half, half),
                vec2(-reach, half),
            ]);
            quad([
                vec2(-half, half),
                vec2(half, half),
                vec2(half, reach),
                vec2(-half, reach),
            ]);
            quad([
                vec2(-half, -reach),
                vec2(half, -reach),
                vec2(half, -half),
                vec2(-half, -half),
            ]);
        }
        HudIcon::Fire => {
            // A tall tongue leaning right and a smaller one licking up on the
            // left, over a round base, around a hotter core. Both are fans
            // from a point low inside, which sees the whole outline.
            let p = |x: f32, y: f32| at + vec2(x, y) * r;
            let bend = |points: &mut Vec<Vec2>, from: Vec2, pull: Vec2, to: Vec2| {
                points.extend((1..=5).map(|i| {
                    let t = i as f32 / 5.;
                    from * (1. - t) * (1. - t) + pull * (2. * t * (1. - t)) + to * (t * t)
                }));
            };
            let round = |points: &mut Vec<Vec2>, middle: Vec2, radius: f32| {
                points.extend((1..=8).map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 8.;
                    middle + vec2(a.cos(), a.sin()) * radius
                }));
            };
            let (left, lick, notch, tip, right) = (
                p(-0.62, 0.32),
                p(-0.5, -0.52),
                p(-0.2, -0.1),
                p(0.1, -1.),
                p(0.62, 0.32),
            );
            let mut outer = vec![p(0., 0.3), left];
            bend(&mut outer, left, p(-0.75, -0.1), lick);
            bend(&mut outer, lick, p(-0.3, -0.3), notch);
            bend(&mut outer, notch, p(-0.25, -0.55), tip);
            bend(&mut outer, tip, p(0.75, -0.35), right);
            round(&mut outer, p(0., 0.32), r * 0.62);
            polygon(&outer, color);
            let (left, tip, right) = (p(-0.3, 0.52), p(-0.06, -0.12), p(0.3, 0.52));
            let mut core = vec![p(0., 0.5), left];
            bend(&mut core, left, p(-0.32, 0.12), tip);
            bend(&mut core, tip, p(0.36, 0.2), right);
            round(&mut core, p(0., 0.52), r * 0.3);
            polygon(
                &core,
                Color {
                    a: color.a,
                    ..mix(color, WHITE, 0.6)
                },
            );
        }
        HudIcon::Bomb => {
            let body = vec2(at.x - r * 0.12, at.y + r * 0.18);
            let radius = r * 0.7;
            draw_circle(body.x, body.y, radius, color);
            // The fuse leaves the body at its top right, with a spark.
            let way = vec2(1., -1.).normalize();
            let from = body + way * (radius + line * 0.5);
            let to = from + way * r * 0.42;
            draw_line(from.x, from.y, to.x, to.y, line * 1.1, color);
            let spark = to + way * r * 0.17;
            draw_circle(
                spark.x,
                spark.y,
                r * 0.17,
                Color {
                    a: color.a,
                    ..mix(color, WHITE, 0.7)
                },
            );
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

/// Draw one widget, over `pinned` when it floats over something; returns
/// the choice under a fresh click, if any.
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
    pinned: Option<Pinned>,
) -> Option<u8> {
    let l = animated_layout(widget, screen, scale, grow, nudge, pinned);
    let scale = l.scale;
    // A pinned widget out of view draws nothing.
    if !l.frame.overlaps(&screen) {
        return None;
    }
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
    let banner = widget.style == HudStyle::Banner && pinned.is_none();
    // A pill is always a capsule; the other panels take the chosen shape.
    let shape = if widget.style == HudStyle::Pill {
        HudShape::Rounded
    } else {
        widget.shape
    };
    let ink = mix(panel, BLACK, 0.75);
    let drift = if ui::reduced_motion() {
        0.
    } else {
        (time * 18.) as f32
    };
    if widget.glow && widget.style != HudStyle::Plain {
        // Rings that widen and fade, so the glow is soft at its edge.
        let strength = 0.075 * (1. + pulse * 0.8);
        for ring in (1..=5).rev() {
            let spread = ring as f32 * 3. * scale;
            let halo = Rect::new(
                frame.x - spread,
                frame.y - spread,
                frame.w + spread * 2.,
                frame.h + spread * 2.,
            );
            let glow_shape = if banner { HudShape::Square } else { shape };
            hud_skin::fill(
                &hud_skin::outline(halo, glow_shape, l.corner + spread).points,
                fade(accent, strength),
                fade(accent, strength),
            );
        }
    }
    let edge = (1.5 * scale).max(1.) * (1. + pulse * 0.6);
    let edge_color = fade(
        accent,
        if widget.border == HudBorder::Thick {
            0.8 + pulse * 0.2
        } else {
            0.45 + pulse * 0.45
        },
    );
    if widget.style != HudStyle::Plain {
        let body_shape = if banner { HudShape::Square } else { shape };
        let body = hud_skin::outline(frame, body_shape, l.corner);
        if !banner {
            let mut shadow = body.points.clone();
            for point in &mut shadow {
                point.y += 3. * scale;
            }
            hud_skin::fill(
                &shadow,
                fade(BLACK, 0.22 * widget.opacity.max(0.3)),
                fade(BLACK, 0.22 * widget.opacity.max(0.3)),
            );
        }
        let second = rgb(
            widget.fill_color,
            match widget.fill {
                HudFill::Gradient => mix(panel, BLACK, 0.45),
                _ => mix(panel, accent, 0.3),
            },
        );
        match widget.fill {
            HudFill::Gradient => hud_skin::fill(
                &body.points,
                fade(panel, widget.opacity),
                fade(second, widget.opacity),
            ),
            HudFill::Glass => {
                hud_skin::fill(
                    &body.points,
                    fade(panel, widget.opacity * 0.5),
                    fade(panel, widget.opacity * 0.72),
                );
                let sheen = [
                    vec2(frame.x, frame.y),
                    vec2(frame.right(), frame.y),
                    vec2(frame.right(), frame.y + frame.h * 0.45),
                    vec2(frame.x, frame.y + frame.h * 0.45),
                ];
                hud_skin::fill(
                    &hud_skin::clip(&sheen, &body.core),
                    fade(WHITE, 0.16),
                    fade(WHITE, 0.02),
                );
            }
            HudFill::Solid | HudFill::Stripes | HudFill::Grid | HudFill::Dots => {
                hud_skin::fill(
                    &body.points,
                    fade(panel, widget.opacity),
                    fade(panel, widget.opacity),
                );
                hud_skin::pattern(
                    widget.fill,
                    &body.core,
                    frame,
                    fade(second, 0.4 * widget.opacity.max(0.4)),
                    scale,
                    if widget.fill == HudFill::Stripes {
                        drift * 0.3
                    } else {
                        0.
                    },
                );
            }
        }
        if widget.style == HudStyle::Card {
            for piece in hud_skin::side_band(frame, shape, l.corner, 5. * scale * grow) {
                hud_skin::fill(&piece, fade(accent, 1.), fade(accent, 1.));
            }
        }
        if banner {
            // A banner's edges run along its top and bottom.
            let line = match widget.border {
                HudBorder::None => 0.,
                HudBorder::Thick => edge * 2.4,
                _ => edge * 1.3,
            };
            if line > 0. {
                for y in [frame.y, frame.y + frame.h - line] {
                    if widget.border == HudBorder::Dashed {
                        let mut x = frame.x;
                        while x < frame.right() {
                            draw_rectangle(
                                x,
                                y,
                                (line * 5.).min(frame.right() - x),
                                line,
                                edge_color,
                            );
                            x += line * 8.;
                        }
                    } else {
                        draw_rectangle(frame.x, y, frame.w, line, edge_color);
                    }
                }
                if widget.border == HudBorder::Double {
                    let gap = line * 2.5;
                    for y in [frame.y + gap, frame.y + frame.h - line - gap] {
                        draw_rectangle(frame.x, y, frame.w, line * 0.7, fade(accent, 0.4));
                    }
                }
            }
        } else {
            hud_skin::border(frame, shape, l.corner, widget.border, edge, edge_color);
        }
    }
    let lettering = hud_skin::settle(
        widget.lettering,
        widget.style == HudStyle::Plain,
        widget.glow,
        crate::accessibility::contrast(),
    );
    let glow_accent = fade(accent, 1. + pulse * 0.6);
    if let Some((at, size)) = l.title {
        hud_skin::words(
            lettering,
            &l.words[0],
            at,
            size,
            fade(rgb(widget.title_color, mix(accent, text_color, 0.35)), 1.),
            glow_accent,
            fade(ink, 1.),
        );
    }
    if let Some((at, size)) = l.icon {
        icon(widget.icon, at, size, fade(accent, 1.));
    }
    if let Some(at) = l.text {
        hud_skin::words(
            lettering,
            &l.words[1],
            at,
            l.size,
            fade(text_color, 1.),
            glow_accent,
            fade(ink, 1.),
        );
    }
    if let Some(bar) = l.bar {
        hud_skin::bar(
            widget.bar_style,
            bar,
            progress,
            widget.segments,
            fade(rgb(widget.bar_color, accent), 1.),
            fade(Color::new(0.2, 0.22, 0.25, 1.), 0.88),
            scale,
            drift,
        );
    }
    if let Some((edge, way)) = l.toward {
        // A pointer at the edge nearest what the widget is pinned to.
        let size = (l.size * 0.85).max(9. * scale);
        let side = vec2(-way.y, way.x) * size * 0.55;
        let base = edge + way * (3. * scale);
        draw_triangle(
            base + side,
            base + way * size,
            base - side,
            fade(accent, 0.85 + pulse * 0.15),
        );
    }
    let mut picked = None;
    for (index, (rect, choice)) in l.choices.iter().zip(&l.choice_words).enumerate() {
        let hovered = pointer.is_some_and(|p| rect.contains(p));
        let base = if widget.style == HudStyle::Plain {
            mix(ui::surface_color(), panel, 0.3)
        } else {
            mix(panel, WHITE, if ui::is_dark() { 0.08 } else { 0.4 })
        };
        hud_skin::choice(
            widget.buttons,
            *rect,
            choice,
            hovered,
            fade(base, 0.95),
            fade(accent, 1.),
            fade(text_color, 1.),
            match shape {
                HudShape::Rounded => l.corner,
                _ => 0.,
            },
            scale,
        );
        if hovered && crate::ui::take_click() {
            picked = Some(index as u8);
        }
    }
    if let Some(board) = &widget.board {
        let skin = crate::hud_board::Skin {
            accent,
            text: text_color,
            panel,
            buttons: widget.buttons,
            bar: widget.bar_style,
            segments: widget.segments,
            lettering,
            ink,
            caps: widget.caps,
            corner: match shape {
                HudShape::Rounded => (6. * l.board_scale).max(2.),
                _ => 0.,
            },
            opacity,
            drift,
        };
        picked = picked.or(crate::hud_board::paint(
            board,
            &l.cells,
            l.board_scale,
            &skin,
            pointer,
        ));
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
    for rect in l.choices.iter_mut().chain(&mut l.cells) {
        *rect = rect.offset(by);
    }
    if let Some((edge, _)) = &mut l.toward {
        *edge += by;
    }
}

fn animated_layout(
    widget: &HudWidget,
    screen: Rect,
    scale: f32,
    grow: f32,
    nudge: Vec2,
    pinned: Option<Pinned>,
) -> Layout {
    let mut laid = layout(widget, screen, scale, grow, pinned);
    if grow != 1. {
        let rest = layout(widget, screen, scale, 1., pinned);
        let shift = rest.frame.center() - laid.frame.center();
        offset(&mut laid, shift);
    }
    offset(&mut laid, nudge);
    laid
}

/// Draw the widgets of `layer` this player sees, with entrances and exits,
/// pinned ones where `place` finds them. With `pointing`, the pointer
/// lights choices and a click picks one. Returns a pick: the menu and the
/// choice.
pub fn draw(
    widgets: &[HudWidget],
    s: f32,
    place: &Place,
    layer: Layer,
    pointing: bool,
) -> Option<(u32, u8)> {
    let screen = Rect::new(0., 0., screen_width(), screen_height());
    let now = get_time();
    let calm = ui::reduced_motion();
    let pointer = (pointing && crate::ui_layout::pointer_visible() && ui::interactive())
        .then(crate::ui_layout::pointer);
    let arena = layer == Layer::Arena;
    let mut picked = None;
    MEMORY.with(|memory| {
        let mut memories = memory.borrow_mut();
        let memory = &mut memories[usize::from(arena)];
        memory.frame += 1;
        let frame = memory.frame;
        let dt = get_frame_time().min(0.1);
        for widget in widgets.iter().filter(|widget| widget.pinned() == arena) {
            // A hidden or departed tank must not leave a marker behind.
            let Some(pinned) = anchor(widget, place) else {
                memory.seen.remove(&identity(widget));
                continue;
            };
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
            let side = pinned.map_or(widget.x, |pinned| pinned.at.x / screen.w.max(1.));
            let (opacity, grow, nudge) = entrance(widget.enter, t, side, s);
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
                pinned,
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
            // A pinned widget fades over what it floated over, moving with
            // the view, and goes at once when that is gone too.
            let Some(pinned) = anchor(&seen.widget, place) else {
                return false;
            };
            let gone = *seen.gone.get_or_insert(now);
            let t = ((now - gone) / EXIT_SECONDS) as f32;
            if t >= 1. {
                return false;
            }
            if seen.widget.valid() {
                paint(
                    &seen.widget,
                    screen,
                    s,
                    1. - t,
                    1.,
                    Vec2::ZERO,
                    seen.progress,
                    now,
                    None,
                    pinned,
                );
            }
            true
        });
    });
    picked
}

/// Whether the pointer is over a widget a player can pick from, so a click
/// there picks instead of firing.
pub fn captures_pointer(widgets: &[HudWidget], s: f32, place: &Place) -> bool {
    captures_layer_pointer(widgets, s, place, None)
}

/// Pointer capture for one layer, so a screen menu blocks controls below it.
pub fn captures_layer_pointer(
    widgets: &[HudWidget],
    s: f32,
    place: &Place,
    layer: Option<Layer>,
) -> bool {
    if !crate::ui_layout::pointer_visible() {
        return false;
    }
    let pointer = crate::ui_layout::pointer();
    let screen = Rect::new(0., 0., screen_width(), screen_height());
    widgets
        .iter()
        .filter(|widget| layer.is_none_or(|layer| widget.pinned() == (layer == Layer::Arena)))
        .filter(|widget| widget.pickable())
        .any(|widget| {
            anchor(widget, place).is_some_and(|pinned| {
                let rest = layout(widget, screen, s, 1., pinned);
                if rest.frame.contains(pointer) {
                    return true;
                }
                // The entrance can put a menu outside its resting frame.
                // Clicking it while it slides in must not fire the gun.
                let age = MEMORY.with(|memory| {
                    memory.borrow()[usize::from(widget.pinned())]
                        .seen
                        .get(&identity(widget))
                        .map_or(ENTER_SECONDS, |seen| get_time() - seen.since)
                });
                let t = if ui::reduced_motion() {
                    1.
                } else {
                    (age / ENTER_SECONDS).clamp(0., 1.) as f32
                };
                let side = pinned.map_or(widget.x, |pin| pin.at.x / screen.w.max(1.));
                let (_, grow, nudge) = entrance(widget.enter, t, side, s);
                animated_layout(widget, screen, s, grow, nudge, pinned)
                    .frame
                    .contains(pointer)
            })
        })
}

/// The choice of `widget` at `point` when drawn into `frame` at `scale`,
/// over `pinned` when it floats over something.
pub fn choice_at(
    widget: &HudWidget,
    frame: Rect,
    scale: f32,
    point: Vec2,
    pinned: Option<Pinned>,
    age: f32,
) -> Option<u8> {
    if !ui::reduced_motion() && age < ENTER_SECONDS as f32 {
        return None;
    }
    let laid = layout(widget, frame, scale, 1., pinned);
    if let Some(board) = &widget.board {
        return crate::hud_board::button_at(board, &laid.cells, point);
    }
    laid.choices
        .iter()
        .position(|rect| rect.contains(point))
        .map(|index| index as u8)
}

/// Whether `point` is on `widget` when drawn into `frame` at `scale`, over
/// `pinned` when it floats over something.
pub fn covers(
    widget: &HudWidget,
    frame: Rect,
    scale: f32,
    point: Vec2,
    pinned: Option<Pinned>,
    age: f32,
) -> bool {
    let t = if ui::reduced_motion() {
        1.
    } else {
        (age / ENTER_SECONDS as f32).clamp(0., 1.)
    };
    let side = pinned.map_or(widget.x, |pin| (pin.at.x - frame.x) / frame.w.max(1.));
    let (_, grow, nudge) = entrance(widget.enter, t, side, scale);
    layout(widget, frame, scale, 1., pinned)
        .frame
        .contains(point)
        || animated_layout(widget, frame, scale, grow, nudge, pinned)
            .frame
            .contains(point)
}

/// Draw `widget` into `frame`, a small picture of the screen, `t` seconds
/// after it appeared, its choices lit under `pointer` and over `pinned`
/// when it floats over something; for the editor.
pub fn preview(
    widget: &HudWidget,
    frame: Rect,
    scale: f32,
    t: f32,
    progress: f32,
    pointer: Option<Vec2>,
    pinned: Option<Pinned>,
) {
    let enter = (t as f64 / ENTER_SECONDS).clamp(0., 1.) as f32;
    let side = pinned.map_or(widget.x, |pinned| (pinned.at.x - frame.x) / frame.w.max(1.));
    let (opacity, grow, nudge) = if ui::reduced_motion() {
        (1., 1., Vec2::ZERO)
    } else {
        entrance(widget.enter, enter, side, scale)
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
        pinned,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_markers_reserve_hud_controls_without_inverting_in_small_windows() {
        let bounds = marker_bounds(Rect::new(0., 0., 1280., 800.), 1.);
        assert!(!bounds.overlaps(&Rect::new(22., 22., 226., 150.)));
        assert!(!bounds.overlaps(&Rect::new(1032., 22., 226., 350.)));
        for (w, h, s) in [(320., 180., 2.), (640., 480., 1.5), (1920., 1080., 1.)] {
            let rect = marker_bounds(Rect::new(0., 0., w, h), s);
            assert!(rect.w > 0. && rect.h > 0. && rect.x >= 0. && rect.y >= 0.);
            assert!(rect.right() <= w && rect.bottom() <= h);
        }
    }

    #[test]
    fn server_identities_survive_moving_pins_and_distinguish_overlapping_labels() {
        let a = HudWidget {
            menu: 17,
            pin: HudPin::World([10., 20.]),
            ..Default::default()
        };
        let b = HudWidget {
            pin: HudPin::World([30., 40.]),
            text: "New text".into(),
            ..a.clone()
        };
        assert_eq!(identity(&a), identity(&b));
        assert_ne!(
            identity(&a),
            identity(&HudWidget {
                menu: 18,
                ..a.clone()
            })
        );
    }

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
    fn pins_give_widgets_their_own_identity() {
        let sign = HudWidget {
            text: "Shop".into(),
            pin: HudPin::World([100., 200.]),
            ..HudWidget::default()
        };
        let other = HudWidget {
            pin: HudPin::World([900., 200.]),
            ..sign.clone()
        };
        let tag = HudWidget {
            pin: HudPin::Tank(7),
            ..sign.clone()
        };
        assert_ne!(
            identity(&sign),
            identity(&other),
            "two signs in one place on screen"
        );
        assert_ne!(identity(&sign), identity(&tag));
    }

    #[test]
    fn pointers_leave_the_frame_toward_what_is_out_of_view() {
        let frame = Rect::new(100., 100., 200., 50.);
        let (edge, way) = edge_toward(frame, vec2(1000., 125.)).expect("a way");
        assert!((edge - vec2(300., 125.)).length() < 1e-3, "{edge}");
        assert!((way - vec2(1., 0.)).length() < 1e-4);
        let (edge, _) = edge_toward(frame, vec2(200., -500.)).expect("a way");
        assert!((edge - vec2(200., 100.)).length() < 1e-3, "{edge}");
        // A corner direction leaves through the nearer side.
        let (edge, way) = edge_toward(frame, vec2(2200., 1125.)).expect("a way");
        assert!(frame.contains(edge - way * 0.01) && !frame.contains(edge + way * 0.5));
        assert!(edge_toward(frame, frame.center()).is_none());
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
