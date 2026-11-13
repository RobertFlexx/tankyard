//! The inspector beside the canvas: map settings, the selected objects'
//! properties, trigger logic and its preview, checks, rules and files.
use super::{Editor, Layout, Tab, brushes, directory, read_document};
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
    Document, Key, Object, defeated_boss, describe_trigger, map_id, trigger_tag,
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
                *self.seen = true;
            }
        }
    }
    /// A slider whose value can also be clicked and typed.
    pub fn number(&mut self, title: &str, value: &mut f32, limits: [f32; 2], step: f32) -> bool {
        let before = *value;
        let s = self.s;
        let format = move |v: f32| {
            if step >= 1. {
                format!("{v:.0}")
            } else if step >= 0.1 {
                format!("{v:.1}")
            } else {
                format!("{v:.2}")
            }
        };
        let id = self.id(title);
        let typing = *self.focus == Some(id);
        slider_value(
            Rect::new(self.x, self.y, self.w, 31. * s),
            title,
            value,
            limits,
            step,
            s * 0.74,
            |v| if typing { String::new() } else { format(v) },
        );
        let field = Rect::new(self.x + self.w - 86. * s, self.y - 4. * s, 86. * s, 24. * s);
        if let Some(text) = self.typed(id, field) {
            if let Ok(typed) = text.parse::<f32>()
                && typed.is_finite()
            {
                let typed = if step >= 1. { typed.round() } else { typed };
                *value = typed.clamp(limits[0], limits[1]);
            }
        } else {
            self.open(id, field, format(*value));
        }
        self.y += 48. * s;
        *value != before
    }
    pub fn integer(&mut self, title: &str, value: &mut i32, limits: [i32; 2]) -> bool {
        let mut v = *value as f32;
        let changed = self.number(title, &mut v, [limits[0] as f32, limits[1] as f32], 1.);
        *value = (v.round() as i32).clamp(limits[0], limits[1]);
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
        let box_w = 46. * s;
        let start = self.x + self.w - free_w - box_w - h * 2. - 8. * s;
        label(
            &fitted_label(title, 13. * s, start - self.x - 6. * s),
            self.x,
            self.y + h * 0.66,
            13. * s,
            text_color(),
        );
        if button(
            Rect::new(start, self.y, h, h),
            "−",
            false,
            *value > range[0],
            s * 0.8,
        ) {
            *value -= 1;
        }
        let field = Rect::new(start + h + 2. * s, self.y, box_w, h);
        let id = self.id(title);
        if let Some(text) = self.typed(id, field) {
            if let Ok(typed) = text.parse::<u8>() {
                *value = typed.clamp(range[0], range[1]);
            }
        } else {
            let shown = if *value == 0 && range[0] == 0 && free.is_some() {
                "none".to_owned()
            } else {
                value.to_string()
            };
            center_label(
                &shown,
                field.center().x,
                field.y + h * 0.68,
                14. * s,
                text_color(),
            );
            self.open(id, field, value.to_string());
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
        for (i, title) in ["Red", "Green", "Blue"].into_iter().enumerate() {
            let mut value = f32::from(rgb[i]);
            self.number(title, &mut value, [0., 255.], 1.);
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
    Notice(String),
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
        if clip.contains(ui_layout::pointer()) && !pointer_captured() {
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
        self.content_height = (end - clip.y + 22. * s).max(0.);
        self.scroll = self.scroll.min((self.content_height - clip.h).max(0.));
        if self.content_height > clip.h {
            let h = (clip.h * clip.h / self.content_height).max(20. * s);
            let at = self.scroll / (self.content_height - clip.h).max(1.) * (clip.h - h);
            draw_rectangle(r.right() - 5. * s, clip.y + at, 3. * s, h, accent_color());
        }
        self.fulfil(request);
    }
    fn fulfil(&mut self, request: Request) {
        match request {
            Request::None => {}
            Request::Locate(key) => {
                self.set_selection(vec![key]);
                self.center_on(key);
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
        let used = self.doc.used_groups();
        let members = {
            let mut mask = 0u32;
            for (_, _, group) in self.doc.items() {
                mask |= MapDef::group_bit(group);
            }
            mask
        };
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
        f.heading("Hidden when a round starts");
        let cell = (w - 7. * 4. * s) / 8.;
        for i in 0..32u8 {
            let group = i + 1;
            let r = Rect::new(
                x + f32::from(i % 8) * (cell + 4. * s),
                f.y + f32::from(i / 8) * (27. * s),
                cell,
                23. * s,
            );
            let on = hidden.contains(&group);
            if button(r, &group.to_string(), on, true, s * 0.6) {
                if on {
                    hidden.retain(|&g| g != group);
                } else {
                    hidden.push(group);
                    hidden.sort_unstable();
                }
            }
            if MapDef::group_bit(group) & members != 0 {
                draw_circle(r.right() - 4. * s, r.y + 4. * s, 2.5 * s, VIOLET);
            }
        }
        f.y += 4. * 27. * s + 6. * s;
        f.note("Lit groups start hidden; a trigger can show them. A dot marks groups with objects. Group 0 is always shown.");
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
            || hidden != map.hidden_groups;
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
            });
        }
        (end, Request::None)
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
        let free_channel = self.doc.next_free_channel();
        let (map_w, map_h) = (self.doc.map.width, self.doc.map.height);
        let mut hidden = self.doc.map.hidden_groups.clone();
        let mut locked = self.doc.locked;
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
                if z.kind == ZoneKind::Conveyor {
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
                let mut opacity = f32::from(d.alpha);
                f.number("Opacity", &mut opacity, [0., 255.], 1.);
                d.alpha = opacity as u8;
                if d.kind == DecorKind::Light {
                    f.number("Flicker", &mut d.flicker, [0., 1.], 0.05);
                    f.note("Lights brighten what lies under them. Opacity sets how strongly; flicker makes them waver like a flame.");
                }
                f.toggle("Above tanks", &mut d.above);
                if d.kind == DecorKind::Emitter {
                    let effect = d.effect.get_or_insert_default();
                    effect_form(&mut f, effect);
                } else {
                    f.heading("Colour");
                    f.color(&mut d.color);
                }
            }
            Object::Trigger(t) => trigger_form(&mut f, t, live.as_ref(), free_channel),
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
                    if z.kind == ZoneKind::Conveyor {
                        f.number("Angle", &mut z.angle, [0., 359.], 1.);
                    }
                }
                Object::Decor(d) => {
                    f.heading("Decorations");
                    let mut opacity = f32::from(d.alpha);
                    f.number("Opacity", &mut opacity, [0., 255.], 1.);
                    d.alpha = opacity as u8;
                    f.number("Rotation", &mut d.rotation, [0., 359.], 1.);
                    f.toggle("Above tanks", &mut d.above);
                    f.color(&mut d.color);
                }
                Object::Trigger(t) => {
                    f.heading("Triggers");
                    let mut chance = f32::from(t.chance);
                    f.number("Chance %", &mut chance, [1., 100.], 1.);
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
                    summary: describe_trigger(t),
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
        for (i, value) in counters.iter().enumerate() {
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
                &format!("#{}", i + 1),
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
            let counter = i as u8 + 1;
            if over && is_mouse_button_pressed(MouseButton::Left) {
                let add = if super::shift() { -1 } else { 1 };
                send = Some(tankyard_core::LogicInput::AddCounter(counter, add));
            } else if over && is_mouse_button_pressed(MouseButton::Right) {
                send = Some(tankyard_core::LogicInput::SetCounter(counter, 0));
            }
        }
        f.y += 84. * s;
        f.note("Click a counter to add 1, Shift-click to take 1 away, right-click to clear it. Triggers watching it hear the change.");
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
        f.stepper("Signal channel", &mut channel, [1, MAP_GROUPS], None);
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
            ("Walls and blocks", m.walls.len() + m.blocks.len(), 2048),
            ("Pads", m.zones.len(), 256),
            ("Decor", m.decor.len(), 1024),
            ("Particle emitters", emitters, 48),
            ("Triggers", m.triggers.len(), 128),
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
    free_channel: Option<u8>,
) {
    let s = f.s;
    let summary = describe_trigger(t);
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
            t.source = t.source.clamp(1, MAP_GROUPS);
            f.stepper("Listen on channel", &mut t.source, [1, MAP_GROUPS], None);
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
            f.note("{1} to {8} show the value of that counter.");
        }
        TriggerAction::Boss => {
            let mut boss = BossKind::from_id(&t.boss).unwrap_or(BossKind::Sentinel);
            f.cycle("Bring", &mut boss, &BossKind::ALL, |b| b.name().to_owned());
            t.boss = boss.id().into();
            f.note("The boss arrives at the middle of this trigger's area.");
        }
        TriggerAction::Signal => {
            t.group = t.group.clamp(1, MAP_GROUPS);
            f.stepper("Send on channel", &mut t.group, [1, MAP_GROUPS], None);
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
            t.group = t.group.clamp(1, MAP_GROUPS - 1);
            f.stepper("First channel", &mut t.group, [1, MAP_GROUPS - 1], None);
            let most = i32::from(MAP_GROUPS - t.group) + 1;
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
            t.group = t.group.clamp(1, MAP_COUNTERS);
            f.stepper("Counter", &mut t.group, [1, MAP_COUNTERS], None);
            f.pick("", &mut t.math, &MathOp::ALL, MathOp::name, 3);
            if t.math.uses_operand() {
                let mut other = t.operand > 0;
                f.toggle("With another counter", &mut other);
                if other {
                    t.operand = t.operand.clamp(1, MAP_COUNTERS);
                    f.stepper("Other counter", &mut t.operand, [1, MAP_COUNTERS], None);
                } else {
                    t.operand = 0;
                    f.integer("Number", &mut t.amount, [-COUNTER_LIMIT, COUNTER_LIMIT]);
                }
            }
            f.note("Results stay within \u{b1}99,999. Dividing by zero changes nothing.");
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
    let mut chance = f32::from(t.chance);
    f.number("Chance %", &mut chance, [1., 100.], 1.);
    t.chance = chance as u8;
    f.toggle("Once per round", &mut t.once);
}

/// How a Move, Place, Turn or Fade plays: how long, along which curve, and
/// whether it goes back.
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
            f.note("All 32 signal channels are in use. Free a channel before adding an otherwise signal.");
        }
        if t.otherwise > 0 {
            f.stepper("Otherwise channel", &mut t.otherwise, [1, MAP_GROUPS], None);
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
