//! The dock under the canvas: tool modes, the build palette, the edit
//! buttons, delete tools and view switches.
use super::{Editor, Layout, Mode, brushes, shift};
use crate::{settings::Settings, ui::*};
use macroquad::prelude::*;
use tankyard_client::editor_document::{Align, Key, Transform};
use tankyard_content::BlockStyle;

/// Nudge steps: one world unit, half a cell, a cell, four cells.
pub const STEPS: [&str; 4] = ["1 unit", "Half cell", "1 cell", "4 cells"];

/// A row of equal buttons filling `width`; returns the clicked index.
fn row(
    x: f32,
    y: f32,
    width: f32,
    h: f32,
    labels: &[&str],
    enabled: bool,
    s: f32,
) -> Option<usize> {
    let gap = 5. * s;
    let w = (width - gap * (labels.len() as f32 - 1.)) / labels.len() as f32;
    let mut clicked = None;
    for (i, text) in labels.iter().enumerate() {
        if button(
            Rect::new(x + i as f32 * (w + gap), y, w, h),
            text,
            false,
            enabled,
            s * 0.7,
        ) {
            clicked = Some(i);
        }
    }
    clicked
}

impl Editor {
    pub(super) fn dock(&mut self, layout: &Layout, settings: &Settings) {
        let r = layout.dock;
        let s = layout.s;
        draw_rectangle(r.x, r.y, r.w, r.h, surface_color());
        draw_line(r.x, r.y, r.right(), r.y, 1.5, line_color());
        let pad = 10. * s;
        let modes = Rect::new(r.x + pad, r.y + pad, 104. * s, r.h - pad * 2.);
        let h = (modes.h - 15. * s) / 4.;
        for (i, (title, mode)) in [
            ("Build", Mode::Build),
            ("Edit", Mode::Edit),
            ("Delete", Mode::Delete),
            ("Pan", Mode::Pan),
        ]
        .into_iter()
        .enumerate()
        {
            let b = Rect::new(modes.x, modes.y + i as f32 * (h + 5. * s), modes.w, h);
            if button(b, title, self.mode == mode, true, s * 0.85) {
                self.mode = mode;
            }
            right_label(
                &(i + 1).to_string(),
                b.right() - 6. * s,
                b.y + 13. * s,
                10. * s,
                muted_color(),
            );
        }
        let switches = Rect::new(r.right() - 250. * s, r.y + pad, 240. * s, r.h - pad * 2.);
        self.switches(switches, layout.canvas.center(), s);
        let content = Rect::new(
            modes.right() + 14. * s,
            r.y + pad,
            switches.x - modes.right() - 28. * s,
            r.h - pad * 2.,
        );
        draw_line(
            content.x - 7. * s,
            r.y + pad,
            content.x - 7. * s,
            r.bottom() - pad,
            1.,
            line_color(),
        );
        draw_line(
            switches.x - 7. * s,
            r.y + pad,
            switches.x - 7. * s,
            r.bottom() - pad,
            1.,
            line_color(),
        );
        match self.mode {
            Mode::Build => self.palette(content, settings, s),
            Mode::Edit => self.edit_buttons(content, s),
            Mode::Delete => self.delete_tools(content, s),
            Mode::Pan => self.navigation_controls(content, layout.canvas.center(), s),
        }
    }

    fn navigation_controls(&mut self, r: Rect, center: Vec2, s: f32) {
        let h = ((r.h - 12. * s) / 3.).min(32. * s);
        let half = (r.w - 10. * s) * 0.5;
        let scroll = self.prefs.scroll;
        if small_button(
            Rect::new(r.x, r.y, half, h),
            &format!("Scroll: {}   \u{203a}", scroll.name()),
            s * 0.72,
        ) {
            let modes = super::panels::ScrollMode::ALL;
            let at = modes.iter().position(|&m| m == scroll).unwrap_or(0);
            let step = if shift() { modes.len() - 1 } else { 1 };
            self.prefs.scroll = modes[(at + step) % modes.len()];
        }
        if small_button(
            Rect::new(r.x + half + 10. * s, r.y, half, h),
            "100% zoom",
            s * 0.72,
        ) {
            self.zoom_at(center, 1. / self.view.zoom);
        }
        for (index, (title, value)) in [
            ("Pan", &mut self.prefs.pan_speed),
            ("Zoom", &mut self.prefs.zoom_speed),
        ]
        .into_iter()
        .enumerate()
        {
            let x = r.x + index as f32 * (half + 10. * s);
            let y = r.y + h + 6. * s;
            label(
                &format!("{title} {:.2}×", *value),
                x,
                y + h * 0.7,
                12. * s,
                text_color(),
            );
            let button_w = (half * 0.22).min(40. * s);
            if small_button(
                Rect::new(x + half - button_w * 2. - 4. * s, y, button_w, h),
                "−",
                s * 0.7,
            ) {
                *value = (*value - 0.25).max(0.25);
            }
            if small_button(Rect::new(x + half - button_w, y, button_w, h), "+", s * 0.7) {
                *value = (*value + 0.25).min(3.);
            }
        }
        label(
            match self.prefs.scroll {
                super::panels::ScrollMode::Auto => {
                    "Trackpads pan and wheels zoom. Ctrl/Cmd-scroll always zooms."
                }
                super::panels::ScrollMode::Trackpad => {
                    "Two-finger scroll pans. Ctrl/Cmd-scroll zooms. Shift-scroll pans across."
                }
                super::panels::ScrollMode::Mouse => {
                    "Scroll zooms. Drag with Pan or hold Space to move the view."
                }
            },
            r.x,
            r.y + (h + 6. * s) * 2. + h * 0.7,
            11. * s,
            muted_color(),
        );
    }
    fn switches(&mut self, r: Rect, canvas_center: Vec2, s: f32) {
        let h = (r.h - 15. * s) / 4.;
        let w = (r.w - 8. * s) * 0.5;
        let cell = |col: usize, line: usize| {
            Rect::new(
                r.x + col as f32 * (w + 8. * s),
                r.y + line as f32 * (h + 5. * s),
                w,
                h,
            )
        };
        toggle(cell(0, 0), "Swipe", &mut self.swipe, s * 0.72);
        toggle(cell(1, 0), "Snap", &mut self.snap, s * 0.72);
        if small_button(cell(0, 1), &format!("Grid {}", self.grid as u32), s * 0.72) {
            self.grid = next_grid(self.grid, shift());
        }
        toggle(cell(1, 1), "Hidden", &mut self.show_hidden, s * 0.72);
        toggle(cell(0, 2), "Groups", &mut self.show_groups, s * 0.72);
        toggle(cell(1, 2), "Links", &mut self.show_links, s * 0.72);
        let third = (r.w - 10. * s) / 3.;
        let y = r.y + 3. * (h + 5. * s);
        if small_button(Rect::new(r.x, y, third, h), "Zoom −", s * 0.72) {
            self.zoom_at(canvas_center, 1. / 1.25);
        }
        if small_button(
            Rect::new(r.x + third + 5. * s, y, third, h),
            "Zoom +",
            s * 0.72,
        ) {
            self.zoom_at(canvas_center, 1.25);
        }
        if small_button(
            Rect::new(r.x + (third + 5. * s) * 2., y, third, h),
            "Fit",
            s * 0.72,
        ) {
            self.fitted = false;
        }
    }
    /// The group new objects join, with a shortcut to an unused one.
    fn group_chooser(&mut self, r: Rect, s: f32) {
        label("Group", r.x, r.y + r.h * 0.66, 13. * s, muted_color());
        let x = r.x + 46. * s;
        let w = 26. * s;
        if button(
            Rect::new(x, r.y, w, r.h),
            "−",
            false,
            self.build_group > 0,
            s * 0.8,
        ) {
            self.build_group -= 1;
        }
        let shown = if self.build_group == 0 {
            "none".to_owned()
        } else {
            self.build_group.to_string()
        };
        center_label(
            &shown,
            x + w + 22. * s,
            r.y + r.h * 0.68,
            14. * s,
            text_color(),
        );
        if button(
            Rect::new(x + w + 44. * s, r.y, w, r.h),
            "+",
            false,
            self.build_group < tankyard_content::MAP_GROUPS,
            s * 0.8,
        ) {
            self.build_group += 1;
        }
        let free = self.doc.next_free_group();
        if button(
            Rect::new(x + w * 2. + 50. * s, r.y, 52. * s, r.h),
            "Free",
            false,
            free.is_some(),
            s * 0.7,
        ) && let Some(group) = free
        {
            self.build_group = group;
            self.say(format!(
                "New objects join group {group}; new triggers act on it."
            ));
        }
    }
    fn palette(&mut self, r: Rect, settings: &Settings, s: f32) {
        let tab_h = 26. * s;
        let chooser = 210. * s;
        let tab_w = ((r.w - chooser - 10. * s) / brushes::CATEGORIES.len() as f32).min(110. * s);
        for (i, name) in brushes::CATEGORIES.into_iter().enumerate() {
            if button(
                Rect::new(r.x + i as f32 * (tab_w + 4. * s), r.y, tab_w, tab_h),
                name,
                self.category == i,
                true,
                s * 0.7,
            ) {
                self.category = i;
                self.brush = 0;
                self.stamp = None;
            }
        }
        self.group_chooser(
            Rect::new(r.right() - chooser + 10. * s, r.y, chooser, tab_h),
            s,
        );
        let area = Rect::new(r.x, r.y + tab_h + 8. * s, r.w, r.h - tab_h - 8. * s);
        let count = self.brushes[self.category].len();
        let rows = if count as f32 * 64. * s > area.w {
            2
        } else {
            1
        };
        let per_row = count.div_ceil(rows);
        let tile = ((area.h - 6. * s * (rows as f32 - 1.)) / rows as f32)
            .min((area.w - 6. * s * (per_row as f32 - 1.)) / per_row as f32)
            .min(84. * s);
        let pointer = crate::ui_layout::pointer();
        let mut hovered = None;
        for i in 0..count {
            let (line, col) = (i / per_row, i % per_row);
            let t = Rect::new(
                area.x + col as f32 * (tile + 6. * s),
                area.y + line as f32 * (tile + 6. * s),
                tile,
                tile,
            );
            let chosen = self.stamp.is_none() && self.brush == i;
            let over = contains(t);
            if over {
                hovered = Some(i);
            }
            draw_rectangle(
                t.x,
                t.y,
                t.w,
                t.h,
                if chosen {
                    alpha(accent(), 0.16)
                } else if over {
                    hover_color()
                } else {
                    mix(surface_color(), bg_color(), 0.35)
                },
            );
            draw_rectangle_lines(
                t.x,
                t.y,
                t.w,
                t.h,
                if chosen { 2. } else { 1. },
                if chosen { accent() } else { line_color() },
            );
            let picture = Rect::new(t.x, t.y, t.w, t.h - 14. * s);
            let brush = &self.brushes[self.category][i];
            brushes::thumbnail(brush, picture, &self.doc.map, settings, self.time);
            // Two rows of smaller tiles get a smaller name to fit it whole.
            let size = if rows > 1 { 8.8 * s } else { 10. * s };
            center_label(
                &fitted_label(brush.name, size, t.w - 4. * s),
                t.center().x,
                t.bottom() - 4. * s,
                size,
                text_color(),
            );
            if over && is_mouse_button_pressed(MouseButton::Left) {
                self.brush = i;
                self.stamp = None;
                self.mode = Mode::Build;
            }
        }
        if let Some(i) = hovered
            && let hint = self.brushes[self.category][i].hint
            && !hint.is_empty()
        {
            let size = 12. * s;
            let width = label_width(hint, size) + 16. * s;
            let x = (pointer.x - width * 0.5).clamp(4., screen_width() - width - 4.);
            let y = r.y - size - 22. * s;
            draw_rectangle(x, y, width, size + 12. * s, alpha(text_color(), 0.9));
            label(hint, x + 8. * s, y + size + 3. * s, size, surface_color());
        }
    }
    fn edit_buttons(&mut self, r: Rect, s: f32) {
        let any = !self.selection.is_empty();
        let h = (r.h - 12. * s) / 3.;
        let half = (r.w - 12. * s) * 0.5;
        let step_w = 96. * s;
        if let Some(i) = row(
            r.x,
            r.y,
            half - step_w - 6. * s,
            h,
            &["←", "↑", "↓", "→"],
            any,
            s,
        ) {
            let (dx, dy) = [(-1., 0.), (0., -1.), (0., 1.), (1., 0.)][i];
            self.nudge_by(dx, dy, false);
        }
        if small_button(
            Rect::new(r.x + half - step_w, r.y, step_w, h),
            STEPS[self.step],
            s * 0.68,
        ) {
            self.step = (self.step + if shift() { STEPS.len() - 1 } else { 1 }) % STEPS.len();
        }
        if let Some(i) = row(
            r.x + half + 12. * s,
            r.y,
            half,
            h,
            &[
                "Turn left",
                "Turn right",
                "Flip X",
                "Flip Y",
                "Smaller",
                "Larger",
            ],
            any,
            s,
        ) {
            match i {
                0 => self.transform(Transform::TurnLeft),
                1 => self.transform(Transform::TurnRight),
                2 => self.transform(Transform::FlipX),
                3 => self.transform(Transform::FlipY),
                4 => self.scale(0.8),
                _ => self.scale(1.25),
            }
        }
        let y = r.y + h + 6. * s;
        // Aligning needs two objects; ordering works on one.
        let eighth = (r.w - 5. * s * 7.) / 8.;
        if let Some(i) = row(
            r.x,
            y,
            eighth * 6. + 5. * s * 5.,
            h,
            &[
                "Align left",
                "Center X",
                "Align right",
                "Align top",
                "Center Y",
                "Align bottom",
            ],
            self.selection.len() > 1,
            s,
        ) {
            let align = [
                Align::Left,
                Align::CenterX,
                Align::Right,
                Align::Top,
                Align::CenterY,
                Align::Bottom,
            ][i];
            self.apply(|editor| {
                let keys = editor.selection.clone();
                editor.doc.align(&keys, align);
            });
        }
        if let Some(i) = row(
            r.x + (eighth + 5. * s) * 6.,
            y,
            eighth * 2. + 5. * s,
            h,
            &["To front", "To back"],
            any,
            s,
        ) {
            self.order(i == 0);
        }
        let y = y + h + 6. * s;
        let pasting = !self.clipboard.is_empty();
        let labels = [
            "Copy",
            "Paste",
            "Duplicate",
            "Copy style",
            "Paste style",
            "Select group",
            "Select kind",
            "New group",
            "Deselect",
        ];
        let gap = 5. * s;
        let w = (r.w - gap * (labels.len() as f32 - 1.)) / labels.len() as f32;
        for (i, text) in labels.into_iter().enumerate() {
            let enabled = match i {
                1 => pasting,
                4 => self.style_clipboard.is_some() && any,
                _ => any,
            };
            if button(
                Rect::new(r.x + i as f32 * (w + gap), y, w, h),
                text,
                false,
                enabled,
                s * 0.7,
            ) {
                match i {
                    0 => self.copy(),
                    1 => self.paste(None),
                    2 => self.duplicate(),
                    3 => self.copy_style(),
                    4 => self.paste_style(),
                    5 => self.select_group(),
                    6 => self.select_kind(),
                    7 => self.new_group(),
                    _ => self.selection.clear(),
                }
            }
        }
    }
    fn delete_tools(&mut self, r: Rect, s: f32) {
        let h = 30. * s;
        let (object, _) = self.current_brush();
        let kind = if let tankyard_client::editor_document::Object::Block(b) = object {
            if b.style == BlockStyle::Map {
                "map-style blocks".to_owned()
            } else {
                format!("{} blocks", b.style.name())
            }
        } else {
            format!("{}s", object.name().to_lowercase())
        };
        let count = self.selection.len();
        let labels = [
            format!("Delete selected ({count})"),
            format!("Delete all {kind}"),
            if self.build_group > 0 {
                format!("Delete group {}", self.build_group)
            } else {
                "Delete group…".to_owned()
            },
        ];
        let gap = 6. * s;
        let w = (r.w - 210. * s - gap * 3.) / 3.;
        for (i, text) in labels.iter().enumerate() {
            let enabled = match i {
                0 => count > 0,
                2 => self.build_group > 0,
                _ => true,
            };
            if button(
                Rect::new(r.x + i as f32 * (w + gap), r.y, w, h),
                text,
                false,
                enabled,
                s * 0.72,
            ) {
                let keys: Vec<Key> = match i {
                    0 => self.selection.clone(),
                    1 => self.matching_brush(),
                    _ => self.doc.in_group(self.build_group),
                };
                let deleted = keys.len();
                self.apply(|editor| {
                    editor.doc.delete(&keys);
                    editor.selection.clear();
                });
                self.say(format!(
                    "Deleted {deleted} {}. Ctrl+Z brings them back.",
                    if deleted == 1 { "object" } else { "objects" }
                ));
            }
        }
        self.group_chooser(Rect::new(r.right() - 200. * s, r.y, 200. * s, h), s);
        wrap(
            "Click an object to delete it, or drag across empty floor to delete a region. With Swipe on, everything the pointer crosses goes. The Build palette chooses which kind “Delete all” removes.",
            r.x,
            r.y + h + 24. * s,
            r.w,
            13. * s,
            muted_color(),
        );
    }
    /// Every object like the current brush; blocks match by style.
    fn matching_brush(&self) -> Vec<Key> {
        let (brush, _) = self.current_brush();
        self.doc
            .items()
            .map(|(key, _, _)| key)
            .filter(|&key| {
                self.doc
                    .object(key)
                    .is_some_and(|object| match (&object, brush) {
                        (
                            tankyard_client::editor_document::Object::Block(a),
                            tankyard_client::editor_document::Object::Block(b),
                        ) => a.style == b.style,
                        _ => object.same_kind(brush),
                    })
            })
            .collect()
    }
}

/// Grid sizes in world units.
pub const GRIDS: [f32; 6] = [10., 20., 40., 80., 160., 320.];
pub fn next_grid(grid: f32, back: bool) -> f32 {
    let index = GRIDS.iter().position(|&g| g == grid).unwrap_or(2);
    GRIDS[if back {
        (index + GRIDS.len() - 1) % GRIDS.len()
    } else {
        (index + 1) % GRIDS.len()
    }]
}
