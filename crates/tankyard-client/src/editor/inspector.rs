//! The inspector beside the canvas: map settings, the selected objects'
//! properties, trigger logic and its preview, checks, rules and files.
use super::{Editor, Layout, Mode, Tab, brushes, directory, read_document};
use crate::{
    ui::{AMBER, RED, VIOLET, *},
    ui_layout,
};
use macroquad::prelude::*;
// The inspector scrolls, so shapes go through the helpers that follow it.
use super::preview::GateView;
use crate::ui::{
    ui_circle as draw_circle, ui_line as draw_line, ui_rectangle as draw_rectangle,
    ui_rectangle_lines as draw_rectangle_lines,
};
use std::hash::{Hash, Hasher};
use tankyard_client::editor_document::{
    Document, Key, Object, defeated_boss, describe_trigger_named, map_id, trigger_tag,
};
use tankyard_content::*;
use tankyard_core::BossKind;

const SWATCHES: [[u8; 3]; 12] = [
    [113, 158, 181],
    [70, 175, 215],
    [70, 174, 121],
    [155, 121, 80],
    [212, 104, 69],
    [236, 196, 112],
    [196, 92, 172],
    [141, 133, 204],
    [222, 228, 232],
    [92, 98, 108],
    [40, 44, 52],
    [232, 196, 92],
];

/// Lays out one column of controls and tracks which one has the keyboard.
pub(super) struct Form<'a> {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub s: f32,
    focus: &'a mut Option<u64>,
    seen: &'a mut bool,
    entry: &'a mut String,
    salt: u64,
}

impl Form<'_> {
    /// Use `salt` for the ids of the controls that follow, so repeated
    /// titles in a list stay apart; returns the previous salt.
    pub fn set_salt(&mut self, salt: u64) -> u64 {
        std::mem::replace(&mut self.salt, salt)
    }
    fn id(&self, title: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        title.hash(&mut hasher);
        self.salt.hash(&mut hasher);
        hasher.finish()
    }
    pub fn heading(&mut self, text: &str) {
        self.y += 8. * self.s;
        label(
            text,
            self.x,
            self.y + 13. * self.s,
            15. * self.s,
            text_color(),
        );
        draw_line(
            self.x,
            self.y + 20. * self.s,
            self.x + self.w,
            self.y + 20. * self.s,
            1.,
            alpha(line_color(), 0.7),
        );
        self.y += 32. * self.s;
    }
    /// How much of a budget is used, as a labelled bar.
    pub fn meter(&mut self, title: &str, used: usize, limit: usize) {
        let s = self.s;
        let share = (used as f32 / limit.max(1) as f32).clamp(0., 1.);
        label(title, self.x, self.y + 14. * s, 12.5 * s, text_color());
        right_label(
            &format!("{used} / {limit}"),
            self.x + self.w,
            self.y + 14. * s,
            12. * s,
            muted_color(),
        );
        let bar = Rect::new(self.x, self.y + 21. * s, self.w, 5. * s);
        draw_rectangle(
            bar.x,
            bar.y,
            bar.w,
            bar.h,
            mix(surface_color(), bg_color(), 0.5),
        );
        let color = if share >= 1. {
            RED
        } else if share > 0.85 {
            AMBER
        } else {
            accent()
        };
        draw_rectangle(bar.x, bar.y, bar.w * share, bar.h, color);
        self.y += 34. * s;
    }
    pub fn note(&mut self, text: &str) {
        self.y = wrap(
            text,
            self.x,
            self.y + 4. * self.s,
            self.w,
            12.5 * self.s,
            muted_color(),
        ) + 10. * self.s;
    }
    /// A typed value box; returns the text when Enter or a click elsewhere
    /// confirms it.
    fn typed(&mut self, id: u64, r: Rect) -> Option<String> {
        if *self.focus != Some(id) {
            return None;
        }
        draw_rectangle(r.x, r.y, r.w, r.h, surface_color());
        let mut active = true;
        text_field(r, self.entry, &mut active, 24, self.s * 0.6, false);
        if is_key_pressed(KeyCode::Escape) {
            *self.focus = None;
            return None;
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) || !active {
            *self.focus = None;
            return Some(self.entry.trim().to_owned());
        }
        *self.seen = true;
        None
    }
    fn open(&mut self, id: u64, r: Rect, text: String) {
        if *self.focus != Some(id) && contains(r) {
            draw_line(
                r.x + 6.,
                r.bottom() - 3.,
                r.right() - 4.,
                r.bottom() - 3.,
                1.,
                accent(),
            );
            if is_mouse_button_pressed(MouseButton::Left) {
                *self.focus = Some(id);
                *self.entry = text;
                reset_field(self.entry);
                select_field_all(self.entry);
                *self.seen = true;
            }
        }
    }
    /// A slider whose value can also be clicked and typed.
    pub fn number(&mut self, title: &str, value: &mut f32, limits: [f32; 2], step: f32) -> bool {
        self.numeric(title, value, limits, step, false)
    }
    fn numeric(
        &mut self,
        title: &str,
        value: &mut f32,
        limits: [f32; 2],
        step: f32,
        integer: bool,
    ) -> bool {
        let before = *value;
        let previous = numeric_scope(self.id(title));
        let rect = Rect::new(self.x, self.y, self.w, 31. * self.s);
        if integer {
            slider_integer_value(rect, title, value, limits, step, self.s * 0.8, |v| {
                format!("{v:.0}")
            });
        } else {
            slider_value(rect, title, value, limits, step, self.s * 0.8, |v| {
                format!("{v:.3}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_owned()
            });
        }
        numeric_scope(previous);
        self.y += 48. * self.s;
        *value != before
    }
    pub fn integer(&mut self, title: &str, value: &mut i32, limits: [i32; 2]) -> bool {
        let mut v = *value as f32;
        self.numeric(
            title,
            &mut v,
            [limits[0] as f32, limits[1] as f32],
            1.,
            true,
        );
        let next = (v.round() as i32).clamp(limits[0], limits[1]);
        let changed = *value != next;
        *value = next;
        changed
    }
    pub fn text(&mut self, title: &str, value: &mut String, max: usize) -> bool {
        let s = self.s;
        label(title, self.x, self.y, 12. * s, muted_color());
        let id = self.id(title);
        let mut active = *self.focus == Some(id);
        let before = value.clone();
        text_field(
            Rect::new(self.x, self.y + 10. * s, self.w, 31. * s),
            value,
            &mut active,
            max,
            s * 0.75,
            false,
        );
        if active
            && (is_key_pressed(KeyCode::Escape)
                || is_key_pressed(KeyCode::Enter)
                || is_key_pressed(KeyCode::KpEnter))
        {
            active = false;
        }
        if active {
            *self.focus = Some(id);
            *self.seen = true;
        } else if *self.focus == Some(id) {
            *self.focus = None;
        }
        self.y += 58. * s;
        *value != before
    }
    /// A grid of choices with the current one lit.
    pub fn pick<T: Copy + PartialEq>(
        &mut self,
        title: &str,
        value: &mut T,
        options: &[T],
        name: impl Fn(T) -> &'static str,
        columns: usize,
    ) -> bool {
        let s = self.s;
        if !title.is_empty() {
            label(title, self.x, self.y + 2. * s, 12. * s, muted_color());
            self.y += 10. * s;
        }
        let gap = 4. * s;
        let w = (self.w - gap * (columns as f32 - 1.)) / columns as f32;
        let h = 27. * s;
        let before = *value;
        for (i, &option) in options.iter().enumerate() {
            let r = Rect::new(
                self.x + (i % columns) as f32 * (w + gap),
                self.y + (i / columns) as f32 * (h + gap),
                w,
                h,
            );
            if button(r, name(option), *value == option, true, s * 0.66) {
                *value = option;
            }
        }
        self.y += options.len().div_ceil(columns) as f32 * (h + gap) + 10. * s;
        *value != before
    }
    /// One button that steps through many choices; Shift steps back.
    pub fn cycle<T: Copy + PartialEq>(
        &mut self,
        title: &str,
        value: &mut T,
        options: &[T],
        name: impl Fn(T) -> String,
    ) -> bool {
        let s = self.s;
        label(title, self.x, self.y + 2. * s, 12. * s, muted_color());
        let index = options.iter().position(|v| v == value).unwrap_or(0);
        let r = Rect::new(self.x, self.y + 10. * s, self.w, 29. * s);
        let mut changed = false;
        if small_button(r, &format!("{}   ›", name(*value)), s * 0.78) {
            *value = options[if super::shift() {
                (index + options.len() - 1) % options.len()
            } else {
                (index + 1) % options.len()
            }];
            changed = true;
        }
        self.y += 50. * s;
        changed
    }
    pub fn toggle(&mut self, title: &str, value: &mut bool) -> bool {
        let changed = toggle(
            Rect::new(self.x, self.y, self.w, 30. * self.s),
            title,
            value,
            self.s * 0.8,
        );
        self.y += 38. * self.s;
        changed
    }
    /// A small whole number with − and + buttons, typed by clicking it, and
    /// optionally a button for the lowest unused group.
    pub fn stepper(
        &mut self,
        title: &str,
        value: &mut u8,
        range: [u8; 2],
        free: Option<Option<u8>>,
    ) -> bool {
        let s = self.s;
        let before = *value;
        let h = 28. * s;
        let free_w = if free.is_some() { 52. * s } else { 0. };
        let box_w = 60. * s;
        let start = self.x + self.w - free_w - box_w - h * 2. - 8. * s;
        label(
            &fitted_label(title, 13. * s, start - self.x - 6. * s),
            self.x,
            self.y + h * 0.66,
            13. * s,
            text_color(),
        );
        let decrease = button(
            Rect::new(start, self.y, h, h),
            "−",
            false,
            *value > range[0],
            s * 0.8,
        );
        let field = Rect::new(start + h + 2. * s, self.y, box_w, h);
        let shown = if *value == 0 && range[0] == 0 && free.is_some() {
            "none".to_owned()
        } else {
            value.to_string()
        };
        let previous = numeric_scope(self.id(title));
        let mut exact = f32::from(*value);
        number_box(
            field,
            title,
            &mut exact,
            range.map(f32::from),
            s * 0.85,
            &shown,
        );
        numeric_scope(previous);
        *value = exact.round() as u8;
        if decrease {
            *value = value.saturating_sub(1).max(range[0]);
        }
        if button(
            Rect::new(field.right() + 2. * s, self.y, h, h),
            "+",
            false,
            *value < range[1],
            s * 0.8,
        ) {
            *value += 1;
        }
        if let Some(free) = free
            && button(
                Rect::new(
                    self.x + self.w - free_w + 4. * s,
                    self.y,
                    free_w - 4. * s,
                    h,
                ),
                "Free",
                false,
                free.is_some(),
                s * 0.66,
            )
            && let Some(group) = free
        {
            *value = group;
        }
        self.y += h + 12. * s;
        *value != before
    }
    pub fn color(&mut self, rgb: &mut [u8; 3]) -> bool {
        let s = self.s;
        let before = *rgb;
        let size = (self.w - 5. * 5. * s) / 6.;
        for (i, swatch) in SWATCHES.into_iter().enumerate() {
            let r = Rect::new(
                self.x + (i % 6) as f32 * (size + 5. * s),
                self.y + (i / 6) as f32 * (size * 0.6 + 5. * s),
                size,
                size * 0.6,
            );
            draw_rectangle(r.x, r.y, r.w, r.h, crate::scenery::color(swatch));
            let chosen = *rgb == swatch;
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                if chosen { 3. } else { 1. },
                if chosen { accent() } else { line_color() },
            );
            if contains(r) && is_mouse_button_pressed(MouseButton::Left) {
                *rgb = swatch;
            }
        }
        self.y += (size * 0.6 + 5. * s) * 2. + 8. * s;
        let field = Rect::new(self.x + 44. * s, self.y, self.w - 44. * s, 28. * s);
        label("Hex", self.x, self.y + 19. * s, 12. * s, muted_color());
        let id = self.id("Hex color");
        let mut invalid = false;
        if let Some(text) = self.typed(id, field) {
            if let Some(color) = tankyard_client::number_input::hex_color(&text) {
                *rgb = color;
            } else if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                *self.focus = Some(id);
                *self.seen = true;
                invalid = true;
            }
        } else if *self.focus != Some(id) {
            draw_rectangle(
                field.x,
                field.y,
                field.w,
                field.h,
                mix(surface_color(), bg_color(), 0.35),
            );
            draw_rectangle_lines(field.x, field.y, field.w, field.h, 1., line_color());
            label(
                &format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
                field.x + 7. * s,
                field.y + 19. * s,
                13. * s,
                text_color(),
            );
            self.open(
                id,
                field,
                format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
            );
        }
        if *self.focus == Some(id)
            && (invalid || tankyard_client::number_input::hex_color(self.entry).is_none())
        {
            draw_rectangle_lines(field.x, field.y, field.w, field.h, 1., RED);
            label(
                "Use #RGB or #RRGGBB",
                self.x,
                self.y + 40. * s,
                10. * s,
                RED,
            );
            self.y += 12. * s;
        }
        self.y += 40. * s;
        for (i, title) in ["Red", "Green", "Blue"].into_iter().enumerate() {
            let mut value = i32::from(rgb[i]);
            self.integer(title, &mut value, [0, 255]);
            rgb[i] = value as u8;
        }
        *rgb != before
    }
    pub fn buttons(&mut self, labels: &[&str], enabled: bool) -> Option<usize> {
        let s = self.s;
        let gap = 5. * s;
        let w = (self.w - gap * (labels.len() as f32 - 1.)) / labels.len() as f32;
        let mut clicked = None;
        for (i, text) in labels.iter().enumerate() {
            if button(
                Rect::new(self.x + i as f32 * (w + gap), self.y, w, 28. * s),
                text,
                false,
                enabled,
                s * 0.7,
            ) {
                clicked = Some(i);
            }
        }
        self.y += 38. * s;
        clicked
    }
}

/// What the inspector asks of the editor once its form is done.
enum Request {
    None,
    Locate(Key),
    Delete,
    Duplicate,
    Open(Box<Document>, Option<std::path::PathBuf>, &'static str),
    Import,
    Refresh,
    Reveal,
    Check,
    Restart,
    /// Carry out one trigger in the preview now.
    Fire(usize),
    RunTool(usize, usize),
    ReloadPlugins,
    Distribute(usize),
    MatchSize(usize),
    Notice(String),
    /// Select every object in a group.
    SelectGroup(u8),
}

impl Editor {
    fn form(&mut self, x: f32, y: f32, w: f32, s: f32, salt: u64) -> Form<'_> {
        Form {
            x,
            y,
            w,
            s,
            focus: &mut self.text_focus,
            seen: &mut self.focus_seen,
            entry: &mut self.entry,
            salt,
        }
    }

    pub(super) fn inspector(&mut self, layout: &Layout) {
        let r = layout.inspector;
        let s = layout.s;
        draw_rectangle(r.x, r.y, r.w, r.h, surface_color());
        draw_line(r.x, r.y, r.x, r.bottom(), 1.5, line_color());
        let tabs = [
            ("Map", Tab::Map),
            ("Object", Tab::Object),
            ("Logic", Tab::Logic),
            ("Check", Tab::Check),
            ("Rules", Tab::Rules),
            ("Files", Tab::Files),
            ("Tools", Tab::Tools),
        ];
        let w = (r.w - 16. * s) / tabs.len() as f32;
        for (i, (title, tab)) in tabs.into_iter().enumerate() {
            let mut text = title.to_owned();
            if tab == Tab::Check
                && let Some(problems) = &self.problems
                && !problems.is_empty()
            {
                text = format!("Check {}", problems.len());
            }
            if button(
                Rect::new(
                    r.x + 8. * s + i as f32 * w,
                    r.y + 9. * s,
                    w - 3. * s,
                    27. * s,
                ),
                &text,
                self.tab == tab,
                true,
                s * 0.55,
            ) {
                self.tab = tab;
                self.scroll = 0.;
                self.text_focus = None;
                if tab == Tab::Check {
                    self.check();
                }
            }
        }
        let clip = Rect::new(r.x + 6. * s, r.y + 46. * s, r.w - 12. * s, r.h - 52. * s);
        if !is_mouse_button_down(MouseButton::Left) {
            self.scroll_drag = None;
        }
        if clip.contains(ui_layout::pointer()) && !self.typing() {
            let page = clip.h * 0.85;
            if is_key_pressed(KeyCode::PageDown) {
                self.scroll += page;
            }
            if is_key_pressed(KeyCode::PageUp) {
                self.scroll -= page;
            }
            if is_key_pressed(KeyCode::Home) {
                self.scroll = 0.;
            }
            if is_key_pressed(KeyCode::End) {
                self.scroll = (self.content_height - clip.h).max(0.);
            }
        }
        if clip.contains(ui_layout::pointer()) && !pointer_captured() && self.scroll_drag.is_none()
        {
            self.scroll = (self.scroll - ui_layout::wheel_lines().y * 56. * s)
                .clamp(0., (self.content_height - clip.h).max(0.));
        }
        let (end, request) = {
            let _region = ui_layout::region(clip, clip.h, self.scroll);
            let x = clip.x + 9. * s;
            let y = clip.y + 12. * s;
            let width = clip.w - 23. * s;
            match self.tab {
                Tab::Map => self.map_tab(x, y, width, s),
                Tab::Object => self.object_tab(x, y, width, s),
                Tab::Logic => self.logic_tab(x, y, width, s),
                Tab::Check => self.check_tab(x, y, width, s),
                Tab::Rules => self.rules_tab(x, y, width, s),
                Tab::Files => self.files_tab(x, y, width, s),
                Tab::Tools => self.tools_tab(x, y, width, s),
            }
        };
        if let Some(focus) = numeric_focus_rect() {
            let margin = 8. * s;
            if focus.y < clip.y + margin {
                self.scroll -= clip.y + margin - focus.y;
            } else if focus.bottom() > clip.bottom() - margin {
                self.scroll += focus.bottom() - clip.bottom() + margin;
            }
        }
        self.content_height = (end - clip.y + 22. * s).max(0.);
        self.scroll = self
            .scroll
            .clamp(0., (self.content_height - clip.h).max(0.));
        if self.content_height > clip.h {
            let h = (clip.h * clip.h / self.content_height).max(20. * s);
            let at = self.scroll / (self.content_height - clip.h).max(1.) * (clip.h - h);
            let track = Rect::new(r.right() - 18. * s, clip.y, 12. * s, clip.h);
            let thumb = Rect::new(track.x, clip.y + at, track.w, h);
            if contains(track) && is_mouse_button_pressed(MouseButton::Left) {
                self.scroll_drag = Some(if contains(thumb) {
                    ui_layout::pointer().y - thumb.y
                } else {
                    h * 0.5
                });
            }
            if let Some(grab) = self.scroll_drag {
                self.scroll = ((ui_layout::pointer().y - track.y - grab) / (track.h - h).max(1.))
                    .clamp(0., 1.)
                    * (self.content_height - clip.h);
            }
            let at = self.scroll / (self.content_height - clip.h).max(1.) * (clip.h - h);
            let thick = if contains(track) || self.scroll_drag.is_some() {
                8. * s
            } else {
                4. * s
            };
            draw_rectangle(
                track.center().x - thick * 0.5,
                clip.y + at,
                thick,
                h,
                accent_color(),
            );
        }
        self.fulfil(request);
    }
    fn fulfil(&mut self, request: Request) {
        match request {
            Request::None => {}
            Request::Distribute(axis) => self.apply(|editor| {
                let keys = editor.selection.clone();
                editor.doc.distribute(&keys, axis);
            }),
            Request::MatchSize(axis) => self.apply(|editor| {
                let keys = editor.selection.clone();
                editor.doc.match_size(&keys, axis);
            }),
            Request::Locate(key) => {
                self.set_selection(vec![key]);
                self.center_on(key);
            }
            Request::SelectGroup(group) => {
                let keys = self.doc.in_group(group);
                if let Some(&first) = keys.first() {
                    self.center_on(first);
                }
                self.set_selection(keys);
                // Ready to drag, with the group's settings from the top.
                self.mode = Mode::Edit;
                self.tab = Tab::Object;
                self.scroll = 0.;
            }
            Request::Delete => self.delete(),
            Request::Duplicate => self.duplicate(),
            Request::Open(doc, path, notice) => self.open(*doc, path, notice),
            Request::Import => {
                if self.import_pending.is_none() {
                    self.import_pending = Some(crate::file_dialog::choose_mod_pack());
                }
            }
            Request::Refresh => self.refresh_library(),
            Request::Reveal => {
                if let Err(error) = directory().and_then(|path| crate::file_dialog::reveal(&path)) {
                    self.say(error);
                }
            }
            Request::Check => self.check(),
            Request::Restart => {
                self.preview.restart(&self.doc.map);
            }
            Request::RunTool(plugin, tool) => self.run_tool(plugin, tool, None),
            Request::ReloadPlugins => self.reload_plugins(),
            Request::Fire(index) => {
                let config = self.config();
                self.preview.fire(&self.doc.map, &config, index, self.probe);
            }
            Request::Notice(text) => self.say(text),
        }
    }

    fn map_tab(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        let map = &self.doc.map;
        let (mut name, mut id, mut description) =
            (map.name.clone(), map.id.clone(), map.description.clone());
        let (mut width, mut height) = (map.width, map.height);
        let mut players = i32::from(map.capacity);
        let mut style = map.style;
        let mut background = map.background.clone();
        let mut hidden = map.hidden_groups.clone();
        let mut ambient = map.ambient;
        let mut paths = map.paths.clone();
        let mut pick = self.path_pick;
        let mut locked = self.doc.locked;
        let mut request = Request::None;
        let used = self.doc.used_groups();
        // What each group holds: blocks, pads, decor and triggers.
        let mut counts = [[0u32; 4]; MAP_GROUPS as usize + 1];
        for (key, _, group) in self.doc.items() {
            let kind = match key {
                Key::Block(_) => 0,
                Key::Zone(_) => 1,
                Key::Decor(_) => 2,
                Key::Trigger(_) => 3,
                _ => continue,
            };
            if let Some(row) = counts.get_mut(usize::from(group)) {
                row[kind] += 1;
            }
        }
        // Each group worth listing, with its name and whether tanks push it.
        let groups: Vec<(u8, Option<String>, bool)> = (1..=MAP_GROUPS)
            .filter(|&g| {
                counts[usize::from(g)].iter().any(|&n| n > 0)
                    || hidden.contains(&g)
                    || map.group_name(g).is_some()
            })
            .map(|g| {
                (
                    g,
                    map.group_name(g).map(str::to_owned),
                    map.body(g).is_some(),
                )
            })
            .collect();
        let mut f = self.form(x, y, w, s, 1);
        f.text("Name", &mut name, 64);
        f.text("Map id", &mut id, 48);
        f.text("Description", &mut description, 240);
        f.heading("Size");
        f.number("Width", &mut width, [900., 16000.], 100.);
        f.number("Height", &mut height, [700., 16000.], 100.);
        f.note("Objects keep their place relative to the arena when its size changes.");
        f.integer("Recommended players", &mut players, [2, 128]);
        f.heading("Look");
        f.pick(
            "Wall style",
            &mut style,
            &BlockStyle::ALL,
            BlockStyle::name,
            4,
        );
        let palettes = ["slate", "teal", "sand", "violet"];
        let mut floor = palettes.iter().position(|p| *p == background).unwrap_or(0);
        if f.pick(
            "Floor",
            &mut floor,
            &[0, 1, 2, 3],
            |i| ["Slate", "Teal", "Sand", "Violet"][i],
            4,
        ) {
            background = palettes[floor].into();
        }
        f.heading("Lighting");
        let mut darkness = ambient.darkness * 100.;
        if f.number(
            "Darkness (%)",
            &mut darkness,
            [0., DARKNESS_LIMIT * 100.],
            5.,
        ) {
            ambient.darkness = darkness / 100.;
        }
        if ambient.darkness > 0. {
            f.color(&mut ambient.color);
            f.note("Lights, spotlights and beams shine through the dark, and every tank carries a small light. A Lighting trigger changes it during a round.");
        }
        paths_form(&mut f, &mut paths, &mut pick);
        f.heading("Groups");
        if groups.is_empty() {
            f.note("No groups yet. Set an object's Group to gather things a trigger moves, hides or animates together, or make the group a pushable body.");
        }
        for (group, group_name, pushable) in groups {
            let [blocks, pads, decor, triggers] = counts[usize::from(group)];
            let mut parts = Vec::new();
            for (n, one, many) in [
                (blocks, "block", "blocks"),
                (pads, "pad", "pads"),
                (decor, "decor", "decor"),
                (triggers, "trigger", "triggers"),
            ] {
                match n {
                    0 => {}
                    1 => parts.push(format!("1 {one}")),
                    n => parts.push(format!("{n} {many}")),
                }
            }
            if pushable {
                parts.push("pushable".into());
            }
            let summary = if parts.is_empty() {
                "empty".to_owned()
            } else {
                parts.join(" \u{b7} ")
            };
            let title = match group_name {
                Some(name) => format!("{group}  {name}"),
                None => format!("Group {group}"),
            };
            let h = 40. * s;
            let r = Rect::new(x, f.y, w, h);
            draw_rectangle(r.x, r.y, r.w, r.h, mix(surface_color(), bg_color(), 0.3));
            draw_rectangle(r.x, r.y, 3. * s, r.h, VIOLET);
            let bw = 50. * s;
            let gap = 4. * s;
            let buttons = |i: f32| Rect::new(r.right() - (bw + gap) * i, r.y + 9. * s, bw, 22. * s);
            let text_w = (buttons(3.).x - x - 14. * s).max(20. * s);
            label(
                &fitted_label(&title, 13. * s, text_w),
                x + 9. * s,
                r.y + 16. * s,
                13. * s,
                text_color(),
            );
            label(
                &fitted_label(&summary, 11. * s, text_w),
                x + 9. * s,
                r.y + 32. * s,
                11. * s,
                muted_color(),
            );
            let bit = MapDef::group_bit(group);
            let starts_hidden = hidden.contains(&group);
            if button(buttons(3.), "Hidden", starts_hidden, true, s * 0.55) {
                hidden.retain(|&g| g != group);
                if !starts_hidden {
                    hidden.push(group);
                    hidden.sort_unstable();
                }
            }
            if button(buttons(2.), "Lock", locked & bit != 0, true, s * 0.55) {
                locked ^= bit;
            }
            if button(buttons(1.), "Select", false, true, s * 0.55) {
                request = Request::SelectGroup(group);
            }
            f.y += h + 4. * s;
        }
        f.note("Hidden groups start the round out of sight until a trigger shows them. Locked groups can't be selected on the canvas, so they stay put while you work around them. Name a group from any of its objects.");
        let unused = (1..=MAP_GROUPS)
            .filter(|&g| MapDef::group_bit(g) & used == 0)
            .count();
        f.note(&format!("{} of 32 groups are free.", unused));
        let end = f.y;
        let map = &self.doc.map;
        // The id follows the name until someone edits it by hand.
        if name != map.name && id == map.id && map.id == map_id(&map.name) {
            id = map_id(&name);
        }
        let changed = name != map.name
            || id != map.id
            || description != map.description
            || width != map.width
            || height != map.height
            || players != i32::from(map.capacity)
            || style != map.style
            || background != map.background
            || hidden != map.hidden_groups
            || ambient != map.ambient
            || paths != map.paths;
        self.path_pick = pick;
        if changed {
            self.edit_property(|doc| {
                let map = &mut doc.map;
                map.name = name;
                map.id = id;
                map.description = description;
                map.width = width;
                map.height = height;
                map.capacity = players as u16;
                map.style = style;
                map.background = background;
                map.hidden_groups = hidden;
                map.ambient = ambient;
                map.paths = paths;
            });
        }
        if locked != self.doc.locked {
            self.edit_property(|doc| doc.locked = locked);
        }
        (end, request)
    }

    fn object_tab(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        match self.selection.len() {
            0 => {
                let mut f = self.form(x, y, w, s, 2);
                f.heading("Nothing selected");
                f.note("In Edit mode, click an object to select it. Shift-click adds or removes one; drag across empty floor to select a region; double-click selects a whole group.");
                f.note("Right-click any object to build copies of it. Alt-drag copies the selection. Press F1 for every shortcut.");
                (f.y, Request::None)
            }
            1 => self.single_object(x, y, w, s),
            _ => self.many_objects(x, y, w, s),
        }
    }

    fn single_object(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        let key = self.selection[0];
        let Some(before) = self.doc.object(key) else {
            return (y, Request::None);
        };
        let mut object = before.clone();
        let free = self.doc.next_free_group();
        let used_channels = self.doc.used_channels();
        let free_channel = (1..=MAP_CHANNELS).find(|&c| !used_channels[usize::from(c)]);
        let (map_w, map_h) = (self.doc.map.width, self.doc.map.height);
        let mut hidden = self.doc.map.hidden_groups.clone();
        let mut locked = self.doc.locked;
        let mut bodies = self.doc.map.bodies.clone();
        let mut names = self.doc.map.group_names.clone();
        let map_paths = self.doc.map.paths.len();
        let salt = 100 + key_salt(key);
        let mut request = Request::None;
        // The gate as the preview sees it, once the preview has run.
        let live = match &object {
            Object::Trigger(t) if self.preview.running || !self.preview.at_start() => {
                self.preview.gate_view(t, &self.config())
            }
            _ => None,
        };
        let mut f = self.form(x, y, w, s, salt);
        let title = match &object {
            Object::Block(b) => format!("{} block", b.style.name()),
            Object::Trigger(_) => format!("Trigger {}", key_index(key) + 1),
            other => other.name().to_owned(),
        };
        label(&title, x, f.y + 14. * s, 18. * s, text_color());
        f.y += 30. * s;
        if let Some(i) = f.buttons(&["Duplicate", "Delete"], true) {
            request = if i == 0 {
                Request::Duplicate
            } else {
                Request::Delete
            };
        }
        if object.groupable() {
            let mut group = object.group();
            let title = if matches!(object, Object::Trigger(_)) {
                "Own group"
            } else {
                "Group"
            };
            f.stepper(title, &mut group, [0, MAP_GROUPS], Some(free));
            object.set_group(group);
            if group > 0 {
                let mut starts_hidden = hidden.contains(&group);
                if f.toggle(&format!("Group {group} starts hidden"), &mut starts_hidden) {
                    hidden.retain(|&g| g != group);
                    if starts_hidden {
                        hidden.push(group);
                        hidden.sort_unstable();
                    }
                }
                let bit = MapDef::group_bit(group);
                let mut lock = locked & bit != 0;
                if f.toggle(&format!("Lock group {group} in the editor"), &mut lock) {
                    locked = if lock { locked | bit } else { locked & !bit };
                }
                group_name_form(&mut f, &mut names, group);
                if matches!(object, Object::Block(_)) {
                    body_form(&mut f, &mut bodies, group);
                }
            }
            if matches!(object, Object::Trigger(_)) && group > 0 {
                f.note("Hiding its own group switches this trigger off.");
            }
        }
        f.heading("Place");
        let mut r = object.rect();
        if object.is_point() {
            let mut at = [(r[0] + r[2] * 0.5) * map_w, (r[1] + r[3] * 0.5) * map_h];
            f.number("X", &mut at[0], [0., map_w], 1.);
            f.number("Y", &mut at[1], [0., map_h], 1.);
            r = [
                at[0] / map_w - r[2] * 0.5,
                at[1] / map_h - r[3] * 0.5,
                r[2],
                r[3],
            ];
        } else {
            let scale = [map_w, map_h, map_w, map_h];
            for (i, title) in ["X", "Y", "Width", "Height"].into_iter().enumerate() {
                let mut value = r[i] * scale[i];
                let low = if i < 2 { 0. } else { 4. };
                f.number(title, &mut value, [low, scale[i]], 1.);
                r[i] = value / scale[i];
            }
        }
        if r != object.rect() {
            object.set_rect(r);
        }
        match &mut object {
            Object::Wall(_) => {
                f.note("A fixed wall from the original layout. It cannot join a group.")
            }
            Object::Block(b) => {
                f.heading("Block");
                f.pick("Style", &mut b.style, &BlockStyle::ALL, BlockStyle::name, 4);
                let mut tint = b.color.is_some();
                f.toggle("Tint", &mut tint);
                if tint {
                    let rgb = b.color.get_or_insert([100, 170, 190]);
                    f.color(rgb);
                } else {
                    b.color = None;
                }
            }
            Object::Zone(z) => {
                f.heading("Pad");
                f.pick("Kind", &mut z.kind, &ZoneKind::ALL, ZoneKind::name, 4);
                f.number("Strength", &mut z.strength, [0.25, 2.], 0.05);
                if z.kind.aims() {
                    label("Direction", x, f.y + 2. * s, 12. * s, muted_color());
                    f.y += 10. * s;
                    let names = ["→", "↘", "↓", "↙", "←", "↖", "↑", "↗"];
                    let mut choice = ((z.angle.rem_euclid(360.) / 45.).round() as usize) % 8;
                    let exact = (z.angle.rem_euclid(360.) - choice as f32 * 45.).abs() < 0.01;
                    let gap = 4. * s;
                    let bw = (w - gap * 7.) / 8.;
                    for (i, name) in names.into_iter().enumerate() {
                        if button(
                            Rect::new(x + i as f32 * (bw + gap), f.y, bw, 28. * s),
                            name,
                            exact && choice == i,
                            true,
                            s * 0.8,
                        ) {
                            choice = i;
                            z.angle = i as f32 * 45.;
                        }
                    }
                    f.y += 38. * s;
                    f.number("Angle", &mut z.angle, [0., 359.], 1.);
                    f.note(&format!(
                        "Carries tanks at {:.0} units a second at this strength.",
                        CONVEYOR_SPEED * z.strength
                    ));
                }
                if z.kind == ZoneKind::Portal {
                    let mut exit = [z.exit[0] * map_w, z.exit[1] * map_h];
                    f.number("Exit X", &mut exit[0], [0., map_w], 1.);
                    f.number("Exit Y", &mut exit[1], [0., map_h], 1.);
                    z.exit = [exit[0] / map_w, exit[1] / map_h];
                    f.note("Drag the diamond on the canvas to move the exit. Tanks keep their speed and rest a moment before they can use a portal again.");
                }
            }
            Object::Decor(d) => {
                f.heading(d.kind.name());
                if d.kind == DecorKind::Label {
                    f.text("Text", &mut d.text, 64);
                    f.note("{1} to {8} show the current value of that counter.");
                }
                f.number("Rotation", &mut d.rotation, [0., 359.], 1.);
                f.number("Spin per second", &mut d.spin, [-720., 720.], 5.);
                f.number("Pulse per second", &mut d.pulse, [0., 4.], 0.1);
                let mut opacity = i32::from(d.alpha);
                f.integer("Opacity", &mut opacity, [0, 255]);
                d.alpha = opacity as u8;
                if d.kind.lights() {
                    f.number("Flicker", &mut d.flicker, [0., 1.], 0.05);
                    f.note(match d.kind {
                        DecorKind::Spotlight => "Shines a cone along its rotation from its back edge; its width sets the reach and its height the spread. In a dark arena it lights what it falls on.",
                        DecorKind::Beam => "A shaft of light along its rotation. Thin and bright, it reads as a laser; wide and faint, as a sunbeam.",
                        _ => "Lights brighten what lies under them. Opacity sets how strongly; flicker makes them waver like a flame.",
                    });
                }
                f.toggle("Above tanks", &mut d.above);
                f.pick(
                    "Near a tank",
                    &mut d.react,
                    &DecorReact::ALL,
                    DecorReact::name,
                    3,
                );
                if d.react != DecorReact::None {
                    let mut reach = d.react_reach();
                    if f.number("Within (units)", &mut reach, [10., 2000.], 10.) {
                        d.reach = reach;
                    }
                    f.note(match d.react {
                        DecorReact::Fade => "Clears as the player's tank comes close: a roof or a canopy over a hidden path. Turn on Above tanks for it to cover them.",
                        DecorReact::Appear => "Shows only as the player's tank comes close: a hidden sign or a secret marker.",
                        DecorReact::Grow => "Swells as the player's tank comes close.",
                        _ => "Brightens as the player's tank comes close.",
                    });
                }
                if d.kind == DecorKind::Emitter {
                    let effect = d.effect.get_or_insert_default();
                    effect_form(&mut f, effect);
                } else {
                    f.heading("Colour");
                    f.color(&mut d.color);
                }
            }
            Object::Trigger(t) => trigger_form(
                &mut f,
                t,
                live.as_ref(),
                (free_channel, &used_channels),
                map_paths,
                &names,
            ),
            Object::Spawn(region) => {
                f.heading("Spawn region");
                f.stepper("Team (0 for any)", &mut region.team, [0, 4], None);
            }
            Object::Base(base) => {
                f.heading("Team base");
                f.stepper("Team", &mut base.team, [1, 4], None);
            }
            Object::Objective(_) => f.note("Objective modes place their goal here."),
        }
        let end = f.y;
        if bodies != self.doc.map.bodies {
            self.edit_property(|doc| doc.map.bodies = bodies);
        }
        if names != self.doc.map.group_names {
            self.edit_property(|doc| doc.map.group_names = names);
        }
        if hidden != self.doc.map.hidden_groups {
            self.edit_property(|doc| doc.map.hidden_groups = hidden);
        }
        if locked != self.doc.locked {
            self.edit_property(|doc| doc.locked = locked);
            if locked & MapDef::group_bit(object.group()) != 0 {
                self.selection.clear();
                return (
                    end,
                    Request::Notice("Locked. Clicks now pass through this group; Ctrl+Shift+L unlocks every group.".into()),
                );
            }
        }
        if object != before {
            if let (Object::Base(a), Object::Base(b)) = (&object, &before)
                && a.team != b.team
                && self
                    .doc
                    .map
                    .team_bases
                    .iter()
                    .any(|base| base.team == a.team)
            {
                return (
                    end,
                    Request::Notice(format!("Team {} already has a base.", a.team)),
                );
            }
            self.edit_property(|doc| doc.replace(key, object));
        }
        (end, request)
    }

    fn many_objects(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        let objects: Vec<Object> = self
            .selection
            .iter()
            .filter_map(|&key| self.doc.object(key))
            .collect();
        let Some(first) = objects.first().cloned() else {
            return (y, Request::None);
        };
        let same = objects
            .iter()
            .all(|o| std::mem::discriminant(o) == std::mem::discriminant(&first));
        let mut counts: Vec<(&'static str, usize)> = Vec::new();
        for object in &objects {
            let name = object.name();
            match counts.iter_mut().find(|(n, _)| *n == name) {
                Some((_, count)) => *count += 1,
                None => counts.push((name, 1)),
            }
        }
        let free = self.doc.next_free_group();
        let groupable: Vec<bool> = objects.iter().map(Object::groupable).collect();
        let groups: Vec<u8> = objects.iter().map(Object::group).collect();
        let mut request = Request::None;
        let mut f = self.form(x, y, w, s, 3);
        label(
            &format!("{} objects", objects.len()),
            x,
            f.y + 14. * s,
            18. * s,
            text_color(),
        );
        f.y += 26. * s;
        let summary: Vec<String> = counts
            .iter()
            .map(|(name, count)| format!("{count} {}", name.to_lowercase()))
            .collect();
        f.note(&summary.join(", "));
        if let Some(i) = f.buttons(&["Duplicate", "Delete"], true) {
            request = if i == 0 {
                Request::Duplicate
            } else {
                Request::Delete
            };
        }
        // Batch edits start from the first object and apply only what changes.
        let mut template = first.clone();
        if groupable.iter().any(|&g| g) {
            let mut group = groups[0];
            let mixed = groups.iter().any(|&g| g != group);
            f.stepper(
                if mixed { "Group (mixed)" } else { "Group" },
                &mut group,
                [0, MAP_GROUPS],
                Some(free),
            );
            template.set_group(group);
        }
        if same {
            match &mut template {
                Object::Block(b) => {
                    f.heading("Blocks");
                    f.pick("Style", &mut b.style, &BlockStyle::ALL, BlockStyle::name, 4);
                    let mut tint = b.color.is_some();
                    f.toggle("Tint", &mut tint);
                    if tint {
                        f.color(b.color.get_or_insert([100, 170, 190]));
                    } else {
                        b.color = None;
                    }
                }
                Object::Zone(z) => {
                    f.heading("Pads");
                    f.pick("Kind", &mut z.kind, &ZoneKind::ALL, ZoneKind::name, 4);
                    f.number("Strength", &mut z.strength, [0.25, 2.], 0.05);
                    if z.kind.aims() {
                        f.number("Angle", &mut z.angle, [0., 359.], 1.);
                    }
                }
                Object::Decor(d) => {
                    f.heading("Decorations");
                    let mut opacity = i32::from(d.alpha);
                    f.integer("Opacity", &mut opacity, [0, 255]);
                    d.alpha = opacity as u8;
                    f.number("Rotation", &mut d.rotation, [0., 359.], 1.);
                    f.toggle("Above tanks", &mut d.above);
                    f.color(&mut d.color);
                }
                Object::Trigger(t) => {
                    f.heading("Triggers");
                    let mut chance = i32::from(t.chance);
                    f.integer("Chance %", &mut chance, [1, 100]);
                    t.chance = chance as u8;
                    f.number("Delay (s)", &mut t.delay, [0., 3600.], 0.5);
                    f.toggle("Once per round", &mut t.once);
                }
                Object::Spawn(region) => {
                    f.stepper("Team (0 for any)", &mut region.team, [0, 4], None);
                }
                _ => {}
            }
        } else {
            f.note("Select objects of one type to edit their settings together.");
        }
        f.heading("Arrange");
        if let Some(axis) = f.buttons(&["Space X evenly", "Space Y evenly"], objects.len() >= 3) {
            request = Request::Distribute(axis);
        }
        if let Some(axis) = f.buttons(&["Match widest", "Match tallest"], true) {
            request = Request::MatchSize(axis);
        }
        f.note("Spacing keeps the outermost centers fixed. Matching sizes keeps centers where the map bounds allow. Each action is one undo step.");
        f.note("The Edit dock turns, flips, aligns and scales the selection together. Arrow keys or WASD nudge it; Shift moves one unit.");
        let end = f.y;
        if template != first {
            let keys = self.selection.clone();
            self.edit_property(|doc| {
                for key in keys {
                    if let Some(mut object) = doc.object(key) {
                        carry_changes(&first, &template, &mut object);
                        doc.replace(key, object);
                    }
                }
            });
        }
        (end, request)
    }

    fn logic_tab(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        let mut request = Request::None;
        let config = self.config();
        let preview = &self.preview;
        let (running, speed, time, counters, hidden) = (
            preview.running,
            preview.speed,
            preview.time,
            preview.counters,
            preview.hidden,
        );
        let (mut probe_team, mut probe_bot) = (preview.probe_team, preview.probe_bot);
        let clock = |at: f32| {
            let minutes = (at / 60.).floor();
            format!("{minutes:.0}:{:04.1}", at - minutes * 60.)
        };
        let events: Vec<String> = preview
            .events
            .iter()
            .rev()
            .take(6)
            .map(|e| format!("{}  {}", clock(e.at), e.text))
            .collect();
        let started = running || !preview.at_start();
        let waiting = preview.waiting();
        let map = &self.doc.map;
        // Groups worth showing: anything placed in one, switched or watched.
        let groups = self.doc.used_groups();
        // Counters worth showing: the first eight, any the map uses and any
        // not at 0.
        let mut shown_counters: Vec<u8> = (1..=8).collect();
        shown_counters.extend(self.doc.used_counters());
        shown_counters
            .extend((1..=MAP_COUNTERS).filter(|&counter| counters[usize::from(counter) - 1] != 0));
        shown_counters.sort_unstable();
        shown_counters.dedup();
        let watches_bosses = map.triggers.iter().any(|t| t.when == TriggerWhen::Defeated);
        // The boss Defeat pretends to beat: the one picked, else the first a
        // trigger watches for.
        let watched_boss = map
            .triggers
            .iter()
            .filter(|t| t.when == TriggerWhen::Defeated)
            .find_map(|t| defeated_boss(t).and_then(BossKind::from_id))
            .unwrap_or(BossKind::ALL[0]);
        struct LogicRow {
            tag: String,
            summary: String,
            state: Vec<(String, Option<bool>)>,
            chosen: bool,
        }
        let rows: Vec<_> = map
            .triggers
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut state = Vec::new();
                if started {
                    if preview.spent(i) {
                        state.push(("spent".to_owned(), None));
                    } else if let Some(at) = preview.next_at(i).filter(|&at| at > time) {
                        state.push((format!("next {:.1} s", at - time), None));
                    }
                    if t.action == TriggerAction::Sequence {
                        let channel = t.group.saturating_add(preview.sequence_step(i));
                        state.push((format!("next sends {channel}"), None));
                    }
                    if let Some(view) = preview.gate_view(t, &config) {
                        state.push((
                            if view.open { "open" } else { "shut" }.to_owned(),
                            Some(view.open),
                        ));
                    }
                }
                LogicRow {
                    tag: trigger_tag(t),
                    summary: describe_trigger_named(t, &self.doc.map.group_names),
                    state,
                    chosen: self.selected(Key::Trigger(i)),
                }
            })
            .collect();
        let mut toggle_run = false;
        let mut step = false;
        let mut new_speed = speed;
        let mut send = None;
        let mut channel = self.send_channel;
        let mut boss = self.send_boss.unwrap_or(watched_boss);
        let mut f = self.form(x, y, w, s, 4);
        f.heading("Preview");
        label(&clock(time), x, f.y + 8. * s, 22. * s, text_color());
        if waiting > 0 {
            right_label(
                &format!(
                    "{waiting} delayed action{} waiting",
                    if waiting == 1 { "" } else { "s" }
                ),
                x + w,
                f.y + 6. * s,
                11.5 * s,
                muted_color(),
            );
        }
        f.y += 22. * s;
        let speed_text = format!("\u{d7}{speed}");
        match f.buttons(
            &[
                if running { "Pause  P" } else { "Play  P" },
                "Step",
                "Restart",
                &speed_text,
            ],
            true,
        ) {
            Some(0) => toggle_run = true,
            Some(1) => step = true,
            Some(2) => request = Request::Restart,
            Some(_) => new_speed = if speed >= 4. { 0.25 } else { speed * 2. },
            None => {}
        }
        f.note("Hold T over the canvas to drive a test tank through area triggers. Step moves one tick while paused.");
        f.stepper("Test tank team", &mut probe_team, [0, 4], None);
        f.toggle("Test tank is a bot", &mut probe_bot);
        f.heading("Counters");
        let cell = (w - 3. * 6. * s) / 4.;
        for (i, &counter) in shown_counters.iter().enumerate() {
            let value = &counters[usize::from(counter) - 1];
            let r = Rect::new(
                x + (i % 4) as f32 * (cell + 6. * s),
                f.y + (i / 4) as f32 * 40. * s,
                cell,
                34. * s,
            );
            let over = contains(r);
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                if over {
                    hover_color()
                } else {
                    mix(surface_color(), bg_color(), 0.4)
                },
            );
            label(
                &format!("#{counter}"),
                r.x + 5. * s,
                r.y + 12. * s,
                10. * s,
                muted_color(),
            );
            right_label(
                &value.to_string(),
                r.right() - 6. * s,
                r.y + 28. * s,
                15. * s,
                if *value == 0 {
                    muted_color()
                } else {
                    text_color()
                },
            );
            if over && is_mouse_button_pressed(MouseButton::Left) {
                let add = if super::shift() { -1 } else { 1 };
                send = Some(tankyard_core::LogicInput::AddCounter(counter, add));
            } else if over && is_mouse_button_pressed(MouseButton::Right) {
                send = Some(tankyard_core::LogicInput::SetCounter(counter, 0));
            }
        }
        f.y += shown_counters.len().div_ceil(4) as f32 * 40. * s + 4. * s;
        f.note("Counters 1 to 8 show here, with any other the map uses or that isn't 0. Click a counter to add 1, Shift-click to take 1 away, right-click to clear it. Triggers watching it hear the change.");
        if groups != 0 {
            f.heading("Groups");
            let size = 30. * s;
            let per_row = ((w + 4. * s) / (size + 4. * s)).floor().max(1.) as usize;
            let used: Vec<u8> = (1..=MAP_GROUPS)
                .filter(|&g| groups & MapDef::group_bit(g) != 0)
                .collect();
            for (n, &group) in used.iter().enumerate() {
                let r = Rect::new(
                    x + (n % per_row) as f32 * (size + 4. * s),
                    f.y + (n / per_row) as f32 * (size + 4. * s),
                    size,
                    size,
                );
                let shown = hidden & MapDef::group_bit(group) == 0;
                let over = contains(r);
                if shown {
                    draw_rectangle(
                        r.x,
                        r.y,
                        r.w,
                        r.h,
                        alpha(accent(), if over { 0.5 } else { 0.32 }),
                    );
                } else {
                    draw_rectangle(
                        r.x,
                        r.y,
                        r.w,
                        r.h,
                        if over {
                            hover_color()
                        } else {
                            mix(surface_color(), bg_color(), 0.4)
                        },
                    );
                    draw_line(
                        r.x + 4. * s,
                        r.bottom() - 4. * s,
                        r.right() - 4. * s,
                        r.y + 4. * s,
                        1.,
                        muted_color(),
                    );
                }
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 1., line_color());
                center_label(
                    &group.to_string(),
                    r.center().x,
                    r.y + size * 0.66,
                    13. * s,
                    if shown { text_color() } else { muted_color() },
                );
                if over && is_mouse_button_pressed(MouseButton::Left) {
                    send = Some(tankyard_core::LogicInput::Group(group, !shown));
                }
            }
            f.y += used.len().div_ceil(per_row) as f32 * (size + 4. * s) + 6. * s;
            f.note("Lit groups are shown. Click one to show or hide it, as a script would; watchers and gates see it.");
        }
        f.heading("Send");
        f.stepper("Signal channel", &mut channel, [1, MAP_CHANNELS], None);
        let mut labels = vec!["Send signal"];
        let defeat = format!("Defeat {}", boss.name());
        if watches_bosses {
            labels.push(&defeat);
        }
        match f.buttons(&labels, true) {
            Some(0) => send = Some(tankyard_core::LogicInput::Signal(channel)),
            Some(_) => send = Some(tankyard_core::LogicInput::Defeated(boss)),
            None => {}
        }
        if watches_bosses {
            f.cycle("Boss to defeat", &mut boss, &BossKind::ALL, |b| {
                b.name().to_owned()
            });
        }
        f.heading("What happened");
        if events.is_empty() {
            f.note("Nothing yet. Press Play to run the arena's triggers.");
        }
        for event in &events {
            f.note(event);
        }
        f.heading(&format!("Triggers ({})", rows.len()));
        if rows.is_empty() {
            f.note("No triggers yet. Build one from the Logic palette.");
        }
        for (
            i,
            LogicRow {
                tag,
                summary,
                state,
                chosen,
            },
        ) in rows.iter().enumerate()
        {
            let lines = wrapped_lines(summary, w - 16. * s, 12. * s);
            let chips = if state.is_empty() { 0. } else { 22. * s };
            let h = 26. * s + lines.len() as f32 * 16. * s + chips;
            let r = Rect::new(x, f.y, w, h);
            let over = contains(r);
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                if *chosen {
                    alpha(accent(), 0.14)
                } else if over {
                    hover_color()
                } else {
                    mix(surface_color(), bg_color(), 0.3)
                },
            );
            draw_rectangle(r.x, r.y, 3. * s, r.h, brushes::LOGIC);
            let fire = Rect::new(r.right() - 50. * s, r.y + 4. * s, 44. * s, 19. * s);
            label(
                &fitted_label(&format!("{}  {tag}", i + 1), 13. * s, fire.x - x - 16. * s),
                x + 9. * s,
                r.y + 17. * s,
                13. * s,
                text_color(),
            );
            let firing = small_button(fire, "Fire", s * 0.62);
            for (n, line) in lines.iter().enumerate() {
                label(
                    line,
                    x + 9. * s,
                    r.y + 34. * s + n as f32 * 16. * s,
                    12. * s,
                    muted_color(),
                );
            }
            let mut right = r.right() - 6. * s;
            for (text, gate) in state.iter().rev() {
                let chip_y = r.bottom() - 21. * s;
                match gate {
                    Some(open) => status_pill(text, right, chip_y, *open, s),
                    None => {
                        let cw = label_width(text, 11. * s) + 12. * s;
                        draw_rectangle(right - cw, chip_y, cw, 17. * s, alpha(line_color(), 0.35));
                        label(
                            text,
                            right - cw + 6. * s,
                            chip_y + 12.5 * s,
                            11. * s,
                            text_color(),
                        );
                    }
                }
                right -= label_width(text, 11. * s) + 18. * s;
            }
            if firing {
                request = Request::Fire(i);
            } else if over && !contains(fire) && is_mouse_button_pressed(MouseButton::Left) {
                request = Request::Locate(Key::Trigger(i));
            }
            f.y += h + 5. * s;
        }
        let end = f.y;
        self.preview.probe_team = probe_team;
        self.preview.probe_bot = probe_bot;
        self.preview.speed = new_speed;
        self.send_channel = channel;
        if boss != self.send_boss.unwrap_or(watched_boss) {
            self.send_boss = Some(boss);
        }
        if toggle_run {
            self.preview.running = !self.preview.running;
        }
        if step {
            self.preview.step_once(&self.doc.map, &config);
        }
        if let Some(input) = send {
            self.preview.send(&self.doc.map, &config, input);
        }
        (end, request)
    }

    fn check_tab(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        let mut request = Request::None;
        let problems = self.problems.clone().unwrap_or_default();
        let checked = self.problems.is_some();
        let m = &self.doc.map;
        let emitters = m
            .decor
            .iter()
            .filter(|d| d.kind == DecorKind::Emitter)
            .count();
        let budgets = [
            (
                "Walls and blocks",
                m.walls.len() + m.blocks.len(),
                MAP_BLOCKS,
            ),
            ("Pads", m.zones.len(), MAP_ZONES),
            ("Decor", m.decor.len(), MAP_DECOR),
            ("Particle emitters", emitters, 48),
            ("Triggers", m.triggers.len(), MAP_TRIGGERS),
            ("Paths", m.paths.len(), MAP_PATHS),
            ("Bodies", m.bodies.len(), usize::from(MAP_GROUPS)),
            ("Spawn regions", m.spawn_regions.len(), 32),
        ];
        let mut f = self.form(x, y, w, s, 6);
        if f.buttons(&["Check again"], true).is_some() {
            request = Request::Check;
        }
        if !checked {
            f.note("Checks look for unfinished triggers, missing spawn regions, blocked portal exits and logic that nothing uses.");
        } else if problems.is_empty() {
            f.heading("Ready to play");
            f.note("No problems found. Playtest with F5, or test from the pointer with Shift+F5.");
        }
        for problem in &problems {
            let lines = wrapped_lines(&problem.text, w - 22. * s, 12.5 * s);
            let h = 14. * s + lines.len() as f32 * 17. * s;
            let r = Rect::new(x, f.y, w, h);
            let over = problem.key.is_some() && contains(r);
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                if over {
                    hover_color()
                } else {
                    mix(surface_color(), bg_color(), 0.3)
                },
            );
            draw_rectangle(
                r.x,
                r.y,
                3. * s,
                r.h,
                if problem.blocking { RED } else { AMBER },
            );
            for (n, line) in lines.iter().enumerate() {
                label(
                    line,
                    x + 11. * s,
                    r.y + 18. * s + n as f32 * 17. * s,
                    12.5 * s,
                    text_color(),
                );
            }
            if over
                && is_mouse_button_pressed(MouseButton::Left)
                && let Some(key) = problem.key
            {
                request = Request::Locate(key);
            }
            f.y += h + 5. * s;
        }
        if problems.iter().any(|p| p.blocking) {
            f.y += 4. * s;
            f.note("Red items stop playtests and exports. Click an item to find it.");
        } else if !problems.is_empty() {
            f.y += 4. * s;
            f.note("Amber notes are worth a look but do not stop anything.");
        }
        f.heading("Room left");
        for (title, used, limit) in budgets {
            f.meter(title, used, limit);
        }
        (f.y, request)
    }

    fn rules_tab(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        let mut rules = self.doc.rules;
        let mut y = y;
        label(
            "Playtest and pack rules",
            x,
            y + 12. * s,
            16. * s,
            text_color(),
        );
        y += 34. * s;
        crate::rules_ui::controls(&mut rules, Rect::new(x, y, w, 280. * s), s);
        y += 9. * 54. * s;
        crate::rules_ui::presets(&mut rules, Rect::new(x, y, w, 30. * s), s);
        y += 90. * s;
        let end = wrap(
            "Rules travel with the exported pack. Hosts can also change them live from Server → Rules. Customized matches do not count toward standard records.",
            x,
            y,
            w,
            13. * s,
            muted_color(),
        ) + 25. * s;
        if rules != self.doc.rules {
            self.edit_property(|doc| doc.rules = rules);
        }
        (end, Request::None)
    }

    fn tools_tab(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        use tankyard_lua::editor::{Param, ParamValue, Tool};
        let mut request = Request::None;
        let (packs, errors) = match &self.workshop.plugins {
            Some(plugins) => (
                plugins
                    .plugins
                    .iter()
                    .map(|p| {
                        let checks = p.checks.join(", ");
                        (
                            p.id.clone(),
                            format!("{}  {}", p.name, p.version),
                            p.tools.clone(),
                            checks,
                            p.stopped.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
                plugins.errors.clone(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        let held = self.workshop.held.clone();
        let report = self.workshop.report.clone();
        let values: Vec<Vec<Vec<ParamValue>>> = packs
            .iter()
            .enumerate()
            .map(|(p, (_, _, tools, _, _))| {
                (0..tools.len()).map(|t| self.tool_values(p, t)).collect()
            })
            .collect();
        let mut f = self.form(x, y, w, s, 7);
        f.heading("Plugins");
        if f.buttons(&["Reload plugins"], true).is_some() {
            request = Request::ReloadPlugins;
        }
        if packs.is_empty() {
            f.note("No workshop plugins are on. Enable a pack with editor tools in Mods, such as Workshop Toolkit, then press Reload.");
        }
        for line in held.iter().chain(&errors) {
            f.note(line);
        }
        if !report.is_empty() {
            f.heading("Last run");
            for line in report.iter().take(8) {
                f.note(line);
            }
        }
        let mut y = f.y;
        let mut changed = Vec::new();
        let mut fold = None;
        for (p, (id, title, tools, checks, stopped)) in packs.iter().enumerate() {
            let folded = self.prefs.folded.contains(id);
            let mut f = self.form(x, y, w, s, 800 + p as u64 * 64);
            let toggle = Rect::new(x + w - 64. * s, f.y + 2. * s, 64. * s, 22. * s);
            if button(
                toggle,
                if folded { "Show" } else { "Hide" },
                false,
                true,
                s * 0.62,
            ) {
                fold = Some(id.clone());
            }
            f.heading(title);
            if let Some(reason) = stopped {
                f.note(&format!(
                    "Stopped: {reason}. Reload plugins to start it again."
                ));
            }
            if folded {
                f.note(&match tools.len() {
                    1 => "1 tool put away.".to_owned(),
                    n => format!("{n} tools put away."),
                });
                y = f.y;
                continue;
            }
            if !checks.is_empty() {
                f.note(&format!("Adds checks to Check: {checks}."));
            }
            y = f.y;
            for (
                t,
                Tool {
                    id: tool_id,
                    name,
                    description,
                    params,
                },
            ) in tools.iter().enumerate()
            {
                let mut f = self.form(x, y, w, s, 801 + p as u64 * 64 + t as u64);
                f.y += 8. * s;
                draw_rectangle(x, f.y, 3. * s, 18. * s, accent());
                label(name, x + 10. * s, f.y + 14. * s, 14. * s, text_color());
                f.y += 32. * s;
                if !description.is_empty() {
                    f.note(description);
                }
                let mut current = values[p][t].clone();
                for (spec, value) in params.iter().zip(current.iter_mut()) {
                    match (&spec.kind, value) {
                        (Param::Number { min, max, step, .. }, ParamValue::Number(v)) => {
                            f.number(&spec.name, v, [*min, *max], *step);
                        }
                        (Param::Integer { min, max, .. }, ParamValue::Integer(v)) => {
                            f.integer(&spec.name, v, [*min, *max]);
                        }
                        (Param::Toggle(_), ParamValue::Toggle(v)) => {
                            f.toggle(&spec.name, v);
                        }
                        (Param::Choice { options, .. }, ParamValue::Choice(v)) => {
                            let indices: Vec<usize> = (0..options.len()).collect();
                            f.cycle(&spec.name, v, &indices, |i| options[i].clone());
                        }
                        (Param::Text { limit, .. }, ParamValue::Text(v)) => {
                            f.text(&spec.name, v, *limit);
                        }
                        _ => {}
                    }
                }
                if f.buttons(&[&format!("Run {name}")], stopped.is_none())
                    .is_some()
                {
                    request = Request::RunTool(p, t);
                }
                if current != values[p][t] {
                    changed.push(((id.clone(), tool_id.clone()), current));
                }
                y = f.y + 6. * s;
            }
        }
        let mut f = self.form(x, y, w, s, 9);
        f.note("Ctrl+R runs the last tool again. A tool's changes undo as one step.");
        let end = f.y;
        for (key, value) in changed {
            self.workshop.values.insert(key, value);
        }
        if let Some(id) = fold {
            match self.prefs.folded.iter().position(|known| *known == id) {
                Some(at) => {
                    self.prefs.folded.remove(at);
                }
                None => self.prefs.folded.push(id),
            }
        }
        (end, request)
    }

    fn files_tab(&mut self, x: f32, y: f32, w: f32, s: f32) -> (f32, Request) {
        let mut request = Request::None;
        let mut y = y;
        if button(
            Rect::new(x, y, w, 30. * s),
            "Import a map pack…",
            false,
            self.import_pending.is_none(),
            s * 0.8,
        ) {
            request = Request::Import;
        }
        y += 60. * s;
        label("Saved projects", x, y, 16. * s, text_color());
        y += 14. * s;
        if small_button(Rect::new(x, y, w * 0.48, 28. * s), "Refresh", s * 0.75) {
            request = Request::Refresh;
        }
        if small_button(
            Rect::new(x + w * 0.52, y, w * 0.48, 28. * s),
            "Open folder",
            s * 0.75,
        ) {
            request = Request::Reveal;
        }
        y += 40. * s;
        if self.library.is_empty() {
            y = wrap(
                "Projects you save appear here.",
                x,
                y + 4. * s,
                w,
                12.5 * s,
                muted_color(),
            ) + 8. * s;
        }
        let page = self.library_page;
        for index in (page * 8)..(page * 8 + 8).min(self.library.len()) {
            let path = &self.library[index];
            let title = path.file_stem().unwrap_or_default().to_string_lossy();
            let current = self.saved_path.as_ref() == Some(path);
            if button(
                Rect::new(x, y, w, 30. * s),
                &fitted_label(&title, 14. * s, w - 12. * s),
                current,
                true,
                s * 0.8,
            ) {
                request = match read_document(path) {
                    Ok(doc) => Request::Open(
                        Box::new(doc),
                        Some(path.clone()),
                        "Project opened. Undo returns to the map you had.",
                    ),
                    Err(error) => Request::Notice(error),
                };
            }
            y += 36. * s;
        }
        if self.library.len() > 8 {
            if small_button(Rect::new(x, y, w * 0.48, 28. * s), "Previous", s * 0.75) {
                self.library_page = self.library_page.saturating_sub(1);
            }
            if small_button(
                Rect::new(x + w * 0.52, y, w * 0.48, 28. * s),
                "Next",
                s * 0.75,
            ) {
                self.library_page = (self.library_page + 1).min((self.library.len() - 1) / 8);
            }
            y += 42. * s;
        }
        y = wrap(
            "Save keeps an editable project. Export makes a ZIP pack in exports/: import it on the Mods page, or list it in a dedicated server's mods. Packs include editor.json, so others can keep editing.",
            x,
            y + 6. * s,
            w,
            12.5 * s,
            muted_color(),
        ) + 26. * s;
        label("Start from a built-in arena", x, y, 15. * s, text_color());
        y += 14. * s;
        for template in &self.templates {
            if small_button(Rect::new(x, y, w, 29. * s), &template.name, s * 0.8) {
                let mut doc = Document::from_map(template.clone());
                request = match doc.bake_procedure() {
                    Ok(()) => {
                        doc.map.id = format!("my_{}", doc.map.id);
                        doc.map.name = format!("My {}", doc.map.name);
                        Request::Open(
                            Box::new(doc),
                            None,
                            "Arena copied. Undo returns to the map you had.",
                        )
                    }
                    Err(error) => Request::Notice(error),
                };
            }
            y += 35. * s;
        }
        (y, request)
    }
}

fn key_index(key: Key) -> usize {
    match key {
        Key::Wall(i)
        | Key::Block(i)
        | Key::Zone(i)
        | Key::Decor(i)
        | Key::Trigger(i)
        | Key::Spawn(i)
        | Key::Base(i)
        | Key::Objective(i) => i,
    }
}
/// Keeps typed fields of different objects apart.
fn key_salt(key: Key) -> u64 {
    let variant = match key {
        Key::Wall(_) => 0,
        Key::Block(_) => 1,
        Key::Zone(_) => 2,
        Key::Decor(_) => 3,
        Key::Trigger(_) => 4,
        Key::Spawn(_) => 5,
        Key::Base(_) => 6,
        Key::Objective(_) => 7,
    };
    variant << 32 | key_index(key) as u64
}

/// Copy onto `target` each setting that differs between `before` and `after`.
fn carry_changes(before: &Object, after: &Object, target: &mut Object) {
    if before.group() != after.group() && target.groupable() {
        target.set_group(after.group());
    }
    macro_rules! carry {
        ($a:expr, $b:expr, $t:expr, $($field:ident),+) => {{
            $(if $a.$field != $b.$field {
                $t.$field = $b.$field.clone();
            })+
        }};
    }
    match (before, after, target) {
        (Object::Block(a), Object::Block(b), Object::Block(t)) => carry!(a, b, t, style, color),
        (Object::Zone(a), Object::Zone(b), Object::Zone(t)) => {
            carry!(a, b, t, kind, strength, angle)
        }
        (Object::Decor(a), Object::Decor(b), Object::Decor(t)) => {
            carry!(a, b, t, alpha, rotation, above, color)
        }
        (Object::Trigger(a), Object::Trigger(b), Object::Trigger(t)) => {
            carry!(a, b, t, chance, delay, once)
        }
        (Object::Spawn(a), Object::Spawn(b), Object::Spawn(t)) => carry!(a, b, t, team),
        _ => {}
    }
}

/// Shown beside a condition that holds in the preview, and an open gate.
const HOLDS: Color = Color::new(0.24, 0.66, 0.38, 1.0);

/// A small status pill: "holds", "fails", "open", "shut".
fn status_pill(text: &str, right: f32, y: f32, ok: bool, s: f32) {
    let w = label_width(text, 11. * s) + 12. * s;
    let color = if ok { HOLDS } else { RED };
    draw_rectangle(right - w, y, w, 17. * s, alpha(color, 0.18));
    draw_rectangle(right - w, y, 2. * s, 17. * s, color);
    label(
        text,
        right - w + 7. * s,
        y + 12.5 * s,
        11. * s,
        text_color(),
    );
}

fn trigger_form(
    f: &mut Form,
    t: &mut MapTrigger,
    live: Option<&GateView>,
    (free_channel, used_channels): (Option<u8>, &[bool]),
    map_paths: usize,
    names: &[GroupName],
) {
    let s = f.s;
    let summary = describe_trigger_named(t, names);
    let lines = wrapped_lines(&summary, f.w - 18. * s, 13. * s);
    let h = 14. * s + lines.len() as f32 * 18. * s;
    draw_rectangle(f.x, f.y, f.w, h, alpha(brushes::LOGIC, 0.14));
    draw_rectangle(f.x, f.y, 3. * s, h, brushes::LOGIC);
    for (i, line) in lines.iter().enumerate() {
        label(
            line,
            f.x + 10. * s,
            f.y + 19. * s + i as f32 * 18. * s,
            13. * s,
            text_color(),
        );
    }
    f.y += h + 10. * s;
    f.heading("When");
    f.pick("", &mut t.when, &TriggerWhen::ALL, TriggerWhen::name, 3);
    match t.when {
        TriggerWhen::Every => {
            t.seconds = t.seconds.max(0.5);
            f.number("Every (s)", &mut t.seconds, [0.5, 3600.], 0.5);
            f.number("First wait (s)", &mut t.offset, [0., 3600.], 0.5);
        }
        TriggerWhen::Start => {
            f.number("Seconds into the round", &mut t.seconds, [0., 3600.], 0.5);
        }
        TriggerWhen::Enter | TriggerWhen::Exit | TriggerWhen::Cleared | TriggerWhen::Destroyed => {
            f.pick("Who", &mut t.who, &TriggerWho::ALL, TriggerWho::name, 3);
            f.stepper("Team (0 for any)", &mut t.team, [0, 4], None);
            f.number("Rest after acting (s)", &mut t.seconds, [0., 3600.], 0.5);
            f.note(match t.when {
                TriggerWhen::Cleared => "Acts when the last noticed tank leaves or is destroyed.",
                TriggerWhen::Destroyed => "Acts when a noticed tank is destroyed in the area.",
                _ => "The dashed area on the canvas is what tanks cross.",
            });
        }
        TriggerWhen::Occupied => {
            f.pick("Who", &mut t.who, &TriggerWho::ALL, TriggerWho::name, 3);
            f.stepper("Team (0 for any)", &mut t.team, [0, 4], None);
            t.threshold = t.threshold.clamp(1, TRIGGER_CROWD);
            f.integer("Tanks needed", &mut t.threshold, [1, TRIGGER_CROWD]);
            t.seconds = t.seconds.max(0.25);
            f.number("Every (s)", &mut t.seconds, [0.25, 3600.], 0.25);
            f.note("Acts as soon as enough tanks are inside, then again each period.");
        }
        TriggerWhen::Signal => {
            t.source = t.source.clamp(1, MAP_CHANNELS);
            f.stepper("Listen on channel", &mut t.source, [1, MAP_CHANNELS], None);
        }
        TriggerWhen::Counter => {
            t.source = t.source.clamp(1, MAP_COUNTERS);
            f.stepper("Watch counter", &mut t.source, [1, MAP_COUNTERS], None);
            f.integer("Reaches", &mut t.threshold, [-COUNTER_LIMIT, COUNTER_LIMIT]);
            f.note("Acts when the counter arrives at this value from either side.");
        }
        TriggerWhen::Changed => {
            t.source = t.source.clamp(1, MAP_COUNTERS);
            f.stepper("Watch counter", &mut t.source, [1, MAP_COUNTERS], None);
            f.note("Acts every time the counter's value changes, up or down. Pair it with a gate to react to some values only.");
        }
        TriggerWhen::Shown | TriggerWhen::Hidden => {
            t.source = t.source.clamp(1, MAP_GROUPS);
            f.stepper("Watch group", &mut t.source, [1, MAP_GROUPS], None);
            f.note(if t.when == TriggerWhen::Shown {
                "Acts when a trigger, a restore timer or a script shows the group. The starting layout does not count."
            } else {
                "Acts when a trigger, a restore timer or a script hides the group. The starting layout does not count."
            });
        }
        TriggerWhen::Touch | TriggerWhen::Release => {
            t.source = t.source.clamp(1, MAP_GROUPS);
            f.stepper("Watch group's blocks", &mut t.source, [1, MAP_GROUPS], None);
            f.number("Rest after acting (s)", &mut t.seconds, [0., 3600.], 0.5);
            f.note(if t.when == TriggerWhen::Touch {
                "Acts when any block of the group starts to overlap the dashed area: a crate pushed onto a plate, a door sliding shut or a lift arriving."
            } else {
                "Acts when the last block of the group stops overlapping the dashed area. Pair it with On touch to make a pressure plate."
            });
        }
        TriggerWhen::Arrived => {
            t.source = t.source.clamp(1, MAP_GROUPS);
            f.stepper("Watch group", &mut t.source, [1, MAP_GROUPS], None);
            f.note("Acts each time a motion of the group ends: a move, turn or fade, the trip back included. Chain motions with it, or open a door once a lift arrives.");
        }
        TriggerWhen::Defeated => {
            let mut boss = defeated_boss(t).and_then(BossKind::from_id);
            let options: Vec<Option<BossKind>> = std::iter::once(None)
                .chain(BossKind::ALL.into_iter().map(Some))
                .collect();
            f.cycle("Boss", &mut boss, &options, |b| {
                b.map_or("Any boss".to_owned(), |b| b.name().to_owned())
            });
            t.source = boss.map_or(0, |b| {
                BOSS_IDS
                    .iter()
                    .position(|&id| id == b.id())
                    .map_or(0, |i| i as u8 + 1)
            });
            f.note("Acts when that boss is destroyed anywhere in the arena, whether a Boss trigger or the mode brought it.");
        }
    }
    f.heading("Do");
    let chosen = t.action;
    f.pick(
        "",
        &mut t.action,
        &TriggerAction::ALL,
        TriggerAction::name,
        3,
    );
    match t.action {
        TriggerAction::Toggle | TriggerAction::Show | TriggerAction::Hide => {
            if t.group == 0 {
                t.group = 1;
            }
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            f.number("Undo after (s, 0 keeps)", &mut t.duration, [0., 3600.], 0.5);
        }
        TriggerAction::Announce => {
            f.text("Announcement", &mut t.text, 96);
            f.note("{1} to {8} show counters. {cause} names the tank that set it off, or stays empty when there is none.");
        }
        TriggerAction::Boss => {
            let mut boss = BossKind::from_id(&t.boss).unwrap_or(BossKind::Sentinel);
            f.cycle("Bring", &mut boss, &BossKind::ALL, |b| b.name().to_owned());
            t.boss = boss.id().into();
            f.note("The boss arrives at the middle of this trigger's area.");
        }
        TriggerAction::Signal => {
            t.group = t.group.clamp(1, MAP_CHANNELS);
            f.stepper("Send on channel", &mut t.group, [1, MAP_CHANNELS], None);
        }
        TriggerAction::Count => {
            t.group = t.group.clamp(1, MAP_COUNTERS);
            f.stepper("Counter", &mut t.group, [1, MAP_COUNTERS], None);
            t.amount = t.amount.clamp(-COUNTER_LIMIT, COUNTER_LIMIT);
            f.integer(
                "Add (negative takes away)",
                &mut t.amount,
                [-COUNTER_LIMIT, COUNTER_LIMIT],
            );
        }
        TriggerAction::Shapes => {
            t.amount = t.amount.clamp(1, TRIGGER_SHAPES);
            f.integer("Shapes", &mut t.amount, [1, TRIGGER_SHAPES]);
            f.note("Shapes appear at random inside this trigger's area.");
        }
        TriggerAction::Set => {
            t.group = t.group.clamp(1, MAP_COUNTERS);
            f.stepper("Counter", &mut t.group, [1, MAP_COUNTERS], None);
            t.amount = t.amount.clamp(-COUNTER_LIMIT, COUNTER_LIMIT);
            f.integer("Value", &mut t.amount, [-COUNTER_LIMIT, COUNTER_LIMIT]);
        }
        TriggerAction::Random | TriggerAction::Sequence => {
            t.group = t.group.clamp(1, MAP_CHANNELS - 1);
            f.stepper("First channel", &mut t.group, [1, MAP_CHANNELS - 1], None);
            let most = i32::from(MAP_CHANNELS - t.group) + 1;
            t.amount = t.amount.clamp(2, most);
            f.integer("Channels", &mut t.amount, [2, most]);
            f.note(if t.action == TriggerAction::Random {
                "Sends on one channel, picked at random each time."
            } else {
                "Sends on the next channel each time it acts and starts over after the last. Reset triggers takes it back to the first."
            });
        }
        TriggerAction::Heal | TriggerAction::Hurt => {
            t.amount = t.amount.clamp(1, 100);
            f.integer("Percent of health", &mut t.amount, [1, 100]);
        }
        TriggerAction::Award => {
            t.amount = t.amount.clamp(1, TRIGGER_AWARD);
            f.integer("Points", &mut t.amount, [1, TRIGGER_AWARD]);
        }
        TriggerAction::Calculate => {
            counter_pick(
                f,
                "Counter",
                &mut t.group,
                &mut t.target_offset,
                0x0ca1_c001,
            );
            f.pick("", &mut t.math, &MathOp::ALL, MathOp::name, 3);
            if t.math.uses_operand() {
                let mut other = t.operand > 0 || t.operand_offset > 0;
                f.toggle("With another counter", &mut other);
                if other {
                    if t.operand == 0 && t.operand_offset == 0 {
                        t.operand = 1;
                    }
                    counter_pick(
                        f,
                        "Other counter",
                        &mut t.operand,
                        &mut t.operand_offset,
                        0x0ca1_c002,
                    );
                } else {
                    t.operand = 0;
                    t.operand_offset = 0;
                    f.integer("Number", &mut t.amount, [-COUNTER_LIMIT, COUNTER_LIMIT]);
                }
            }
            f.note(&format!(
                "Results stay within \u{b1}{}. Dividing by zero changes nothing, and so does a power below 0. Bits count from the lowest; shifting left by 1 doubles.",
                tankyard_content::counter_field("1:,", &[COUNTER_LIMIT]).unwrap_or_default()
            ));
        }
        TriggerAction::Script => {
            // Picked just now: turn whatever text it had into a name.
            if chosen != TriggerAction::Script && !script_event_name(t.text.trim()) {
                t.text = script_name(&t.text);
                if t.text.is_empty() {
                    t.text = "event".into();
                }
            }
            if f.text("Event name", &mut t.text, 32) {
                t.text = script_name(&t.text);
            }
            f.note("A script pack hears it with tankyard.on(\"map_event\", ...); event.name is this name. Without such a pack it does nothing.");
        }
        TriggerAction::Move | TriggerAction::Place => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            let place = t.action == TriggerAction::Place;
            let limit = TRAVEL_LIMIT * 100.;
            for (axis, title) in [
                if place {
                    "Right of its place (% of width)"
                } else {
                    "Right (% of width)"
                },
                if place {
                    "Below its place (% of height)"
                } else {
                    "Down (% of height)"
                },
            ]
            .into_iter()
            .enumerate()
            {
                let mut percent = t.travel[axis] * 100.;
                if f.number(title, &mut percent, [-limit, limit], 0.5) {
                    t.travel[axis] = percent / 100.;
                }
            }
            motion_form(f, t);
            f.note(if place {
                "Sends the group's blocks, pads, decor and trigger areas to this offset from where the map puts them; 0 and 0 bring them home. Drag the diamond on the canvas to set it."
            } else {
                "Moves the group's blocks, pads, decor and trigger areas this far from wherever they are now. Drag the diamond on the canvas to set it. Cover pushes tanks in its way."
            });
        }
        TriggerAction::Turn => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            t.amount = t.amount.clamp(-TURN_LIMIT, TURN_LIMIT);
            f.integer(
                "Turn by (degrees)",
                &mut t.amount,
                [-TURN_LIMIT, TURN_LIMIT],
            );
            motion_form(f, t);
            f.note("Turns the group's decor and emitters about their own centers, on top of any spin. Blocks and pads keep their shape.");
        }
        TriggerAction::Fade => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            t.amount = t.amount.clamp(0, 100);
            f.integer("Opacity (%)", &mut t.amount, [0, 100]);
            motion_form(f, t);
            f.note("Fades the group's decor and emitters. Blocks and pads stay solid; hide the group to take them away.");
        }
        TriggerAction::Tint => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            f.color(&mut t.color);
            t.amount = t.amount.clamp(0, 100);
            f.integer("Strength (%)", &mut t.amount, [0, 100]);
            motion_form(f, t);
            f.note("Mixes the colour into the group's decor, lights and particles; 100% paints them over completely and 0% clears the tint. A short Over and Go back after make a flash.");
        }
        TriggerAction::Scale => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            let most = (SCALE_LIMIT * 100.) as i32;
            t.amount = t.amount.clamp(0, most);
            f.integer("Size (% of placed)", &mut t.amount, [0, most]);
            motion_form(f, t);
            f.note("Grows or shrinks the group's decor and emitters about their own centres. Blocks and pads keep their size.");
        }
        TriggerAction::Animate => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            let style = t.style;
            f.pick("", &mut t.style, &AnimateStyle::ALL, AnimateStyle::name, 5);
            if t.style != style && t.speed == style.default_speed() {
                t.speed = 0.;
            }
            t.amount = t.amount.clamp(0, t.style.limit());
            f.integer(
                &format!("Size ({})", t.style.unit().trim()),
                &mut t.amount,
                [0, t.style.limit()],
            );
            let mut speed = t.animate_speed();
            if f.number("Times a second", &mut speed, [0.05, ANIMATE_SPEED], 0.05) {
                t.speed = speed;
            }
            motion_form(f, t);
            f.note(match t.style {
                AnimateStyle::Bob => "Floats the group's decor up and down. Over eases it in; size 0 settles it.",
                AnimateStyle::Sway => "Rocks the group's decor back and forth about their own centres. Size 0 settles it.",
                AnimateStyle::Throb => "Swells and shrinks the group's decor, like a heartbeat. Size 0 settles it.",
                AnimateStyle::Shake => "Makes the group's decor tremble. Use Go back after for a short jolt; size 0 settles it.",
                AnimateStyle::Orbit => "Circles the group's decor around where it rests. Size 0 settles it.",
            });
            f.note("Animations play together, so one group can bob and sway at once. They only change how decor looks: blocks, pads and trigger areas stay put. Reduced motion holds them still.");
        }
        TriggerAction::Stop => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            f.note("Halts the group's moves, turns, fades, tints and sizes where they are, freezes its animations and cancels any trip back. Nothing it was doing arrives, so On arrive triggers stay quiet.");
        }
        TriggerAction::Shake => {
            t.amount = t.amount.clamp(1, 100);
            f.integer("Strength (%)", &mut t.amount, [1, 100]);
            t.tween = t.tween.clamp(0.1, SHAKE_SECONDS);
            f.number("For (s)", &mut t.tween, [0.1, SHAKE_SECONDS], 0.1);
            f.note("Shakes only the views of the noticed tanks inside the area. Players can turn camera shake down in Settings, and reduced motion turns it off.");
        }
        TriggerAction::Sound => {
            f.pick("", &mut t.sound, &MapSound::ALL, MapSound::name, 4);
            t.amount = t.amount.clamp(1, 100);
            f.integer("Volume (%)", &mut t.amount, [1, 100]);
            f.note("Plays at the middle of this trigger's area, quieter for players further away. It uses the Effects volume.");
        }
        TriggerAction::Shove => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Body (group)", &mut t.group, [1, MAP_GROUPS], None);
            angle_form(f, &mut t.angle);
            t.amount = t.amount.clamp(1, SHOVE_LIMIT);
            f.integer("Speed (units/s)", &mut t.amount, [1, SHOVE_LIMIT]);
            f.note("Gives the body a push, as if a tank had rammed it; heavier bodies move less. The group must be a body: select one of its blocks and turn on Pushable.");
        }
        TriggerAction::Fling => {
            angle_form(f, &mut t.angle);
            t.amount = t.amount.clamp(1, FLING_LIMIT);
            f.integer("Speed (units/s)", &mut t.amount, [1, FLING_LIMIT]);
            f.note("Throws the noticed tanks inside the area that way; they coast for a moment, like off a launch pad.");
        }
        TriggerAction::Blast => {
            if t.amount == 0 {
                t.amount = 600;
            }
            t.amount = t.amount.clamp(-FLING_LIMIT, FLING_LIMIT);
            f.integer(
                "Speed (negative pulls in)",
                &mut t.amount,
                [-FLING_LIMIT, FLING_LIMIT],
            );
            f.note("Throws the noticed tanks inside the area away from its middle, or pulls them toward it with a negative speed.");
        }
        TriggerAction::Zoom => {
            t.amount = t.amount.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
            f.integer(
                "View (% of usual)",
                &mut t.amount,
                [ZOOM_RANGE.0, ZOOM_RANGE.1],
            );
            f.toggle("Hold until the next zoom", &mut t.hold);
            if !t.hold {
                t.tween = t.tween.clamp(0.1, ZOOM_SECONDS);
                f.number("For (s)", &mut t.tween, [0.1, ZOOM_SECONDS], 0.1);
            }
            let mut ease = if t.ease > 0. {
                t.ease
            } else {
                tankyard_core::ZoomView::EASE
            };
            if f.number("Change over (s)", &mut ease, [0.1, ZOOM_EASE], 0.1) {
                t.ease = ease;
            }
            f.pick("Curve", &mut t.easing.curve, &Curve::ALL, Curve::name, 4);
            f.pick("", &mut t.easing.mode, &EaseMode::ALL, EaseMode::name, 2);
            curve_graph(f, t.easing);
            let mut focus = i32::from(t.focus);
            f.integer("Turn toward the area (%)", &mut focus, [0, 100]);
            t.focus = focus.clamp(0, 100) as u8;
            f.note("Below 100 shows more of the arena; above 100 closes in. Only the noticed tanks inside see it. A new zoom changes the view from wherever it is; one that holds stays until the next, and a zoom of 100 that holds sets the view back. Turning moves the camera from the tank toward the middle of this trigger's area: 100% centers it there.");
        }
        TriggerAction::Light => {
            let most = (DARKNESS_LIMIT * 100.) as i32;
            t.amount = t.amount.clamp(0, most);
            f.integer("Darkness (%)", &mut t.amount, [0, most]);
            f.color(&mut t.color);
            motion_form(f, t);
            f.note("Darkens the whole arena toward the color. Lights, spotlights and beams shine through, and every tank carries a small light of its own. 0% brings full light back. With Go back after, the lighting returns to what it was, so a quick 0% makes a lightning flash.");
        }
        TriggerAction::Path => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Target group", &mut t.group, [1, MAP_GROUPS], None);
            let paths = i32::try_from(map_paths).unwrap_or(0).max(1);
            t.amount = t.amount.clamp(1, paths);
            f.integer("Path", &mut t.amount, [1, paths]);
            f.note(if map_paths == 0 {
                "This map has no paths yet. Add one in the Map tab, under Paths."
            } else {
                "Sends the group's blocks, pads, decor and trigger areas through each point of the path. A Move, Move to or Stop on the group takes it off the path."
            });
        }
        TriggerAction::Ui => {
            t.group = t.group.clamp(1, MAP_UI_SLOTS);
            f.stepper("UI slot", &mut t.group, [1, MAP_UI_SLOTS], None);
            f.note("Each slot holds one widget per player; showing another in the same slot replaces it.");
            let ui = t.ui.get_or_insert_with(|| Box::new(MapUi::starter()));
            ui_form(f, ui, &mut t.duration, free_channel, used_channels);
        }
        TriggerAction::HideUi => {
            t.group = t.group.min(MAP_UI_SLOTS);
            f.stepper("UI slot (0 for all)", &mut t.group, [0, MAP_UI_SLOTS], None);
            let mut to = t.ui.as_ref().map_or(UiAudience::Everyone, |ui| ui.to);
            f.pick("Hide from", &mut to, &UiAudience::ALL, UiAudience::name, 1);
            if to == UiAudience::Everyone && t.ui.is_none() {
                // Nothing to remember.
            } else {
                t.ui.get_or_insert_with(Default::default).to = to;
            }
        }
        TriggerAction::Reset => {
            t.group = t.group.min(MAP_GROUPS);
            f.stepper("Group (0 for all)", &mut t.group, [0, MAP_GROUPS], None);
            f.note("Starts that group's triggers afresh: waiting delays are dropped, Once per round triggers can act again, timers count from now and sequences go back to their first channel. A trigger's group is its Own group above.");
        }
    }
    let script = t.action == TriggerAction::Script;
    if (t.action.affects_tanks() || script || t.counts_inside()) && !t.when.watches_area() {
        f.pick("Who", &mut t.who, &TriggerWho::ALL, TriggerWho::name, 3);
        f.stepper("Team (0 for any)", &mut t.team, [0, 4], None);
    }
    if t.action.affects_tanks() || script {
        f.note(if script {
            "event.tanks lists the noticed tanks inside the trigger's area."
        } else {
            "Acts on every noticed tank inside the trigger's area."
        });
    }
    gate_form(f, t, live, free_channel);
    f.heading("Timing");
    f.number("Delay before acting (s)", &mut t.delay, [0., 3600.], 0.5);
    let mut chance = i32::from(t.chance);
    f.integer("Chance %", &mut chance, [1, 100]);
    t.chance = chance as u8;
    f.toggle("Once per round", &mut t.once);
}

thread_local! {
    /// The design last previewed and when it changed, so an edit replays
    /// the entrance.
    static UI_PREVIEW: std::cell::RefCell<(Option<HudWidget>, f64)> =
        const { std::cell::RefCell::new((None, 0.)) };
}

/// A Show UI trigger's widget: words, look, place, bar, choices and who
/// sees it, with a picture of the screen.
fn ui_form(
    f: &mut Form,
    ui: &mut MapUi,
    duration: &mut f32,
    free_channel: Option<u8>,
    used_channels: &[bool],
) {
    let s = f.s;
    // The picture first, so every change below shows straight away.
    let frame = Rect::new(f.x, f.y + 4. * s, f.w, f.w * 9. / 16.);
    draw_rectangle(
        frame.x,
        frame.y,
        frame.w,
        frame.h,
        Color::new(0.07, 0.09, 0.12, 1.),
    );
    let step = frame.w / 16.;
    for i in 1..16 {
        let x = frame.x + i as f32 * step;
        draw_line(
            x,
            frame.y,
            x,
            frame.bottom(),
            1.,
            Color::new(1., 1., 1., 0.04),
        );
    }
    for i in 1..9 {
        let y = frame.y + i as f32 * step;
        draw_line(
            frame.x,
            y,
            frame.right(),
            y,
            1.,
            Color::new(1., 1., 1., 0.04),
        );
    }
    // A tank in the middle, for scale, and the area a widget pinned over
    // its area floats over.
    let tank = vec2(frame.center().x, frame.y + frame.h * 0.62);
    if ui.pin == UiPin::Area {
        let area = Rect::new(
            frame.center().x - step * 2.5,
            frame.center().y - step * 1.5,
            step * 5.,
            step * 3.,
        );
        draw_rectangle(area.x, area.y, area.w, area.h, alpha(brushes::LOGIC, 0.12));
        draw_rectangle_lines(
            area.x,
            area.y,
            area.w,
            area.h,
            1.5,
            alpha(brushes::LOGIC, 0.7),
        );
    }
    draw_circle(tank.x, tank.y, step * 0.4, Color::new(0.0, 0.70, 0.88, 0.9));
    let pinned = match ui.pin {
        UiPin::Screen => None,
        UiPin::Area => Some(crate::hud_widgets::Pinned {
            at: frame.center(),
            above: None,
        }),
        UiPin::Cause => Some(crate::hud_widgets::Pinned {
            at: tank,
            above: Some(step * 0.4),
        }),
    };
    let mut shown = ui.widget.clone();
    if ui.bar > 0 || ui.timer {
        shown.progress = Some(0.6);
    }
    shown.text = fill_preview(&shown.text);
    shown.title = fill_preview(&shown.title);
    if let Some(board) = &mut shown.board {
        super::board_design::sample(board, &sample_counters(), fill_preview);
    }
    let age = UI_PREVIEW.with(|cell| {
        let mut cell = cell.borrow_mut();
        let mut design = ui.widget.clone();
        design.progress = None;
        if cell.0.as_ref() != Some(&design) {
            *cell = (Some(design), get_time());
        }
        (get_time() - cell.1) as f32
    });
    // Somewhat larger than its share of a real screen, so the words in the
    // picture stay readable.
    let scale = frame.w / 1000.;
    if shown.valid() {
        let pointer = ui_layout::pointer();
        crate::hud_widgets::preview(
            &shown,
            frame,
            scale,
            age,
            0.6,
            frame.contains(pointer).then_some(pointer),
            pinned,
        );
    } else {
        center_label(
            "Give the widget a title, text, a bar, choices or board cells",
            frame.center().x,
            frame.center().y + 20. * s,
            12. * s,
            muted_color(),
        );
    }
    draw_rectangle_lines(frame.x, frame.y, frame.w, frame.h, 1.5, line_color());
    f.y = frame.bottom() + 10. * s;
    if f.buttons(&["Replay entrance"], true).is_some() {
        UI_PREVIEW.with(|cell| cell.borrow_mut().1 = get_time());
    }

    f.heading("Board");
    let mut boarded = ui.widget.board.is_some();
    if f.toggle("Show a board of cells", &mut boarded) {
        if boarded {
            // A board takes the place of the text, bar and choices.
            ui.widget.board = Some(HudBoard::default());
            ui.widget.text.clear();
            ui.widget.progress = None;
            ui.widget.choices.clear();
            ui.channels.clear();
            (ui.bar, ui.bar_max, ui.timer) = (0, 0, false);
        } else {
            ui.widget.board = None;
        }
    }
    if let Some(board) = &mut ui.widget.board {
        super::board_design::board_form(f, board, used_channels);
    } else {
        f.note("A board is a grid of buttons, labels, screens, digits, lights and meters: a control panel, a keypad or the front of a computer.");
    }
    let boarded = ui.widget.board.is_some();
    let w = &mut ui.widget;
    f.heading("Words");
    f.text("Title", &mut w.title, HUD_TITLE);
    if !boarded {
        f.text("Text", &mut w.text, HUD_TEXT);
    }
    f.note("{1} shows counter 1, up to {255}; {left} the seconds before it goes, {time} the round's clock, {name} and {score} the player's own, and {cause} the tank that set it off. {1:04} pads to 4 digits, {1:,} groups thousands, {1:b} and {1:x} show binary and hex, and {3|2|1:4} joins counters as 4-digit parts of one big number.");
    f.heading("Look");
    f.note("A look sets the colors, shape, border, fill, lettering, bar and buttons in one click. You can change any of them afterwards.");
    for row in HudLook::ALL.chunks(4) {
        let names: Vec<&str> = row.iter().map(|look| look.name()).collect();
        if let Some(index) = f.buttons(&names, true) {
            row[index].apply(w);
        }
    }
    f.pick("Style", &mut w.style, &HudStyle::ALL, HudStyle::name, 3);
    if matches!(w.style, HudStyle::Panel | HudStyle::Card) {
        f.pick("Shape", &mut w.shape, &HudShape::ALL, HudShape::name, 3);
    }
    if w.style != HudStyle::Plain {
        f.pick("Border", &mut w.border, &HudBorder::ALL, HudBorder::name, 3);
        f.pick("Fill", &mut w.fill, &HudFill::ALL, HudFill::name, 3);
        if matches!(
            w.fill,
            HudFill::Gradient | HudFill::Stripes | HudFill::Grid | HudFill::Dots
        ) {
            let mut own = w.fill_color.is_some();
            if f.toggle("Own fill color", &mut own) {
                w.fill_color = own.then_some(w.fill_color.unwrap_or([90, 60, 160]));
            }
            if let Some(rgb) = &mut w.fill_color {
                f.color(rgb);
            }
        }
    }
    f.pick(
        "Lettering",
        &mut w.lettering,
        &HudLettering::ALL,
        HudLettering::name,
        3,
    );
    f.toggle("Capital letters", &mut w.caps);
    f.cycle("Icon", &mut w.icon, &HudIcon::ALL, |icon| {
        icon.name().to_owned()
    });
    f.number("Text size", &mut w.size, [10., 48.], 1.);
    f.toggle("Glow in the accent color", &mut w.glow);
    if w.style != HudStyle::Plain {
        let mut opacity = w.opacity * 100.;
        if f.number("Panel opacity (%)", &mut opacity, [0., 100.], 5.) {
            w.opacity = (opacity / 100.).clamp(0., 1.);
        }
    }
    let mut colors = [w.color, w.panel, w.accent, w.title_color];
    let defaults = [[240, 244, 248], [24, 30, 40], [0, 178, 225], [255, 214, 90]];
    for ((title, color), fallback) in [
        "Own text color",
        "Own panel color",
        "Own accent color",
        "Own title color",
    ]
    .into_iter()
    .zip(&mut colors)
    .zip(defaults)
    {
        if (title == "Own panel color" && w.style == HudStyle::Plain)
            || (title == "Own title color" && w.title.trim().is_empty())
        {
            continue;
        }
        let mut own = color.is_some();
        if f.toggle(title, &mut own) {
            *color = own.then_some(color.unwrap_or(fallback));
        }
        if let Some(rgb) = color {
            f.color(rgb);
        }
    }
    [w.color, w.panel, w.accent, w.title_color] = colors;
    f.note("Without its own colors, a widget follows the player's theme. The accent colors the border, stripe, icon, bar and choices.");
    f.heading("Place");
    f.pick("Where", &mut ui.pin, &UiPin::ALL, UiPin::name, 1);
    let w = &mut ui.widget;
    if ui.pin == UiPin::Screen {
        // Only pinned widgets point the way; keep the file plain.
        w.point = false;
        match f.buttons(&["Top", "Middle", "Bottom"], true) {
            Some(0) => w.y = 0.14,
            Some(1) => w.y = 0.5,
            Some(2) => w.y = 0.84,
            _ => {}
        }
        match f.buttons(&["Left", "Center", "Right"], true) {
            Some(0) => (w.x, w.align) = (0.02, HudAlign::Left),
            Some(1) => (w.x, w.align) = (0.5, HudAlign::Center),
            Some(2) => (w.x, w.align) = (0.98, HudAlign::Right),
            _ => {}
        }
        let mut x = w.x * 100.;
        if f.number("Across (%)", &mut x, [0., 100.], 1.) {
            w.x = (x / 100.).clamp(0., 1.);
        }
        let mut y = w.y * 100.;
        if f.number("Down (%)", &mut y, [0., 100.], 1.) {
            w.y = (y / 100.).clamp(0., 1.);
        }
    } else {
        f.toggle("Point the way when out of view", &mut w.point);
        f.note(if ui.pin == UiPin::Area {
            "Floats over the middle of this trigger's area and moves with the arena, like a sign."
        } else {
            "Floats just above the tank that set it off and follows it, like a name tag. It hides while that tank is down."
        });
        if w.point {
            f.note("While what it floats over is out of view, it waits near the edge of the screen, clear of the HUD, with an arrow toward it.");
        }
    }
    f.pick(
        "Line up by",
        &mut w.align,
        &HudAlign::ALL,
        HudAlign::name,
        3,
    );
    f.heading("Motion");
    f.pick("Entrance", &mut w.enter, &HudEnter::ALL, HudEnter::name, 5);
    f.toggle("Pulse to draw the eye", &mut w.pulse);

    if boarded {
        // Board buttons and meters take the widget's button and bar looks.
        let has = |kind: HudCellKind| {
            w.board
                .as_ref()
                .is_some_and(|board| board.cells.iter().any(|cell| cell.kind == kind))
        };
        let (buttons, meters) = (has(HudCellKind::Button), has(HudCellKind::Meter));
        if buttons || meters {
            f.heading("Cell looks");
        }
        if buttons {
            f.pick(
                "Buttons",
                &mut w.buttons,
                &HudButtons::ALL,
                HudButtons::name,
                4,
            );
        }
        if meters {
            f.pick("Meters", &mut w.bar_style, &HudBar::ALL, HudBar::name, 3);
            let mut segments = w.segments;
            f.stepper(
                "Meter pieces (0 for whole)",
                &mut segments,
                [0, HUD_SEGMENTS],
                None,
            );
            w.segments = segments;
        }
        f.heading("Who and how long");
        f.pick("Show to", &mut ui.to, &UiAudience::ALL, UiAudience::name, 1);
        f.number("Hide after (s, 0 keeps)", duration, [0., 3600.], 0.5);
        f.note("Boards stay up when a button is clicked. Hide them with a Hide UI trigger.");
        return;
    }
    f.heading("Bar");
    #[derive(Clone, Copy, PartialEq)]
    enum Bar {
        None,
        Fixed,
        Counter,
        Timer,
    }
    let mut bar = if ui.bar > 0 {
        Bar::Counter
    } else if ui.timer {
        Bar::Timer
    } else if ui.widget.progress.is_some() {
        Bar::Fixed
    } else {
        Bar::None
    };
    let before = bar;
    f.cycle(
        "Bar shows",
        &mut bar,
        &[Bar::None, Bar::Fixed, Bar::Counter, Bar::Timer],
        |bar| {
            match bar {
                Bar::None => "No bar",
                Bar::Fixed => "A set amount",
                Bar::Counter => "A counter",
                Bar::Timer => "Time left",
            }
            .to_owned()
        },
    );
    if bar != before {
        ui.bar = u8::from(bar == Bar::Counter);
        ui.bar_max = if bar == Bar::Counter {
            ui.bar_max.max(10)
        } else {
            0
        };
        ui.timer = bar == Bar::Timer;
        ui.widget.progress = (bar == Bar::Fixed).then_some(0.5);
    }
    match bar {
        Bar::None => {}
        Bar::Fixed => {
            let mut percent = ui.widget.progress.unwrap_or(0.5) * 100.;
            if f.number("Filled (%)", &mut percent, [0., 100.], 5.) {
                ui.widget.progress = Some((percent / 100.).clamp(0., 1.));
            }
        }
        Bar::Counter => {
            ui.bar = ui.bar.clamp(1, MAP_COUNTERS);
            f.stepper("Counter", &mut ui.bar, [1, MAP_COUNTERS], None);
            ui.bar_max = ui.bar_max.clamp(1, COUNTER_LIMIT);
            f.integer("Full at", &mut ui.bar_max, [1, COUNTER_LIMIT]);
        }
        Bar::Timer => {
            if *duration <= 0. {
                f.note("Set Hide after below: the bar empties as that time runs out.");
            }
        }
    }
    if bar != Bar::None {
        let w = &mut ui.widget;
        let mut segments = w.segments;
        f.stepper(
            "Pieces (0 for whole)",
            &mut segments,
            [0, HUD_SEGMENTS],
            None,
        );
        w.segments = segments;
        let mut width = w.width * 100.;
        if f.number("Bar width (% of screen)", &mut width, [2., 100.], 1.) {
            w.width = (width / 100.).clamp(0.02, 1.);
        }
        f.pick("Bar look", &mut w.bar_style, &HudBar::ALL, HudBar::name, 3);
        let mut own = w.bar_color.is_some();
        if f.toggle("Own bar color", &mut own) {
            w.bar_color = own.then_some(w.bar_color.unwrap_or([255, 176, 0]));
        }
        if let Some(rgb) = &mut w.bar_color {
            f.color(rgb);
        }
    }

    f.heading("Choices");
    match f.buttons(&["Add choice", "Remove choice"], true) {
        Some(0) if ui.widget.choices.len() < HUD_CHOICES => {
            let number = ui.widget.choices.len() + 1;
            ui.widget.choices.push(format!("Option {number}"));
            ui.channels.resize(ui.widget.choices.len() - 1, 0);
            ui.channels.push(free_channel.unwrap_or(0));
        }
        Some(1) => {
            ui.widget.choices.pop();
            ui.channels.truncate(ui.widget.choices.len());
        }
        _ => {}
    }
    ui.channels.resize(ui.widget.choices.len(), 0);
    let salt = f.set_salt(0x0c40_1ce5);
    for (index, (choice, channel)) in ui
        .widget
        .choices
        .iter_mut()
        .zip(ui.channels.iter_mut())
        .enumerate()
    {
        f.set_salt(0x0c40_1ce5 + index as u64);
        f.text(&format!("Choice {}", index + 1), choice, HUD_CHOICE);
        f.stepper(
            "Signals channel",
            channel,
            [0, MAP_CHANNELS],
            Some(free_channel),
        );
    }
    f.set_salt(salt);
    if ui.widget.choices.is_empty() {
        f.note("Add choices to make a menu. Picking one sends its signal, and the player who picked is the tank that set off what listens: set Who to Tank that set it off to reward or move just them.");
    } else {
        if ui.widget.choices.len() > 1 {
            f.pick(
                "Arrange",
                &mut ui.widget.layout,
                &HudLayout::ALL,
                HudLayout::name,
                3,
            );
        }
        f.pick(
            "Buttons",
            &mut ui.widget.buttons,
            &HudButtons::ALL,
            HudButtons::name,
            4,
        );
        f.toggle("Stay open after a pick", &mut ui.keep);
        // Keep choices pickable: never leave one blank.
        for (index, choice) in ui.widget.choices.iter_mut().enumerate() {
            if choice.trim().is_empty() {
                *choice = format!("Option {}", index + 1);
            }
        }
    }

    f.heading("Who and how long");
    f.pick("Show to", &mut ui.to, &UiAudience::ALL, UiAudience::name, 1);
    f.number("Hide after (s, 0 keeps)", duration, [0., 3600.], 0.5);
}

/// A Calculate counter: `base`, or with "Move along" on, the counter that
/// many places past it, by counter `by`'s value: a list of counters read
/// or written by position. `salt` keeps the two pickers' fields apart.
fn counter_pick(f: &mut Form, title: &str, base: &mut u8, by: &mut u8, salt: u64) {
    let previous = f.set_salt(salt);
    let mut moved = *by > 0;
    let lowest = if moved { 0 } else { 1 };
    *base = (*base).clamp(lowest, MAP_COUNTERS);
    f.stepper(title, base, [lowest, MAP_COUNTERS], None);
    if f.toggle("Move along by a counter's value", &mut moved) {
        *by = if moved { 1 } else { 0 };
        *base = (*base).max(1);
    }
    if moved {
        *by = (*by).clamp(1, MAP_COUNTERS);
        f.stepper("Places past it: counter", by, [1, MAP_COUNTERS], None);
        f.note(&format!(
            "Uses counter {} plus the value of counter {}: with counter {} at 3, counter {}. Past the last counter it does nothing.",
            base,
            by,
            by,
            u16::from(*base) + 3
        ));
    }
    f.set_salt(previous);
}

/// Sample counter values for the editor's picture of a widget.
fn sample_counters() -> MapCounters {
    const SAMPLES: [i32; 8] = [3, 7, 12, 1, 0, 25, 50, 99];
    let mut counters = MapCounters::default();
    for (counter, sample) in counters.iter_mut().zip(SAMPLES.iter().cycle()) {
        *counter = *sample;
    }
    counters
}

/// Sample values for a widget's fields in the editor's picture.
fn fill_preview(text: &str) -> String {
    let counters = sample_counters();
    tankyard_content::fill_ui_text(
        text,
        &tankyard_content::UiFill {
            counters: &counters,
            hidden: 0,
            left: 12.,
            time: 95.,
            name: "Player",
            cause: "Runner",
            score: 1250,
        },
        HUD_TEXT,
    )
}

/// The map's keyframed paths: add, remove and edit them a point at a time.
/// `pick` is the path and point being edited.
fn paths_form(f: &mut Form, paths: &mut Vec<MapPath>, pick: &mut (usize, usize)) {
    f.heading("Paths");
    let mut add = false;
    let mut remove = false;
    match f.buttons(&["Add path", "Remove path"], true) {
        Some(0) => add = paths.len() < MAP_PATHS,
        Some(1) => remove = !paths.is_empty(),
        _ => {}
    }
    if add {
        paths.push(MapPath {
            name: String::new(),
            points: vec![
                PathPoint {
                    at: [0.1, 0.],
                    ..PathPoint::default()
                },
                PathPoint::default(),
            ],
            repeat: PathRepeat::Loop,
        });
        *pick = (paths.len() - 1, 0);
    }
    if remove {
        paths.remove(pick.0.min(paths.len() - 1));
        *pick = (pick.0.saturating_sub(1), 0);
    }
    if paths.is_empty() {
        f.note("A path is a custom animation: a list of points a group travels through, each with its own timing and curve. A Play path trigger starts it.");
        return;
    }
    let count = paths.len() as i32;
    let mut number = (pick.0.min(paths.len() - 1) + 1) as i32;
    if f.integer(&format!("Path (of {count})"), &mut number, [1, count]) {
        *pick = ((number - 1) as usize, 0);
    }
    pick.0 = pick.0.min(paths.len() - 1);
    let path = &mut paths[pick.0];
    f.text("Name", &mut path.name, 32);
    f.pick(
        "Repeat",
        &mut path.repeat,
        &PathRepeat::ALL,
        PathRepeat::name,
        3,
    );
    match f.buttons(&["Add point", "Remove point"], true) {
        Some(0) if path.points.len() < PATH_POINTS => {
            let at = (pick.1 + 1).min(path.points.len());
            let copy = path.points[pick.1.min(path.points.len() - 1)];
            path.points.insert(at, copy);
            pick.1 = at;
        }
        Some(1) if path.points.len() > 1 => {
            path.points.remove(pick.1.min(path.points.len() - 1));
            pick.1 = pick.1.saturating_sub(1);
        }
        _ => {}
    }
    let points = path.points.len() as i32;
    pick.1 = pick.1.min(path.points.len() - 1);
    let mut point = (pick.1 + 1) as i32;
    if f.integer(&format!("Point (of {points})"), &mut point, [1, points]) {
        pick.1 = (point - 1) as usize;
    }
    let previous = if pick.1 == 0 {
        [0., 0.]
    } else {
        path.points[pick.1 - 1].at
    };
    let p = &mut path.points[pick.1];
    let limit = TRAVEL_LIMIT * 100.;
    for (axis, title) in [
        "Right of its place (% of width)",
        "Below its place (% of height)",
    ]
    .into_iter()
    .enumerate()
    {
        let mut percent = p.at[axis] * 100.;
        if f.number(title, &mut percent, [-limit, limit], 0.5) {
            p.at[axis] = percent / 100.;
        }
    }
    f.number(
        "Seconds to get here",
        &mut p.seconds,
        [0., TWEEN_SECONDS],
        0.1,
    );
    // Long hops take longer rather than breaking the speed limit.
    let far = (p.at[0] - previous[0])
        .abs()
        .max((p.at[1] - previous[1]).abs());
    p.seconds = p.seconds.max((far / MOVE_SPEED_LIMIT * 10.).ceil() / 10.);
    f.number("Wait here (s)", &mut p.pause, [0., TWEEN_SECONDS], 0.1);
    f.number("Decor turned (degrees)", &mut p.turn, [-3600., 3600.], 5.);
    if p.seconds > 0. {
        f.pick("Curve", &mut p.easing.curve, &Curve::ALL, Curve::name, 4);
        f.pick("", &mut p.easing.mode, &EaseMode::ALL, EaseMode::name, 2);
        curve_graph(f, p.easing);
    }
    f.note("Points are offsets from where the map places the group, so one path can move any group. The first point is reached from wherever the group is when the path starts.");
}

/// A direction in degrees, with buttons for the eight compass points.
fn angle_form(f: &mut Form, angle: &mut f32) {
    const POINTS: [(&str, f32); 8] = [
        ("→", 0.),
        ("↘", 45.),
        ("↓", 90.),
        ("↙", 135.),
        ("←", 180.),
        ("↖", 225.),
        ("↑", 270.),
        ("↗", 315.),
    ];
    let labels = POINTS.map(|(arrow, _)| arrow);
    if let Some(i) = f.buttons(&labels, true) {
        *angle = POINTS[i].1;
    }
    let mut degrees = angle.rem_euclid(360.);
    if f.number("Direction (degrees)", &mut degrees, [0., 359.], 5.) {
        *angle = degrees;
    }
}

/// A name for `group`, kept in `names`; an empty name removes it.
fn group_name_form(f: &mut Form, names: &mut Vec<GroupName>, group: u8) {
    let mut name = names
        .iter()
        .find(|n| n.group == group)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    if f.text(&format!("Group {group} name"), &mut name, 24) {
        names.retain(|n| n.group != group);
        if !name.trim().is_empty() {
            names.push(GroupName {
                group,
                name: name.trim().to_owned(),
            });
            names.sort_by_key(|n| n.group);
        }
    }
}

/// Whether `group`'s blocks make a body tanks can push, and how it moves.
fn body_form(f: &mut Form, bodies: &mut Vec<MapBody>, group: u8) {
    let mut pushable = bodies.iter().any(|b| b.group == group);
    if f.toggle(
        &format!("Pushable: group {group} is one body"),
        &mut pushable,
    ) {
        bodies.retain(|b| b.group != group);
        if pushable {
            bodies.push(MapBody {
                group,
                ..MapBody::default()
            });
            bodies.sort_by_key(|b| b.group);
        }
    }
    let Some(body) = bodies.iter_mut().find(|b| b.group == group) else {
        return;
    };
    f.heading("Body");
    f.pick("Slides", &mut body.axis, &BodyAxis::ALL, BodyAxis::name, 3);
    let titles = [
        "Reach left (% of width)",
        "Reach up (% of height)",
        "Reach right (% of width)",
        "Reach down (% of height)",
    ];
    for (i, title) in titles.into_iter().enumerate() {
        let sideways = i % 2 == 0;
        if (body.axis == BodyAxis::Across && !sideways)
            || (body.axis == BodyAxis::Along && sideways)
        {
            continue;
        }
        let mut percent = body.reach[i] * 100.;
        if f.number(title, &mut percent, [0., 100.], 1.) {
            body.reach[i] = percent / 100.;
        }
    }
    f.number("Weight", &mut body.weight, [0.25, 8.], 0.25);
    let mut spring = body.spring * 100.;
    if f.number(
        "Spring back (% of width a second)",
        &mut spring,
        [0., 100.],
        1.,
    ) {
        body.spring = spring / 100.;
    }
    f.number("Friction", &mut body.friction, [0.5, 20.], 0.5);
    let mut bounce = body.bounce * 100.;
    if f.number("Bounce (%)", &mut bounce, [0., 90.], 5.) {
        body.bounce = bounce / 100.;
    }
    f.toggle("Shots push it", &mut body.shots);
    f.note("Tanks push the whole group by driving into its blocks: decor, pads and trigger areas in the group ride along. One tank pushes weight 1 at its own speed. With a spring it slides home when let go, like a push door. It stops at other cover and never crushes a tank.");
}

/// How a Move, Place, Turn, Fade, Tint, Scale or Animate plays: how long,
/// along which curve, and whether it goes back.
fn motion_form(f: &mut Form, t: &mut MapTrigger) {
    f.number(
        "Over (s, 0 at once)",
        &mut t.tween,
        [0., TWEEN_SECONDS],
        0.1,
    );
    if t.action.moves() && t.tween > 0. {
        // Long moves take longer rather than breaking the speed limit.
        t.tween = t.tween.max(least_tween(t.travel));
    }
    if t.tween > 0. {
        f.pick("Curve", &mut t.easing.curve, &Curve::ALL, Curve::name, 4);
        f.pick("", &mut t.easing.mode, &EaseMode::ALL, EaseMode::name, 2);
        curve_graph(f, t.easing);
    }
    f.number(
        "Go back after (s, 0 stays)",
        &mut t.duration,
        [0., 3600.],
        0.5,
    );
}

/// The easing curve as a small graph, with a dot riding it so its feel shows.
fn curve_graph(f: &mut Form, easing: Easing) {
    let s = f.s;
    let r = Rect::new(f.x, f.y, f.w, 76. * s);
    draw_rectangle(r.x, r.y, r.w, r.h, alpha(text_color(), 0.05));
    // Overshooting curves need room above and below.
    let (low, high) = (-0.45, 1.45);
    let point = |t: f32, v: f32| {
        vec2(
            r.x + 10. * s + t * (r.w - 20. * s),
            r.bottom() - (v - low) / (high - low) * r.h,
        )
    };
    for v in [0., 1.] {
        let (a, b) = (point(0., v), point(1., v));
        draw_line(a.x, a.y, b.x, b.y, 1., alpha(muted_color(), 0.45));
    }
    let mut last = point(0., easing.apply(0.));
    for i in 1..=60 {
        let t = i as f32 / 60.;
        let p = point(t, easing.apply(t));
        draw_line(last.x, last.y, p.x, p.y, 2. * s, brushes::LOGIC);
        last = p;
    }
    // Play it over a second, hold, then start again.
    let t = if reduced_motion() {
        0.5
    } else {
        ((get_time() as f32) % 1.6).min(1.)
    };
    let value = easing.apply(t);
    let p = point(t, value);
    draw_circle(p.x, p.y, 4. * s, accent_color());
    // The same value as a slide along the bottom, as a group would move.
    let track = r.bottom() - 7. * s;
    let x = r.x + 10. * s + value.clamp(low, high) * (r.w - 20. * s);
    draw_circle(x, track, 3. * s, alpha(accent_color(), 0.7));
    f.y += r.h + 12. * s;
}

/// The gate: up to four conditions, how they combine and what a shut gate
/// sends instead, with what each one says in the preview right now.
fn gate_form(f: &mut Form, t: &mut MapTrigger, live: Option<&GateView>, free_channel: Option<u8>) {
    let s = f.s;
    f.heading("Only if");
    let mut tests: Vec<TriggerTest> = t.conditions().copied().collect();
    let mut remove = None;
    let mut raise = None;
    for (i, test) in tests.iter_mut().enumerate() {
        let salt = f.set_salt(0);
        f.set_salt(salt ^ (0x51_7c_c1_b7 * (i as u64 + 1)));
        let top = f.y;
        label(
            &format!("Condition {}", i + 1),
            f.x + 10. * s,
            f.y + 14. * s,
            13. * s,
            text_color(),
        );
        let close = Rect::new(f.x + f.w - 58. * s, f.y, 58. * s, 20. * s);
        if small_button(close, "Remove", s * 0.6) {
            remove = Some(i);
        }
        let mut right = close.x - 6. * s;
        if i > 0 {
            let up = Rect::new(right - 26. * s, f.y, 26. * s, 20. * s);
            if small_button(up, "\u{2191}", s * 0.6) {
                raise = Some(i);
            }
            right = up.x - 6. * s;
        }
        if let Some(holds) = live.and_then(|view| view.holds.get(i)) {
            status_pill(
                if *holds { "holds" } else { "fails" },
                right,
                f.y + 1.5 * s,
                *holds,
                s,
            );
        }
        f.y += 28. * s;
        let (x, w) = (f.x, f.w);
        f.x += 10. * s;
        f.w -= 10. * s;
        f.pick("", &mut test.kind, &TestKind::ALL, TestKind::name, 3);
        match test.kind {
            TestKind::Shown | TestKind::Hidden => {
                test.index = test.index.clamp(1, MAP_GROUPS);
                f.stepper("Group", &mut test.index, [1, MAP_GROUPS], None);
            }
            kind => {
                if kind == TestKind::Counter {
                    test.index = test.index.clamp(1, MAP_COUNTERS);
                    f.stepper("Counter", &mut test.index, [1, MAP_COUNTERS], None);
                }
                f.pick("", &mut test.compare, &Compare::ALL, Compare::symbol, 6);
                let mut other = test.against > 0;
                f.toggle("Against a counter", &mut other);
                if other {
                    test.against = test.against.clamp(1, MAP_COUNTERS);
                    f.stepper("Other counter", &mut test.against, [1, MAP_COUNTERS], None);
                } else {
                    test.against = 0;
                    let (title, limits) = match kind {
                        TestKind::Inside => ("Tanks", [0, TRIGGER_CROWD]),
                        TestKind::Time => ("Seconds", [0, COUNTER_LIMIT]),
                        _ => ("Value", [-COUNTER_LIMIT, COUNTER_LIMIT]),
                    };
                    test.value = test.value.clamp(limits[0], limits[1]);
                    f.integer(title, &mut test.value, limits);
                }
                match kind {
                    TestKind::Inside => {
                        f.note("Counts the noticed tanks (Who and Team) in this trigger's area.")
                    }
                    TestKind::Time => f.note("Whole seconds since the round started."),
                    _ => {}
                }
            }
        }
        f.x = x;
        f.w = w;
        draw_rectangle(
            f.x,
            top,
            3. * s,
            f.y - top - 6. * s,
            alpha(brushes::LOGIC, 0.8),
        );
        f.set_salt(salt);
    }
    if let Some(i) = remove {
        tests.remove(i);
    } else if let Some(i) = raise {
        tests.swap(i - 1, i);
    }
    if tests.len() < TRIGGER_CONDITIONS
        && f.buttons(
            &[if tests.is_empty() {
                "Add a condition"
            } else {
                "Add another condition"
            }],
            true,
        )
        .is_some()
    {
        // A copy of the last one is usually close to what comes next.
        tests.push(tests.last().copied().unwrap_or_default());
    }
    let count = tests.len();
    t.set_conditions(tests);
    if count >= 2 {
        f.y += 6. * s;
        f.pick("Combine", &mut t.gate, &GateMode::ALL, GateMode::name, 2);
    }
    if count > 0 {
        if let Some(view) = live {
            let passed = view.holds.iter().filter(|&&h| h).count();
            let text = format!(
                "{passed} of {} hold{}",
                view.holds.len(),
                if passed == 1 && view.holds.len() == 1 {
                    "s"
                } else {
                    ""
                }
            );
            label(&text, f.x, f.y + 13. * s, 12.5 * s, muted_color());
            status_pill(
                if view.open { "open" } else { "shut" },
                f.x + f.w,
                f.y + 1.5 * s,
                view.open,
                s,
            );
            f.y += 26. * s;
        }
        if free_channel.is_some() || t.otherwise > 0 {
            let mut otherwise = t.otherwise > 0;
            if f.toggle("Otherwise send a signal", &mut otherwise) {
                // A fresh channel; the trigger's own would feed back into it.
                t.otherwise = if otherwise {
                    free_channel.unwrap_or(t.otherwise)
                } else {
                    0
                };
            }
        } else {
            f.buttons(&["Otherwise signal: channels full"], false);
            f.note("All 255 signal channels are in use. Free a channel before adding an otherwise signal.");
        }
        if t.otherwise > 0 {
            f.stepper(
                "Otherwise channel",
                &mut t.otherwise,
                [1, MAP_CHANNELS],
                None,
            );
        }
        f.note(match t.gate {
            _ if count == 1 => "Checked when the trigger fires. While the gate is shut nothing happens, apart from the otherwise signal, and the trigger waits for its next chance.",
            GateMode::All => "Acts only while every condition holds: an AND gate.",
            GateMode::Any => "Acts while at least one condition holds: an OR gate.",
            GateMode::NotAll => "Acts unless every condition holds: a NAND gate.",
            GateMode::NoneOf => "Acts only while no condition holds: a NOR gate.",
            GateMode::One => "Acts while exactly one condition holds: an XOR gate.",
        });
    } else {
        f.note("Conditions look at counters, groups, the tanks inside or the round time when the trigger fires. Several make AND, OR, NAND, NOR and XOR gates.");
    }
}

/// Typed text made into a script event name: lowercase, `_` for anything
/// else, starting with a letter.
fn script_name(text: &str) -> String {
    // No trimming: a space typed between words must become `_` at once.
    let mut name: String = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    while name.starts_with(|c: char| !c.is_ascii_lowercase()) {
        name.remove(0);
    }
    name
}

fn effect_form(f: &mut Form, e: &mut ParticleEffect) {
    f.heading("Particles");
    for row in brushes::EFFECT_PRESETS.chunks(5).enumerate() {
        if let Some(i) = f.buttons(row.1, true) {
            *e = brushes::effect_preset(row.0 * 5 + i);
        }
    }
    f.pick(
        "Shape",
        &mut e.shape,
        &ParticleShape::ALL,
        ParticleShape::name,
        4,
    );
    f.pick("Emit from", &mut e.area, &EmitArea::ALL, EmitArea::name, 4);
    for (title, value, limits, step) in [
        ("Rate", &mut e.rate, [0.2, 60.], 0.2),
        ("Lifetime", &mut e.lifetime, [0.1, 8.], 0.1),
        ("Speed", &mut e.speed, [0., 800.], 5.),
        ("Direction", &mut e.direction, [0., 359.], 1.),
        ("Spread", &mut e.spread, [0., 360.], 5.),
        ("Gravity", &mut e.gravity, [-800., 800.], 10.),
        ("Drag", &mut e.drag, [0., 8.], 0.1),
        ("Start size", &mut e.size, [1., 120.], 1.),
        ("End size", &mut e.end_size, [0., 120.], 1.),
        ("Spin", &mut e.spin, [-1080., 1080.], 15.),
        ("Wobble", &mut e.wobble, [0., 200.], 2.),
        ("Twinkle", &mut e.twinkle, [0., 1.], 0.05),
    ] {
        f.number(title, value, limits, step);
    }
    e.rate = e.rate.min(240. / e.lifetime);
    f.toggle("Fade out", &mut e.fade);
    f.toggle("Glow (adds light)", &mut e.glow);
    f.heading("Start colour");
    f.color(&mut e.color);
    f.heading("End colour");
    f.color(&mut e.end_color);
}
