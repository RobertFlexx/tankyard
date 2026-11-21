//! Boards: a widget's grid of buttons, labels, screens, digit displays,
//! lights and meters, drawn the way the rest of the widget is.
use crate::hud_skin;
use crate::ui::{self, alpha, fitted_label, label_width, mix};
// Page-space shapes, so boards scroll with an editor page like its text.
use crate::ui::{
    ui_circle as draw_circle, ui_rectangle as draw_rectangle, ui_triangle as draw_triangle,
};
use macroquad::prelude::*;
use tankyard_mods::{HudAlign, HudBar, HudBoard, HudButtons, HudCell, HudCellKind, HudLettering};

/// Where each of `board`'s cells lands when its top-left corner is at
/// `origin` and a grid unit is `scale` pixels.
pub fn cells(board: &HudBoard, origin: Vec2, scale: f32) -> Vec<Rect> {
    board
        .cells
        .iter()
        .map(|cell| {
            let [x, y, w, h] = board.place(cell.at, cell.span);
            Rect::new(
                origin.x + x * scale,
                origin.y + y * scale,
                w * scale,
                h * scale,
            )
        })
        .collect()
}

/// The button among `rects` under `point`: the topmost, as cells drawn
/// later sit over earlier ones.
pub fn button_at(board: &HudBoard, rects: &[Rect], point: Vec2) -> Option<u8> {
    board
        .cells
        .iter()
        .zip(rects)
        .rposition(|(cell, rect)| cell.kind == HudCellKind::Button && rect.contains(point))
        .and_then(|index| u8::try_from(index).ok())
}

/// The widget's look, as its cells take it on.
pub struct Skin {
    pub accent: Color,
    pub text: Color,
    pub panel: Color,
    pub buttons: HudButtons,
    pub bar: HudBar,
    pub segments: u8,
    pub lettering: HudLettering,
    pub ink: Color,
    pub caps: bool,
    /// Rounding for boxes, at the board's scale.
    pub corner: f32,
    /// How much of the widget shows, from 0 to 1.
    pub opacity: f32,
    pub drift: f32,
}
impl Skin {
    fn fade(&self, color: Color) -> Color {
        alpha(color, color.a * self.opacity)
    }
}

fn rgb(color: Option<[u8; 3]>, fallback: Color) -> Color {
    color.map_or(fallback, |[r, g, b]| Color::from_rgba(r, g, b, 255))
}

/// Draw `board`'s cells into `rects` at `scale`, buttons lit under
/// `pointer`. Returns the button a click picked.
pub fn paint(
    board: &HudBoard,
    rects: &[Rect],
    scale: f32,
    skin: &Skin,
    pointer: Option<Vec2>,
) -> Option<u8> {
    let hovered = pointer.and_then(|point| button_at(board, rects, point));
    let pressed = is_mouse_button_down(MouseButton::Left);
    let mut picked = None;
    for (index, (cell, &rect)) in board.cells.iter().zip(rects).enumerate() {
        let text = if skin.caps {
            cell.text.to_uppercase()
        } else {
            cell.text.clone()
        };
        let over = hovered == Some(index as u8);
        match cell.kind {
            HudCellKind::Label => {
                if let Some(fill) = cell.fill {
                    crate::hud_widgets::rounded(
                        rect,
                        skin.corner,
                        skin.fade(rgb(Some(fill), BLACK)),
                    );
                }
                words(cell, &text, rect, rgb(cell.color, skin.text), skin, 0.08);
            }
            HudCellKind::Button => {
                button(cell, &text, rect, scale, skin, over, over && pressed);
                if over && crate::ui::take_click() {
                    picked = Some(index as u8);
                }
            }
            HudCellKind::Screen => {
                let glass = screen(cell, rect, scale, skin);
                let color = rgb(cell.color, mix(skin.accent, WHITE, 0.45));
                words(cell, &text, glass, color, skin, 0.06);
            }
            HudCellKind::Digits => {
                let glass = screen(cell, rect, scale, skin);
                let color = rgb(cell.color, mix(skin.accent, WHITE, 0.25));
                digits(
                    &text,
                    glass,
                    cell,
                    skin.fade(color),
                    skin.fade(alpha(color, 0.08)),
                );
            }
            HudCellKind::Light => light(cell, &text, rect, skin),
            HudCellKind::Meter => meter(cell, &text, rect, scale, skin),
        }
    }
    picked
}

/// `value` in `rect`, sized to fit unless the cell sets a size, and placed
/// by its alignment `pad` of the shorter side in from the edges.
fn words(cell: &HudCell, value: &str, rect: Rect, color: Color, skin: &Skin, pad: f32) {
    if value.is_empty() {
        return;
    }
    let short = rect.w.min(rect.h);
    let inset = (short * pad).max(2.);
    let room = (rect.w - inset * 2.).max(1.);
    let mut size = (rect.h * 0.5).min(rect.h - inset * 2.).max(6.);
    if cell.size > 0. {
        size *= cell.size;
    } else {
        // Shrink the words to the width before cutting any off, down to
        // a third of the shorter side. Text widths don't follow the size
        // exactly, so check again after each step.
        let least = (short * 0.3).max(6.).min(size);
        let width = label_width(value, size);
        if width > room {
            size = (size * room / width * 0.97).max(least);
            while size > least && label_width(value, size) > room {
                size = (size * 0.92).max(least);
            }
        }
    }
    let shown = fitted_label(value, size, room);
    let width = label_width(&shown, size);
    let x = match cell.align {
        HudAlign::Left => rect.x + inset,
        HudAlign::Center => rect.x + (rect.w - width) * 0.5,
        HudAlign::Right => rect.right() - inset - width,
    };
    hud_skin::words(
        skin.lettering,
        &shown,
        vec2(x, rect.y + rect.h * 0.5 + size * 0.33),
        size,
        skin.fade(color),
        skin.fade(skin.accent),
        skin.fade(skin.ink),
    );
}

fn button(
    cell: &HudCell,
    value: &str,
    rect: Rect,
    scale: f32,
    skin: &Skin,
    hovered: bool,
    down: bool,
) {
    // Pressed buttons sink a little.
    let rect = if down {
        rect.offset(vec2(0., 1.5 * scale))
    } else {
        rect
    };
    let lamp = rgb(cell.color, skin.accent);
    let mut base = rgb(
        cell.fill,
        mix(skin.panel, WHITE, if ui::is_dark() { 0.1 } else { 0.4 }),
    );
    if cell.lit {
        base = mix(base, lamp, 0.55);
    }
    if down {
        base = mix(base, BLACK, 0.2);
    }
    hud_skin::choice(
        skin.buttons,
        rect,
        "",
        hovered,
        skin.fade(base),
        skin.fade(lamp),
        skin.fade(skin.text),
        skin.corner,
        scale,
    );
    if cell.lit {
        // A lit strip along the top, like a key's indicator.
        let strip = (2. * scale).max(1.);
        let inset = rect.w * 0.2;
        draw_rectangle(
            rect.x + inset,
            rect.y + strip * 1.5,
            rect.w - inset * 2.,
            strip,
            skin.fade(mix(lamp, WHITE, 0.4)),
        );
    }
    let ink = if cell.lit && cell.color.is_none() {
        mix(skin.text, WHITE, 0.3)
    } else {
        skin.text
    };
    words(cell, value, rect, ink, skin, 0.12);
}

/// A dark, slightly sunken screen in `rect`; returns where its contents go.
fn screen(cell: &HudCell, rect: Rect, scale: f32, skin: &Skin) -> Rect {
    let glass = rgb(cell.fill, mix(skin.panel, BLACK, 0.72));
    let corner = skin.corner.min(rect.h * 0.25);
    crate::hud_widgets::rounded(rect, corner, skin.fade(mix(glass, WHITE, 0.08)));
    let bevel = (1.5 * scale).max(1.);
    let inner = Rect::new(
        rect.x + bevel,
        rect.y + bevel,
        (rect.w - bevel * 2.).max(1.),
        (rect.h - bevel * 2.).max(1.),
    );
    crate::hud_widgets::rounded(inner, (corner - bevel).max(0.), skin.fade(glass));
    // Faint scan lines.
    let step = (3. * scale).max(2.);
    let mut y = inner.y + step;
    while y < inner.bottom() - 1. {
        draw_rectangle(
            inner.x + corner * 0.5,
            y,
            inner.w - corner,
            1.,
            skin.fade(alpha(BLACK, 0.18)),
        );
        y += step;
    }
    inner
}

fn light(cell: &HudCell, value: &str, rect: Rect, skin: &Skin) {
    let lamp = rgb(cell.color, skin.accent);
    // A caption goes under the lamp in a square or tall cell, beside it in
    // a wide one.
    let (center, radius, caption) = if value.is_empty() {
        (rect.center(), rect.w.min(rect.h) * 0.36, None)
    } else if rect.h >= rect.w * 0.75 {
        let radius = rect.w.min(rect.h * 0.6) * 0.24;
        let caption = Rect::new(rect.x, rect.y + rect.h * 0.62, rect.w, rect.h * 0.38);
        (
            vec2(rect.center().x, rect.y + rect.h * 0.34),
            radius,
            Some((caption, cell.align)),
        )
    } else {
        let radius = rect.h.min(rect.w * 0.5) * 0.3;
        let center = vec2(rect.x + radius * 1.6, rect.y + rect.h * 0.5);
        let left = center.x + radius * 1.6;
        let caption = Rect::new(left, rect.y, (rect.right() - left).max(1.), rect.h);
        (center, radius, Some((caption, HudAlign::Left)))
    };
    if cell.lit {
        for ring in (1..=3).rev() {
            let reach = radius * (1. + ring as f32 * 0.35);
            draw_circle(center.x, center.y, reach, skin.fade(alpha(lamp, 0.12)));
        }
        draw_circle(center.x, center.y, radius, skin.fade(lamp));
        draw_circle(
            center.x - radius * 0.3,
            center.y - radius * 0.3,
            radius * 0.35,
            skin.fade(alpha(WHITE, 0.55)),
        );
    } else {
        draw_circle(
            center.x,
            center.y,
            radius,
            skin.fade(mix(lamp, BLACK, 0.72)),
        );
        draw_circle(
            center.x - radius * 0.3,
            center.y - radius * 0.3,
            radius * 0.3,
            skin.fade(alpha(WHITE, 0.12)),
        );
    }
    if let Some((caption, align)) = caption {
        let shown = HudCell {
            align,
            ..cell.clone()
        };
        words(&shown, value, caption, rgb(None, skin.text), skin, 0.05);
    }
}

fn meter(cell: &HudCell, value: &str, rect: Rect, scale: f32, skin: &Skin) {
    let fill = rgb(cell.color, skin.accent);
    // Plain against the panel even while empty.
    let track = alpha(mix(skin.panel, skin.text, 0.2), 0.9);
    let pad = (rect.w.min(rect.h) * 0.12).max(2.);
    // A tall cell fills from the bottom up, with its words below.
    if rect.h > rect.w * 1.5 {
        let words_h = if value.is_empty() {
            0.
        } else {
            (rect.w * 0.6).min(rect.h * 0.3)
        };
        let width = (rect.w * 0.42).max(4.);
        let bar = Rect::new(
            rect.center().x - width * 0.5,
            rect.y + pad,
            width,
            (rect.h - pad * 2. - words_h).max(1.),
        );
        crate::hud_widgets::rounded(bar, width * 0.5, skin.fade(track));
        let level = bar.h * cell.progress.clamp(0., 1.);
        if level >= 1. {
            let filled = Rect::new(bar.x, bar.bottom() - level, bar.w, level);
            crate::hud_widgets::rounded(filled, width * 0.5, skin.fade(fill));
        }
        if words_h > 0. {
            let under = Rect::new(rect.x, bar.bottom(), rect.w, rect.bottom() - bar.bottom());
            words(cell, value, under, skin.text, skin, 0.06);
        }
        return;
    }
    // Words over the bar when there is room for both, beside it otherwise.
    let (label, bar) = if value.is_empty() {
        (
            None,
            Rect::new(
                rect.x + pad,
                rect.y + rect.h * 0.3,
                rect.w - pad * 2.,
                rect.h * 0.4,
            ),
        )
    } else if rect.h >= 34. * scale {
        (
            Some(Rect::new(rect.x, rect.y, rect.w, rect.h * 0.55)),
            Rect::new(
                rect.x + pad,
                rect.y + rect.h * 0.58,
                rect.w - pad * 2.,
                rect.h * 0.3,
            ),
        )
    } else {
        let split = rect.w * 0.4;
        (
            Some(Rect::new(rect.x, rect.y, split, rect.h)),
            Rect::new(
                rect.x + split,
                rect.y + rect.h * 0.3,
                rect.w - split - pad,
                rect.h * 0.4,
            ),
        )
    };
    hud_skin::bar(
        skin.bar,
        bar,
        cell.progress,
        skin.segments,
        skin.fade(fill),
        skin.fade(track),
        scale,
        skin.drift,
    );
    if let Some(label) = label {
        words(cell, value, label, skin.text, skin, 0.06);
    }
}

/// The seven segments a character lights, as bits: a (top), b (top
/// right), c (bottom right), d (bottom), e (bottom left), f (top left)
/// and g (middle); none for a character it cannot show.
fn segments(c: char) -> u8 {
    const A: u8 = 1;
    const B: u8 = 2;
    const C: u8 = 4;
    const D: u8 = 8;
    const E: u8 = 16;
    const F: u8 = 32;
    const G: u8 = 64;
    match c {
        '0' | 'O' => A | B | C | D | E | F,
        '1' | 'I' => B | C,
        '2' | 'Z' | 'z' => A | B | D | E | G,
        '3' => A | B | C | D | G,
        '4' => B | C | F | G,
        '5' | 'S' | 's' => A | C | D | F | G,
        '6' => A | C | D | E | F | G,
        '7' => A | B | C,
        '8' | 'B' => A | B | C | D | E | F | G,
        '9' | 'g' => A | B | C | D | F | G,
        'A' | 'a' => A | B | C | E | F | G,
        'b' => C | D | E | F | G,
        'C' => A | D | E | F,
        'c' => D | E | G,
        'D' | 'd' => B | C | D | E | G,
        'E' | 'e' => A | D | E | F | G,
        'F' | 'f' => A | E | F | G,
        'G' => A | C | D | E | F,
        'H' => B | C | E | F | G,
        'h' => C | E | F | G,
        'i' => C,
        'J' | 'j' => B | C | D | E,
        'L' | 'l' => D | E | F,
        'N' | 'n' => C | E | G,
        'o' => C | D | E | G,
        'P' | 'p' => A | B | E | F | G,
        'Q' | 'q' => A | B | C | F | G,
        'R' | 'r' => E | G,
        'T' | 't' => D | E | F | G,
        'U' => B | C | D | E | F,
        'u' | 'V' | 'v' => C | D | E,
        'Y' | 'y' => B | C | D | F | G,
        '-' => G,
        '_' => D,
        '=' => D | G,
        '\'' => B,
        '"' => B | F,
        '\u{b0}' => A | B | F | G,
        _ => 0,
    }
}

/// `value` as seven-segment digits across `rect`. Points, commas and
/// colons sit between the digits.
fn digits(value: &str, rect: Rect, cell: &HudCell, lit: Color, unlit: Color) {
    // Each digit with any point after it.
    let mut shown: Vec<(char, bool)> = Vec::with_capacity(value.len());
    for c in value.chars() {
        match (c, shown.last_mut()) {
            ('.' | ',' | ':', Some((_, point))) if !*point => *point = true,
            ('.' | ',' | ':', _) => shown.push((' ', true)),
            _ => shown.push((c, false)),
        }
    }
    if shown.is_empty() {
        return;
    }
    let count = shown.len() as f32;
    let pad = rect.h * 0.14;
    let mut height = (rect.h - pad * 2.)
        * if cell.size > 0. {
            cell.size.min(1.)
        } else {
            1.
        };
    // Digits are about half as wide as tall, with a gap of a fifth.
    let advance = |height: f32| height * 0.62;
    if advance(height) * count > rect.w - pad * 2. {
        height = (rect.w - pad * 2.) / (count * 0.62);
    }
    let width = advance(height) * count;
    let left = match cell.align {
        HudAlign::Left => rect.x + pad,
        HudAlign::Center => rect.x + (rect.w - width) * 0.5,
        HudAlign::Right => rect.right() - pad - width,
    };
    let top = rect.y + (rect.h - height) * 0.5;
    let thick = (height * 0.11).max(1.);
    for (index, &(c, point)) in shown.iter().enumerate() {
        let x = left + advance(height) * index as f32;
        let bits = segments(c);
        digit(vec2(x, top), height * 0.5, height, thick, bits, lit, unlit);
        let dot = Rect::new(x + height * 0.52, top + height - thick, thick, thick);
        draw_rectangle(dot.x, dot.y, dot.w, dot.h, if point { lit } else { unlit });
    }
}

/// One digit `w` wide and `h` tall at `at`, its segments in `bits` lit.
fn digit(at: Vec2, w: f32, h: f32, t: f32, bits: u8, lit: Color, unlit: Color) {
    let half = h * 0.5;
    // Each segment runs between two points; ends are pointed.
    let spans = [
        (vec2(0., 0.), vec2(w, 0.)),
        (vec2(w, 0.), vec2(w, half)),
        (vec2(w, half), vec2(w, h)),
        (vec2(0., h), vec2(w, h)),
        (vec2(0., half), vec2(0., h)),
        (vec2(0., 0.), vec2(0., half)),
        (vec2(0., half), vec2(w, half)),
    ];
    for (index, (from, to)) in spans.into_iter().enumerate() {
        let color = if bits & (1 << index) != 0 { lit } else { unlit };
        segment(at + from, at + to, t, color);
    }
}

/// A bar from `a` to `b`, `t` thick, with pointed ends and a small gap at
/// each so neighbours stay apart.
fn segment(a: Vec2, b: Vec2, t: f32, color: Color) {
    let along = (b - a).normalize_or_zero();
    let side = vec2(-along.y, along.x) * t * 0.5;
    let gap = t * 0.35;
    let (a, b) = (a + along * gap, b - along * gap);
    let reach = along * t * 0.5;
    let (a1, b1) = (a + reach, b - reach);
    draw_triangle(a, a1 + side, a1 - side, color);
    draw_triangle(a1 + side, b1 + side, b1 - side, color);
    draw_triangle(a1 + side, b1 - side, a1 - side, color);
    draw_triangle(b, b1 + side, b1 - side, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_show_numbers_and_common_words() {
        for c in "0123456789-AbCdEFHLoPrUy".chars() {
            assert_ne!(segments(c), 0, "{c}");
        }
        assert_eq!(segments('8'), 0x7f);
        assert_eq!(segments(' '), 0);
        assert_eq!(segments('0'), segments('O'));
        assert_ne!(segments('o'), segments('O'));
    }

    #[test]
    fn cells_land_on_the_grid_and_the_topmost_button_answers() {
        let board = HudBoard {
            columns: 3,
            rows: 2,
            cell: 40.,
            gap: 4.,
            cells: vec![
                HudCell {
                    kind: HudCellKind::Button,
                    at: [0, 0],
                    span: [2, 1],
                    ..HudCell::default()
                },
                HudCell {
                    kind: HudCellKind::Label,
                    at: [2, 0],
                    ..HudCell::default()
                },
                HudCell {
                    kind: HudCellKind::Button,
                    at: [1, 0],
                    span: [1, 2],
                    ..HudCell::default()
                },
            ],
        };
        let rects = cells(&board, vec2(100., 50.), 2.);
        assert_eq!(rects[0], Rect::new(100., 50., 168., 80.));
        assert_eq!(rects[1], Rect::new(276., 50., 80., 80.));
        assert_eq!(rects[2], Rect::new(188., 50., 80., 168.));
        assert_eq!(button_at(&board, &rects, vec2(120., 60.)), Some(0));
        assert_eq!(
            button_at(&board, &rects, vec2(200., 60.)),
            Some(2),
            "on top"
        );
        assert_eq!(button_at(&board, &rects, vec2(300., 60.)), None, "a label");
        assert_eq!(button_at(&board, &rects, vec2(130., 200.)), None, "a gap");
    }
}
