//! The canvas: navigation, pointer gestures for the editor modes, and the
//! arena drawn with the editor's overlays.
use super::{Editor, Layout, Mode, Tab, alt, brushes, command, shift};
use crate::{
    scenery,
    settings::Settings,
    ui::{AMBER, CYAN, RED, VIOLET, *},
    ui_layout,
};
use macroquad::prelude::*;
use std::collections::HashSet;
use tankyard_client::editor_document::{
    Key, Link, Object, describe_trigger, link, trigger_badge_tag,
};
use tankyard_content::*;
use tankyard_core::{V2, WorldConfig};

/// Objects as they were when a drag began.
type Originals = Vec<(Key, Object)>;

pub enum Gesture {
    /// Drawing one object's rectangle, in normalized map coordinates.
    Draw { start: Vec2, last: Vec2 },
    /// Placing a brush in every cell the pointer crosses.
    Paint {
        cells: HashSet<(i32, i32)>,
        last: Vec2,
    },
    /// Moving the selection; nothing moves until the pointer travels a little.
    Move {
        press: Vec2,
        start: Vec2,
        objects: Vec<(Key, Object)>,
        clicked: Key,
        moved: bool,
    },
    /// Dragging one edge or corner of an object or of a selection's bounds.
    Resize {
        originals: Originals,
        from: [f32; 4],
        handle: (i8, i8),
    },
    /// Dragging a portal's exit.
    Exit { key: Key },
    /// Dragging where a Move or Place trigger sends its group, measured
    /// from the middle of the group.
    Travel { index: usize, center: [f32; 2] },
    /// Selecting (Edit) or deleting (Delete) a screen region.
    Marquee { start: Vec2, additive: bool },
    /// Deleting whatever the pointer crosses.
    Erase { last: Vec2 },
}

/// How near, in pixels, the pointer must be to grab a handle.
const HANDLE_REACH: f32 = 11.;
/// A selection side shorter than this on screen has the handles across it
/// stand outside the edge, so pressing in a thin selection still moves it.
const HANDLE_ROOM: f32 = 40.;

const HANDLES: [(i8, i8); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
];

/// `r` with the side or corner `handle` dragged to `p`, never thinner than
/// `least`. `keep` holds the proportions; `centered` grows both sides.
fn resized(
    r: [f32; 4],
    handle: (i8, i8),
    p: [f32; 2],
    least: [f32; 2],
    keep: bool,
    centered: bool,
) -> [f32; 4] {
    let sides = [handle.0, handle.1];
    let mid = [r[0] + r[2] * 0.5, r[1] + r[3] * 0.5];
    let (mut lo, mut hi) = ([r[0], r[1]], [r[0] + r[2], r[1] + r[3]]);
    for axis in 0..2 {
        let side = sides[axis];
        if side == 0 {
            continue;
        }
        if centered {
            let half = ((p[axis] - mid[axis]) * f32::from(side)).max(least[axis] * 0.5);
            lo[axis] = mid[axis] - half;
            hi[axis] = mid[axis] + half;
        } else if side < 0 {
            lo[axis] = p[axis].min(hi[axis] - least[axis]);
        } else {
            hi[axis] = p[axis].max(lo[axis] + least[axis]);
        }
    }
    if keep && r[2] > 0. && r[3] > 0. {
        let factors = [(hi[0] - lo[0]) / r[2], (hi[1] - lo[1]) / r[3]];
        let factor = match sides {
            [0, _] => factors[1],
            [_, 0] => factors[0],
            _ => factors[0].max(factors[1]),
        }
        .max(least[0] / r[2])
        .max(least[1] / r[3]);
        for axis in 0..2 {
            let size = r[axis + 2] * factor;
            (lo[axis], hi[axis]) = match sides[axis] {
                _ if centered => (mid[axis] - size * 0.5, mid[axis] + size * 0.5),
                0 => (mid[axis] - size * 0.5, mid[axis] + size * 0.5),
                side if side < 0 => (r[axis] + r[axis + 2] - size, r[axis] + r[axis + 2]),
                _ => (r[axis], r[axis] + size),
            };
        }
    }
    [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]]
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x <= b.right() && b.x <= a.right() && a.y <= b.bottom() && b.y <= a.bottom()
}

fn scroll_zoom(lines: f32, speed: f32) -> f32 {
    (lines * 0.18 * speed).clamp(-0.35, 0.35).exp()
}
fn screen_area(a: Vec2, b: Vec2) -> Rect {
    Rect::new(
        a.x.min(b.x),
        a.y.min(b.y),
        (a.x - b.x).abs(),
        (a.y - b.y).abs(),
    )
}
/// Points along a pointer stroke, so fast strokes leave no gaps.
fn stroke(from: Vec2, to: Vec2, spacing: f32) -> impl Iterator<Item = Vec2> {
    let steps = ((to - from).length() / spacing.max(1.))
        .ceil()
        .clamp(1., 256.) as usize;
    (0..=steps).map(move |i| from.lerp(to, i as f32 / steps as f32))
}
pub fn dashed_rect(r: Rect, thickness: f32, dash: f32, color: Color) {
    let edges = [
        (vec2(r.x, r.y), vec2(r.right(), r.y)),
        (vec2(r.right(), r.y), vec2(r.right(), r.bottom())),
        (vec2(r.right(), r.bottom()), vec2(r.x, r.bottom())),
        (vec2(r.x, r.bottom()), vec2(r.x, r.y)),
    ];
    for (a, b) in edges {
        let length = (b - a).length();
        let count = (length / (dash * 2.)).ceil().clamp(1., 200.) as usize;
        for i in 0..count {
            let t0 = i as f32 / count as f32;
            let t1 = (t0 + 0.5 / count as f32).min(1.);
            let (p, q) = (a.lerp(b, t0), a.lerp(b, t1));
            draw_line(p.x, p.y, q.x, q.y, thickness, color);
        }
    }
}
fn arrow(from: Vec2, to: Vec2, thickness: f32, color: Color) {
    let d = to - from;
    let length = d.length();
    if length < 2. {
        return;
    }
    let dir = d / length;
    let head = (thickness * 4.5).min(length * 0.5);
    let base = to - dir * head;
    draw_line(from.x, from.y, base.x, base.y, thickness, color);
    let side = vec2(-dir.y, dir.x) * head * 0.55;
    draw_triangle(to, base + side, base - side, color);
}

impl Editor {
    /// A normalized map rectangle on screen.
    pub(super) fn on_screen(&self, r: [f32; 4]) -> Rect {
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let p = self.view.screen(V2::new(r[0] * w, r[1] * h));
        Rect::new(
            p.x,
            p.y,
            r[2] * w * self.view.zoom,
            r[3] * h * self.view.zoom,
        )
    }
    pub(super) fn point_on_screen(&self, p: [f32; 2]) -> Vec2 {
        self.view.screen(V2::new(
            p[0] * self.doc.map.width,
            p[1] * self.doc.map.height,
        ))
    }
    /// Where an object answers the pointer: its rectangle, grown so small
    /// objects and trigger badges stay easy to hit.
    pub(super) fn hit_area(&self, key: Key, r: [f32; 4]) -> Rect {
        let mut a = self.on_screen(r);
        let least = match key {
            Key::Trigger(_) => 28.,
            Key::Base(_) | Key::Objective(_) => 20.,
            _ => 10.,
        };
        if a.w < least {
            a.x -= (least - a.w) * 0.5;
            a.w = least;
        }
        if a.h < least {
            a.y -= (least - a.h) * 0.5;
            a.h = least;
        }
        a
    }
    /// The normalized point under `p`, on the grid when snapping is on.
    pub(super) fn snapped(&self, p: Vec2) -> Vec2 {
        let world = self.view.world(p);
        let (x, y) = if self.snap {
            (
                (world.x / self.grid).round() * self.grid,
                (world.y / self.grid).round() * self.grid,
            )
        } else {
            (world.x, world.y)
        };
        vec2(
            (x / self.doc.map.width).clamp(0., 1.),
            (y / self.doc.map.height).clamp(0., 1.),
        )
    }
    /// Groups hidden right now: by the preview once it has run, otherwise
    /// at the start of a round.
    pub(super) fn ghost_mask(&self) -> u32 {
        if self.preview.running || !self.preview.at_start() {
            self.preview.hidden
        } else {
            self.doc.map.starting_mask()
        }
    }
    pub(super) fn pickable(&self, group: u8) -> bool {
        let bit = MapDef::group_bit(group);
        self.focus.is_none_or(|focus| focus == group)
            && bit & self.doc.locked == 0
            && (self.show_hidden || bit & self.ghost_mask() == 0)
    }
    pub(super) fn pick(&self, p: Vec2) -> Option<Key> {
        self.doc
            .items()
            .rev()
            .find(|&(key, r, group)| self.pickable(group) && self.hit_area(key, r).contains(p))
            .map(|(key, _, _)| key)
    }

    pub(super) fn cycle_pick(&mut self, pointer: Vec2, backwards: bool) {
        let hits: Vec<Key> = self
            .doc
            .items()
            .rev()
            .filter(|&(key, r, group)| {
                self.pickable(group) && self.hit_area(key, r).contains(pointer)
            })
            .map(|(key, _, _)| key)
            .collect();
        if hits.is_empty() {
            return;
        }
        let current = hits.iter().position(|key| self.selected(*key));
        let next = match current {
            Some(index) if backwards => (index + hits.len() - 1) % hits.len(),
            Some(index) => (index + 1) % hits.len(),
            None if backwards => hits.len() - 1,
            None => 0,
        };
        self.set_selection(vec![hits[next]]);
        self.mode = Mode::Edit;
        self.tab = Tab::Object;
        self.last_click = None;
    }

    /// The object and size, in world units, that a click places.
    pub(super) fn current_brush(&self) -> (&Object, [f32; 2]) {
        if let Some((object, size)) = &self.stamp {
            return (object, *size);
        }
        let brush = &self.brushes[self.category][self.brush];
        (&brush.object, brush.size)
    }
    /// The brush's footprint, rounded to whole grid cells when snapping.
    fn brush_size(&self) -> [f32; 2] {
        let (_, size) = self.current_brush();
        let cell = |v: f32| {
            if v <= 0. {
                self.grid
            } else if self.snap {
                ((v / self.grid).round() * self.grid).max(self.grid)
            } else {
                v
            }
        };
        [cell(size[0]), cell(size[1])]
    }
    /// The rectangle a click at `p` fills, centered on the pointer.
    /// The area a drag from `start` to `last` builds, or None for a click.
    fn drawn(&self, start: Vec2, last: Vec2) -> Option<[f32; 4]> {
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let small = (last.x - start.x).abs() * w < self.grid * 0.5
            && (last.y - start.y).abs() * h < self.grid * 0.5;
        if small || self.current_brush().0.is_point() {
            return None;
        }
        // A line drawn along the grid still gets one cell of depth.
        let least = if self.snap { self.grid } else { 4. };
        let span = |a: f32, b: f32, size: f32| {
            let length = (b - a).abs().max(least / size).min(1.);
            (a.min(b).min(1. - length).max(0.), length)
        };
        let (x, width) = span(start.x, last.x, w);
        let (y, height) = span(start.y, last.y, h);
        Some([x, y, width, height])
    }
    fn brush_cell(&self, p: Vec2) -> [f32; 4] {
        let world = self.view.world(p);
        let size = self.brush_size();
        let mut corner = [world.x - size[0] * 0.5, world.y - size[1] * 0.5];
        if self.snap {
            corner = corner.map(|v| (v / self.grid).round() * self.grid);
        }
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        [corner[0] / w, corner[1] / h, size[0] / w, size[1] / h]
    }
    /// A copy of the brush filling `r`, in the build group.
    fn brush_object(&self, r: [f32; 4]) -> Object {
        let (object, _) = self.current_brush();
        let mut object = object.clone();
        object.set_rect(r);
        if self.build_group > 0 && self.stamp.is_none() {
            match &mut object {
                // A trigger built while a group is chosen acts on that group;
                // joining it would switch the trigger off whenever it hides.
                // Watchers watch it, and a Rearm resets it.
                Object::Trigger(trigger) => {
                    if trigger.action.targets_group() || trigger.action == TriggerAction::Reset {
                        trigger.group = self.build_group;
                    }
                    if trigger.when.watches_group() {
                        trigger.source = self.build_group;
                    }
                }
                object if object.groupable() => object.set_group(self.build_group),
                _ => {}
            }
        }
        if let Object::Zone(zone) = &mut object
            && zone.kind == ZoneKind::Portal
            && self.stamp.is_none()
        {
            // Start the exit just beside the new pad, where it is easy to find.
            let r = zone.rect;
            zone.exit = [
                (r[0] + r[2] * 2.5).min(0.97),
                (r[1] + r[3] * 0.5).clamp(0.03, 0.97),
            ];
        }
        object
    }
    fn place(&mut self, r: [f32; 4]) {
        let object = self.brush_object(r);
        let name = object.name();
        self.apply(|editor| match editor.doc.insert(object) {
            Some(key) => {
                editor.set_selection(vec![key]);
                if !matches!(editor.tab, Tab::Rules | Tab::Files | Tab::Tools) {
                    editor.tab = Tab::Object;
                }
            }
            None => editor.say(format!(
                "Cannot add another {}: the map is at its limit, or that team already has a base.",
                name.to_lowercase()
            )),
        });
    }
    fn occupied(&self, object: &Object, r: [f32; 4]) -> bool {
        let close = |a: [f32; 4]| a.iter().zip(r).all(|(x, y)| (x - y).abs() < 1e-4);
        self.doc.items().any(|(key, rect, _)| {
            close(rect)
                && self
                    .doc
                    .object(key)
                    .is_some_and(|other| other.same_kind(object))
        })
    }
    fn paint(&mut self, cells: &mut HashSet<(i32, i32)>, from: Vec2, to: Vec2) {
        let size = self.brush_size();
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let spacing = (size[0].min(size[1]) * self.view.zoom * 0.5).max(3.);
        for p in stroke(from, to, spacing) {
            let world = self.view.world(p);
            if world.x < 0. || world.y < 0. || world.x >= w || world.y >= h {
                continue;
            }
            let cell = (
                (world.x / size[0]).floor() as i32,
                (world.y / size[1]).floor() as i32,
            );
            if cells.len() >= 4096 || !cells.insert(cell) {
                continue;
            }
            let r = [
                cell.0 as f32 * size[0] / w,
                cell.1 as f32 * size[1] / h,
                size[0] / w,
                size[1] / h,
            ];
            let object = self.brush_object(r);
            if !self.occupied(&object, object.rect()) && self.doc.insert(object).is_none() {
                self.say("The map is at its object limit.");
                break;
            }
        }
    }
    fn erase(&mut self, from: Vec2, to: Vec2) {
        let mut keys: Vec<Key> = stroke(from, to, 6.).filter_map(|p| self.pick(p)).collect();
        if keys.is_empty() {
            return;
        }
        keys.sort_unstable();
        keys.dedup();
        self.doc.delete(&keys);
        self.selection.clear();
    }
    /// What the handles reshape: one object, or several by their bounds.
    fn handle_target(&self) -> Option<(Originals, [f32; 4])> {
        let originals: Originals = self
            .selection
            .iter()
            .filter_map(|&key| Some((key, self.doc.object(key)?)))
            .collect();
        match originals.as_slice() {
            [] => None,
            [(_, object)] if object.is_point() => None,
            [(_, object)] => {
                let r = object.rect();
                Some((originals, r))
            }
            _ => {
                let b = self.doc.bounds(&self.selection)?;
                (b[2] > 0. && b[3] > 0.).then_some((originals, b))
            }
        }
    }
    fn handle_spot(r: Rect, (x, y): (i8, i8)) -> Vec2 {
        let out = |side: f32| if side < HANDLE_ROOM { HANDLE_REACH } else { 0. };
        vec2(
            r.x + r.w * (f32::from(x) + 1.) * 0.5 + f32::from(x) * out(r.w),
            r.y + r.h * (f32::from(y) + 1.) * 0.5 + f32::from(y) * out(r.h),
        )
    }
    /// The selected Move or Place trigger and the bounds of the group it
    /// moves, as the map places it.
    fn travel_target(&self) -> Option<(usize, [f32; 4])> {
        let [Key::Trigger(index)] = *self.selection.as_slice() else {
            return None;
        };
        let trigger = self.doc.map.triggers.get(index)?;
        if !trigger.action.moves() || trigger.group == 0 {
            return None;
        }
        let members: Vec<Key> = self
            .doc
            .items()
            .filter(|&(key, _, group)| group == trigger.group && !matches!(key, Key::Trigger(_)))
            .map(|(key, _, _)| key)
            .collect();
        Some((index, self.doc.bounds(&members)?))
    }
    fn handle_at(&self, p: Vec2) -> Option<Gesture> {
        if let Some((index, bounds)) = self.travel_target() {
            let travel = self.doc.map.triggers[index].travel;
            let center = [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5];
            if self
                .point_on_screen([center[0] + travel[0], center[1] + travel[1]])
                .distance(p)
                < 12.
            {
                return Some(Gesture::Travel { index, center });
            }
        }
        let (originals, from) = self.handle_target()?;
        if let [(key, Object::Zone(zone))] = originals.as_slice()
            && zone.kind == ZoneKind::Portal
            && self.point_on_screen(zone.exit).distance(p) < 12.
        {
            return Some(Gesture::Exit { key: *key });
        }
        let r = self.on_screen(from);
        let handle = HANDLES
            .into_iter()
            .find(|&h| Self::handle_spot(r, h).distance(p) < HANDLE_REACH)?;
        Some(Gesture::Resize {
            originals,
            from,
            handle,
        })
    }

    /// Right click: build copies of the object under the pointer, or clear
    /// the selection over empty floor.
    fn eyedropper(&mut self, p: Vec2) {
        let Some(object) = self.pick(p).and_then(|key| self.doc.object(key)) else {
            self.selection.clear();
            return;
        };
        let r = object.rect();
        let size = if object.is_point() {
            [0., 0.]
        } else {
            [r[2] * self.doc.map.width, r[3] * self.doc.map.height]
        };
        let name = object.name();
        self.stamp = Some((object, size));
        self.mode = Mode::Build;
        self.say(format!(
            "Building copies of that {}. Choose a palette item to stop.",
            name.to_lowercase()
        ));
    }

    pub(super) fn canvas_input(&mut self, layout: &Layout) {
        let pointer = ui_layout::pointer();
        let canvas = layout.canvas;
        let inside = canvas.contains(pointer) && !self.help;
        if inside {
            // Where tools run from the Tools tab: the last click, since the
            // pointer always crosses the canvas edge on its way there.
            if is_mouse_button_pressed(MouseButton::Left)
                || is_mouse_button_pressed(MouseButton::Right)
            {
                self.last_pointer = Some(self.view.world(pointer));
            }
            let wheel = ui_layout::wheel_lines();
            if self.gesture.is_none() && !is_mouse_button_down(MouseButton::Left) {
                if command() || (!self.prefs.scroll.pans() && wheel.y != 0.) {
                    self.zoom_at(pointer, scroll_zoom(wheel.y, self.prefs.zoom_speed));
                } else {
                    let movement = if shift() {
                        vec2(wheel.x + wheel.y, 0.)
                    } else {
                        wheel
                    };
                    self.view.center -= V2::new(movement.x, movement.y)
                        * (56. * layout.s * self.prefs.pan_speed / self.view.zoom);
                }
            }
        }
        let pan = is_mouse_button_down(MouseButton::Middle)
            || (self.mode == Mode::Pan && is_mouse_button_down(MouseButton::Left))
            || (is_key_down(KeyCode::Space)
                && !self.typing()
                && is_mouse_button_down(MouseButton::Left));
        if pan && self.gesture.is_none() {
            if let Some(last) = self.panning {
                self.view.center -=
                    V2::new(pointer.x - last.x, pointer.y - last.y) / self.view.zoom;
            }
            if inside || self.panning.is_some() {
                self.panning = Some(pointer);
            }
            self.hover = None;
            return;
        }
        self.panning = None;
        self.hover = (inside && self.gesture.is_none())
            .then(|| self.pick(pointer))
            .flatten();
        if inside
            && self.mode != Mode::Pan
            && self.gesture.is_none()
            && is_mouse_button_pressed(MouseButton::Right)
        {
            self.eyedropper(pointer);
        }
        if inside && self.gesture.is_none() && is_mouse_button_pressed(MouseButton::Left) {
            self.press(pointer);
        }
        let Some(mut gesture) = self.gesture.take() else {
            return;
        };
        self.drag(&mut gesture, pointer);
        if is_mouse_button_down(MouseButton::Left) {
            self.gesture = Some(gesture);
        } else {
            self.release(gesture, pointer);
        }
    }
    pub(super) fn zoom_at(&mut self, p: Vec2, factor: f32) {
        let before = self.view.world(p);
        self.view.zoom = (self.view.zoom * factor).clamp(0.015, 4.);
        let after = self.view.world(p);
        self.view.center += before - after;
    }
    fn press(&mut self, pointer: Vec2) {
        let at = self.snapped(pointer);
        match self.mode {
            Mode::Pan => {}
            Mode::Build if self.swipe => {
                self.begin();
                let mut cells = HashSet::new();
                self.paint(&mut cells, pointer, pointer);
                self.gesture = Some(Gesture::Paint {
                    cells,
                    last: pointer,
                });
            }
            Mode::Build => {
                self.gesture = Some(Gesture::Draw {
                    start: at,
                    last: at,
                })
            }
            Mode::Edit => {
                if let Some(gesture) = self.handle_at(pointer) {
                    self.begin();
                    self.gesture = Some(gesture);
                    return;
                }
                let hit = if self.swipe { None } else { self.hover };
                let Some(key) = hit else {
                    if !shift() {
                        self.selection.clear();
                    }
                    self.gesture = Some(Gesture::Marquee {
                        start: pointer,
                        additive: shift(),
                    });
                    return;
                };
                if shift() {
                    self.toggle_selected(key);
                    return;
                }
                let now = get_time();
                let double = self
                    .last_click
                    .is_some_and(|(last, at)| last == key && now - at < 0.35);
                self.last_click = Some((key, now));
                if double
                    && let Some(group) = self.doc.object(key).map(|o| o.group())
                    && group > 0
                {
                    let keys = self.doc.in_group(group);
                    let count = keys.len();
                    self.set_selection(keys);
                    self.say(format!(
                        "Selected group {group}: {}.",
                        super::objects(count)
                    ));
                    return;
                }
                if !self.selected(key) {
                    self.set_selection(vec![key]);
                }
                self.tab = Tab::Object;
                self.begin();
                if alt() {
                    // Alt-drag leaves the originals and carries copies.
                    let copies: Vec<Object> = self
                        .selection
                        .iter()
                        .filter_map(|&key| self.doc.object(key))
                        .collect();
                    if let Some(b) = self.doc.bounds(&self.selection) {
                        let added = self.doc.place(&copies, [b[0], b[1]]);
                        self.set_selection(added);
                    }
                }
                let objects = self
                    .selection
                    .iter()
                    .filter_map(|&key| self.doc.object(key).map(|o| (key, o)))
                    .collect();
                self.gesture = Some(Gesture::Move {
                    press: pointer,
                    start: at,
                    objects,
                    clicked: key,
                    moved: false,
                });
            }
            Mode::Delete if self.swipe => {
                self.begin();
                self.erase(pointer, pointer);
                self.gesture = Some(Gesture::Erase { last: pointer });
            }
            Mode::Delete => match self.hover {
                Some(key) => self.apply(|editor| {
                    editor.doc.delete(&[key]);
                    editor.selection.clear();
                }),
                None => {
                    self.gesture = Some(Gesture::Marquee {
                        start: pointer,
                        additive: false,
                    })
                }
            },
        }
    }
    fn drag(&mut self, gesture: &mut Gesture, pointer: Vec2) {
        match gesture {
            Gesture::Draw { last, .. } => *last = self.snapped(pointer),
            Gesture::Paint { cells, last } => {
                self.paint(cells, *last, pointer);
                *last = pointer;
            }
            Gesture::Move {
                press,
                start,
                objects,
                moved,
                ..
            } => {
                if !*moved && press.distance(pointer) < 4. {
                    return;
                }
                *moved = true;
                let delta = self.snapped(pointer) - *start;
                for (key, original) in objects.iter() {
                    self.doc.replace(*key, original.clone());
                }
                let keys: Vec<Key> = objects.iter().map(|(key, _)| *key).collect();
                self.doc.nudge(&keys, delta.x, delta.y);
            }
            Gesture::Resize {
                originals,
                from,
                handle,
            } => {
                let p = self.snapped(pointer);
                let least = [4. / self.doc.map.width, 4. / self.doc.map.height];
                let to = resized(*from, *handle, [p.x, p.y], least, shift(), alt());
                self.doc.stretch(originals, *from, to);
            }
            Gesture::Exit { key } => {
                let p = self.snapped(pointer);
                if let Some(Object::Zone(mut zone)) = self.doc.object(*key) {
                    zone.exit = [p.x, p.y];
                    self.doc.replace(*key, Object::Zone(zone));
                }
            }
            Gesture::Travel { index, center } => {
                let world = self.view.world(pointer);
                let (w, h) = (self.doc.map.width, self.doc.map.height);
                let mut travel = [world.x / w - center[0], world.y / h - center[1]];
                if self.snap {
                    travel[0] = (travel[0] * w / self.grid).round() * self.grid / w;
                    travel[1] = (travel[1] * h / self.grid).round() * self.grid / h;
                }
                // Shift keeps the move straight along its longer axis.
                if shift() {
                    let along = usize::from(travel[1].abs() > travel[0].abs());
                    travel[1 - along] = 0.;
                }
                let travel = travel.map(|v| v.clamp(-TRAVEL_LIMIT, TRAVEL_LIMIT));
                let key = Key::Trigger(*index);
                if let Some(Object::Trigger(mut trigger)) = self.doc.object(key) {
                    trigger.travel = travel;
                    if trigger.tween > 0. {
                        trigger.tween = trigger.tween.max(least_tween(travel));
                    }
                    self.doc.replace(key, Object::Trigger(trigger));
                }
            }
            Gesture::Marquee { .. } => {}
            Gesture::Erase { last } => {
                self.erase(*last, pointer);
                *last = pointer;
            }
        }
    }
    fn release(&mut self, gesture: Gesture, pointer: Vec2) {
        match gesture {
            Gesture::Draw { start, last } => {
                let r = self
                    .drawn(start, last)
                    .unwrap_or_else(|| self.brush_cell(pointer));
                self.place(r);
            }
            Gesture::Move { clicked, moved, .. } => {
                if !moved && self.selection.len() > 1 {
                    self.set_selection(vec![clicked]);
                }
                self.commit();
            }
            Gesture::Marquee { start, additive } => {
                let area = screen_area(start, pointer);
                if area.w < 3. && area.h < 3. {
                    return;
                }
                let keys: Vec<Key> = self
                    .doc
                    .items()
                    .filter(|&(key, r, group)| {
                        self.pickable(group) && overlaps(area, self.hit_area(key, r))
                    })
                    .map(|(key, _, _)| key)
                    .collect();
                if self.mode == Mode::Delete {
                    let count = keys.len();
                    self.apply(|editor| {
                        editor.doc.delete(&keys);
                        editor.selection.clear();
                    });
                    if count > 0 {
                        self.say(format!("Deleted {count} objects. Ctrl+Z brings them back."));
                    }
                } else if additive {
                    let mut all = std::mem::take(&mut self.selection);
                    all.extend(keys);
                    self.set_selection(all);
                } else {
                    self.set_selection(keys);
                }
            }
            Gesture::Paint { .. }
            | Gesture::Resize { .. }
            | Gesture::Exit { .. }
            | Gesture::Travel { .. }
            | Gesture::Erase { .. } => self.commit(),
        }
    }

    pub(super) fn draw_canvas(&self, layout: &Layout, settings: &Settings, config: &WorldConfig) {
        let canvas = layout.canvas;
        let s = layout.s;
        draw_rectangle(
            canvas.x,
            canvas.y,
            canvas.w,
            canvas.h,
            mix(bg_color(), BLACK, 0.08),
        );
        let clip = ui_layout::region(canvas, screen_height(), 0.);
        let map = &self.doc.map;
        let arena = self.on_screen([0., 0., 1., 1.]);
        draw_rectangle(
            arena.x,
            arena.y,
            arena.w,
            arena.h,
            mix(bg_color(), scenery::color([140, 165, 174]), 0.1),
        );
        self.draw_grid(canvas, arena);
        let ghost = self.ghost_mask();
        let shown = scenery::Shown {
            hidden: if self.show_hidden { 0 } else { ghost },
            counters: self.preview.counters,
            poses: self.preview.poses(),
        };
        scenery::floor(map, config, &shown, &self.view, self.time);
        scenery::decorations(map, config, &shown, &self.view, self.time, false, settings);
        let style = |style: BlockStyle| {
            scenery::wall_style(
                if style == BlockStyle::Map {
                    map.style
                } else {
                    style
                },
                &map.id,
            )
        };
        let view = Rect::new(
            canvas.x - 40.,
            canvas.y - 40.,
            canvas.w + 80.,
            canvas.h + 80.,
        );
        for (key, r, group) in self.doc.items() {
            // The logic preview glides groups; their homes stay outlined,
            // since that is where editing moves them.
            let moved = shown.poses.offset(group) != [0., 0.];
            if moved {
                dashed_rect(self.on_screen(r), 1., 4., alpha(muted_color(), 0.45));
            }
            let rect = self.on_screen(shown.poses.shift(r, group));
            if !overlaps(view, rect) {
                continue;
            }
            let hidden = MapDef::group_bit(group) & ghost != 0;
            if hidden && !self.show_hidden {
                continue;
            }
            let point = |least: f32| {
                let c = rect.center();
                let size = rect.w.max(least);
                Rect::new(c.x - size * 0.5, c.y - size * 0.5, size, size)
            };
            match key {
                Key::Wall(_) => scenery::block(
                    rect,
                    style(BlockStyle::Map),
                    None,
                    settings.dark_arena,
                    settings.shape_quality,
                ),
                Key::Block(i) => {
                    let b = &map.blocks[i];
                    scenery::block(
                        rect,
                        style(b.style),
                        b.color.map(scenery::color),
                        settings.dark_arena,
                        settings.shape_quality,
                    );
                }
                Key::Trigger(i) => self.draw_trigger(i, rect),
                Key::Spawn(i) => brushes::spawn_mark(map.spawn_regions[i].team, rect, s),
                Key::Base(i) => brushes::base_mark(map.team_bases[i].team, point(20.)),
                Key::Objective(_) => brushes::objective_mark(point(20.)),
                Key::Decor(i) if map.decor[i].kind == DecorKind::Emitter => {
                    dashed_rect(rect, 1., 5., alpha(line_color(), 0.7));
                    if !self.animate && rect.w > 24. {
                        center_label(
                            "FX",
                            rect.center().x,
                            rect.center().y + 4.,
                            11.,
                            muted_color(),
                        );
                    }
                }
                Key::Zone(_) | Key::Decor(_) => {}
            }
            if hidden {
                draw_rectangle(rect.x, rect.y, rect.w, rect.h, alpha(bg_color(), 0.5));
                dashed_rect(rect, 1.5, 6., alpha(text_color(), 0.55));
            }
        }
        scenery::decorations(map, config, &shown, &self.view, self.time, true, settings);
        if let Some(focus) = self.focus {
            for (key, r, group) in self.doc.items() {
                if group != focus {
                    let a = self.hit_area(key, r);
                    if overlaps(view, a) {
                        draw_rectangle(a.x, a.y, a.w, a.h, alpha(bg_color(), 0.62));
                    }
                }
            }
        }
        draw_rectangle_lines(arena.x, arena.y, arena.w, arena.h, 2., muted_color());
        self.draw_links();
        self.draw_trigger_tags(view, s);
        if self.show_groups {
            self.draw_group_badges(view, s);
        }
        self.draw_selection(s);
        self.draw_preview_marks(s);
        self.draw_gesture(canvas);
        drop(clip);
        self.draw_canvas_status(layout);
    }
    fn draw_grid(&self, canvas: Rect, arena: Rect) {
        let zoom = self.view.zoom;
        let mut step = self.grid;
        while step * zoom < 7. {
            step *= 2.;
        }
        let major = step * 4.;
        let lo = self.view.world(vec2(canvas.x, canvas.y));
        let hi = self.view.world(vec2(canvas.right(), canvas.bottom()));
        let (w, h) = (self.doc.map.width, self.doc.map.height);
        let top = arena.y.max(canvas.y);
        let bottom = arena.bottom().min(canvas.bottom());
        let left = arena.x.max(canvas.x);
        let right = arena.right().min(canvas.right());
        let line = |value: f32| {
            let on_major = (value / major).fract().abs() < 1e-3;
            alpha(line_color(), if on_major { 0.42 } else { 0.2 })
        };
        // Pixel centers keep one-pixel lines even; fractional positions
        // would make some lines look heavier than others.
        let crisp = |v: f32| v.floor() + 0.5;
        let first = (lo.x.max(0.) / step).floor() as i32;
        let last = (hi.x.min(w) / step).ceil() as i32;
        for i in first..=last.min(first + 600) {
            let x = i as f32 * step;
            let p = crisp(self.view.screen(V2::new(x, 0.)).x);
            draw_line(p, top, p, bottom, 1., line(x));
        }
        let first = (lo.y.max(0.) / step).floor() as i32;
        let last = (hi.y.min(h) / step).ceil() as i32;
        for i in first..=last.min(first + 600) {
            let y = i as f32 * step;
            let p = crisp(self.view.screen(V2::new(0., y)).y);
            draw_line(left, p, right, p, 1., line(y));
        }
    }
    /// Where a trigger's badge sits: the corner of an area trigger, or the
    /// whole of a point trigger.
    fn badge_rect(trigger: &MapTrigger, rect: Rect) -> Rect {
        if brushes::trigger_area(trigger) {
            let size = (rect.w.min(rect.h) * 0.3).clamp(18., 34.);
            Rect::new(rect.x + 3., rect.y + 3., size, size)
        } else {
            let size = rect.w.min(rect.h).clamp(26., 52.);
            let c = rect.center();
            Rect::new(c.x - size * 0.5, c.y - size * 0.5, size, size)
        }
    }
    fn draw_trigger(&self, index: usize, rect: Rect) {
        let trigger = &self.doc.map.triggers[index];
        let lit = self.preview.flashes.get(index).map_or(0., |&at| {
            (1. - (self.preview.time - at) / 0.7).clamp(0., 1.)
        });
        if brushes::trigger_area(trigger) {
            draw_rectangle(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                alpha(brushes::LOGIC, 0.06 + lit * 0.18),
            );
            dashed_rect(rect, 2., 7., alpha(brushes::LOGIC, 0.85));
        }
        let badge = Self::badge_rect(trigger, rect);
        if lit > 0. {
            let c = badge.center();
            draw_circle(
                c.x,
                c.y,
                badge.w * (0.8 + 0.5 * (1. - lit)),
                alpha(brushes::LOGIC, lit * 0.35),
            );
        }
        brushes::trigger_badge(trigger.action, badge, lit);
        if trigger.only_if.is_some() {
            brushes::gate_mark(trigger, badge, lit);
        }
    }
    /// Each trigger's tag under its badge, or above it when that spot is
    /// taken. A tag that fits neither is left out; the pointer shows it.
    /// Selected and hovered triggers place theirs first, so they always show.
    fn draw_trigger_tags(&self, view: Rect, s: f32) {
        if self.view.zoom <= 0.12 {
            return;
        }
        let map = &self.doc.map;
        let ghost = self.ghost_mask();
        let badges: Vec<Option<Rect>> = map
            .triggers
            .iter()
            .map(|t| {
                let rect = self.on_screen(t.rect);
                let shown = self.show_hidden || MapDef::group_bit(t.member) & ghost == 0;
                (shown && overlaps(view, rect)).then(|| Self::badge_rect(t, rect))
            })
            .collect();
        let first = |index: &usize| {
            let key = Key::Trigger(*index);
            !(self.selection.contains(&key) || self.hover == Some(key))
        };
        let mut order: Vec<usize> = (0..map.triggers.len()).collect();
        order.sort_by_key(first);
        let size = 11. * s.max(0.8);
        let height = size + 5.;
        let mut placed: Vec<Rect> = Vec::new();
        for index in order {
            let Some(badge) = badges[index] else {
                continue;
            };
            if badge.w < 26. {
                continue;
            }
            let tag = trigger_badge_tag(&map.triggers[index]);
            let width = label_width(&tag, size) + 8.;
            let c = badge.center();
            let below = Rect::new(c.x - width * 0.5, badge.bottom() + 3., width, height);
            let above = Rect::new(c.x - width * 0.5, badge.y - 3. - height, width, height);
            let free = |spot: &Rect| {
                !placed.iter().any(|r| overlaps(*r, *spot))
                    && !badges
                        .iter()
                        .enumerate()
                        .any(|(i, b)| i != index && b.is_some_and(|b| overlaps(b, *spot)))
            };
            let spot = if free(&below) {
                below
            } else if free(&above) {
                above
            } else if !first(&index) {
                below
            } else {
                continue;
            };
            draw_rectangle(
                spot.x,
                spot.y,
                spot.w,
                spot.h,
                Color::new(0.13, 0.12, 0.10, 0.82),
            );
            center_label(&tag, c.x, spot.y + size, size, brushes::LOGIC);
            placed.push(spot);
        }
    }
    fn trigger_point(&self, index: usize) -> Vec2 {
        let trigger = &self.doc.map.triggers[index];
        Self::badge_rect(trigger, self.on_screen(trigger.rect)).center()
    }
    fn draw_links(&self) {
        let map = &self.doc.map;
        if map.triggers.is_empty() {
            return;
        }
        let watched = |index: usize| {
            self.show_links
                || self.selected(Key::Trigger(index))
                || self.hover == Some(Key::Trigger(index))
        };
        // Objects of a watched group show which triggers act on them.
        let mut watched_groups = 0u32;
        for &key in self.selection.iter().chain(self.hover.iter()) {
            if !matches!(key, Key::Trigger(_))
                && let Some(object) = self.doc.object(key)
            {
                watched_groups |= MapDef::group_bit(object.group());
            }
        }
        let group_color = mix(accent_color(), CYAN, 0.4);
        for (index, trigger) in map.triggers.iter().enumerate() {
            let outgoing = watched(index);
            let incoming = trigger.action.targets_group()
                && MapDef::group_bit(trigger.group) & watched_groups != 0;
            if !outgoing && !incoming {
                continue;
            }
            let from = self.trigger_point(index);
            if trigger.action.targets_group() {
                let mut drawn = 0;
                for (key, r, group) in self.doc.items() {
                    if group != trigger.group || key == Key::Trigger(index) {
                        continue;
                    }
                    let to = match key {
                        Key::Trigger(other) => self.trigger_point(other),
                        _ => self.on_screen(r).center(),
                    };
                    arrow(from, to, 2., alpha(group_color, 0.8));
                    drawn += 1;
                    if drawn >= 64 {
                        break;
                    }
                }
            }
            if !outgoing {
                continue;
            }
            let color = |kind: Link| match kind {
                Link::Signal | Link::Otherwise => VIOLET,
                Link::Counter => AMBER,
                Link::Group => group_color,
                Link::Reset => RED,
            };
            // What it sets off, and, when only it is watched, what sets it off.
            for (other, target) in map.triggers.iter().enumerate() {
                if other == index {
                    continue;
                }
                if let Some(kind) = link(trigger, target) {
                    let width = if kind == Link::Otherwise { 1.3 } else { 2. };
                    arrow(
                        from,
                        self.trigger_point(other),
                        width,
                        alpha(color(kind), 0.85),
                    );
                }
                if !self.show_links
                    && let Some(kind) = link(target, trigger)
                {
                    arrow(
                        self.trigger_point(other),
                        from,
                        1.5,
                        alpha(color(kind), 0.5),
                    );
                }
            }
        }
    }
    fn draw_group_badges(&self, view: Rect, s: f32) {
        let size = 10. * s.max(0.8);
        for (key, r, group) in self.doc.items() {
            if group == 0 {
                continue;
            }
            let a = self.hit_area(key, r);
            if !overlaps(view, a) || a.w < 14. {
                continue;
            }
            let text = group.to_string();
            let width = label_width(&text, size) + 6.;
            let x = a.x - 2.;
            let y = a.y - size - 4.;
            draw_rectangle(x, y, width, size + 4., alpha(VIOLET, 0.9));
            label(
                &text,
                x + 3.,
                y + size,
                size,
                Color::new(0.1, 0.08, 0.14, 1.),
            );
        }
    }
    fn draw_selection(&self, s: f32) {
        let accent = accent_color();
        for &key in &self.selection {
            if let Some(r) = self.doc.rect(key) {
                let a = self.hit_area(key, r);
                draw_rectangle_lines(a.x - 2., a.y - 2., a.w + 4., a.h + 4., 2., accent);
            }
        }
        if let Some(key) = self.hover
            && !self.selected(key)
            && let Some(r) = self.doc.rect(key)
        {
            let a = self.hit_area(key, r);
            let color = if self.mode == Mode::Delete {
                RED
            } else {
                text_color()
            };
            draw_rectangle_lines(
                a.x - 1.,
                a.y - 1.,
                a.w + 2.,
                a.h + 2.,
                1.5,
                alpha(color, 0.8),
            );
        }
        if self.mode != Mode::Edit {
            return;
        }
        if let Some(b) = self.doc.bounds(&self.selection)
            && self.selection.len() > 1
        {
            dashed_rect(self.on_screen(b), 1., 4., alpha(accent, 0.6));
        }
        let Some((originals, from)) = self.handle_target() else {
            return;
        };
        // Mid-drag, the handles follow the new bounds.
        let from = match &self.gesture {
            Some(Gesture::Resize { .. }) => self.doc.bounds(&self.selection).unwrap_or(from),
            _ => from,
        };
        let r = self.on_screen(from);
        let size = 7. * s.max(0.8);
        for handle in HANDLES {
            let p = Self::handle_spot(r, handle);
            draw_rectangle(
                p.x - size * 0.5,
                p.y - size * 0.5,
                size,
                size,
                surface_color(),
            );
            draw_rectangle_lines(p.x - size * 0.5, p.y - size * 0.5, size, size, 1.5, accent);
        }
        if let [(_, Object::Zone(zone))] = originals.as_slice()
            && zone.kind == ZoneKind::Portal
        {
            let exit = self.point_on_screen(zone.exit);
            let color = scenery::zone_color(ZoneKind::Portal);
            arrow(r.center(), exit, 2., alpha(color, 0.8));
            // A polygon's first corner points right, so no turn is a diamond.
            draw_poly(exit.x, exit.y, 4, 9., 0., color);
            draw_poly_lines(exit.x, exit.y, 4, 9., 0., 1.5, WHITE);
        }
        self.draw_travel(s);
    }
    /// Where the selected Move or Place trigger sends its group: a ghost of
    /// each member there, an arrow, a dot riding the arrow with the
    /// trigger's easing, and the diamond that drags the destination.
    fn draw_travel(&self, s: f32) {
        let Some((index, bounds)) = self.travel_target() else {
            return;
        };
        let trigger = &self.doc.map.triggers[index];
        let [dx, dy] = trigger.travel;
        let color = brushes::LOGIC;
        let center = [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5];
        let from = self.point_on_screen(center);
        let to = self.point_on_screen([center[0] + dx, center[1] + dy]);
        dashed_rect(self.on_screen(bounds), 1., 4., alpha(color, 0.55));
        let members = self
            .doc
            .items()
            .filter(|&(key, _, group)| group == trigger.group && !matches!(key, Key::Trigger(_)));
        for (_, r, _) in members.take(256) {
            let ghost = self.on_screen([r[0] + dx, r[1] + dy, r[2], r[3]]);
            draw_rectangle(ghost.x, ghost.y, ghost.w, ghost.h, alpha(color, 0.10));
            dashed_rect(ghost, 1.5, 5., alpha(color, 0.75));
        }
        if from.distance(to) > 2. {
            arrow(from, to, 2., alpha(color, 0.85));
        }
        if trigger.tween > 0. && !reduced_motion() {
            let phase = ((self.time % (trigger.tween + 0.6)) / trigger.tween).min(1.);
            let p = from.lerp(to, trigger.easing.apply(phase));
            draw_circle(p.x, p.y, 4. * s.max(0.8), alpha(color, 0.95));
        }
        draw_poly(to.x, to.y, 4, 9., 0., color);
        draw_poly_lines(to.x, to.y, 4, 9., 0., 1.5, WHITE);
    }
    fn draw_preview_marks(&self, s: f32) {
        let preview = &self.preview;
        if preview.at_start() && !preview.running {
            return;
        }
        for event in &preview.events {
            let Some(place) = event.place else {
                continue;
            };
            let age = preview.time - event.at;
            if !(0. ..2.5).contains(&age) {
                continue;
            }
            let fade = 1. - age / 2.5;
            let p = self.view.screen(place);
            draw_circle_lines(p.x, p.y, 18. + age * 30., 2., alpha(brushes::LOGIC, fade));
            center_label(
                &event.text,
                p.x,
                p.y - 26.,
                13. * s,
                alpha(text_color(), fade),
            );
        }
        if let Some(probe) = self.probe {
            let p = self.view.screen(probe);
            let radius = 28. * self.view.zoom;
            draw_circle(p.x, p.y, radius.max(5.), alpha(player_color(), 0.55));
            draw_circle_lines(p.x, p.y, radius.max(5.), 2., player_color());
        }
    }
    fn draw_gesture(&self, canvas: Rect) {
        let pointer = ui_layout::pointer();
        match &self.gesture {
            Some(Gesture::Draw { start, last }) => {
                // Until the drag is long enough it is still a click.
                let area = self
                    .drawn(*start, *last)
                    .unwrap_or_else(|| self.brush_cell(pointer));
                let r = self.on_screen(area);
                draw_rectangle(r.x, r.y, r.w, r.h, alpha(accent_color(), 0.12));
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2., accent_color());
                let size = [area[2] * self.doc.map.width, area[3] * self.doc.map.height];
                // Below and right of the pointer, where the cursor won't cover it.
                label(
                    &format!("{:.0} × {:.0}", size[0], size[1]),
                    pointer.x + 18.,
                    pointer.y + 34.,
                    14.,
                    text_color(),
                );
            }
            Some(Gesture::Marquee { start, .. }) => {
                let r = screen_area(*start, pointer);
                let color = if self.mode == Mode::Delete {
                    RED
                } else {
                    accent_color()
                };
                draw_rectangle(r.x, r.y, r.w, r.h, alpha(color, 0.1));
                dashed_rect(r, 1.5, 5., color);
            }
            Some(Gesture::Erase { .. }) => {
                draw_circle_lines(pointer.x, pointer.y, 7., 2., RED);
            }
            None if self.mode == Mode::Build
                && canvas.contains(pointer)
                && self.panning.is_none()
                && !self.help =>
            {
                // Where a click would build.
                let r = self.on_screen(self.brush_cell(pointer));
                draw_rectangle(r.x, r.y, r.w, r.h, alpha(accent_color(), 0.1));
                dashed_rect(r, 1.5, 5., alpha(accent_color(), 0.8));
            }
            _ => {}
        }
    }
    fn draw_canvas_status(&self, layout: &Layout) {
        let canvas = layout.canvas;
        let s = layout.s;
        let size = 13. * s;
        let pointer = ui_layout::pointer();
        let mode = match self.mode {
            Mode::Build => {
                let (object, _) = self.current_brush();
                let name = if self.stamp.is_some() {
                    format!("copy of {}", object.name().to_lowercase())
                } else {
                    self.brushes[self.category][self.brush].name.to_owned()
                };
                format!("Build · {name}{}", if self.swipe { " · swipe" } else { "" })
            }
            Mode::Edit => match self.selection.len() {
                0 => "Edit".to_owned(),
                1 => "Edit · 1 selected".to_owned(),
                n => format!("Edit · {n} selected"),
            },
            Mode::Delete => format!("Delete{}", if self.swipe { " · swipe" } else { "" }),
            Mode::Pan => "Pan · drag to move the view".to_owned(),
        };
        let chip = |text: &str, x: f32, y: f32| {
            let width = label_width(text, size) + 14. * s;
            draw_rectangle(x, y, width, size + 10. * s, alpha(surface_color(), 0.88));
            label(text, x + 7. * s, y + size + 3. * s, size, text_color());
            width
        };
        let mut x = canvas.x + 10. * s;
        x += chip(&mode, x, canvas.y + 10. * s) + 6. * s;
        if let Some(focus) = self.focus {
            chip(&format!("Only group {focus}"), x, canvas.y + 10. * s);
        }
        let world = self.view.world(pointer);
        if canvas.contains(pointer) {
            chip(
                &format!(
                    "{:.0}, {:.0}   {:.0}%",
                    world.x,
                    world.y,
                    self.view.zoom * 100.
                ),
                canvas.x + 10. * s,
                canvas.bottom() - size - 20. * s,
            );
        }
        let preview = &self.preview;
        if preview.running || !preview.at_start() {
            let minutes = (preview.time / 60.).floor();
            let text = format!(
                "{} preview  {minutes:.0}:{:04.1}  ×{}",
                if preview.running { "Logic" } else { "Paused" },
                preview.time - minutes * 60.,
                preview.speed
            );
            let width = label_width(&text, size) + 14. * s;
            chip(&text, canvas.right() - width - 10. * s, canvas.y + 10. * s);
            let counters: Vec<String> = preview
                .counters
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != 0)
                .map(|(i, v)| format!("#{} {v}", i + 1))
                .collect();
            if !counters.is_empty() {
                let text = counters.join("   ");
                let width = label_width(&text, size) + 14. * s;
                chip(
                    &text,
                    canvas.right() - width - 10. * s,
                    canvas.bottom() - size - 20. * s,
                );
            }
            if let Some((at, text)) = &preview.banner
                && preview.time - at < 4.
            {
                let big = 20. * s;
                let width = label_width(text, big).min(canvas.w - 40. * s) + 30. * s;
                let x = canvas.center().x - width * 0.5;
                let y = canvas.y + 52. * s;
                draw_rectangle(x, y, width, big + 18. * s, alpha(surface_color(), 0.94));
                draw_rectangle(x, y, 4. * s, big + 18. * s, brushes::LOGIC);
                center_label(
                    &fitted_label(text, big, width - 30. * s),
                    canvas.center().x,
                    y + big + 4. * s,
                    big,
                    text_color(),
                );
            }
        }
        if !self.notice.is_empty() && get_time() - self.notice_at < 4.5 {
            let fade = (4.5 - (get_time() - self.notice_at) as f32).clamp(0., 1.);
            let width = (label_width(&self.notice, size) + 24. * s).min(canvas.w - 20. * s);
            let x = canvas.center().x - width * 0.5;
            let y = canvas.bottom() - size - 26. * s;
            draw_rectangle(
                x,
                y,
                width,
                size + 14. * s,
                alpha(text_color(), 0.86 * fade),
            );
            center_label(
                &fitted_label(&self.notice, size, width - 20. * s),
                canvas.center().x,
                y + size + 4. * s,
                size,
                alpha(surface_color(), fade),
            );
        }
        if let Some(key) = self.hover
            && self.gesture.is_none()
        {
            self.draw_tooltip(key, pointer, layout);
        }
    }
    fn draw_tooltip(&self, key: Key, pointer: Vec2, layout: &Layout) {
        let Some(object) = self.doc.object(key) else {
            return;
        };
        let s = layout.s;
        let size = 12. * s;
        let mut title = object.name().to_owned();
        if let Object::Block(b) = &object {
            title = format!("{} block", b.style.name());
        }
        if object.group() > 0 {
            title += &format!("  ·  group {}", object.group());
        }
        let detail = match &object {
            Object::Trigger(t) => Some(describe_trigger(t)),
            Object::Zone(z) if z.kind == ZoneKind::Portal => {
                Some("Sends tanks to its exit marker.".into())
            }
            Object::Zone(z) if z.kind == ZoneKind::Conveyor => {
                Some(format!("Pushes tanks toward {:.0}°.", z.angle))
            }
            Object::Zone(z) if z.kind == ZoneKind::Launch => {
                Some(format!("Flings tanks toward {:.0}°.", z.angle))
            }
            _ => None,
        };
        let width = 260. * s;
        let lines = detail
            .as_deref()
            .map(|text| wrapped_lines(text, width - 16. * s, size))
            .unwrap_or_default();
        let height = size + 14. * s + lines.len() as f32 * (size + 4. * s);
        let x = (pointer.x + 18.).min(layout.canvas.right() - width - 4.);
        let y = (pointer.y + 18.).min(layout.canvas.bottom() - height - 4.);
        draw_rectangle(x, y, width, height, alpha(surface_color(), 0.96));
        draw_rectangle_lines(x, y, width, height, 1., line_color());
        label(&title, x + 8. * s, y + size + 5. * s, size, text_color());
        for (i, line) in lines.iter().enumerate() {
            label(
                line,
                x + 8. * s,
                y + size * 2. + 9. * s + i as f32 * (size + 4. * s),
                size,
                muted_color(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_scroll_deltas_zoom_proportionally_without_frame_rate_dependence() {
        assert!(scroll_zoom(0.05, 1.) < scroll_zoom(1., 1.));
        let divided = (0..10).fold(1., |zoom, _| zoom * scroll_zoom(0.05, 1.));
        assert!((divided - scroll_zoom(0.5, 1.)).abs() < 0.00001);
        assert!((scroll_zoom(0.5, 1.) * scroll_zoom(-0.5, 1.) - 1.).abs() < 0.00001);
        assert!(scroll_zoom(1000., 3.) < 1.5);
    }

    #[test]
    fn thin_selections_move_when_pressed_inside() {
        // Three blocks in a row at a zoomed-out view: 56 by 19 pixels.
        let r = Rect::new(100., 100., 56., 19.);
        // Away from the ends, which grab the side handles as on any object.
        let inside = [
            r.center(),
            vec2(r.center().x, r.y + 1.),
            vec2(r.x + 12., r.bottom() - 1.),
        ];
        for p in inside {
            let near = HANDLES
                .into_iter()
                .find(|&h| Editor::handle_spot(r, h).distance(p) < HANDLE_REACH);
            assert_eq!(near, None, "{p:?} grabs a handle");
        }
        // The wide side keeps its handles on the edge; the thin one stands off.
        assert_eq!(Editor::handle_spot(r, (1, 0)), vec2(156., 109.5));
        assert_eq!(Editor::handle_spot(r, (0, -1)), vec2(128., 89.));
        // A roomy selection's handles sit on its corners.
        let big = Rect::new(0., 0., 200., 120.);
        assert_eq!(Editor::handle_spot(big, (1, 1)), vec2(200., 120.));
    }
}
