//! The canvas: navigation, pointer gestures for the editor modes, and the
//! arena drawn with the editor's overlays.
use super::{Editor, Layout, Mode, Tab, alt, brushes, command, shift};
use crate::graphics::{draw_circle, draw_poly};
use crate::{
    profile, scenery,
    settings::Settings,
    ui::{CYAN, RED, VIOLET, *},
    ui_layout,
};
use macroquad::prelude::*;
use std::collections::{HashMap, HashSet};
use tankyard_client::editor_document::{
    Key, LAYERS, Object, describe_trigger_named, trigger_badge_tag,
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
    /// A click that started on the one selected object, over others, goes
    /// down to the next of them when it ends without moving (`cycle`).
    Move {
        press: Vec2,
        start: Vec2,
        objects: Vec<(Key, Object)>,
        clicked: Key,
        moved: bool,
        cycle: bool,
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
/// The smallest a crowded trigger's badge gets on screen, in pixels.
const LEAST_BADGE: f32 = 9.;
/// How much of the gap to its nearest neighbor a crowded badge fills.
const BADGE_FILL: f32 = 0.94;
/// How many layer shortcuts the strip above the layer bar shows at most,
/// and how wide each one is.
const MOST_LAYER_CHIPS: usize = 16;
const LAYER_CHIP: f32 = 34.;
/// How strongly objects on layers other than the one being edited show.
const FADED: f32 = 0.3;
/// A selection side shorter than this on screen has the handles across it
/// stand outside the edge, so pressing in a thin selection still moves it.
const HANDLE_ROOM: f32 = 40.;
/// How long, in seconds, what an undo changed stays lit.
const FLASH_LIFE: f32 = 0.9;

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

/// A distance with its direction, as a ruler would say it.
fn signed(value: f32) -> String {
    let rounded = value.round();
    if rounded > 0. {
        format!("+{rounded:.0}")
    } else if rounded < 0. {
        format!("−{:.0}", -rounded)
    } else {
        "0".to_owned()
    }
}

/// A short measurement below and right of the pointer, where the cursor
/// won't cover it, kept on the canvas.
fn measurement(text: &str, pointer: Vec2, canvas: Rect, s: f32) {
    let s = s.max(0.8);
    let size = 13. * s;
    let (pad, height) = (6. * s, size + 10. * s);
    let width = label_width(text, size) + pad * 2.;
    let x = (pointer.x + 18.)
        .min(canvas.right() - width - 4.)
        .max(canvas.x + 4.);
    let y = (pointer.y + 22.).min(canvas.bottom() - height - 4.);
    draw_rectangle(x, y, width, height, alpha(surface_color(), 0.9));
    label(text, x + pad, y + size + 3. * s, size, text_color());
}

/// Which size step an object's area on screen falls in, each step half as
/// big again as the last, so objects of about one size count as equal.
fn pick_size(area: Rect) -> i32 {
    ((area.w * area.h).max(1.).log2() / 0.585).floor() as i32
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
fn dashed_line(a: Vec2, b: Vec2, thickness: f32, dash: f32, color: Color) {
    let count = ((b - a).length() / (dash * 2.)).ceil().clamp(1., 200.) as usize;
    for i in 0..count {
        let t0 = i as f32 / count as f32;
        let t1 = (t0 + 0.5 / count as f32).min(1.);
        let (p, q) = (a.lerp(b, t0), a.lerp(b, t1));
        draw_line(p.x, p.y, q.x, q.y, thickness, color);
    }
}
/// Something a trigger tag must keep clear of.
#[derive(Clone, Copy)]
enum Mark {
    /// A trigger's badge, by trigger index.
    Badge(usize),
    /// A tag placed already, by its place in the placed list.
    Tag(usize),
}
/// Marks by screen cell, for finding what is near a spot.
#[derive(Default)]
struct Cells(HashMap<(i32, i32), Vec<Mark>>);
impl Cells {
    const SIZE: f32 = 96.;
    fn span(r: Rect) -> impl Iterator<Item = (i32, i32)> {
        let cell = |v: f32| (v / Self::SIZE).floor() as i32;
        let (x0, x1, y0, y1) = (cell(r.x), cell(r.right()), cell(r.y), cell(r.bottom()));
        (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
    }
    fn add(&mut self, r: Rect, mark: Mark) {
        for cell in Self::span(r) {
            self.0.entry(cell).or_default().push(mark);
        }
    }
    /// Whether `test` holds for any mark in a cell `r` touches.
    fn any(&self, r: Rect, mut test: impl FnMut(Mark) -> bool) -> bool {
        Self::span(r).any(|cell| {
            self.0
                .get(&cell)
                .is_some_and(|marks| marks.iter().any(|&mark| test(mark)))
        })
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
            // A crowded badge answers only for its own spot, so each of a
            // tight row can be picked.
            Key::Trigger(index) => self
                .badge_room
                .get(index)
                .map_or(28., |room| (room * self.view.zoom).clamp(LEAST_BADGE, 28.)),
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
    /// Whether clicks and boxes can select the object at `key`, in `group`:
    /// it is in the group being worked on, unlocked, showing, and on the
    /// layer being edited.
    pub(super) fn pickable(&self, key: Key, group: u8) -> bool {
        let bit = MapDef::group_bit(group);
        self.focus.is_none_or(|focus| focus == group)
            && bit & self.doc.locked == 0
            && (self.show_hidden || bit & self.ghost_mask() == 0)
            && self.on_layer(key)
    }
    /// Whether `key` is on the layer being edited, or every layer is.
    pub(super) fn on_layer(&self, key: Key) -> bool {
        self.layer
            .is_none_or(|layer| self.doc.layers.get(key) == layer)
    }
    /// What a click at `p` selects: the smallest thing there, so a trigger
    /// on a wide spawn region or floor tile can be reached directly. Among
    /// things of about one size, the one on top.
    pub(super) fn pick(&self, p: Vec2) -> Option<Key> {
        self.doc
            .items_from_top()
            .filter(|&(key, r, group)| {
                self.pickable(key, group) && self.hit_area(key, r).contains(p)
            })
            .min_by_key(|&(key, r, _)| pick_size(self.hit_area(key, r)))
            .map(|(key, _, _)| key)
    }
    /// Every object a click at `p` could select, in the order clicking goes
    /// through them: smallest first, and from the top down within a size.
    pub(super) fn stack(&self, p: Vec2) -> Vec<Key> {
        let mut hits: Vec<(i32, Key)> = self
            .doc
            .items_from_top()
            .filter_map(|(key, r, group)| {
                let area = self.hit_area(key, r);
                (self.pickable(key, group) && area.contains(p)).then(|| (pick_size(area), key))
            })
            .collect();
        hits.sort_by_key(|&(size, _)| size);
        hits.into_iter().map(|(_, key)| key).collect()
    }

    /// A click that ended where it began, on `clicked` in the selection:
    /// several selected narrow to it; with `cycle`, one selected over
    /// others gives way to the next in the stack, and past the last, the first.
    fn click_selection(&mut self, clicked: Key, pointer: Vec2, cycle: bool) {
        if self.selection.len() > 1 {
            self.set_selection(vec![clicked]);
            return;
        }
        if !cycle {
            return;
        }
        let stack = self.stack(pointer);
        let Some(at) = stack.iter().position(|&key| key == clicked) else {
            return;
        };
        let next = (at + 1) % stack.len();
        self.set_selection(vec![stack[next]]);
        self.last_click = None;
        let name = self.doc.object(stack[next]).map_or("Object", |o| o.name());
        self.say(format!(
            "{name}: {} of {} under the pointer. Click again for the next.",
            next + 1,
            stack.len()
        ));
    }

    pub(super) fn cycle_pick(&mut self, pointer: Vec2, backwards: bool) {
        let hits = self.stack(pointer);
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
                editor.doc.layers.set(key, editor.layer.unwrap_or(0));
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
            if self.occupied(&object, object.rect()) {
                continue;
            }
            let Some(key) = self.doc.insert(object) else {
                self.say("The map is at its object limit.");
                break;
            };
            self.doc.layers.set(key, self.layer.unwrap_or(0));
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
        let inside = canvas.contains(pointer)
            && !self.help
            && !self.layer_bar_rect(layout).contains(pointer)
            && !(self.layer_strip && self.layer_strip_rect(layout).contains(pointer))
            && !self
                .legend_rect(layout)
                .is_some_and(|legend| legend.contains(pointer))
            && !self
                .find_rect(layout)
                .is_some_and(|bar| bar.contains(pointer));
        // The preview's widgets sit over the arena: a click on one of their
        // choices or board buttons picks it, and a click elsewhere on them
        // does nothing.
        if inside && self.gesture.is_none() && !self.preview.ui.is_empty() {
            let config = self.config();
            let widgets = self.preview.widgets(&config);
            // The last drawn is on top.
            let over = widgets.iter().rev().find_map(|(index, widget, age)| {
                let pinned = self.pin_place(widget.pin);
                if !widget.pickable() || (widget.pinned() && pinned.is_none()) {
                    return None;
                }
                crate::hud_widgets::covers(widget, canvas, layout.s, pointer, pinned, *age)
                    .then_some((index, widget, pinned, *age))
            });
            if let Some((index, widget, pinned, age)) = over {
                if is_mouse_button_pressed(MouseButton::Left)
                    && let Some(option) = crate::hud_widgets::choice_at(
                        widget, canvas, layout.s, pointer, pinned, age,
                    )
                {
                    let index = *index;
                    self.preview.pick(&self.doc.map, &config, index, option);
                }
                self.hover = None;
                return;
            }
        }
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
            self.hub_hover = None;
            return;
        }
        self.panning = None;
        self.hover = (inside && self.gesture.is_none())
            .then(|| self.pick(pointer))
            .flatten();
        // A hub sits over whatever is beneath it.
        self.hub_hover =
            (inside && self.show_links && self.mode == Mode::Edit && self.gesture.is_none())
                .then(|| self.hub_at(pointer))
                .flatten();
        if let Some(net) = self.hub_hover {
            self.hover = None;
            if is_mouse_button_pressed(MouseButton::Left) {
                self.select_net(net as usize, shift());
                return;
            }
        }
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
        self.handle_press = None;
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
                    self.handle_press = Some(pointer);
                    return;
                }
                let hit = if self.swipe { None } else { self.hover };
                // A press on a selected object holds it even under others,
                // so one reached by clicking down through a pile can move.
                let stack = if hit.is_some() {
                    self.stack(pointer)
                } else {
                    Vec::new()
                };
                let held = stack.iter().copied().find(|&key| self.selected(key));
                let Some(hit) = hit else {
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
                    self.toggle_selected(hit);
                    return;
                }
                let key = held.unwrap_or(hit);
                let cycle = held.is_some() && stack.len() > 1;
                let now = get_time();
                let double = self
                    .last_click
                    .is_some_and(|(last, at)| last == key && now - at < 0.35);
                self.last_click = Some((key, now));
                if double
                    && let Some(group) = self.doc.object(key).map(|o| o.group())
                    && group > 0
                {
                    let keys: Vec<Key> = self
                        .doc
                        .in_group(group)
                        .into_iter()
                        .filter(|&key| self.pickable(key, group))
                        .collect();
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
                    let (copies, layers) = self.copies(&self.selection);
                    if let Some(b) = self.doc.bounds(&self.selection) {
                        let added = self.doc.place(&copies, &layers, [b[0], b[1]]);
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
                    cycle: cycle && !alt(),
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
        // A handle stays put until the pointer travels a little, as the
        // selection does, so a click on one changes nothing.
        if let Some(press) = self.handle_press {
            if press.distance(pointer) < 4. {
                return;
            }
            self.handle_press = None;
        }
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
            Gesture::Move {
                clicked,
                moved,
                cycle,
                ..
            } => {
                self.commit();
                if !moved {
                    self.click_selection(clicked, pointer, cycle);
                }
            }
            // A click on a handle that lies over the selection is a click on
            // the selection.
            Gesture::Resize { .. } | Gesture::Exit { .. } | Gesture::Travel { .. }
                if self.handle_press.take().is_some() =>
            {
                self.commit();
                let stack = self.stack(pointer);
                if let Some(&clicked) = stack.iter().find(|&&key| self.selected(key)) {
                    self.click_selection(clicked, pointer, stack.len() > 1);
                }
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
                        self.pickable(key, group) && overlaps(area, self.hit_area(key, r))
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
        profile::phase("editor grid", || self.draw_grid(canvas, arena));
        let ghost = self.ghost_mask();
        let shown = scenery::Shown {
            hidden: if self.show_hidden { 0 } else { ghost },
            counters: self.preview.counters,
            poses: self.preview.poses(),
            focus: self.preview.probe(),
            fades: self.layer_fades(),
        };
        // Lift the darkness while placing objects. Play Logic previews the
        // map's full lighting, including how its glow responds to the floor.
        let mut lighting = shown.poses.lighting(&map.ambient);
        if !self.preview.running {
            lighting[3] *= 0.5;
        }
        let _daylight =
            crate::glow::daylight(crate::lighting::daylight(settings.dark_arena, lighting));
        profile::phase("editor decor", || {
            scenery::floor(map, config, &shown, &self.view, self.time);
            scenery::decorations(map, config, &shown, &self.view, self.time, false, settings);
        });
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
        let body_mask = map
            .bodies
            .iter()
            .fold(0, |mask, body| mask | MapDef::group_bit(body.group));
        profile::phase("editor items", || {
            let mut badges = brushes::Badges::default();
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
                let fade = if self.on_layer(key) { 1. } else { FADED };
                // Markers come after the triggers and stand over their badges.
                if matches!(key, Key::Spawn(_) | Key::Base(_) | Key::Objective(_)) {
                    badges.draw();
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
                        if body_mask & MapDef::group_bit(b.group) != 0 {
                            scenery::body_marks(rect, settings.dark_arena);
                        }
                    }
                    Key::Trigger(i) => self.draw_trigger(i, rect, hidden, fade, &mut badges),
                    Key::Spawn(i) => brushes::spawn_mark(map.spawn_regions[i].team, rect, s, fade),
                    Key::Base(i) => brushes::base_mark(map.team_bases[i].team, point(20.), fade),
                    Key::Objective(_) => brushes::objective_mark(point(20.), fade),
                    Key::Decor(i) if map.decor[i].kind == DecorKind::Emitter => {
                        dashed_rect(rect, 1., 5., alpha(line_color(), 0.7 * fade));
                        if !self.animate && rect.w > 24. && fade == 1. {
                            badges.words.push(Word {
                                text: "FX".into(),
                                x: rect.center().x,
                                y: rect.center().y + 4.,
                                size: 11.,
                                color: muted_color(),
                            });
                        }
                    }
                    Key::Zone(_) | Key::Decor(_) => {}
                }
                // Blocks are solid, so fading toward the floor matches
                // drawing them see-through.
                if fade < 1. && matches!(key, Key::Wall(_) | Key::Block(_)) {
                    draw_rectangle(rect.x, rect.y, rect.w, rect.h, alpha(bg_color(), 1. - fade));
                }
                if hidden {
                    draw_rectangle(rect.x, rect.y, rect.w, rect.h, alpha(bg_color(), 0.5));
                    dashed_rect(rect, 1.5, 6., alpha(text_color(), 0.55));
                }
            }
            badges.draw();
        });
        profile::phase("editor decor above", || {
            scenery::decorations(map, config, &shown, &self.view, self.time, true, settings)
        });
        if lighting[3] > 0.004 {
            profile::phase("editor darkness", || {
                let mut lights = Vec::new();
                scenery::lights(
                    map,
                    config,
                    &shown,
                    &self.view,
                    self.time,
                    settings.reduced_motion,
                    &mut lights,
                );
                if let Some(probe) = shown.focus {
                    let at = self.view.screen(probe);
                    lights.push(crate::lighting::Light::Spot {
                        at: vec2(at.x, at.y),
                        radius: 380. * self.view.zoom,
                        color: alpha(WHITE, 0.95),
                    });
                }
                crate::lighting::darken(lighting, &lights, arena.intersect(canvas));
            });
        }
        profile::phase("editor marks", || {
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
            let tags = self.draw_trigger_tags(view, s);
            if self.show_groups {
                self.draw_group_badges(view, s, &tags);
            }
            self.draw_paths(s);
            self.draw_find_hits(view);
            self.draw_flash(view);
            self.draw_selection(s);
            self.draw_preview_marks(s);
            self.draw_gesture(canvas, s);
        });
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
    /// Where trigger `index`'s badge sits: the corner of an area trigger,
    /// or the whole of a point trigger, which shrinks in a tight cluster so
    /// each one stays visible beside its neighbors.
    pub(super) fn badge_rect(&self, index: usize, trigger: &MapTrigger, rect: Rect) -> Rect {
        if brushes::trigger_area(trigger) {
            let size = (rect.w.min(rect.h) * 0.3).clamp(18., 34.);
            Rect::new(rect.x + 3., rect.y + 3., size, size)
        } else {
            let room = self.badge_room.get(index).copied().unwrap_or(f32::MAX);
            let size = rect
                .w
                .min(rect.h)
                .clamp(26., 52.)
                .min(room * self.view.zoom * BADGE_FILL)
                .max(LEAST_BADGE);
            let c = rect.center();
            Rect::new(c.x - size * 0.5, c.y - size * 0.5, size, size)
        }
    }
    fn draw_trigger(
        &self,
        index: usize,
        rect: Rect,
        hidden: bool,
        fade: f32,
        badges: &mut brushes::Badges,
    ) {
        let trigger = &self.doc.map.triggers[index];
        let lit = self.preview.flashes.get(index).map_or(0., |&at| {
            (1. - (self.preview.time - at) / 0.7).clamp(0., 1.)
        }) * fade;
        if brushes::trigger_area(trigger) {
            draw_rectangle(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                alpha(brushes::LOGIC, 0.06 * fade + lit * 0.18),
            );
            dashed_rect(rect, 2., 7., alpha(brushes::LOGIC, 0.85 * fade));
        }
        let badge = self.badge_rect(index, trigger, rect);
        if lit > 0. {
            let c = badge.center();
            draw_circle(
                c.x,
                c.y,
                badge.w * (0.8 + 0.5 * (1. - lit)),
                alpha(brushes::LOGIC, lit * 0.35),
            );
        }
        badges.add(trigger, badge, lit, if hidden { 0.5 } else { 1. } * fade);
    }
    /// The pads and decor to fade while one layer is being edited.
    fn layer_fades(&self) -> scenery::Fades {
        let Some(layer) = self.layer else {
            return scenery::Fades::default();
        };
        let (zones, decor) = self.doc.layers.floor();
        let fades = |layers: &[u8], count: usize| -> Vec<f32> {
            (0..count)
                .map(|i| {
                    if layers.get(i).copied().unwrap_or(0) == layer {
                        1.
                    } else {
                        FADED
                    }
                })
                .collect()
        };
        scenery::Fades {
            zones: fades(zones, self.doc.map.zones.len()),
            decor: fades(decor, self.doc.map.decor.len()),
        }
    }
    /// Where the layer bar sits: the canvas's lower left corner.
    pub(super) fn layer_bar_rect(&self, layout: &Layout) -> Rect {
        let s = layout.s;
        let h = 26. * s;
        Rect::new(
            layout.canvas.x + 10. * s,
            layout.canvas.bottom() - h - 10. * s,
            204. * s,
            h,
        )
    }
    /// The layers worth offering as shortcuts: those holding something and
    /// the one being edited, nearest the current one when there are many.
    fn layer_chips(&self) -> Vec<u8> {
        let mut chips: Vec<u8> = (0..LAYERS)
            .filter(|&layer| self.layer == Some(layer) || self.layer_counts[usize::from(layer)] > 0)
            .collect();
        if chips.len() > MOST_LAYER_CHIPS {
            let middle = i32::from(self.layer.unwrap_or(self.last_layer));
            chips.sort_by_key(|&layer| (i32::from(layer) - middle).abs());
            chips.truncate(MOST_LAYER_CHIPS);
            chips.sort_unstable();
        }
        chips
    }
    /// The strip of layer shortcuts that opens above the layer bar, with
    /// the gap under it, so the pointer can cross from one to the other.
    fn layer_strip_rect(&self, layout: &Layout) -> Rect {
        let bar = self.layer_bar_rect(layout);
        let s = layout.s;
        let h = 24. * s;
        let chips = self.layer_chips().len() + 1;
        let w = (chips as f32 * LAYER_CHIP * s + 6. * s).min(layout.canvas.w - 20. * s);
        Rect::new(bar.x, bar.y - h - 8. * s, w, h + 8. * s)
    }
    /// Layer shortcuts: every layer in use, one click each, with a thin bar
    /// showing how much each holds.
    fn layer_strip(&mut self, layout: &Layout) {
        let pointer = ui_layout::pointer();
        let bar = self.layer_bar_rect(layout);
        let strip = self.layer_strip_rect(layout);
        let chips = self.layer_chips();
        let useful = chips.len() > 1 || self.layer.is_some();
        self.layer_strip = useful
            && interactive()
            && (bar.contains(pointer) || (self.layer_strip && strip.contains(pointer)));
        if !self.layer_strip {
            return;
        }
        let s = layout.s;
        let h = strip.h - 8. * s;
        draw_rectangle(strip.x, strip.y, strip.w, h, alpha(surface_color(), 0.94));
        draw_rectangle_lines(strip.x, strip.y, strip.w, h, 1., line_color());
        let most = chips
            .iter()
            .map(|&layer| self.layer_counts[usize::from(layer)])
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        let width = LAYER_CHIP * s;
        let mut chosen = None;
        for (i, layer) in std::iter::once(None)
            .chain(chips.iter().map(|&layer| Some(layer)))
            .enumerate()
        {
            let x = strip.x + 3. * s + i as f32 * width;
            if x + width > strip.right() {
                break;
            }
            let chip = Rect::new(x + 1. * s, strip.y + 2. * s, width - 2. * s, h - 4. * s);
            let name = layer.map_or_else(|| "All".to_owned(), |layer| layer.to_string());
            if small_button(chip, &name, s * 0.68) {
                chosen = Some(layer);
            }
            if layer == self.layer {
                draw_rectangle_lines(chip.x, chip.y, chip.w, chip.h, 1.5, accent_color());
            }
            if let Some(layer) = layer {
                let share = self.layer_counts[usize::from(layer)] as f32 / most;
                draw_rectangle(
                    chip.x + 2. * s,
                    chip.bottom() - 2.5 * s,
                    (chip.w - 4. * s) * share,
                    1.5 * s,
                    alpha(accent_color(), 0.8),
                );
            }
        }
        if let Some(layer) = chosen {
            self.set_layer(layer);
        }
    }
    /// The layer bar: the layer being edited, or every layer, between
    /// arrows that step through them; Shift skips empty layers. Clicking
    /// the name switches between every layer and the last one edited.
    pub(super) fn layer_bar(&mut self, layout: &Layout) {
        let r = self.layer_bar_rect(layout);
        let s = layout.s;
        let arrow = r.h + 2. * s;
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(surface_color(), 0.88));
        if small_button(Rect::new(r.x, r.y, arrow, r.h), "‹", s * 0.8) {
            self.step_layer(false, shift());
        }
        let name = match self.layer {
            None => "All layers".to_owned(),
            Some(layer) => format!("Layer {layer} · {}", self.layer_counts[usize::from(layer)]),
        };
        let middle = Rect::new(r.x + arrow + 4. * s, r.y, r.w - arrow * 2. - 8. * s, r.h);
        if small_button(middle, &name, s * 0.72) {
            self.set_layer(match self.layer {
                None => Some(self.last_layer),
                Some(_) => None,
            });
        }
        if small_button(Rect::new(r.right() - arrow, r.y, arrow, r.h), "›", s * 0.8) {
            self.step_layer(true, shift());
        }
        self.layer_strip(layout);
    }
    /// Each trigger's tag under its badge, or above it when that spot is
    /// taken. A tag that fits neither is left out; the pointer shows it.
    /// Selected and hovered triggers place theirs first, so they always show.
    /// Returns where the tags went.
    fn draw_trigger_tags(&self, view: Rect, s: f32) -> Vec<Rect> {
        if self.view.zoom <= 0.12 {
            return Vec::new();
        }
        let map = &self.doc.map;
        let ghost = self.ghost_mask();
        let badges: Vec<Option<Rect>> = map
            .triggers
            .iter()
            .enumerate()
            .map(|(index, t)| {
                let rect = self.on_screen(t.rect);
                let shown = (self.show_hidden || MapDef::group_bit(t.member) & ghost == 0)
                    && self.on_layer(Key::Trigger(index));
                (shown && overlaps(view, rect)).then(|| self.badge_rect(index, t, rect))
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
        // Badges and placed tags by cell, so each tag checks only its
        // neighbors: a map of thousands of triggers would otherwise check
        // every pair each frame.
        let mut cells = Cells::default();
        for (index, badge) in badges.iter().enumerate() {
            if let Some(badge) = badge {
                cells.add(*badge, Mark::Badge(index));
            }
        }
        let mut placed: Vec<Rect> = Vec::new();
        let mut tags: Vec<(Rect, String)> = Vec::new();
        for index in order {
            let Some(badge) = badges[index] else {
                continue;
            };
            // A crowded badge has no room for a tag unless it's being looked at.
            if badge.w < 26. && first(&index) {
                continue;
            }
            let tag = trigger_badge_tag(&map.triggers[index]);
            let width = label_width(&tag, size) + 8.;
            let c = badge.center();
            // Kept on the canvas, however near its edge the badge is.
            let left = (c.x - width * 0.5)
                .clamp(view.x + 2., (view.right() - width - 2.).max(view.x + 2.));
            let below = Rect::new(left, badge.bottom() + 3., width, height);
            let above = Rect::new(left, badge.y - 3. - height, width, height);
            let free = |spot: &Rect| {
                !cells.any(*spot, |mark| match mark {
                    Mark::Badge(other) => {
                        other != index && badges[other].is_some_and(|b| overlaps(b, *spot))
                    }
                    Mark::Tag(at) => overlaps(placed[at], *spot),
                })
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
            cells.add(spot, Mark::Tag(placed.len()));
            placed.push(spot);
            tags.push((spot, tag));
        }
        for (spot, ..) in &tags {
            draw_rectangle(
                spot.x,
                spot.y,
                spot.w,
                spot.h,
                Color::new(0.13, 0.12, 0.10, 0.82),
            );
        }
        for (spot, tag) in &tags {
            center_label(tag, spot.center().x, spot.y + size, size, brushes::LOGIC);
        }
        placed
    }
    /// Each grouped object's group number at its corner. Where badges
    /// would pile up, one of the same group stands for the rest and another
    /// group's moves beside it, or is left out when that leaves its object;
    /// the pointer still shows it. Badges keep off trigger `tags` too, but
    /// selected and hovered objects go first and may cover them.
    fn draw_group_badges(&self, view: Rect, s: f32, tags: &[Rect]) {
        // Placed badges are found by the cells of this grid they touch.
        const CELL: f32 = 48.;
        let size = 10. * s.max(0.8);
        let first = |key: Key| self.selected(key) || self.hover == Some(key);
        let mut wanted: Vec<(bool, Rect, u8)> = self
            .doc
            .items()
            .filter(|&(key, _, group)| group != 0 && self.on_layer(key))
            .filter_map(|(key, r, group)| {
                let a = self.hit_area(key, r);
                (overlaps(view, a) && a.w >= 14.).then_some((!first(key), a, group))
            })
            .collect();
        // Stable, so the rest keep their order and the picture stays still.
        wanted.sort_by_key(|&(later, _, _)| later);
        let cells_of = |r: Rect| {
            let (x0, x1) = (
                (r.x / CELL).floor() as i32,
                (r.right() / CELL).floor() as i32,
            );
            let (y0, y1) = (
                (r.y / CELL).floor() as i32,
                (r.bottom() / CELL).floor() as i32,
            );
            (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
        };
        // Tags stand in as group 0, which no badge shares.
        let mut placed: Vec<(Rect, u8)> = tags.iter().map(|&tag| (tag, 0)).collect();
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (index, &(tag, _)) in placed.iter().enumerate() {
            for cell in cells_of(tag) {
                cells.entry(cell).or_default().push(index);
            }
        }
        'badges: for (later, a, group) in wanted {
            let text = group.to_string();
            let width = label_width(&text, size) + 6.;
            let mut badge = Rect::new(a.x - 2., a.y - size - 4., width, size + 4.);
            loop {
                let met = cells_of(badge)
                    .filter_map(|cell| cells.get(&cell))
                    .flatten()
                    .map(|&i| placed[i])
                    .find(|&(other, owner)| other.overlaps(&badge) && (later || owner != 0));
                match met {
                    None => break,
                    Some((_, other)) if other == group => continue 'badges,
                    Some((other, _)) if other.right() + 2. + badge.w <= a.right() => {
                        badge.x = other.right() + 2.;
                    }
                    Some(_) => continue 'badges,
                }
            }
            draw_rectangle(badge.x, badge.y, badge.w, badge.h, alpha(VIOLET, 0.9));
            label(
                &text,
                badge.x + 3.,
                badge.y + size,
                size,
                Color::new(0.1, 0.08, 0.14, 1.),
            );
            for cell in cells_of(badge) {
                cells.entry(cell).or_default().push(placed.len());
            }
            placed.push((badge, group));
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
    /// The path a selected Play path trigger sends its group along, or the
    /// one the Map tab is editing: a line through its points from where the
    /// group rests, a ghost of the group at the chosen point and a dot that
    /// rides the path at its own pace.
    fn draw_paths(&self, s: f32) {
        let map = &self.doc.map;
        let selected = match *self.selection.as_slice() {
            [Key::Trigger(index)] => map
                .triggers
                .get(index)
                .filter(|t| t.action == TriggerAction::Path)
                .map(|t| ((t.amount - 1).max(0) as usize, t.group, None)),
            _ => None,
        };
        let editing = (self.tab == Tab::Map).then(|| {
            let path = self.path_pick.0;
            // The group a trigger sends along this path, if any does.
            let group = map
                .triggers
                .iter()
                .find(|t| t.action == TriggerAction::Path && t.amount == path as i32 + 1)
                .map_or(0, |t| t.group);
            (path, group, Some(self.path_pick.1))
        });
        let Some((index, group, picked)) = selected.or(editing) else {
            return;
        };
        let Some(path) = map.paths.get(index) else {
            return;
        };
        let members: Vec<(Key, [f32; 4])> = self
            .doc
            .items()
            .filter(|&(key, _, g)| group > 0 && g == group && !matches!(key, Key::Trigger(_)))
            .map(|(key, r, _)| (key, r))
            .take(256)
            .collect();
        let keys: Vec<Key> = members.iter().map(|&(key, _)| key).collect();
        let bounds = self.doc.bounds(&keys).unwrap_or([0.5, 0.5, 0., 0.]);
        let home = [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5];
        let at =
            |offset: [f32; 2]| self.point_on_screen([home[0] + offset[0], home[1] + offset[1]]);
        let color = mix(brushes::LOGIC, CYAN, 0.35);
        let mut stops = vec![at([0., 0.])];
        stops.extend(path.points.iter().map(|p| at(p.at)));
        for pair in stops.windows(2) {
            arrow(pair[0], pair[1], 2., alpha(color, 0.8));
        }
        if path.repeat == PathRepeat::Loop && path.points.len() > 1 {
            dashed_line(stops[stops.len() - 1], stops[1], 1.5, 6., alpha(color, 0.6));
        }
        for (n, p) in stops.iter().enumerate().skip(1) {
            let chosen = picked == Some(n - 1);
            let radius = if chosen { 11. } else { 8. };
            draw_circle(
                p.x,
                p.y,
                radius,
                if chosen {
                    color
                } else {
                    alpha(bg_color(), 0.9)
                },
            );
            draw_circle_lines(p.x, p.y, radius, 1.5, color);
            center_label(
                &n.to_string(),
                p.x,
                p.y + 4.5 * s.max(0.8),
                11. * s.max(0.8),
                if chosen { WHITE } else { color },
            );
        }
        // The group as it stands at the chosen point.
        if let Some(point) = picked.and_then(|i| path.points.get(i)) {
            for &(_, r) in &members {
                let ghost = self.on_screen([r[0] + point.at[0], r[1] + point.at[1], r[2], r[3]]);
                draw_rectangle(ghost.x, ghost.y, ghost.w, ghost.h, alpha(color, 0.10));
                dashed_rect(ghost, 1.5, 5., alpha(color, 0.75));
            }
        }
        // A dot riding the path at its own timing, pauses included.
        let legs: Vec<(usize, f32, f32)> = path
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.seconds.max(0.), p.pause.max(0.)))
            .collect();
        let total: f32 = legs.iter().map(|&(_, go, rest)| go + rest).sum();
        if total > 0.05 && !reduced_motion() {
            let mut t = self.time % (total + 0.8);
            for (i, go, rest) in legs {
                let (from, to) = (stops[i], stops[i + 1]);
                if t <= go {
                    let f = if go > 0. {
                        path.points[i].easing.apply(t / go)
                    } else {
                        1.
                    };
                    let p = from.lerp(to, f);
                    draw_circle(p.x, p.y, 5. * s.max(0.8), color);
                    break;
                }
                t -= go;
                if t <= rest {
                    draw_circle(to.x, to.y, 5. * s.max(0.8), color);
                    break;
                }
                t -= rest;
            }
        }
    }
    /// Where a widget pinned to `pin` floats on the canvas: over a spot in
    /// the arena, or over the test tank while there is one.
    fn pin_place(&self, pin: HudPin) -> Option<crate::hud_widgets::Pinned> {
        match pin {
            HudPin::Screen => None,
            HudPin::Group { id, at } => {
                if self.preview.hidden & MapDef::group_bit(id) != 0 {
                    return None;
                }
                let offset = self.preview.poses().offset(id);
                Some(crate::hud_widgets::Pinned {
                    at: self.view.screen(V2::new(
                        at[0] + offset[0] * self.doc.map.width,
                        at[1] + offset[1] * self.doc.map.height,
                    )),
                    above: None,
                })
            }
            HudPin::World([x, y]) => Some(crate::hud_widgets::Pinned {
                at: self.view.screen(V2::new(x, y)),
                above: None,
            }),
            HudPin::Tank(_) => self.preview.probe().map(|at| crate::hud_widgets::Pinned {
                at: self.view.screen(at),
                above: Some((28. * self.view.zoom).max(5.)),
            }),
        }
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
    /// Everything the find bar turned up.
    fn draw_find_hits(&self, view: Rect) {
        for &key in self.find.hits() {
            if let Some(r) = self.doc.rect(key) {
                let a = self.hit_area(key, r);
                if overlaps(view, a) {
                    draw_rectangle(a.x, a.y, a.w, a.h, alpha(AMBER, 0.12));
                    draw_rectangle_lines(a.x - 1., a.y - 1., a.w + 2., a.h + 2., 1.5, AMBER);
                }
            }
        }
    }
    /// What the last undo or redo changed, fading out.
    fn draw_flash(&self, view: Rect) {
        let age = (get_time() - self.flash_at) as f32;
        if self.flash.is_empty() || self.flash_revision != self.revision || age >= FLASH_LIFE {
            return;
        }
        let fade = 1. - age / FLASH_LIFE;
        let color = alpha(accent_color(), fade * fade);
        for &key in &self.flash {
            if let Some(r) = self.doc.rect(key) {
                let a = self.hit_area(key, r);
                if overlaps(view, a) {
                    draw_rectangle(a.x, a.y, a.w, a.h, alpha(color, color.a * 0.3));
                    draw_rectangle_lines(a.x - 3., a.y - 3., a.w + 6., a.h + 6., 2., color);
                }
            }
        }
    }
    /// How big the selection is in world units, unless it's one marker whose
    /// size means nothing.
    fn selection_size(&self) -> Option<[f32; 2]> {
        if let [only] = self.selection[..]
            && matches!(only, Key::Base(_) | Key::Objective(_))
        {
            return None;
        }
        let b = self.doc.bounds(&self.selection)?;
        Some([b[2] * self.doc.map.width, b[3] * self.doc.map.height])
    }
    fn draw_gesture(&self, canvas: Rect, s: f32) {
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
                measurement(
                    &format!("{:.0} × {:.0}", size[0], size[1]),
                    pointer,
                    canvas,
                    s,
                );
            }
            Some(Gesture::Move {
                objects,
                moved: true,
                ..
            }) => {
                if let Some((key, original)) = objects.first()
                    && let Some(now) = self.doc.rect(*key)
                {
                    let from = original.rect();
                    let text = format!(
                        "{}, {}",
                        signed((now[0] - from[0]) * self.doc.map.width),
                        signed((now[1] - from[1]) * self.doc.map.height)
                    );
                    measurement(&text, pointer, canvas, s);
                }
            }
            Some(Gesture::Resize { .. }) if self.handle_press.is_none() => {
                if let Some([w, h]) = self.selection_size() {
                    measurement(&format!("{w:.0} × {h:.0}"), pointer, canvas, s);
                }
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
            Mode::Edit => match (self.selection.len(), self.selection_size()) {
                (0, _) => "Edit".to_owned(),
                (n, Some([w, h])) => format!("Edit · {n} selected · {w:.0} × {h:.0}"),
                (n, None) => format!("Edit · {n} selected"),
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
            let bar = self.layer_bar_rect(layout);
            chip(
                &format!(
                    "{:.0}, {:.0}   {:.0}%",
                    world.x,
                    world.y,
                    self.view.zoom * 100.
                ),
                bar.right() + 6. * s,
                bar.bottom() - size - 10. * s,
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
            for (_, widget, age) in preview.widgets(&self.config()) {
                let pinned = self.pin_place(widget.pin);
                if widget.pinned() && pinned.is_none() {
                    continue;
                }
                crate::hud_widgets::preview(
                    &widget,
                    canvas,
                    s,
                    age,
                    0.,
                    canvas.contains(pointer).then_some(pointer),
                    pinned,
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
        if let Some(net) = self.hub_hover
            && self.gesture.is_none()
            && let Some((title, detail)) = self.hub_words(net as usize)
        {
            Self::tooltip_box(&title, Some(&detail), pointer, layout);
        }
    }
    fn draw_tooltip(&self, key: Key, pointer: Vec2, layout: &Layout) {
        let Some(object) = self.doc.object(key) else {
            return;
        };
        let mut title = object.name().to_owned();
        if let Object::Block(b) = &object {
            title = format!("{} block", b.style.name());
        }
        if object.group() > 0 {
            title += &format!("  ·  group {}", object.group());
        }
        let detail = match &object {
            Object::Trigger(t) => Some(describe_trigger_named(t, &self.doc.map.group_names)),
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
        // Say so when there's more under the pointer than the top object.
        let stack = if self.mode == Mode::Edit {
            self.stack(pointer)
        } else {
            Vec::new()
        };
        let pile = (stack.len() > 1).then(|| {
            let at = stack.iter().position(|&other| other == key).unwrap_or(0) + 1;
            format!("{at} of {} here. Click again for the next.", stack.len())
        });
        let detail = match (detail, pile) {
            (Some(detail), Some(pile)) => Some(format!("{detail} {pile}")),
            (detail, pile) => detail.or(pile),
        };
        Self::tooltip_box(&title, detail.as_deref(), pointer, layout);
    }
    /// A small box by the pointer with a title and, under it, wrapped words.
    fn tooltip_box(title: &str, detail: Option<&str>, pointer: Vec2, layout: &Layout) {
        let s = layout.s;
        let size = 12. * s;
        // Wide enough for its title, and for a description to wrap in.
        let fits = label_width(title, size) + 16. * s;
        let width = if detail.is_some() {
            fits.max(260. * s)
        } else {
            fits
        }
        .min(layout.canvas.w - 8.);
        let lines = detail
            .map(|text| wrapped_lines(text, width - 16. * s, size))
            .unwrap_or_default();
        let height = size + 14. * s + lines.len() as f32 * (size + 4. * s);
        let x = (pointer.x + 18.).min(layout.canvas.right() - width - 4.);
        let y = (pointer.y + 18.).min(layout.canvas.bottom() - height - 4.);
        draw_rectangle(x, y, width, height, alpha(surface_color(), 0.96));
        draw_rectangle_lines(x, y, width, height, 1., line_color());
        label(title, x + 8. * s, y + size + 5. * s, size, text_color());
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
    fn distances_read_with_their_direction_and_no_negative_zero() {
        assert_eq!(signed(12.6), "+13");
        assert_eq!(signed(-40.), "−40");
        assert_eq!(signed(0.2), "0");
        assert_eq!(signed(-0.4), "0", "not a minus zero");
    }

    #[test]
    fn a_much_smaller_object_counts_as_smaller_and_a_like_sized_one_as_equal() {
        let square = |side: f32| Rect::new(0., 0., side, side);
        assert!(pick_size(square(28.)) < pick_size(square(600.)));
        assert!(pick_size(square(10.)) < pick_size(square(28.)));
        assert_eq!(pick_size(square(100.)), pick_size(square(101.)));
        // A sliver or an empty area never breaks the ordering.
        assert_eq!(pick_size(square(0.)), pick_size(square(0.5)));
    }

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
