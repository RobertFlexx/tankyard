use macroquad::prelude::*;
use std::{borrow::Cow, cell::Cell};

// Shared colors for the light arcade theme.
pub const BG: Color = Color::new(0.804, 0.804, 0.804, 1.0);
pub const SURFACE: Color = Color::new(0.985, 0.987, 0.99, 1.0);
pub const LINE: Color = Color::new(0.69, 0.72, 0.75, 1.0);
pub const TEXT: Color = Color::new(0.20, 0.23, 0.27, 1.0);
pub const MUTED: Color = Color::new(0.43, 0.48, 0.52, 1.0);
pub const CYAN: Color = Color::new(0.0, 0.698, 0.882, 1.0);
pub const AMBER: Color = Color::new(1.0, 0.91, 0.412, 1.0);
pub const RED: Color = Color::new(0.988, 0.463, 0.467, 1.0);
pub const VIOLET: Color = Color::new(0.749, 0.498, 0.961, 1.0);
pub const STAT_ORDER: [usize; 8] = [1, 0, 2, 3, 4, 5, 6, 7];
pub fn card_color(index: usize) -> Color {
    [CYAN, Color::new(0.66, 0.94, 0.46, 1.0), RED, VIOLET, AMBER][index % 5]
}
pub fn achievement_badge(p: Vec2, radius: f32, color: Color, earned: bool) {
    draw_poly(
        p.x,
        p.y,
        6,
        radius,
        30.0,
        alpha(Color::new(0.333, 0.333, 0.333, 1.0), color.a),
    );
    draw_poly(p.x, p.y, 6, radius * 0.84, 30.0, color);
    if earned {
        let ink = alpha(text_color(), color.a);
        draw_line(
            p.x - radius * 0.36,
            p.y,
            p.x - radius * 0.07,
            p.y + radius * 0.28,
            radius * 0.15,
            ink,
        );
        draw_line(
            p.x - radius * 0.07,
            p.y + radius * 0.28,
            p.x + radius * 0.4,
            p.y - radius * 0.3,
            radius * 0.15,
            ink,
        );
    } else {
        draw_circle(p.x, p.y, radius * 0.14, alpha(muted_color(), color.a));
    }
}
type HoverStates = std::collections::HashMap<(i32, i32, i32, i32), (f32, f64)>;
thread_local! {
    static UI_FONT: std::cell::RefCell<Option<Font>> = const { std::cell::RefCell::new(None) };
    static INTERACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}
pub fn init_font() {
    match load_ttf_font_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans-Bold.ttf")) {
        Ok(font) => UI_FONT.with(|slot| *slot.borrow_mut() = Some(font)),
        Err(error) => log::warn!("Could not load UI font: {error}"),
    }
}
pub fn unload_font() {
    UI_FONT.with(|font| font.borrow_mut().take());
}
pub fn set_interactive(value: bool) -> bool {
    INTERACTIVE.with(|state| state.replace(value))
}
// The main loop sets the theme before drawing menus, the HUD, and overlays.
thread_local! { static DARK_THEME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub fn set_theme(dark: bool) {
    DARK_THEME.with(|theme| theme.set(dark));
}
pub fn is_dark() -> bool {
    DARK_THEME.with(|theme| theme.get())
}
pub fn bg_color() -> Color {
    if is_dark() {
        Color::new(0.12, 0.15, 0.18, 1.0)
    } else {
        BG
    }
}
pub fn surface_color() -> Color {
    if is_dark() {
        Color::new(0.18, 0.21, 0.25, 1.0)
    } else {
        SURFACE
    }
}
pub fn line_color() -> Color {
    if is_dark() {
        Color::new(0.30, 0.34, 0.38, 1.0)
    } else {
        LINE
    }
}
pub fn text_color() -> Color {
    if is_dark() {
        Color::new(0.95, 0.96, 0.97, 1.0)
    } else {
        TEXT
    }
}
pub fn muted_color() -> Color {
    if is_dark() {
        Color::new(0.70, 0.74, 0.78, 1.0)
    } else {
        MUTED
    }
}
pub fn hover_color() -> Color {
    if is_dark() {
        Color::new(0.23, 0.34, 0.39, 1.0)
    } else {
        Color::new(0.85, 0.95, 0.97, 1.0)
    }
}
// Expire old hover states when menus close. Reduced motion skips the transition.
thread_local! {
    static REDUCED_MOTION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static HOVER_STATE: std::cell::RefCell<HoverStates> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}
pub fn set_reduced_motion(reduced: bool) {
    REDUCED_MOTION.with(|state| state.set(reduced));
}
fn hover_ease(rect: Rect, hovered: bool) -> f32 {
    let target = if hovered { 1.0 } else { 0.0 };
    if REDUCED_MOTION.with(|value| value.get()) {
        return target;
    }
    let key = (
        rect.x.round() as i32,
        rect.y.round() as i32,
        rect.w.round() as i32,
        rect.h.round() as i32,
    );
    let time = get_time();
    HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.len() > 512 {
            state.retain(|_, (_, last)| time - *last < 2.0);
        }
        let (value, last) = state.entry(key).or_insert((0.0, time));
        let dt = (time - *last).clamp(0.0, 0.05) as f32;
        *value += (target - *value) * (1.0 - (-dt * 17.0).exp());
        *last = time;
        *value
    })
}
pub fn alpha(color: Color, opacity: f32) -> Color {
    Color {
        a: opacity,
        ..color
    }
}
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
// Rasterize glyphs at their displayed size to keep scaled text sharp.
fn text_raster_size(size: f32) -> u16 {
    size.round().clamp(1.0, 256.0) as u16
}
pub(crate) fn label_width(value: &str, size: f32) -> f32 {
    let pixels = text_raster_size(size);
    UI_FONT.with(|font| {
        measure_text(
            value,
            font.borrow().as_ref(),
            pixels,
            size / f32::from(pixels),
        )
        .width
    })
}
// Keep long names, paths and server addresses out of their neighbors.
pub(crate) fn fitted_label(value: &str, size: f32, width: f32) -> Cow<'_, str> {
    if width <= 0.0 {
        return Cow::Borrowed("");
    }
    if label_width(value, size) <= width {
        return Cow::Borrowed(value);
    }
    const DOTS: &str = "…";
    let dots_width = label_width(DOTS, size);
    if dots_width > width {
        return Cow::Borrowed("");
    }
    let mut fitted = String::new();
    for c in value.chars() {
        fitted.push(c);
        if label_width(&fitted, size) + dots_width > width {
            fitted.pop();
            break;
        }
    }
    fitted.push_str(DOTS);
    Cow::Owned(fitted)
}

pub fn label(value: &str, x: f32, y: f32, size: f32, color: Color) {
    let pixels = text_raster_size(size);
    UI_FONT.with(|font| {
        draw_text_ex(
            value,
            x.round(),
            y.round(),
            TextParams {
                font: font.borrow().as_ref(),
                font_size: pixels,
                font_scale: size / f32::from(pixels),
                color,
                ..Default::default()
            },
        )
    });
}
pub fn right_label(value: &str, x: f32, y: f32, size: f32, color: Color) {
    let width = label_width(value, size);
    label(value, x - width, y, size, color);
}
pub fn center_label(value: &str, x: f32, y: f32, size: f32, color: Color) {
    let width = label_width(value, size);
    label(value, x - width * 0.5, y, size, color);
}
pub fn panel(rect: Rect) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, alpha(surface_color(), 0.92));
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.5, line_color());
}
pub fn contains(rect: Rect) -> bool {
    if !INTERACTIVE.with(|state| state.get()) {
        return false;
    }
    let (x, y) = mouse_position();
    rect.contains(vec2(x, y))
}
pub fn button(rect: Rect, text: &str, selected: bool, enabled: bool, s: f32) -> bool {
    let hovered = enabled && contains(rect);
    let t = hover_ease(rect, hovered);
    let base = if !enabled {
        mix(surface_color(), bg_color(), 0.55)
    } else if selected {
        if is_dark() {
            Color::new(0.11, 0.40, 0.50, 1.0)
        } else {
            Color::new(0.43, 0.85, 0.85, 1.0)
        }
    } else {
        surface_color()
    };
    let fill = if enabled {
        mix(base, hover_color(), t * if selected { 0.20 } else { 0.85 })
    } else {
        base
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        2.0,
        if selected {
            CYAN
        } else {
            mix(line_color(), CYAN, t)
        },
    );
    let text_color = if enabled { text_color() } else { muted_color() };
    let mut size = 19.0 * s;
    let available = (rect.w - 18.0 * s).max(12.0 * s);
    while size > 11.0 * s && label_width(text, size) > available {
        size -= 1.0 * s;
    }
    let fitted = fitted_label(text, size, available);
    center_label(
        &fitted,
        rect.x + rect.w * 0.5,
        rect.y + rect.h * 0.5 + size * 0.31,
        size,
        text_color,
    );
    hovered && is_mouse_button_pressed(MouseButton::Left)
}
pub fn small_button(rect: Rect, text: &str, s: f32) -> bool {
    let hovered = contains(rect);
    let t = hover_ease(rect, hovered);
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        mix(surface_color(), hover_color(), t),
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.5,
        mix(line_color(), CYAN, t),
    );
    center_label(
        &fitted_label(text, 16.0 * s, (rect.w - 12.0 * s).max(0.0)),
        rect.x + rect.w * 0.5,
        rect.y + rect.h * 0.5 + 5.0 * s,
        16.0 * s,
        text_color(),
    );
    hovered && is_mouse_button_pressed(MouseButton::Left)
}
pub fn toggle(rect: Rect, text: &str, value: &mut bool, s: f32) -> bool {
    let click = contains(rect) && is_mouse_button_pressed(MouseButton::Left);
    if click {
        *value = !*value;
    }
    label(
        &fitted_label(text, 18.0 * s, (rect.w - 67.0 * s).max(0.0)),
        rect.x,
        rect.y + 22.0 * s,
        18.0 * s,
        text_color(),
    );
    let bx = rect.x + rect.w - 40.0 * s;
    draw_rectangle(
        bx,
        rect.y + 6.0 * s,
        38.0 * s,
        20.0 * s,
        if *value {
            alpha(CYAN, 0.3)
        } else {
            line_color()
        },
    );
    draw_circle(
        bx + if *value { 28.0 * s } else { 10.0 * s },
        rect.y + 16.0 * s,
        6.0 * s,
        if *value { CYAN } else { muted_color() },
    );
    click
}
pub fn slider(rect: Rect, text: &str, value: &mut f32, lo: f32, hi: f32, s: f32) -> bool {
    slider_value(rect, text, value, [lo, hi], 0., s, |value| {
        let percent = if (lo - 0.8).abs() < f32::EPSILON && (hi - 1.4).abs() < f32::EPSILON {
            value * 100.
        } else {
            (value - lo) / (hi - lo) * 100.
        };
        format!("{percent:.0}%")
    })
}
thread_local! {
    static ACTIVE_SLIDER: Cell<Option<[u32; 4]>> = const { Cell::new(None) };
}
pub fn slider_value(
    rect: Rect,
    text: &str,
    value: &mut f32,
    limits: [f32; 2],
    step: f32,
    s: f32,
    format: impl FnOnce(f32) -> String,
) -> bool {
    let [lo, hi] = limits;
    let track = Rect::new(rect.x, rect.y + 32. * s, rect.w, 5. * s);
    let hit = Rect::new(rect.x, rect.y + 24. * s, rect.w, 24. * s);
    let id = [
        rect.x.to_bits(),
        rect.y.to_bits(),
        rect.w.to_bits(),
        rect.h.to_bits(),
    ];
    let interactive = INTERACTIVE.with(|state| state.get());
    let dragged = ACTIVE_SLIDER.with(|active| {
        if !is_mouse_button_down(MouseButton::Left) {
            active.set(None);
        }
        if contains(hit) && is_mouse_button_pressed(MouseButton::Left) {
            active.set(Some(id));
        }
        interactive && active.get() == Some(id) && is_mouse_button_down(MouseButton::Left)
    });
    if dragged {
        let raw = lo + (hi - lo) * ((mouse_position().0 - track.x) / track.w.max(1.)).clamp(0., 1.);
        *value = if step > 0. {
            (raw / step).round().clamp(lo, hi)
        } else {
            raw
        };
    }
    let display = format(*value);
    let available = (rect.w - label_width(&display, 16. * s) - 12. * s).max(0.);
    label(
        &fitted_label(text, 18. * s, available),
        rect.x,
        rect.y + 18. * s,
        18. * s,
        text_color(),
    );
    right_label(
        &display,
        rect.x + rect.w,
        rect.y + 18. * s,
        16. * s,
        muted_color(),
    );
    let fraction = ((*value - lo) / (hi - lo)).clamp(0., 1.);
    draw_rectangle(track.x, track.y, track.w, track.h, line_color());
    draw_rectangle(track.x, track.y, track.w * fraction, track.h, CYAN);
    draw_circle(
        track.x + track.w * fraction,
        track.y + 2.5 * s,
        5. * s,
        CYAN,
    );
    dragged
}

#[derive(Default)]
struct FieldState {
    editor: tankyard_client::text_edit::Editor,
    repeat: tankyard_client::text_edit::KeyRepeat,
    active: bool,
    scroll: usize,
    touched: f64,
    edited: f64,
    clicked: f64,
    dragging: bool,
}
thread_local! {
    static FIELDS: std::cell::RefCell<std::collections::HashMap<usize, FieldState>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}
fn field_text(value: &str, secret: bool) -> String {
    if secret {
        "•".repeat(value.chars().count())
    } else {
        value.to_owned()
    }
}
fn field_offset(value: &str, at: usize, secret: bool) -> usize {
    if secret {
        value[..at].chars().count() * '•'.len_utf8()
    } else {
        at
    }
}
fn field_view(
    shown: &str,
    caret: usize,
    scroll: &mut usize,
    width: f32,
    size: f32,
) -> std::ops::Range<usize> {
    *scroll = (*scroll).min(shown.len());
    while !shown.is_char_boundary(*scroll) {
        *scroll -= 1;
    }
    *scroll = (*scroll).min(caret);
    if label_width(shown, size) <= width {
        *scroll = 0;
    }
    while *scroll < caret && label_width(&shown[*scroll..caret], size) > width {
        *scroll += shown[*scroll..].chars().next().unwrap().len_utf8();
    }
    while *scroll > 0 {
        let previous = shown[..*scroll].char_indices().next_back().unwrap().0;
        if label_width(&shown[previous..], size) > width {
            break;
        }
        *scroll = previous;
    }
    let mut end = *scroll;
    for c in shown[*scroll..].chars() {
        let next = end + c.len_utf8();
        if label_width(&shown[*scroll..next], size) > width {
            break;
        }
        end = next;
    }
    *scroll..end
}
fn field_hit(shown: &str, visible: std::ops::Range<usize>, x: f32, size: f32) -> usize {
    let mut previous = 0.;
    for (at, c) in shown[visible.clone()].char_indices() {
        let next = visible.start + at + c.len_utf8();
        let width = label_width(&shown[visible.start..next], size);
        if x < (previous + width) * 0.5 {
            return visible.start + at;
        }
        previous = width;
    }
    visible.end
}
pub fn text_field(
    rect: Rect,
    value: &mut String,
    active: &mut bool,
    max: usize,
    s: f32,
    secret: bool,
) {
    use tankyard_client::text_edit::{Edit, KeyRepeat, text_character};
    let interactive = INTERACTIVE.with(|state| state.get());
    let clicked = interactive && is_mouse_button_pressed(MouseButton::Left) && contains(rect);
    if is_mouse_button_pressed(MouseButton::Left) && interactive {
        *active = clicked;
    }
    let editing = *active && interactive;
    let now = get_time();
    let size = 21.0 * s;
    let width = (rect.w - 27.0 * s).max(0.0);
    let padding = 12.0 * s;
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    let alt = is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt);
    let command = !alt
        && [
            KeyCode::LeftControl,
            KeyCode::RightControl,
            KeyCode::LeftSuper,
            KeyCode::RightSuper,
        ]
        .into_iter()
        .any(is_key_down);
    FIELDS.with(|fields| {
        let mut fields = fields.borrow_mut();
        if fields.len() > 64 {
            fields.retain(|_, field| now - field.touched < 30.);
        }
        let field = fields.entry(value as *const String as usize).or_default();
        value.retain(text_character);
        field.editor.sync(value);
        if editing && !field.active {
            field.editor.place(value, value.len(), false);
            field.repeat = KeyRepeat::default();
            field.edited = now;
        }
        if !editing {
            field.scroll = 0;
            field.dragging = false;
        }
        if editing && (clicked || (field.dragging && is_mouse_button_down(MouseButton::Left))) {
            let shown = field_text(value, secret);
            let caret = field_offset(value, field.editor.cursor, secret);
            let visible = field_view(&shown, caret, &mut field.scroll, width, size);
            let hit = field_hit(&shown, visible, mouse_position().0 - rect.x - padding, size);
            let hit = if secret {
                value
                    .char_indices()
                    .nth(shown[..hit].chars().count())
                    .map_or(value.len(), |(at, _)| at)
            } else {
                hit
            };
            field.editor.place(value, hit, shift || !clicked);
            if clicked && field.active && now - field.clicked < 0.3 {
                field.editor.select_word(value);
                field.dragging = false;
            } else if clicked {
                field.dragging = true;
            }
            if clicked {
                field.clicked = now;
            }
            field.edited = now;
        }
        if !is_mouse_button_down(MouseButton::Left) {
            field.dragging = false;
        }
        if editing {
            if command && is_key_pressed(KeyCode::A) {
                field.editor.edit(value, Edit::SelectAll, false, false);
                field.edited = now;
            }
            if command
                && !secret
                && (is_key_pressed(KeyCode::C) || is_key_pressed(KeyCode::X))
                && !field.editor.selection().is_empty()
            {
                miniquad::window::clipboard_set(field.editor.selected(value));
                if is_key_pressed(KeyCode::X) {
                    field.editor.edit(value, Edit::Backspace, false, false);
                }
            }
            if command && is_key_pressed(KeyCode::V) {
                if let Some(clipboard) = miniquad::window::clipboard_get() {
                    field.editor.replace(value, &clipboard, max);
                }
                field.edited = now;
            }
            for (index, (key, edit)) in [
                (KeyCode::Left, Edit::Left),
                (KeyCode::Right, Edit::Right),
                (KeyCode::Backspace, Edit::Backspace),
                (KeyCode::Delete, Edit::Delete),
            ]
            .into_iter()
            .enumerate()
            {
                let repeats =
                    field
                        .repeat
                        .pulses(index, is_key_down(key), is_key_pressed(key), now);
                for _ in 0..repeats {
                    field.editor.edit(value, edit, shift, command);
                }
                if repeats > 0 {
                    field.edited = now;
                }
            }
            for (key, edit) in [(KeyCode::Home, Edit::Home), (KeyCode::End, Edit::End)] {
                if is_key_pressed(key) {
                    field.editor.edit(value, edit, shift, false);
                    field.edited = now;
                }
            }
            let mut typed = String::new();
            while let Some(c) = get_char_pressed() {
                if !command {
                    typed.push(c);
                }
            }
            if field.editor.replace(value, &typed, max) {
                field.edited = now;
            }
        }
        field.active = editing;
        field.touched = now;
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color());
        draw_rectangle_lines(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            1.0,
            if editing { CYAN } else { line_color() },
        );
        let shown = field_text(value, secret);
        let caret = field_offset(value, field.editor.cursor, secret);
        let visible = field_view(
            &shown,
            if editing { caret } else { 0 },
            &mut field.scroll,
            width,
            size,
        );
        if editing {
            let selection = field.editor.selection();
            let left = field_offset(value, selection.start, secret).max(visible.start);
            let right = field_offset(value, selection.end, secret).min(visible.end);
            if left < right {
                let offset = label_width(&shown[visible.start..left], size);
                let selected_width = label_width(&shown[left..right], size);
                draw_rectangle(
                    rect.x + padding + offset,
                    rect.y + 6.0 * s,
                    selected_width,
                    rect.h - 12.0 * s,
                    alpha(CYAN, 0.28),
                );
            }
        }
        label(
            &shown[visible.clone()],
            rect.x + padding,
            rect.y + rect.h * 0.5 + 7.0 * s,
            size,
            text_color(),
        );
        if editing && (now - field.edited).rem_euclid(1.) < 0.55 {
            let offset = label_width(&shown[visible.start..caret], size).min(width);
            draw_rectangle(
                rect.x + padding + offset,
                rect.y + 9.0 * s,
                1.5 * s,
                rect.h - 18.0 * s,
                CYAN,
            );
        }
    });
}
pub fn wrap(value: &str, x: f32, mut y: f32, width: f32, size: f32, color: Color) -> f32 {
    if width <= 0.0 || size <= 0.0 {
        return y;
    }
    let mut line = String::new();
    for word in value.split_whitespace() {
        let proposed = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if label_width(&proposed, size) <= width {
            line = proposed;
            continue;
        }
        if !line.is_empty() {
            label(&line, x, y, size, color);
            y += size * 1.35;
            line.clear();
        }
        // Server addresses and filesystem paths have no spaces. Wrap on UTF-8
        // boundaries instead of letting that shit spill into another widget.
        for c in word.chars() {
            line.push(c);
            if line.len() > c.len_utf8() && label_width(&line, size) > width {
                line.pop();
                label(&line, x, y, size, color);
                y += size * 1.35;
                line.clear();
                line.push(c);
            }
        }
    }
    if !line.is_empty() {
        label(&line, x, y, size, color);
        y += size * 1.35;
    }
    y
}
pub fn wrapped_lines(value: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        let proposed = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if label_width(&proposed, size) <= width {
            line = proposed;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for ch in word.chars() {
            line.push(ch);
            if line.len() > ch.len_utf8() && label_width(&line, size) > width {
                line.pop();
                lines.push(std::mem::take(&mut line));
                line.push(ch);
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
pub fn progress(rect: Rect, ratio: f32, color: Color) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, line_color());
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w * ratio.clamp(0.0, 1.0),
        rect.h,
        color,
    );
}

/// Outlined arcade lettering for names and headings. Each outline adds draw calls.
pub fn arcade_center(value: &str, x: f32, y: f32, size: f32) {
    let width = label_width(value, size);
    let px = x - width * 0.5;
    let ink = if is_dark() {
        Color::new(0.07, 0.09, 0.11, 1.0)
    } else {
        Color::new(0.22, 0.22, 0.23, 1.0)
    };
    let edge = (size * 0.065).clamp(1.0, 2.7);
    for (dx, dy) in [(-edge, 0.0), (edge, 0.0), (0.0, -edge), (0.0, edge)] {
        label(value, px + dx, y + dy, size, ink);
    }
    label(value, px, y, size, WHITE);
}

pub fn arcade_label(value: &str, x: f32, y: f32, size: f32) {
    let ink = Color::new(0.15, 0.15, 0.16, 1.0);
    let edge = (size * 0.06).clamp(1.0, 2.3);
    for (dx, dy) in [(-edge, 0.0), (edge, 0.0), (0.0, -edge), (0.0, edge)] {
        label(value, x + dx, y + dy, size, ink);
    }
    label(value, x, y, size, WHITE);
}

pub fn capsule(rect: Rect, color: Color) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    let cap = (rect.h * 0.5).min(rect.w * 0.5);
    draw_rectangle(
        rect.x + cap,
        rect.y,
        (rect.w - cap * 2.0).max(0.0),
        rect.h,
        color,
    );
    draw_circle(rect.x + cap, rect.y + rect.h * 0.5, cap, color);
    draw_circle(rect.x + rect.w - cap, rect.y + rect.h * 0.5, cap, color);
}
/// Rounded tracks scale their border with the actual framebuffer size.
pub fn arcade_track(rect: Rect, ratio: f32, fill: Color) {
    arcade_track_tinted(rect, ratio, fill, Color::new(0.40, 0.40, 0.40, 1.0));
}
pub fn arcade_track_tinted(rect: Rect, ratio: f32, fill: Color, back: Color) {
    let edge = (rect.h * 0.16).clamp(0.7, 3.0);
    capsule(rect, Color::new(0.333, 0.333, 0.333, 1.0));
    let inner = Rect::new(
        rect.x + edge,
        rect.y + edge,
        (rect.w - edge * 2.0).max(0.0),
        (rect.h - edge * 2.0).max(0.0),
    );
    capsule(inner, back);
    let width = inner.w * ratio.clamp(0.0, 1.0);
    if width > 0.1 {
        capsule(
            Rect::new(inner.x, inner.y, width.max(inner.h).min(inner.w), inner.h),
            fill,
        );
    }
}
