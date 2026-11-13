//! The arena workshop: Build, Edit and Delete modes over a snapped grid, an
//! icon palette, groups, trigger logic with a live preview, checks, particle
//! authoring, portable packs, and playtests from anywhere on the map.
mod brushes;
mod canvas;
mod dock;
mod inspector;
mod panels;
mod plugins;
mod preview;

use crate::{
    render::View,
    settings::Settings,
    ui::{AMBER, *},
};
use macroquad::prelude::*;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tankyard_client::editor_document::{Document, Key, Object, Problem, Transform};
use tankyard_content::*;
use tankyard_core::{V2, WorldConfig};

pub enum Action {
    None,
    Exit,
    Play {
        loaded: Box<tankyard_mods::LoadedMods>,
        config: WorldConfig,
        /// Where the player starts, instead of a spawn region.
        start: Option<V2>,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Build,
    Edit,
    Delete,
    Pan,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Map,
    Object,
    Logic,
    Check,
    Rules,
    Files,
    Tools,
}
impl Tab {
    /// In the order of the inspector's tab bar.
    const ALL: [Self; 7] = [
        Self::Map,
        Self::Object,
        Self::Logic,
        Self::Check,
        Self::Rules,
        Self::Files,
        Self::Tools,
    ];
}
/// Where each part of the editor sits this frame.
#[derive(Clone, Copy)]
struct Layout {
    top: Rect,
    canvas: Rect,
    dock: Rect,
    inspector: Rect,
    s: f32,
}
/// Undo steps kept; each holds a whole document.
const HISTORY: usize = 48;

fn shift() -> bool {
    is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift)
}
fn alt() -> bool {
    is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt)
}
/// "1 object", "3 objects".
fn objects(count: usize) -> String {
    match count {
        1 => "1 object".to_owned(),
        n => format!("{n} objects"),
    }
}
/// A workshop file as "projects/name.json", or the full path elsewhere.
fn short_path(path: &Path) -> String {
    directory()
        .ok()
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}
fn command() -> bool {
    [
        KeyCode::LeftControl,
        KeyCode::RightControl,
        KeyCode::LeftSuper,
        KeyCode::RightSuper,
    ]
    .into_iter()
    .any(is_key_down)
}

pub struct Editor {
    doc: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    /// Bumped by every change; saving and drafts remember what they wrote.
    revision: u64,
    saved_revision: u64,
    draft_revision: u64,
    preview_revision: u64,
    /// The document before the gesture, key hold or property edit under way.
    pending: Option<Document>,
    property_edit: bool,
    key_edit: bool,
    /// Sorted, so lookups are binary searches.
    selection: Vec<Key>,
    hover: Option<Key>,
    clipboard: Vec<Object>,
    style_clipboard: Option<Object>,
    mode: Mode,
    category: usize,
    brush: usize,
    brushes: Vec<Vec<brushes::Brush>>,
    /// An object picked with a right click, built instead of the palette brush.
    stamp: Option<(Object, [f32; 2])>,
    build_group: u8,
    swipe: bool,
    snap: bool,
    grid: f32,
    step: usize,
    show_hidden: bool,
    show_groups: bool,
    show_links: bool,
    focus: Option<u8>,
    preview: preview::Preview,
    probe: Option<V2>,
    /// The Logic tab's signal channel and boss to send into the preview.
    send_channel: u8,
    send_boss: Option<tankyard_core::BossKind>,
    animate: bool,
    time: f32,
    view: View,
    fitted: bool,
    gesture: Option<canvas::Gesture>,
    panning: Option<Vec2>,
    last_click: Option<(Key, f64)>,
    nudge_since: f64,
    nudge_last: f64,
    tab: Tab,
    scroll: f32,
    content_height: f32,
    text_focus: Option<u64>,
    focus_seen: bool,
    entry: String,
    problems: Option<Vec<Problem>>,
    help: bool,
    saved_path: Option<PathBuf>,
    notice: String,
    notice_at: f64,
    last_autosave: f64,
    library: Vec<PathBuf>,
    library_page: usize,
    import_pending: Option<crate::file_dialog::Pending>,
    templates: Vec<MapDef>,
    close_confirm: bool,
    reset_confirm: bool,
    /// Set when a playtest ends, so the key that ended it does nothing here.
    resumed: bool,
    prefs: panels::Prefs,
    prefs_saved: panels::Prefs,
    prefs_checked: f64,
    resizing: Option<panels::Edge>,
    /// The edge under the pointer or being dragged, drawn lit.
    edge_lit: Option<panels::Edge>,
    edge_click: f64,
    cursor: macroquad::miniquad::CursorIcon,
    /// The last duplicate's keys and where its source sat, so the next
    /// duplicate of the moved copy repeats the move.
    repeat: Option<(Vec<Key>, [f32; 2])>,
    workshop: plugins::Workshop,
    /// Where the canvas was last clicked, in world units.
    last_pointer: Option<V2>,
    /// Where the canvas was last drawn, for plugins that ask what is in view.
    canvas_area: Rect,
}

impl Editor {
    pub fn new(map: Option<MapDef>) -> Self {
        let draft = directory()
            .ok()
            .map(|p| p.join("draft.json"))
            .and_then(|p| read_document(&p).ok());
        let restored = draft.is_some();
        let doc = draft.unwrap_or_else(|| map.map(Document::from_map).unwrap_or_default());
        let mut preview = preview::Preview::new(&doc.map);
        let prefs = panels::Prefs::load();
        preview.speed = prefs.logic_speed;
        let catalog = brushes::catalog();
        let category = prefs.palette.min(catalog.len() - 1);
        let brush = if prefs.brush < catalog[category].len() {
            prefs.brush
        } else {
            1.min(catalog[category].len() - 1)
        };
        // Back where the camera was, when this is the map it was over.
        let mut view = View::new();
        let saved_view = prefs.view.as_ref().filter(|saved| {
            saved.map == doc.map.id && saved.size == [doc.map.width, doc.map.height]
        });
        if let Some(saved) = saved_view {
            view.center = V2::new(saved.center[0], saved.center[1]);
            view.zoom = saved.zoom;
        }
        let fitted = saved_view.is_some();
        let mut editor = Self {
            doc,
            undo: vec![],
            redo: vec![],
            revision: 0,
            saved_revision: 0,
            draft_revision: 0,
            preview_revision: 0,
            pending: None,
            property_edit: false,
            key_edit: false,
            selection: vec![],
            hover: None,
            clipboard: vec![],
            style_clipboard: None,
            mode: Mode::Build,
            category,
            brush,
            brushes: catalog,
            stamp: None,
            build_group: 0,
            swipe: false,
            snap: prefs.snap,
            grid: prefs.grid,
            step: prefs.step,
            show_hidden: prefs.show_hidden,
            show_groups: prefs.show_groups,
            show_links: prefs.show_links,
            focus: None,
            preview,
            probe: None,
            send_channel: 1,
            send_boss: None,
            animate: prefs.animate,
            time: 0.,
            view,
            fitted,
            gesture: None,
            panning: None,
            last_click: None,
            nudge_since: 0.,
            nudge_last: 0.,
            tab: Tab::ALL
                .get(prefs.inspector_tab)
                .copied()
                .unwrap_or(Tab::Object),
            scroll: 0.,
            content_height: 0.,
            text_focus: None,
            focus_seen: false,
            entry: String::new(),
            problems: None,
            help: false,
            saved_path: None,
            notice: String::new(),
            notice_at: 0.,
            last_autosave: get_time(),
            library: vec![],
            library_page: 0,
            import_pending: None,
            templates: builtin_maps(),
            close_confirm: false,
            reset_confirm: false,
            resumed: false,
            prefs_saved: prefs.clone(),
            prefs,
            prefs_checked: 0.,
            resizing: None,
            edge_lit: None,
            edge_click: 0.,
            cursor: macroquad::miniquad::CursorIcon::Default,
            repeat: None,
            workshop: plugins::Workshop::default(),
            last_pointer: None,
            canvas_area: Rect::default(),
        };
        editor.say(if restored {
            "Recovered your last draft. Press F1 for shortcuts."
        } else {
            "Choose a tool, or press F1 for every shortcut."
        });
        editor.refresh_library();
        editor
    }

    // History. Discrete actions use `apply`; drags and held keys `begin`
    // once and `commit` when they end; inspector edits use `edit_property`,
    // which folds one slider drag or typing session into one step.
    fn push_undo(&mut self, before: Document) {
        self.undo.push(before);
        if self.undo.len() > HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.touch();
    }
    fn touch(&mut self) {
        self.revision += 1;
        self.close_confirm = false;
        self.reset_confirm = false;
    }
    fn apply(&mut self, change: impl FnOnce(&mut Self)) {
        let before = self.doc.clone();
        change(self);
        if self.doc != before {
            self.push_undo(before);
        }
    }
    fn begin(&mut self) {
        if self.pending.is_none() {
            self.pending = Some(self.doc.clone());
        }
    }
    fn commit(&mut self) {
        if let Some(before) = self.pending.take()
            && before != self.doc
        {
            self.push_undo(before);
        }
    }
    fn edit_property(&mut self, change: impl FnOnce(&mut Document)) {
        if !self.property_edit {
            self.undo.push(self.doc.clone());
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.property_edit = true;
        }
        change(&mut self.doc);
        self.touch();
    }
    fn undo(&mut self, redo: bool) {
        if self.gesture.is_some() || self.pending.is_some() {
            return;
        }
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some(doc) = from.pop() else {
            self.say(if redo {
                "Nothing to redo."
            } else {
                "Nothing to undo."
            });
            return;
        };
        to.push(std::mem::replace(&mut self.doc, doc));
        self.property_edit = false;
        let doc = &self.doc;
        self.selection.retain(|&key| doc.rect(key).is_some());
        self.touch();
    }

    fn say(&mut self, text: impl Into<String>) {
        self.notice = text.into();
        self.notice_at = get_time();
    }
    fn typing(&self) -> bool {
        self.text_focus.is_some()
    }
    fn set_selection(&mut self, mut keys: Vec<Key>) {
        keys.sort_unstable();
        keys.dedup();
        self.selection = keys;
    }
    fn selected(&self, key: Key) -> bool {
        self.selection.binary_search(&key).is_ok()
    }
    fn toggle_selected(&mut self, key: Key) {
        match self.selection.binary_search(&key) {
            Ok(i) => {
                self.selection.remove(i);
            }
            Err(i) => self.selection.insert(i, key),
        }
    }

    // Selection actions shared by the dock, the inspector and shortcuts.
    fn step_size(&self) -> f32 {
        [1., self.grid * 0.5, self.grid, self.grid * 4.][self.step]
    }
    fn nudge_by(&mut self, dx: f32, dy: f32, fine: bool) {
        let step = if fine { 1. } else { self.step_size() };
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let keys = self.selection.clone();
        self.apply(|editor| {
            editor.doc.nudge(&keys, dx * step / w, dy * step / h);
        });
    }
    fn transform(&mut self, transform: Transform) {
        let keys = self.selection.clone();
        self.apply(|editor| editor.doc.transform(&keys, transform));
    }
    fn scale(&mut self, factor: f32) {
        let keys = self.selection.clone();
        self.apply(|editor| editor.doc.scale(&keys, factor));
    }
    fn order(&mut self, front: bool) {
        let keys = self.selection.clone();
        self.apply(|editor| {
            let moved = editor.doc.order(&keys, front);
            editor.set_selection(moved);
        });
        self.say(if front {
            "Moved to the front. Triggers in front run last."
        } else {
            "Moved to the back. Triggers at the back run first."
        });
    }
    fn copy(&mut self) {
        self.clipboard = self
            .selection
            .iter()
            .filter_map(|&key| self.doc.object(key))
            .collect();
        if !self.clipboard.is_empty() {
            self.say(format!("Copied {}.", objects(self.clipboard.len())));
        }
    }
    /// Paste with the copied objects' top-left corner at `at`, or one step
    /// from where they were copied.
    fn paste(&mut self, at: Option<Vec2>) {
        if self.clipboard.is_empty() {
            return;
        }
        let at = at.map(|p| [p.x, p.y]).unwrap_or_else(|| {
            let first = self
                .clipboard
                .iter()
                .map(Object::rect)
                .fold([1f32, 1f32], |lo, r| [lo[0].min(r[0]), lo[1].min(r[1])]);
            [
                first[0] + self.grid / self.doc.map.width,
                first[1] + self.grid / self.doc.map.height,
            ]
        });
        let objects = std::mem::take(&mut self.clipboard);
        self.apply(|editor| {
            let added = editor.doc.place(&objects, at);
            if added.len() < objects.len() {
                editor.say("Some objects did not fit within the map's limits.");
            }
            editor.set_selection(added);
        });
        self.clipboard = objects;
        self.mode = Mode::Edit;
    }
    /// Copy the selection one cell over; once a copy has been moved, the
    /// next duplicate repeats that move, so rows build themselves.
    fn duplicate(&mut self) {
        let objects: Vec<Object> = self
            .selection
            .iter()
            .filter_map(|&key| self.doc.object(key))
            .collect();
        let Some(b) = self.doc.bounds(&self.selection) else {
            return;
        };
        let cell = [
            self.grid / self.doc.map.width,
            self.grid / self.doc.map.height,
        ];
        let offset = match &self.repeat {
            Some((keys, from)) if *keys == self.selection => {
                let moved = [b[0] - from[0], b[1] - from[1]];
                if moved[0].abs() + moved[1].abs() > 1e-5 {
                    moved
                } else {
                    cell
                }
            }
            _ => cell,
        };
        let at = [b[0] + offset[0], b[1] + offset[1]];
        self.apply(|editor| {
            let added = editor.doc.place(&objects, at);
            editor.set_selection(added);
        });
        self.repeat = Some((self.selection.clone(), [b[0], b[1]]));
    }
    /// Copy the selection to the other side of the arena, mirrored across
    /// its middle line.
    fn mirror_copy(&mut self, axis: Transform) {
        let keys = self.selection.clone();
        let mut added = 0;
        self.apply(|editor| {
            let copies = editor.doc.mirror_copy(&keys, axis);
            added = copies.len();
            editor.set_selection(copies);
        });
        self.say(match (added, axis) {
            (0, _) => "The map is at its object limit.".to_owned(),
            (n, Transform::FlipX) => format!(
                "Mirrored {} across the arena. Shift+M mirrors up and down.",
                objects(n)
            ),
            (n, _) => format!("Mirrored {} up and down the arena.", objects(n)),
        });
    }
    /// Zoom so the selection fills most of the canvas.
    fn frame_selection(&mut self, canvas: Rect) {
        let Some(b) = self.doc.bounds(&self.selection) else {
            self.fitted = false;
            return;
        };
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let size = [
            (b[2] * w).max(self.grid * 4.),
            (b[3] * h).max(self.grid * 4.),
        ];
        self.view.center = V2::new((b[0] + b[2] * 0.5) * w, (b[1] + b[3] * 0.5) * h);
        self.view.zoom = (canvas.w * 0.7 / size[0])
            .min(canvas.h * 0.7 / size[1])
            .clamp(0.015, 4.);
    }
    /// Lock the groups of the selection, so clicks pass through them.
    fn lock_selection(&mut self) {
        let mask = self
            .selection
            .iter()
            .filter_map(|&key| self.doc.object(key))
            .fold(0, |mask, object| mask | MapDef::group_bit(object.group()));
        if mask == 0 {
            self.say("Only objects in a group can be locked. Ctrl+G makes one.");
            return;
        }
        self.apply(|editor| editor.doc.locked |= mask);
        self.selection.clear();
        self.say(format!(
            "Locked {}. Ctrl+Shift+L unlocks every group.",
            group_list(mask)
        ));
    }
    fn unlock_all(&mut self) {
        if self.doc.locked == 0 {
            self.say("No groups are locked.");
            return;
        }
        self.apply(|editor| editor.doc.locked = 0);
        self.say("Every group is unlocked.");
    }
    fn copy_style(&mut self) {
        if let Some(object) = self.selection.first().and_then(|&key| self.doc.object(key)) {
            self.say(format!(
                "Copied the settings of this {}.",
                object.name().to_lowercase()
            ));
            self.style_clipboard = Some(object);
        }
    }
    fn paste_style(&mut self) {
        let Some(source) = self.style_clipboard.clone() else {
            return;
        };
        let keys = self.selection.clone();
        let mut changed = 0;
        self.apply(|editor| {
            for key in keys {
                if let Some(mut object) = editor.doc.object(key)
                    && object.paste_style(&source)
                {
                    editor.doc.replace(key, object);
                    changed += 1;
                }
            }
        });
        self.say(format!("Pasted settings onto {}.", objects(changed)));
    }
    fn select_group(&mut self) {
        let mut keys = Vec::new();
        let groups: Vec<u8> = self
            .selection
            .iter()
            .filter_map(|&key| self.doc.object(key).map(|o| o.group()))
            .filter(|&g| g > 0)
            .collect();
        for group in groups {
            keys.extend(self.doc.in_group(group));
        }
        if keys.is_empty() {
            self.say("The selection is not in any group.");
        } else {
            self.set_selection(keys);
        }
    }
    fn select_kind(&mut self) {
        let mut keys = Vec::new();
        for &key in &self.selection {
            keys.extend(self.doc.like(key));
        }
        self.set_selection(keys);
    }
    fn new_group(&mut self) {
        let Some(group) = self.doc.next_free_group() else {
            self.say("All 32 groups are in use.");
            return;
        };
        let keys = self.selection.clone();
        self.apply(|editor| {
            for key in keys {
                if let Some(mut object) = editor.doc.object(key)
                    && object.groupable()
                {
                    object.set_group(group);
                    editor.doc.replace(key, object);
                }
            }
        });
        self.build_group = group;
        self.say(format!("The selection is now group {group}."));
    }
    fn delete(&mut self) {
        let keys = std::mem::take(&mut self.selection);
        self.apply(|editor| editor.doc.delete(&keys));
    }
    fn check(&mut self) {
        let mut problems = self.doc.problems();
        problems.extend(self.plugin_problems());
        self.say(match problems.iter().filter(|p| p.blocking).count() {
            0 if problems.is_empty() => "No problems found.".to_owned(),
            0 if problems.len() == 1 => "1 note, nothing blocking.".to_owned(),
            0 => format!("{} notes, nothing blocking.", problems.len()),
            1 => "1 problem to fix before playing.".to_owned(),
            n => format!("{n} problems to fix before playing."),
        });
        self.problems = Some(problems);
        self.tab = Tab::Check;
        self.scroll = 0.;
    }
    fn center_on(&mut self, key: Key) {
        if let Some(r) = self.doc.rect(key) {
            self.view.center = V2::new(
                (r[0] + r[2] * 0.5) * self.doc.map.width,
                (r[1] + r[3] * 0.5) * self.doc.map.height,
            );
        }
    }
    fn open(&mut self, doc: Document, path: Option<PathBuf>, notice: &str) {
        self.apply(|editor| editor.doc = doc);
        if path.is_some() {
            self.saved_revision = self.revision;
        }
        self.saved_path = path;
        self.selection.clear();
        self.stamp = None;
        self.fitted = false;
        self.problems = None;
        self.preview.restart(&self.doc.map);
        self.say(notice);
    }

    fn config(&self) -> WorldConfig {
        WorldConfig {
            width: self.doc.map.width,
            height: self.doc.map.height,
            map: self.doc.map.id.clone(),
            mode: tankyard_core::Mode::Sandbox,
            target_count: 80,
            dynamic_arena: false,
            rules: self.doc.rules,
            modded: true,
            ..Default::default()
        }
    }
    fn fit(&mut self, canvas: Rect) {
        self.view.center = V2::new(self.doc.map.width * 0.5, self.doc.map.height * 0.5);
        self.view.zoom = ((canvas.w - 48.) / self.doc.map.width)
            .min((canvas.h - 48.) / self.doc.map.height)
            .clamp(0.015, 4.);
        self.fitted = true;
    }

    pub fn draw(&mut self, settings: &Settings, s: f32) -> Action {
        if let Some(result) = self
            .import_pending
            .as_ref()
            .and_then(|pending| pending.poll())
        {
            self.import_pending = None;
            if let Err(error) = result.and_then(|path| path.map(|p| self.import(&p)).transpose()) {
                self.say(error);
            }
        }
        self.ensure_plugins(settings);
        set_interactive(true);
        pointer_scope(0x45444954);
        let s = s
            .min((screen_width() / 1180.).min(screen_height() / 720.))
            .max(0.25);
        let layout = self.layout(s);
        if !self.fitted {
            self.fit(layout.canvas);
        }
        // View centers on the window; offset it to the canvas's center.
        self.view.shake = layout.canvas.center() - vec2(screen_width(), screen_height()) * 0.5;
        let dt = get_frame_time().min(0.1);
        if self.animate && !settings.reduced_motion {
            self.time += dt;
        }
        self.focus_seen = false;
        // The click or key that ended a playtest must not also edit.
        let resumed = std::mem::take(&mut self.resumed);
        let mut action = Action::None;
        let on_edge = self.panel_edges(&layout);
        if !resumed {
            action = self.keys(&layout);
            if !on_edge {
                self.canvas_input(&layout);
            }
        }
        // Keys can hide the panels; the rest of the frame uses the new layout.
        let layout = self.layout(s);
        self.canvas_area = layout.canvas;
        self.run_preview(&layout, dt);
        let config = self.config();
        clear_background(bg_color());
        self.draw_canvas(&layout, settings, &config);
        if self.help || on_edge {
            set_interactive(false);
        }
        if let Some(chosen) = self.top_bar(&layout) {
            action = chosen;
        }
        if !self.prefs.hide_panels {
            self.dock(&layout, settings);
            self.inspector(&layout);
        }
        self.draw_edges(&layout);
        set_interactive(true);
        if self.help {
            self.help_overlay(s);
        }
        if !self.focus_seen {
            self.text_focus = None;
        }
        if !self.typing() && !is_mouse_button_down(MouseButton::Left) {
            self.property_edit = false;
        }
        if matches!(action, Action::None) {
            self.keep_prefs(false);
        } else {
            self.release_cursor();
            self.keep_prefs(true);
        }
        if self.revision != self.draft_revision && get_time() - self.last_autosave > 3. {
            self.last_autosave = get_time();
            match self.write_draft() {
                Ok(()) => self.draft_revision = self.revision,
                Err(error) => self.say(format!("Draft could not be saved: {error}")),
            }
        }
        pointer_scope(0);
        action
    }
    fn run_preview(&mut self, layout: &Layout, dt: f32) {
        if self.preview_revision != self.revision {
            self.preview_revision = self.revision;
            self.preview.follow(&self.doc.map);
        }
        let pointer = crate::ui_layout::pointer();
        self.probe = (is_key_down(KeyCode::T)
            && !self.typing()
            && !self.help
            && layout.canvas.contains(pointer))
        .then(|| self.view.world(pointer));
        if self.probe.is_some() && is_key_pressed(KeyCode::T) && !self.preview.running {
            self.preview.running = true;
            self.say("Preview running. Release T to remove the test tank.");
        }
        let config = self.config();
        self.preview.advance(&self.doc.map, &config, dt, self.probe);
    }

    /// Back from a playtest.
    pub fn resume(&mut self) {
        self.resumed = true;
        self.gesture = None;
        self.panning = None;
    }
    fn keys(&mut self, layout: &Layout) -> Action {
        if self.typing() {
            return Action::None;
        }
        if self.help {
            if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F1) {
                self.help = false;
            }
            return Action::None;
        }
        let pressed = is_key_pressed;
        if pressed(KeyCode::F1) || (shift() && pressed(KeyCode::Slash)) {
            self.help = true;
            return Action::None;
        }
        if pressed(KeyCode::Escape) {
            if self.gesture.take().is_some() {
                // Escape mid-drag puts everything back.
                if let Some(before) = self.pending.take() {
                    self.doc = before;
                    self.touch();
                }
            } else if self.stamp.is_some() {
                self.stamp = None;
            } else {
                self.selection.clear();
            }
        }
        if self.gesture.is_some() {
            return Action::None;
        }
        let pointer = crate::ui_layout::pointer();
        let over_canvas = layout.canvas.contains(pointer);
        if pressed(KeyCode::F5) {
            let start = (shift() && over_canvas).then(|| self.view.world(pointer));
            return self.play(start);
        }
        if command() {
            if pressed(KeyCode::Z) {
                self.undo(shift());
            }
            if pressed(KeyCode::Y) {
                self.undo(true);
            }
            if pressed(KeyCode::C) {
                if shift() {
                    self.copy_style();
                } else {
                    self.copy();
                }
            }
            if pressed(KeyCode::X) {
                self.copy();
                self.delete();
            }
            if pressed(KeyCode::V) {
                if shift() {
                    self.paste_style();
                } else {
                    let at = over_canvas.then(|| self.snapped(pointer));
                    self.paste(at);
                }
            }
            if pressed(KeyCode::D) {
                self.duplicate();
            }
            if pressed(KeyCode::L) {
                if shift() {
                    self.unlock_all();
                } else {
                    self.lock_selection();
                }
            }
            if pressed(KeyCode::A) && shift() {
                // Everything like the selection: every torch, every pad.
                if !self.selection.is_empty() {
                    self.select_kind();
                }
            } else if pressed(KeyCode::A) {
                let keys = self
                    .doc
                    .items()
                    .filter(|&(_, _, group)| self.pickable(group))
                    .map(|(key, _, _)| key)
                    .collect();
                self.set_selection(keys);
                self.mode = Mode::Edit;
            }
            if pressed(KeyCode::G) {
                self.new_group();
            }
            if pressed(KeyCode::S) {
                self.save();
            }
            if pressed(KeyCode::R) {
                let at = over_canvas.then(|| self.view.world(pointer));
                self.repeat_tool(at);
            }
            return Action::None;
        }
        for (key, mode) in [
            (KeyCode::Key1, Mode::Build),
            (KeyCode::Key2, Mode::Edit),
            (KeyCode::Key3, Mode::Delete),
            (KeyCode::Key4, Mode::Pan),
        ] {
            if pressed(key) {
                self.mode = mode;
            }
        }
        if pressed(KeyCode::Delete) || pressed(KeyCode::Backspace) {
            self.delete();
        }
        if over_canvas && pressed(KeyCode::Tab) {
            self.cycle_pick(pointer, shift());
        }
        let any = !self.selection.is_empty();
        if any && pressed(KeyCode::Q) {
            self.transform(Transform::TurnLeft);
        }
        if any && (pressed(KeyCode::E) || pressed(KeyCode::R)) {
            self.transform(Transform::TurnRight);
        }
        if any && pressed(KeyCode::X) {
            self.transform(Transform::FlipX);
        }
        if any && pressed(KeyCode::Y) {
            self.transform(Transform::FlipY);
        }
        if any && pressed(KeyCode::RightBracket) {
            self.order(true);
        }
        if any && pressed(KeyCode::LeftBracket) {
            self.order(false);
        }
        if pressed(KeyCode::F) {
            if shift() {
                self.frame_selection(layout.canvas);
            } else {
                self.fitted = false;
            }
        }
        if any && pressed(KeyCode::M) {
            self.mirror_copy(if shift() {
                Transform::FlipY
            } else {
                Transform::FlipX
            });
        }
        if pressed(KeyCode::Backslash) {
            self.toggle_panels();
        }
        if pressed(KeyCode::H) {
            self.show_hidden = !self.show_hidden;
        }
        if pressed(KeyCode::G) {
            self.show_groups = !self.show_groups;
        }
        if pressed(KeyCode::L) {
            self.show_links = !self.show_links;
        }
        if pressed(KeyCode::P) {
            if shift() {
                self.preview.restart(&self.doc.map);
            } else {
                self.preview.running = !self.preview.running;
            }
        }
        let center = layout.canvas.center();
        if pressed(KeyCode::Equal) || pressed(KeyCode::KpAdd) {
            self.zoom_at(center, 1.25);
        }
        if pressed(KeyCode::Minus) || pressed(KeyCode::KpSubtract) {
            self.zoom_at(center, 0.8);
        }
        self.held_movement();
        Action::None
    }
    /// Arrows and WASD nudge the selection, repeating while held, as one
    /// undo step; with nothing selected they move the view.
    fn held_movement(&mut self) {
        let down = |keys: [KeyCode; 2]| keys.into_iter().any(is_key_down);
        let pressed = |keys: [KeyCode; 2]| keys.into_iter().any(is_key_pressed);
        let directions = [
            ([KeyCode::Left, KeyCode::A], vec2(-1., 0.)),
            ([KeyCode::Right, KeyCode::D], vec2(1., 0.)),
            ([KeyCode::Up, KeyCode::W], vec2(0., -1.)),
            ([KeyCode::Down, KeyCode::S], vec2(0., 1.)),
        ];
        let held: Vec2 = directions
            .iter()
            .filter(|(keys, _)| down(*keys))
            .map(|(_, d)| *d)
            .sum();
        if held == Vec2::ZERO {
            if self.key_edit {
                self.key_edit = false;
                self.commit();
            }
            return;
        }
        if self.selection.is_empty() || self.mode == Mode::Pan {
            let speed = 700. / self.view.zoom * get_frame_time().min(0.1);
            self.view.center += V2::new(held.x, held.y) * speed;
            return;
        }
        let now = get_time();
        let fresh: Vec2 = directions
            .iter()
            .filter(|(keys, _)| pressed(*keys))
            .map(|(_, d)| *d)
            .sum();
        let step = if fresh != Vec2::ZERO {
            self.nudge_since = now;
            self.nudge_last = now;
            fresh
        } else if now - self.nudge_since > 0.35 && now - self.nudge_last > 0.045 {
            self.nudge_last = now;
            held
        } else {
            return;
        };
        if !self.key_edit {
            self.begin();
            self.key_edit = true;
        }
        let size = if shift() { 1. } else { self.step_size() };
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let keys = self.selection.clone();
        self.doc.nudge(&keys, step.x * size / w, step.y * size / h);
    }

    fn top_bar(&mut self, layout: &Layout) -> Option<Action> {
        let r = layout.top;
        let s = layout.s;
        draw_rectangle(r.x, r.y, r.w, r.h, surface_color());
        draw_line(r.x, r.bottom(), r.right(), r.bottom(), 1.5, line_color());
        label("ARENA WORKSHOP", 16. * s, 20. * s, 11. * s, muted_color());
        let dirty = self.revision != self.saved_revision;
        let status = if dirty {
            "Unsaved"
        } else if self.saved_path.is_some() {
            "Saved"
        } else {
            "Draft"
        };
        let name = fitted_label(&self.doc.map.name, 17. * s, 190. * s);
        label(&name, 16. * s, 42. * s, 17. * s, text_color());
        let after = 16. * s + label_width(&name, 17. * s) + 10. * s;
        label(
            status,
            after,
            42. * s,
            12. * s,
            if dirty { AMBER } else { muted_color() },
        );
        let h = 32. * s;
        let y = r.y + (r.h - h) * 0.5;
        let mut x = 300. * s;
        let mut action = None;
        let buttons = |x: &mut f32, entries: &[(&str, f32, bool)]| -> Option<usize> {
            let mut clicked = None;
            for (i, &(title, width, enabled)) in entries.iter().enumerate() {
                let w = width * s;
                if button(Rect::new(*x, y, w, h), title, false, enabled, s * 0.74) {
                    clicked = Some(i);
                }
                *x += w + 6. * s;
            }
            *x += 10. * s;
            clicked
        };
        match buttons(
            &mut x,
            &[
                ("Undo", 64., !self.undo.is_empty()),
                ("Redo", 64., !self.redo.is_empty()),
            ],
        ) {
            Some(0) => self.undo(false),
            Some(_) => self.undo(true),
            None => {}
        }
        match buttons(
            &mut x,
            &[
                ("New", 60., true),
                ("Save", 64., true),
                ("Export", 76., true),
            ],
        ) {
            Some(0) => {
                if self.reset_confirm || self.revision == self.saved_revision {
                    self.open(
                        Document::default(),
                        None,
                        "A fresh arena. Undo brings back the old one.",
                    );
                    self.reset_confirm = false;
                } else {
                    self.reset_confirm = true;
                    self.say("Click New again to start over. Undo can still bring this map back.");
                }
            }
            Some(1) => self.save(),
            Some(_) => self.export(),
            None => {}
        }
        let mut right = r.right() - 10. * s;
        let mut aligned = |title: &str, width: f32, selected: bool| {
            let w = width * s;
            right -= w;
            let clicked = button(Rect::new(right, y, w, h), title, selected, true, s * 0.74);
            right -= 6. * s;
            clicked
        };
        if aligned("Back", 64., false) {
            if self.revision != self.saved_revision && !self.close_confirm {
                self.close_confirm = true;
                self.say(
                    "Click Back again to leave. Your draft is kept; Save makes a named project.",
                );
            } else {
                let _ = self.write_draft();
                action = Some(Action::Exit);
            }
        }
        if aligned("Playtest  F5", 116., true) {
            action = Some(self.play(None));
        }
        if aligned("Play here", 92., false) {
            let at = self.view.world(layout.canvas.center());
            action = Some(self.play(Some(at)));
        }
        if aligned("Help  F1", 84., self.help) {
            self.help = !self.help;
        }
        action
    }

    fn help_overlay(&mut self, s: f32) {
        draw_rectangle(0., 0., screen_width(), screen_height(), alpha(BLACK, 0.45));
        let w = (960. * s).min(screen_width() - 40.);
        let h = (600. * s).min(screen_height() - 40.);
        let r = Rect::new(
            (screen_width() - w) * 0.5,
            (screen_height() - h) * 0.5,
            w,
            h,
        );
        draw_rectangle(r.x, r.y, r.w, r.h, surface_color());
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2., line_color());
        label(
            "Shortcuts",
            r.x + 22. * s,
            r.y + 34. * s,
            20. * s,
            text_color(),
        );
        let columns: [&[(&str, &str)]; 2] = [
            &[
                ("1  2  3  4", "Build, Edit, Delete, Pan"),
                ("Click", "Build one, select, or delete"),
                ("Drag", "Draw a size, move, or sweep a region"),
                ("Swipe", "Paint or erase as you drag"),
                ("Right-click", "Build copies of that object"),
                ("Shift-click", "Add to or remove from the selection"),
                ("Double-click", "Select the object's whole group"),
                ("Tab / Shift Tab", "Cycle objects under the pointer"),
                ("Alt-drag", "Copy the selection as you move it"),
                ("Shift / Alt handle", "Resize: keep shape, or from center"),
                ("WASD, arrows", "Nudge; Shift for one unit"),
                ("Q  E", "Turn left, turn right"),
                ("X  Y", "Flip across, flip up and down"),
                ("M,  Shift M", "Mirror a copy to the far side"),
                ("[  ]", "Send to back, bring to front"),
                ("Delete", "Delete the selection"),
                ("Esc", "Deselect, or cancel a drag"),
            ],
            &[
                ("Ctrl C  X  V", "Copy, cut, paste at the pointer"),
                ("Ctrl D", "Duplicate; repeats your last move"),
                ("Ctrl Shift C  V", "Copy and paste settings"),
                ("Ctrl G", "Put the selection in a new group"),
                ("Ctrl L,  Shift L", "Lock its groups, unlock them all"),
                ("Ctrl A,  Shift A", "Select everything shown, or alike"),
                ("Ctrl Z  Y", "Undo, redo"),
                ("Ctrl S", "Save the project"),
                ("Ctrl R", "Run the last plugin tool again"),
                ("Wheel, Ctrl-scroll", "Zoom at the pointer"),
                ("Trackpad scroll", "Pan (Scroll setting in Pan, 4)"),
                ("+ / −", "Zoom around the canvas center"),
                ("Space-drag", "Move the view, as middle-drag does"),
                ("F,  Shift F", "Fit the arena, frame the selection"),
                ("\\", "Hide or show the panels"),
                ("H  G  L", "Hidden groups, group labels, links"),
                ("P,  Shift P", "Run or pause logic, restart it"),
                ("Hold T", "Test tank at the pointer"),
                ("F5,  Shift F5", "Playtest, or start at the pointer"),
            ],
        ];
        let col_w = (w - 66. * s) * 0.5;
        let keys_w = 150. * s;
        for (c, entries) in columns.into_iter().enumerate() {
            let x = r.x + 22. * s + c as f32 * (col_w + 22. * s);
            let mut y = r.y + 70. * s;
            for (keys, text) in entries {
                label(keys, x, y, 13. * s, accent());
                label(
                    &fitted_label(text, 13. * s, col_w - keys_w),
                    x + keys_w,
                    y,
                    13. * s,
                    text_color(),
                );
                y += 28. * s;
            }
        }
        if button(
            Rect::new(r.right() - 110. * s, r.y + 14. * s, 92. * s, 30. * s),
            "Close",
            false,
            true,
            s * 0.78,
        ) {
            self.help = false;
        }
    }

    fn play(&mut self, start: Option<V2>) -> Action {
        match self.doc.loaded() {
            Ok(loaded) => {
                if self.write_draft().is_ok() {
                    self.draft_revision = self.revision;
                }
                Action::Play {
                    loaded: Box::new(loaded),
                    config: self.config(),
                    start,
                }
            }
            Err(error) => {
                self.check();
                self.say(format!("Cannot playtest yet: {error}"));
                Action::None
            }
        }
    }
    fn write_draft(&self) -> Result<(), String> {
        write_document(&directory()?.join("draft.json"), &self.doc)
    }
    fn save(&mut self) {
        let result: Result<PathBuf, String> = (|| {
            let root = directory()?.join("projects");
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            if !tankyard_mods::identifier(&self.doc.map.id) || self.doc.map.id.len() > 48 {
                return Err("Choose a map id of lowercase letters, digits and underscores (48 at most), on the Map tab.".into());
            }
            let path = root.join(format!("{}.json", self.doc.map.id));
            write_document(&path, &self.doc)?;
            self.write_draft()?;
            Ok(path)
        })();
        match result {
            Ok(path) => {
                self.say(format!("Saved {}", short_path(&path)));
                self.saved_path = Some(path);
                self.saved_revision = self.revision;
                self.draft_revision = self.revision;
                self.refresh_library();
            }
            Err(error) => self.say(error),
        }
    }
    fn export(&mut self) {
        let result: Result<PathBuf, String> = (|| {
            let bytes = self.doc.pack()?.to_zip()?;
            let root = directory()?.join("exports");
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let path = root.join(format!("{}-{suffix}.zip", self.doc.map.id));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            Ok(path)
        })();
        match result {
            Ok(path) => self.say(format!(
                "Exported {}. Import it on the Mods page.",
                short_path(&path)
            )),
            Err(error) => {
                self.check();
                self.say(format!("Cannot export yet: {error}"));
            }
        }
    }
    fn refresh_library(&mut self) {
        self.library = directory()
            .ok()
            .and_then(|root| std::fs::read_dir(root.join("projects")).ok())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .take(256)
            .collect();
        self.library.sort();
        self.library_page = self
            .library_page
            .min(self.library.len().saturating_sub(1) / 8);
    }
    fn import(&mut self, path: &Path) -> Result<(), String> {
        let path = if path.file_name().is_some_and(|name| name == "mod.json") {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let pack = tankyard_mods::PackFiles::read(path)?;
        let mut doc = if let Some(bytes) = pack.get("editor.json") {
            Document::from_json(bytes)?
        } else {
            let manifest = tankyard_mods::parse_manifest(&pack)?;
            let map = manifest
                .maps
                .into_iter()
                .next()
                .ok_or("That pack has no map to edit.")?;
            let mut doc = Document::from_map(map);
            doc.rules = manifest.rules.unwrap_or_default();
            doc
        };
        doc.bake_procedure()?;
        self.open(
            doc,
            None,
            "Map imported. Save keeps your own editable copy.",
        );
        Ok(())
    }
}

/// "group 3" or "groups 2, 5".
fn group_list(mask: u32) -> String {
    let groups: Vec<String> = (1..=MAP_GROUPS)
        .filter(|&g| MapDef::group_bit(g) & mask != 0)
        .map(|g| g.to_string())
        .collect();
    match groups.as_slice() {
        [one] => format!("group {one}"),
        many => format!("groups {}", many.join(", ")),
    }
}

fn directory() -> Result<PathBuf, String> {
    let path = directories::ProjectDirs::from("org", "Tank Yard", "Tank Yard")
        .ok_or("No app data folder is available.")?
        .data_dir()
        .join("workshop");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}
fn read_document(path: &Path) -> Result<Document, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
        return Err("Editor project exceeds 2 MiB.".into());
    }
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Document::from_json(&bytes)
}
fn write_document(path: &Path, doc: &Document) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Editor project exceeds 2 MiB.".into());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_extension(format!("json.{}.{}.tmp", std::process::id(), stamp));
    let mut file = std::fs::File::create_new(&temp).map_err(|e| e.to_string())?;
    let result = file.write_all(&bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())
}
