//! An original arena workshop: grid tools, object palette, properties,
//! groups, trigger logic, particle authoring, portable packs and playtesting.
use crate::{render::View, scenery, settings::Settings, ui::*, ui_layout};
use macroquad::prelude::*;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tankyard_client::editor_document::{Document, Key, Object};
use tankyard_content::*;
use tankyard_core::{BossKind, V2, WorldConfig};

pub enum Action {
    None,
    Exit,
    Play(Box<tankyard_mods::LoadedMods>, WorldConfig),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Build,
    Erase,
}
struct Drag {
    start: Vec2,
    last: Vec2,
    objects: Vec<(Key, Object)>,
    resize: bool,
    marquee: bool,
}

pub struct Editor {
    doc: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    selection: Vec<Key>,
    copied: Vec<Object>,
    tool: Tool,
    palette_tab: usize,
    brush: Object,
    group: u8,
    show_hidden: bool,
    preview: bool,
    time: f32,
    grid: f32,
    snap: bool,
    view: View,
    fitted: bool,
    drag: Option<Drag>,
    panning: Option<Vec2>,
    tab: usize,
    field_focus: [bool; 4],
    scroll: f32,
    content_height: f32,
    property_edit: bool,
    dirty: bool,
    saved_path: Option<PathBuf>,
    notice: String,
    last_autosave: f64,
    library: Vec<PathBuf>,
    library_page: usize,
    import_pending: Option<crate::file_dialog::Pending>,
    templates: Vec<MapDef>,
    close_confirm: bool,
    reset_confirm: bool,
}
impl Editor {
    pub fn new(map: Option<MapDef>) -> Self {
        let draft = directory()
            .ok()
            .map(|p| p.join("draft.json"))
            .and_then(|p| read_document(&p).ok());
        let restored = draft.is_some();
        let doc = draft.unwrap_or_else(|| map.map(Document::from_map).unwrap_or_default());
        let mut editor = Self {
            doc,
            undo: vec![],
            redo: vec![],
            selection: vec![],
            copied: vec![],
            tool: Tool::Select,
            palette_tab: 0,
            brush: block_brush(BlockStyle::Slate),
            group: 0,
            show_hidden: true,
            preview: false,
            time: 0.,
            grid: 40.,
            snap: true,
            view: View::new(),
            fitted: false,
            drag: None,
            panning: None,
            tab: 0,
            field_focus: [false; 4],
            scroll: 0.,
            content_height: 0.,
            property_edit: false,
            dirty: false,
            saved_path: None,
            notice: if restored {
                "Recovered your last editor draft.".into()
            } else {
                "Drag to build. Wheel zooms; middle button or Space + drag pans.".into()
            },
            last_autosave: get_time(),
            library: vec![],
            library_page: 0,
            import_pending: None,
            templates: builtin_maps(),
            close_confirm: false,
            reset_confirm: false,
        };
        editor.refresh_library();
        editor
    }
    fn checkpoint(&mut self, before: Document) {
        if before == self.doc {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > 32 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.close_confirm = false;
        self.reset_confirm = false;
    }
    fn undo(&mut self, redo: bool) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(doc) = from.pop() {
            to.push(std::mem::replace(&mut self.doc, doc));
            self.selection.clear();
            self.dirty = true;
            self.drag = None;
        }
    }
    fn delete(&mut self) {
        let before = self.doc.clone();
        self.doc.delete(&self.selection);
        self.selection.clear();
        self.checkpoint(before);
    }
    fn copy(&mut self) {
        self.copied = self
            .selection
            .iter()
            .filter_map(|&key| self.doc.object(key))
            .collect();
    }
    fn paste(&mut self) {
        let before = self.doc.clone();
        self.selection.clear();
        for mut object in self.copied.clone() {
            let mut r = object.rect();
            r[0] += self.grid / self.doc.map.width;
            r[1] += self.grid / self.doc.map.height;
            object.set_rect(r);
            if let Some(key) = self.doc.insert(object) {
                self.selection.push(key);
            }
        }
        self.checkpoint(before);
    }
    fn transform(&mut self, operation: usize) {
        let before = self.doc.clone();
        let objects: Vec<_> = self
            .selection
            .iter()
            .filter_map(|&key| self.doc.object(key).map(|o| (key, o)))
            .collect();
        let bounds = selection_bounds(&objects.iter().map(|(_, o)| o.rect()).collect::<Vec<_>>());
        for (key, mut object) in objects {
            let mut r = object.rect();
            match operation {
                0 => {
                    if let Object::Decor(item) = &mut object {
                        item.rotation = (item.rotation + 15.).rem_euclid(360.);
                    } else {
                        let w = r[3] * self.doc.map.height / self.doc.map.width;
                        let h = r[2] * self.doc.map.width / self.doc.map.height;
                        r = [r[0] + (r[2] - w) * 0.5, r[1] + (r[3] - h) * 0.5, w, h];
                    }
                }
                1 => r[0] = bounds[0] + bounds[2] - (r[0] - bounds[0]) - r[2],
                2 => r[1] = bounds[1] + bounds[3] - (r[1] - bounds[1]) - r[3],
                _ => {}
            }
            object.set_rect(r);
            self.doc.replace(key, object);
        }
        self.checkpoint(before);
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
        // View uses the window's center; offset it to the tool canvas's center.
        self.view.shake = canvas.center() - vec2(screen_width(), screen_height()) * 0.5;
        self.fitted = true;
    }
    fn normalized(&self, pointer: Vec2) -> Vec2 {
        let p = self.view.world(pointer);
        let x = if self.snap {
            (p.x / self.grid).round() * self.grid
        } else {
            p.x
        };
        let y = if self.snap {
            (p.y / self.grid).round() * self.grid
        } else {
            p.y
        };
        vec2(
            (x / self.doc.map.width).clamp(0., 1.),
            (y / self.doc.map.height).clamp(0., 1.),
        )
    }
    fn pick(&self, pointer: Vec2) -> Option<Key> {
        let cfg = self.config();
        self.doc.keys().into_iter().rev().find(|&key| {
            let object = self.doc.object(key).unwrap();
            (self.show_hidden
                || object.group() == 0
                || self.doc.map.starting_mask() & MapDef::group_bit(object.group()) == 0)
                && scenery::screen_rect(&self.view, &cfg, object.rect()).contains(pointer)
        })
    }
    fn keys(&mut self) {
        if self.field_focus.iter().any(|&focus| focus) {
            return;
        }
        let command = is_key_down(KeyCode::LeftControl)
            || is_key_down(KeyCode::RightControl)
            || is_key_down(KeyCode::LeftSuper)
            || is_key_down(KeyCode::RightSuper);
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if command {
            if is_key_pressed(KeyCode::Z) {
                self.undo(shift);
            }
            if is_key_pressed(KeyCode::Y) {
                self.undo(true);
            }
            if is_key_pressed(KeyCode::C) {
                self.copy();
            }
            if is_key_pressed(KeyCode::X) {
                self.copy();
                self.delete();
            }
            if is_key_pressed(KeyCode::V) {
                self.paste();
            }
            if is_key_pressed(KeyCode::D) {
                self.copy();
                self.paste();
            }
            if is_key_pressed(KeyCode::A) {
                self.selection = self.doc.keys();
            }
            if is_key_pressed(KeyCode::S) {
                self.save();
            }
        } else {
            if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) {
                self.delete();
            }
            if is_key_pressed(KeyCode::Key1) {
                self.tool = Tool::Select;
            }
            if is_key_pressed(KeyCode::Key2) {
                self.tool = Tool::Build;
            }
            if is_key_pressed(KeyCode::Key3) {
                self.tool = Tool::Erase;
            }
            if is_key_pressed(KeyCode::R) {
                self.transform(0);
            }
            if is_key_pressed(KeyCode::F) {
                self.fitted = false;
            }
            let direction = vec2(
                (i32::from(is_key_pressed(KeyCode::Right))
                    - i32::from(is_key_pressed(KeyCode::Left))) as f32,
                (i32::from(is_key_pressed(KeyCode::Down)) - i32::from(is_key_pressed(KeyCode::Up)))
                    as f32,
            );
            if direction != Vec2::ZERO {
                let before = self.doc.clone();
                let step = if shift { 1. } else { self.grid };
                for &key in &self.selection {
                    if let Some(mut o) = self.doc.object(key) {
                        let mut r = o.rect();
                        r[0] += direction.x * step / self.doc.map.width;
                        r[1] += direction.y * step / self.doc.map.height;
                        o.set_rect(r);
                        self.doc.replace(key, o);
                    }
                }
                self.checkpoint(before);
            }
        }
    }
    fn canvas_input(&mut self, canvas: Rect) {
        let pointer = Vec2::from(mouse_position());
        if canvas.contains(pointer) {
            let wheel = mouse_wheel().1;
            if wheel != 0. {
                let before = self.view.world(pointer);
                self.view.zoom = (self.view.zoom * (wheel * 0.12).exp()).clamp(0.015, 4.);
                let after = self.view.world(pointer);
                self.view.center += before - after;
            }
        }
        let pan = is_mouse_button_down(MouseButton::Middle)
            || (is_key_down(KeyCode::Space) && is_mouse_button_down(MouseButton::Left));
        if pan {
            if let Some(last) = self.panning {
                self.view.center -=
                    V2::new(pointer.x - last.x, pointer.y - last.y) / self.view.zoom;
            }
            if canvas.contains(pointer) || self.panning.is_some() {
                self.panning = Some(pointer);
            }
            return;
        }
        self.panning = None;
        if is_mouse_button_pressed(MouseButton::Right) && canvas.contains(pointer) {
            self.tool = Tool::Select;
            self.selection.clear();
        }
        if is_mouse_button_pressed(MouseButton::Left) && canvas.contains(pointer) {
            self.field_focus = [false; 4];
            let start = self.normalized(pointer);
            if self.tool == Tool::Erase {
                if let Some(key) = self.pick(pointer) {
                    let before = self.doc.clone();
                    self.doc.delete(&[key]);
                    self.selection.clear();
                    self.checkpoint(before);
                }
                return;
            }
            let mut resize = false;
            let mut marquee = false;
            if self.tool == Tool::Select {
                if self.selection.len() == 1
                    && let Some(o) = self.doc.object(self.selection[0])
                {
                    let r = scenery::screen_rect(&self.view, &self.config(), o.rect());
                    resize = (pointer - vec2(r.right(), r.bottom())).length() < 12.;
                }
                if !resize {
                    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                    if let Some(key) = self.pick(pointer) {
                        if shift {
                            if self.selection.contains(&key) {
                                self.selection.retain(|&k| k != key);
                            } else {
                                self.selection.push(key);
                            }
                        } else if !self.selection.contains(&key) {
                            self.selection = vec![key];
                        }
                        self.tab = 1;
                    } else {
                        if !shift {
                            self.selection.clear();
                        }
                        marquee = true;
                    }
                }
            }
            let objects = self
                .selection
                .iter()
                .filter_map(|&key| self.doc.object(key).map(|o| (key, o)))
                .collect();
            self.drag = Some(Drag {
                start,
                last: start,
                objects,
                resize,
                marquee,
            });
            // Every drag is one undo entry, including its final clamped shape.
            self.undo.push(self.doc.clone());
            if self.undo.len() > 32 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        drag.last = self.normalized(pointer);
        if self.tool == Tool::Select && !drag.marquee {
            let delta = drag.last - drag.start;
            let bounds = selection_bounds(
                &drag
                    .objects
                    .iter()
                    .map(|(_, o)| o.rect())
                    .collect::<Vec<_>>(),
            );
            let delta = vec2(
                delta.x.clamp(-bounds[0], 1. - bounds[0] - bounds[2]),
                delta.y.clamp(-bounds[1], 1. - bounds[1] - bounds[3]),
            );
            for (key, original) in &drag.objects {
                let mut o = original.clone();
                let mut r = o.rect();
                if drag.resize {
                    r[2] = (drag.last.x - r[0]).max(0.002);
                    r[3] = (drag.last.y - r[1]).max(0.002);
                } else {
                    r[0] += delta.x;
                    r[1] += delta.y;
                }
                o.set_rect(r);
                self.doc.replace(*key, o);
            }
        }
        if is_mouse_button_released(MouseButton::Left) || !is_mouse_button_down(MouseButton::Left) {
            let mut r = drag_rect(drag.start, drag.last);
            if self.tool == Tool::Build {
                if r[2] < 0.001 || r[3] < 0.001 {
                    r = [
                        drag.start.x,
                        drag.start.y,
                        self.grid / self.doc.map.width,
                        self.grid / self.doc.map.height,
                    ];
                }
                let mut o = self.brush.clone();
                o.set_rect(r);
                o.set_group(self.group);
                match self.doc.insert(o) {
                    Some(key) => {
                        self.selection = vec![key];
                        self.tab = 1;
                    }
                    None => {
                        self.notice =
                            "Object limit reached, or that team already has a base.".into()
                    }
                }
            } else if drag.marquee {
                let cfg = self.config();
                let area = scenery::screen_rect(&self.view, &cfg, r);
                for key in self.doc.keys() {
                    let item = scenery::screen_rect(
                        &self.view,
                        &cfg,
                        self.doc.object(key).unwrap().rect(),
                    );
                    if area.overlaps(&item) && !self.selection.contains(&key) {
                        self.selection.push(key);
                    }
                }
            }
            if self.undo.last() == Some(&self.doc) {
                self.undo.pop();
            } else {
                self.dirty = true;
            }
        } else {
            self.drag = Some(drag);
        }
    }
    pub fn draw(&mut self, settings: &Settings, s: f32) -> Action {
        if let Some(result) = self
            .import_pending
            .as_ref()
            .and_then(|pending| pending.poll())
        {
            self.import_pending = None;
            match result.and_then(|path| path.map(|p| self.import(&p)).transpose()) {
                Ok(_) => {}
                Err(error) => self.notice = error,
            }
        }
        set_interactive(true);
        pointer_scope(0x45444954);
        let s = s
            .min((screen_width() / 1100.).min(screen_height() / 700.))
            .max(0.25);
        let top = 92. * s;
        let bottom = screen_height() - 70. * s;
        let left = 186. * s;
        let right = 310. * s;
        let canvas = Rect::new(
            left,
            top,
            (screen_width() - left - right).max(80.),
            (bottom - top).max(80.),
        );
        if !self.fitted {
            self.fit(canvas);
        } else {
            self.view.shake = canvas.center() - vec2(screen_width(), screen_height()) * 0.5;
        }
        if self.preview && !settings.reduced_motion {
            self.time += get_frame_time().min(0.1);
        }
        self.keys();
        self.canvas_input(canvas);
        self.draw_canvas(canvas, settings, s);
        let action = self.toolbar(s);
        self.palette(Rect::new(0., top, left, bottom - top), s);
        self.inspector(
            Rect::new(screen_width() - right, top, right, bottom - top),
            s,
        );
        self.footer(s);
        if self.dirty && get_time() - self.last_autosave > 3. {
            self.last_autosave = get_time();
            if let Err(e) = self.write_draft() {
                self.notice = format!("Draft could not be saved: {e}");
            }
        }
        pointer_scope(0);
        action
    }
    fn draw_canvas(&self, canvas: Rect, settings: &Settings, s: f32) {
        clear_background(mix(bg_color(), BLACK, 0.06));
        let cfg = self.config();
        let bounds = scenery::screen_rect(&self.view, &cfg, [0., 0., 1., 1.]);
        let _clip = ui_layout::region(canvas, screen_height(), 0.);
        draw_rectangle(
            bounds.x,
            bounds.y,
            bounds.w,
            bounds.h,
            mix(bg_color(), scenery::color([140, 165, 174]), 0.1),
        );
        let scale = self.view.zoom;
        let step = if self.grid * scale < 8. {
            (8. / (self.grid * scale)).ceil() * self.grid
        } else {
            self.grid
        };
        let lo = self.view.world(vec2(canvas.x, canvas.y));
        let hi = self.view.world(vec2(canvas.right(), canvas.bottom()));
        for x in (lo.x.max(0.) / step).floor() as i32
            ..=(hi.x.min(self.doc.map.width) / step).ceil() as i32
        {
            let p = self.view.screen(V2::new(x as f32 * step, 0.));
            draw_line(
                p.x,
                bounds.y.max(canvas.y),
                p.x,
                bounds.bottom().min(canvas.bottom()),
                1.,
                alpha(line_color(), 0.25),
            );
        }
        for y in (lo.y.max(0.) / step).floor() as i32
            ..=(hi.y.min(self.doc.map.height) / step).ceil() as i32
        {
            let p = self.view.screen(V2::new(0., y as f32 * step));
            draw_line(
                bounds.x.max(canvas.x),
                p.y,
                bounds.right().min(canvas.right()),
                p.y,
                1.,
                alpha(line_color(), 0.25),
            );
        }
        let hidden = if self.show_hidden {
            0
        } else {
            self.doc.map.starting_mask()
        };
        scenery::floor(&self.doc.map, &cfg, hidden, &self.view);
        scenery::decorations(
            &self.doc.map,
            &cfg,
            hidden,
            &self.view,
            self.time,
            false,
            settings,
        );
        for key in self.doc.keys() {
            let o = self.doc.object(key).unwrap();
            let r = scenery::screen_rect(&self.view, &cfg, o.rect());
            if !canvas.overlaps(&r) {
                continue;
            }
            if !self.show_hidden && MapDef::group_bit(o.group()) & hidden != 0 {
                continue;
            }
            match &o {
                Object::Wall(_) => scenery::block(
                    r,
                    scenery::wall_style(self.doc.map.style, &self.doc.map.id),
                    None,
                    settings.dark_arena,
                    settings.shape_quality,
                ),
                Object::Block(b) => scenery::block(
                    r,
                    scenery::wall_style(
                        if b.style == BlockStyle::Map {
                            self.doc.map.style
                        } else {
                            b.style
                        },
                        &self.doc.map.id,
                    ),
                    b.color.map(scenery::color),
                    settings.dark_arena,
                    settings.shape_quality,
                ),
                Object::Trigger(t) => {
                    draw_rectangle(r.x, r.y, r.w, r.h, alpha(crate::ui::AMBER, 0.08));
                    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1., alpha(crate::ui::AMBER, 0.8));
                    center_label(
                        &format!("{:?} → {:?} {}", t.when, t.action, t.group),
                        r.center().x,
                        r.center().y,
                        10. * s,
                        crate::ui::AMBER,
                    );
                }
                Object::Spawn(region) => {
                    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2., alpha(CYAN, 0.55));
                    center_label(
                        &format!("Spawn {}", region.team),
                        r.center().x,
                        r.center().y,
                        11. * s,
                        CYAN,
                    );
                }
                Object::Base(base) => {
                    draw_circle_lines(r.center().x, r.center().y, 10. * s, 2., CYAN);
                    center_label(
                        &format!("B{}", base.team),
                        r.center().x,
                        r.y - 5. * s,
                        11. * s,
                        CYAN,
                    );
                }
                Object::Objective(_) => {
                    draw_poly_lines(
                        r.center().x,
                        r.center().y,
                        4,
                        10. * s,
                        0.,
                        2.,
                        crate::ui::AMBER,
                    );
                }
                Object::Decor(item) if item.kind == DecorKind::Emitter => {
                    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1., alpha(line_color(), 0.6));
                    if !self.preview {
                        center_label("FX", r.center().x, r.center().y, 11. * s, muted_color());
                    }
                }
                _ => {}
            }
            if self.selection.contains(&key) {
                draw_rectangle_lines(r.x - 1., r.y - 1., r.w + 2., r.h + 2., 2., accent_color());
                if self.selection.len() == 1 {
                    draw_rectangle(
                        r.right() - 4. * s,
                        r.bottom() - 4. * s,
                        8. * s,
                        8. * s,
                        accent_color(),
                    );
                }
            }
        }
        scenery::decorations(
            &self.doc.map,
            &cfg,
            hidden,
            &self.view,
            self.time,
            true,
            settings,
        );
        draw_rectangle_lines(bounds.x, bounds.y, bounds.w, bounds.h, 2., muted_color());
        if let Some(drag) = &self.drag
            && (self.tool == Tool::Build || drag.marquee)
        {
            let r = scenery::screen_rect(&self.view, &cfg, drag_rect(drag.start, drag.last));
            draw_rectangle(r.x, r.y, r.w, r.h, alpha(accent_color(), 0.1));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2., accent_color());
        }
    }
    fn toolbar(&mut self, s: f32) -> Action {
        draw_rectangle(0., 0., screen_width(), 92. * s, surface_color());
        label("ARENA WORKSHOP", 16. * s, 28. * s, 19. * s, text_color());
        label(
            &fitted_label(&self.doc.map.name, 14. * s, 230. * s),
            16. * s,
            50. * s,
            14. * s,
            muted_color(),
        );
        let mut x = 270. * s;
        for (title, op) in [
            ("New", 0),
            ("Save", 1),
            ("Export pack", 2),
            ("Playtest [F5]", 3),
            ("Back", 4),
        ] {
            let w = match op {
                2 | 3 => 135.,
                _ => 76.,
            } * s;
            if button(
                Rect::new(x, 12. * s, w, 32. * s),
                title,
                op == 3,
                true,
                s * 0.8,
            ) {
                match op {
                    0 => {
                        if self.reset_confirm || !self.dirty {
                            let before = self.doc.clone();
                            self.doc = Document::default();
                            self.checkpoint(before);
                            self.selection.clear();
                            self.saved_path = None;
                            self.fitted = false;
                            self.reset_confirm = false;
                        } else {
                            self.reset_confirm = true;
                            self.notice="Click New again to start over. Your recovery draft will be replaced.".into();
                        }
                    }
                    1 => self.save(),
                    2 => self.export(),
                    3 => return self.play(),
                    _ => {
                        if self.dirty && !self.close_confirm {
                            self.close_confirm = true;
                            self.notice="Click Back again to leave. Your recovery draft is kept; Save creates a named project.".into();
                        } else {
                            let _ = self.write_draft();
                            return Action::Exit;
                        }
                    }
                }
            }
            x += w + 7. * s;
        }
        if is_key_pressed(KeyCode::F5) && !self.field_focus.iter().any(|&v| v) {
            return self.play();
        }
        let mut x = 16. * s;
        for (title, tool) in [
            ("Select [1]", Tool::Select),
            ("Build [2]", Tool::Build),
            ("Erase [3]", Tool::Erase),
        ] {
            if button(
                Rect::new(x, 57. * s, 102. * s, 27. * s),
                title,
                self.tool == tool,
                true,
                s * 0.75,
            ) {
                self.tool = tool;
            }
            x += 109. * s;
        }
        if small_button(Rect::new(x, 57. * s, 68. * s, 27. * s), "Undo", s * 0.75) {
            self.undo(false);
        }
        x += 74. * s;
        if small_button(Rect::new(x, 57. * s, 68. * s, 27. * s), "Redo", s * 0.75) {
            self.undo(true);
        }
        x += 82. * s;
        toggle(
            Rect::new(x, 57. * s, 115. * s, 27. * s),
            "Snap",
            &mut self.snap,
            s * 0.75,
        );
        x += 127. * s;
        if small_button(
            Rect::new(x, 57. * s, 100. * s, 27. * s),
            &format!("Grid {}", self.grid as u32),
            s * 0.75,
        ) {
            self.grid = match self.grid as u32 {
                10 => 20.,
                20 => 40.,
                40 => 80.,
                80 => 160.,
                _ => 10.,
            };
        }
        x += 110. * s;
        if small_button(Rect::new(x, 57. * s, 72. * s, 27. * s), "Fit [F]", s * 0.75) {
            self.fitted = false;
        }
        x += 82. * s;
        toggle(
            Rect::new(x, 57. * s, 170. * s, 27. * s),
            "Animate FX",
            &mut self.preview,
            s * 0.75,
        );
        Action::None
    }
    fn palette(&mut self, r: Rect, s: f32) {
        draw_rectangle(r.x, r.y, r.w, r.h, surface_color());
        let x = r.x + 12. * s;
        let w = r.w - 24. * s;
        let mut y = r.y + 12. * s;
        for (i, name) in ["Blocks", "Pads", "Decoration", "Logic", "Spawns"]
            .into_iter()
            .enumerate()
        {
            if button(
                Rect::new(x, y, w, 27. * s),
                name,
                self.palette_tab == i,
                true,
                s * 0.78,
            ) {
                self.palette_tab = i;
            }
            y += 32. * s;
        }
        y += 12. * s;
        let brushes: Vec<Object> = match self.palette_tab {
            0 => BlockStyle::ALL.into_iter().map(block_brush).collect(),
            1 => ZoneKind::ALL
                .into_iter()
                .map(|kind| {
                    Object::Zone(MapZone {
                        kind,
                        rect: [0., 0., 0.05, 0.05],
                        strength: 1.,
                        group: 0,
                    })
                })
                .collect(),
            2 => DecorKind::ALL
                .into_iter()
                .map(|kind| {
                    Object::Decor(MapDecor {
                        kind,
                        rect: [0., 0., 0.05, 0.05],
                        color: [113, 158, 181],
                        alpha: if kind == DecorKind::Tile { 90 } else { 190 },
                        rotation: 0.,
                        pulse: 0.,
                        spin: 0.,
                        above: false,
                        text: if kind == DecorKind::Label {
                            "Arena".into()
                        } else {
                            String::new()
                        },
                        effect: (kind == DecorKind::Emitter).then(ParticleEffect::default),
                        group: 0,
                    })
                })
                .collect(),
            3 => [
                TriggerAction::Toggle,
                TriggerAction::Show,
                TriggerAction::Hide,
                TriggerAction::Announce,
                TriggerAction::Boss,
            ]
            .into_iter()
            .map(|action| {
                Object::Trigger(MapTrigger {
                    when: TriggerWhen::Every,
                    action,
                    rect: [0., 0., 0.05, 0.05],
                    group: 1,
                    seconds: 10.,
                    offset: 0.,
                    duration: 0.,
                    text: if action == TriggerAction::Announce {
                        "A new phase begins.".into()
                    } else {
                        String::new()
                    },
                    boss: if action == TriggerAction::Boss {
                        "sentinel".into()
                    } else {
                        String::new()
                    },
                    once: false,
                })
            })
            .collect(),
            _ => vec![
                Object::Spawn(SpawnRegion {
                    team: 0,
                    rect: [0., 0., 0.1, 0.1],
                }),
                Object::Base(TeamBase {
                    team: 1,
                    position: [0.5, 0.5],
                }),
                Object::Objective([0.5, 0.5]),
            ],
        };
        for brush in brushes {
            let title = match &brush {
                Object::Block(b) => b.style.name().to_owned(),
                Object::Trigger(t) => format!("{:?}", t.action),
                _ => brush.name().into(),
            };
            if button(Rect::new(x, y, w, 29. * s), &title, false, true, s * 0.75) {
                self.brush = brush;
                self.tool = Tool::Build;
            }
            y += 34. * s;
        }
        let y = (r.bottom() - 137. * s).max(y + 5. * s);
        label("BUILD GROUP", x, y, 11. * s, muted_color());
        self.group = choice_u8(Rect::new(x, y + 10. * s, w, 27. * s), self.group, 0, 32, s);
        toggle(
            Rect::new(x, y + 46. * s, w, 28. * s),
            "Show hidden",
            &mut self.show_hidden,
            s * 0.75,
        );
        label(
            &format!("{} selected", self.selection.len()),
            x,
            y + 101. * s,
            12. * s,
            muted_color(),
        );
    }
    fn inspector(&mut self, r: Rect, s: f32) {
        draw_rectangle(r.x, r.y, r.w, r.h, surface_color());
        let tabs = ["Map", "Object", "FX", "Rules", "Files"];
        let w = (r.w - 16. * s) / 5.;
        for (i, title) in tabs.into_iter().enumerate() {
            if button(
                Rect::new(
                    r.x + 8. * s + i as f32 * w,
                    r.y + 9. * s,
                    w - 3. * s,
                    27. * s,
                ),
                title,
                self.tab == i,
                true,
                s * 0.68,
            ) {
                self.tab = i;
                self.scroll = 0.;
                self.field_focus = [false; 4];
            }
        }
        let clip = Rect::new(r.x + 6. * s, r.y + 48. * s, r.w - 12. * s, r.h - 54. * s);
        if clip.contains(Vec2::from(mouse_position())) && !pointer_captured() {
            self.scroll = (self.scroll - mouse_wheel().1 * 42. * s)
                .clamp(0., (self.content_height - clip.h).max(0.));
        }
        // File operations already record their own undo step.
        let before = (self.tab != 4).then(|| self.doc.clone());
        let end = {
            let _guard = ui_layout::region(clip, clip.h, self.scroll);
            let x = clip.x + 9. * s;
            let y = clip.y + 16. * s;
            let width = clip.w - 23. * s;
            match self.tab {
                0 => self.map_properties(x, y, width, s),
                1 => self.object_properties(x, y, width, s, false),
                2 => self.object_properties(x, y, width, s, true),
                3 => self.rule_properties(x, y, width, s),
                _ => self.files(x, y, width, s),
            }
        };
        self.content_height = (end - clip.y + 22. * s).max(0.);
        let editing =
            is_mouse_button_down(MouseButton::Left) || self.field_focus.iter().any(|&v| v);
        if let Some(before) = before.filter(|before| *before != self.doc) {
            if !self.property_edit {
                self.checkpoint(before);
            }
            self.dirty = true;
            self.property_edit = editing;
        }
        if !editing {
            self.property_edit = false;
        }
        if self.content_height > clip.h {
            let h = (clip.h * clip.h / self.content_height).max(20. * s);
            let at = self.scroll / (self.content_height - clip.h).max(1.) * (clip.h - h);
            draw_rectangle(r.right() - 5. * s, clip.y + at, 3. * s, h, accent_color());
        }
    }
    fn map_properties(&mut self, x: f32, mut y: f32, w: f32, s: f32) -> f32 {
        field(
            "Name",
            &mut self.doc.map.name,
            &mut self.field_focus[0],
            x,
            &mut y,
            w,
            s,
            64,
        );
        field(
            "Map id",
            &mut self.doc.map.id,
            &mut self.field_focus[1],
            x,
            &mut y,
            w,
            s,
            48,
        );
        field(
            "Description",
            &mut self.doc.map.description,
            &mut self.field_focus[2],
            x,
            &mut y,
            w,
            s,
            240,
        );
        number(
            "Width",
            &mut self.doc.map.width,
            [900., 16000.],
            100.,
            x,
            &mut y,
            w,
            s,
        );
        number(
            "Height",
            &mut self.doc.map.height,
            [700., 16000.],
            100.,
            x,
            &mut y,
            w,
            s,
        );
        let mut capacity = f32::from(self.doc.map.capacity);
        number(
            "Recommended players",
            &mut capacity,
            [2., 128.],
            1.,
            x,
            &mut y,
            w,
            s,
        );
        self.doc.map.capacity = capacity as u16;
        label("Wall style", x, y + 12. * s, 13. * s, text_color());
        y += 23. * s;
        self.doc.map.style = cycle(
            Rect::new(x, y, w, 29. * s),
            self.doc.map.style,
            &BlockStyle::ALL,
            |v| v.name().into(),
            s,
        );
        y += 45. * s;
        let palettes = ["slate", "teal", "sand", "violet"];
        let current = palettes
            .iter()
            .position(|p| *p == self.doc.map.background)
            .unwrap_or(0);
        if small_button(
            Rect::new(x, y, w, 30. * s),
            &format!("Floor: {}", palettes[current]),
            s * 0.8,
        ) {
            self.doc.map.background = palettes[(current + 1) % palettes.len()].into();
        }
        y += 49. * s;
        label("Initially hidden groups", x, y, 13. * s, text_color());
        y += 14. * s;
        for row in 0..4 {
            for col in 0..8 {
                let group = (row * 8 + col + 1) as u8;
                let hidden = self.doc.map.hidden_groups.contains(&group);
                if button(
                    Rect::new(
                        x + col as f32 * w / 8.,
                        y + row as f32 * 29. * s,
                        w / 8. - 3. * s,
                        25. * s,
                    ),
                    &group.to_string(),
                    hidden,
                    true,
                    s * 0.65,
                ) {
                    if hidden {
                        self.doc.map.hidden_groups.retain(|&g| g != group);
                    } else {
                        self.doc.map.hidden_groups.push(group);
                    }
                }
            }
        }
        y += 136. * s;
        wrap(
            "Groups connect blocks, pads and decorations to triggers. Group 0 is always visible. Collision blocks rotate in 90° steps.",
            x,
            y,
            w,
            12. * s,
            muted_color(),
        ) + 20. * s
    }
    fn object_properties(&mut self, x: f32, mut y: f32, w: f32, s: f32, fx: bool) -> f32 {
        let Some(key) = self.selection.first().copied() else {
            return wrap(
                if fx {
                    "Select a particle emitter, or build one from Decoration → Particles."
                } else {
                    "Select an object on the canvas. Shift-click adds objects; drag empty space to select a region."
                },
                x,
                y + 6. * s,
                w,
                14. * s,
                muted_color(),
            );
        };
        if self.selection.len() > 1 && !fx {
            label(
                &format!("{} objects selected", self.selection.len()),
                x,
                y,
                16. * s,
                text_color(),
            );
            y += 25. * s;
            let mut group = self.group;
            group = choice_u8(Rect::new(x, y, w, 30. * s), group, 0, 32, s);
            self.group = group;
            y += 43. * s;
            if small_button(
                Rect::new(x, y, w, 30. * s),
                "Assign group to selection",
                s * 0.8,
            ) {
                for &key in &self.selection {
                    if let Some(mut o) = self.doc.object(key) {
                        o.set_group(group);
                        self.doc.replace(key, o);
                    }
                }
            }
            y += 45. * s;
            return wrap(
                "Copy, duplicate, rotate or flip the selection using the bottom toolbar. Arrow keys nudge by one grid cell; Shift nudges one world unit.",
                x,
                y,
                w,
                13. * s,
                muted_color(),
            );
        }
        let Some(mut object) = self.doc.object(key) else {
            return y;
        };
        label(object.name(), x, y, 18. * s, text_color());
        y += 27. * s;
        if fx {
            if let Object::Decor(item) = &mut object {
                if item.kind == DecorKind::Emitter {
                    let e = item.effect.get_or_insert_default();
                    y = effect_properties(e, x, y, w, s);
                } else {
                    y = wrap(
                        "The FX tab edits particle emitters. For this object's colour and rotation use Object.",
                        x,
                        y,
                        w,
                        13. * s,
                        muted_color(),
                    );
                }
            } else {
                y = wrap(
                    "Select a particle emitter to edit its particles.",
                    x,
                    y,
                    w,
                    13. * s,
                    muted_color(),
                );
            }
        } else {
            let mut group = object.group();
            label("Group", x, y, 12. * s, muted_color());
            y += 10. * s;
            group = choice_u8(Rect::new(x, y, w, 28. * s), group, 0, 32, s);
            object.set_group(group);
            y += 47. * s;
            let mut rect = object.rect();
            let dimensions = [
                self.doc.map.width,
                self.doc.map.height,
                self.doc.map.width,
                self.doc.map.height,
            ];
            for (i, title) in ["X", "Y", "Width", "Height"].into_iter().enumerate() {
                let mut value = rect[i] * dimensions[i];
                number(
                    title,
                    &mut value,
                    [if i < 2 { 0. } else { 4. }, dimensions[i]],
                    1.,
                    x,
                    &mut y,
                    w,
                    s,
                );
                rect[i] = value / dimensions[i];
            }
            object.set_rect(rect);
            match &mut object {
                Object::Block(b) => {
                    b.style = cycle(
                        Rect::new(x, y, w, 29. * s),
                        b.style,
                        &BlockStyle::ALL,
                        |v| v.name().into(),
                        s,
                    );
                    y += 45. * s;
                    let mut tint = b.color.is_some();
                    toggle(
                        Rect::new(x, y, w, 30. * s),
                        "Custom tint",
                        &mut tint,
                        s * 0.8,
                    );
                    y += 40. * s;
                    if tint {
                        let c = b.color.get_or_insert([100, 170, 190]);
                        y = color_properties(c, x, y, w, s);
                    } else {
                        b.color = None;
                    }
                }
                Object::Zone(z) => {
                    z.kind = cycle(
                        Rect::new(x, y, w, 29. * s),
                        z.kind,
                        &ZoneKind::ALL,
                        |v| v.name().into(),
                        s,
                    );
                    y += 45. * s;
                    number(
                        "Strength",
                        &mut z.strength,
                        [0.25, 2.],
                        0.05,
                        x,
                        &mut y,
                        w,
                        s,
                    );
                }
                Object::Decor(d) => {
                    number("Rotation", &mut d.rotation, [0., 360.], 5., x, &mut y, w, s);
                    number(
                        "Spin / second",
                        &mut d.spin,
                        [-720., 720.],
                        15.,
                        x,
                        &mut y,
                        w,
                        s,
                    );
                    number(
                        "Pulse / second",
                        &mut d.pulse,
                        [0., 4.],
                        0.1,
                        x,
                        &mut y,
                        w,
                        s,
                    );
                    let mut opacity = f32::from(d.alpha);
                    number("Opacity", &mut opacity, [0., 255.], 1., x, &mut y, w, s);
                    d.alpha = opacity as u8;
                    toggle(
                        Rect::new(x, y, w, 30. * s),
                        "Above players",
                        &mut d.above,
                        s * 0.8,
                    );
                    y += 43. * s;
                    if d.kind == DecorKind::Label {
                        field(
                            "Text",
                            &mut d.text,
                            &mut self.field_focus[3],
                            x,
                            &mut y,
                            w,
                            s,
                            64,
                        );
                    }
                    y = color_properties(&mut d.color, x, y, w, s);
                }
                Object::Trigger(t) => {
                    t.when = cycle(
                        Rect::new(x, y, w, 30. * s),
                        t.when,
                        &[TriggerWhen::Every, TriggerWhen::Enter, TriggerWhen::Start],
                        |v| format!("When: {v:?}"),
                        s,
                    );
                    y += 44. * s;
                    t.action = cycle(
                        Rect::new(x, y, w, 30. * s),
                        t.action,
                        &[
                            TriggerAction::Toggle,
                            TriggerAction::Show,
                            TriggerAction::Hide,
                            TriggerAction::Announce,
                            TriggerAction::Boss,
                        ],
                        |v| format!("Action: {v:?}"),
                        s,
                    );
                    y += 44. * s;
                    if matches!(
                        t.action,
                        TriggerAction::Toggle | TriggerAction::Show | TriggerAction::Hide
                    ) && t.group == 0
                    {
                        t.group = 1;
                    }
                    number(
                        if t.when == TriggerWhen::Enter {
                            "Cooldown (s)"
                        } else {
                            "Seconds"
                        },
                        &mut t.seconds,
                        [
                            if t.when == TriggerWhen::Every {
                                0.5
                            } else {
                                0.
                            },
                            120.,
                        ],
                        0.5,
                        x,
                        &mut y,
                        w,
                        s,
                    );
                    number(
                        "Start offset",
                        &mut t.offset,
                        [0., 120.],
                        0.5,
                        x,
                        &mut y,
                        w,
                        s,
                    );
                    number(
                        "Restore after (0 = keep)",
                        &mut t.duration,
                        [0., 120.],
                        0.5,
                        x,
                        &mut y,
                        w,
                        s,
                    );
                    toggle(
                        Rect::new(x, y, w, 30. * s),
                        "Once per round",
                        &mut t.once,
                        s * 0.8,
                    );
                    y += 43. * s;
                    if t.action == TriggerAction::Announce {
                        field(
                            "Announcement",
                            &mut t.text,
                            &mut self.field_focus[3],
                            x,
                            &mut y,
                            w,
                            s,
                            96,
                        );
                    }
                    if t.action == TriggerAction::Boss {
                        let current = BossKind::from_id(&t.boss).unwrap_or(BossKind::Sentinel);
                        let selected = cycle(
                            Rect::new(x, y, w, 30. * s),
                            current,
                            &BossKind::ALL,
                            |v| v.name().into(),
                            s,
                        );
                        t.boss = selected.id().into();
                        y += 43. * s;
                    }
                }
                Object::Spawn(region) => {
                    label("Team (0 = unteamed)", x, y, 12. * s, muted_color());
                    y += 12. * s;
                    region.team = choice_u8(Rect::new(x, y, w, 30. * s), region.team, 0, 4, s);
                    y += 45. * s;
                }
                Object::Base(base) => {
                    base.team = choice_u8(Rect::new(x, y, w, 30. * s), base.team, 1, 4, s);
                    y += 45. * s;
                }
                _ => {}
            }
        }
        self.doc.replace(key, object);
        y
    }
    fn rule_properties(&mut self, x: f32, mut y: f32, w: f32, s: f32) -> f32 {
        label("Playtest / pack rules", x, y, 16. * s, text_color());
        y += 30. * s;
        // The shared form switches to one column in the narrow inspector.
        let rs = s;
        crate::rules_ui::controls(&mut self.doc.rules, Rect::new(x, y, w, 280. * rs), rs);
        y += 9. * 54. * rs;
        crate::rules_ui::presets(&mut self.doc.rules, Rect::new(x, y, w, 30. * rs), rs);
        y += 90. * rs;
        wrap(
            "Rules travel with the exported pack. Hosts can also adjust them live from Server → Rules. Customized matches do not earn standard records.",
            x,
            y,
            w,
            13. * s,
            muted_color(),
        ) + 25. * s
    }
    fn files(&mut self, x: f32, mut y: f32, w: f32, s: f32) -> f32 {
        if button(
            Rect::new(x, y, w, 30. * s),
            "Import a map pack…",
            false,
            self.import_pending.is_none(),
            s * 0.8,
        ) {
            self.import_pending = Some(crate::file_dialog::choose_mod_pack());
        }
        y += 52. * s;
        label("Saved projects", x, y, 16. * s, text_color());
        y += 19. * s;
        if small_button(Rect::new(x, y, w, 29. * s), "Refresh list", s * 0.8) {
            self.refresh_library();
        }
        y += 47. * s;
        let paths = self.library.clone();
        for path in paths.iter().skip(self.library_page * 8).take(8) {
            let title = path.file_stem().unwrap_or_default().to_string_lossy();
            if small_button(
                Rect::new(x, y, w, 30. * s),
                &fitted_label(&title, 14. * s, w - 12. * s),
                s * 0.8,
            ) {
                match read_document(path) {
                    Ok(doc) => {
                        let before = self.doc.clone();
                        self.doc = doc;
                        self.checkpoint(before);
                        self.saved_path = Some(path.clone());
                        self.selection.clear();
                        self.fitted = false;
                        self.notice =
                            "Project opened. Undo returns to your previous project.".into();
                    }
                    Err(e) => self.notice = e,
                }
            }
            y += 38. * s;
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
            y += 45. * s;
        }
        if small_button(Rect::new(x, y, w, 30. * s), "Open workshop folder", s * 0.8)
            && let Ok(path) = directory()
            && let Err(e) = crate::file_dialog::reveal(&path)
        {
            self.notice = e;
        }
        y += 51. * s;
        y = wrap(
            "Save keeps an editable project. Export creates a ZIP pack in exports/. Import that ZIP on Mods, or add its path to a dedicated server's mods list. editor.json is included so another author can keep editing.",
            x,
            y,
            w,
            13. * s,
            muted_color(),
        ) + 35. * s;
        label("Copy a built-in map", x, y, 15. * s, text_color());
        y += 16. * s;
        for template in self.templates.clone() {
            if small_button(Rect::new(x, y, w, 29. * s), &template.name, s * 0.8) {
                let mut document = Document::from_map(template);
                match document.bake_procedure() {
                    Ok(()) => {
                        document.map.id = format!("my_{}", document.map.id);
                        document.map.name = format!("My {}", document.map.name);
                        let before = std::mem::replace(&mut self.doc, document);
                        self.checkpoint(before);
                        self.selection.clear();
                        self.fitted = false;
                        self.notice = "Template copied. Undo returns to your previous map.".into();
                    }
                    Err(error) => self.notice = error,
                }
            }
            y += 36. * s;
        }
        y
    }
    fn footer(&mut self, s: f32) {
        let y = screen_height() - 70. * s;
        draw_rectangle(0., y, screen_width(), 70. * s, surface_color());
        let mut x = 14. * s;
        for (title, op) in [
            ("Copy", 0),
            ("Paste", 1),
            ("Duplicate", 2),
            ("Rotate [R]", 3),
            ("Flip X", 4),
            ("Flip Y", 5),
            ("Delete", 6),
        ] {
            if small_button(Rect::new(x, y + 6. * s, 92. * s, 26. * s), title, s * 0.7) {
                match op {
                    0 => self.copy(),
                    1 => self.paste(),
                    2 => {
                        self.copy();
                        self.paste();
                    }
                    3 => self.transform(0),
                    4 => self.transform(1),
                    5 => self.transform(2),
                    _ => self.delete(),
                }
            }
            x += 98. * s;
        }
        let status = format!(
            "{}  ·  zoom {:.0}%  ·  {} objects  ·  {}",
            if self.dirty {
                "Unsaved changes"
            } else if self.saved_path.is_some() {
                "Project saved"
            } else {
                "Draft"
            },
            self.view.zoom * 100.,
            self.doc.keys().len(),
            self.notice
        );
        label(
            &fitted_label(&status, 12. * s, screen_width() - 30. * s),
            14. * s,
            y + 56. * s,
            12. * s,
            muted_color(),
        );
    }
    fn play(&mut self) -> Action {
        match self.doc.loaded() {
            Ok(loaded) => {
                let _ = self.write_draft();
                Action::Play(Box::new(loaded), self.config())
            }
            Err(e) => {
                self.notice = format!("Cannot playtest: {e}");
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
                return Err("Choose a map id using lowercase letters, digits and underscores (48 characters maximum).".into());
            }
            let path = root.join(format!("{}.json", self.doc.map.id));
            write_document(&path, &self.doc)?;
            self.write_draft()?;
            Ok(path)
        })();
        match result {
            Ok(path) => {
                self.notice = format!("Saved {}", path.display());
                self.saved_path = Some(path);
                self.dirty = false;
                self.refresh_library();
            }
            Err(e) => self.notice = e,
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
            Ok(path) => self.notice = format!("Exported {}", path.display()),
            Err(e) => self.notice = format!("Cannot export: {e}"),
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
        let before = std::mem::replace(&mut self.doc, doc);
        self.checkpoint(before);
        self.saved_path = None;
        self.selection.clear();
        self.fitted = false;
        self.notice = "Map imported. Save keeps your own editable copy.".into();
        Ok(())
    }
}

fn block_brush(style: BlockStyle) -> Object {
    Object::Block(MapBlock {
        rect: [0., 0., 0.05, 0.05],
        style,
        color: None,
        group: 0,
    })
}
fn drag_rect(a: Vec2, b: Vec2) -> [f32; 4] {
    [
        a.x.min(b.x),
        a.y.min(b.y),
        (b.x - a.x).abs(),
        (b.y - a.y).abs(),
    ]
}
fn selection_bounds(rects: &[[f32; 4]]) -> [f32; 4] {
    if rects.is_empty() {
        return [0., 0., 0., 0.];
    }
    let lo = rects
        .iter()
        .fold(vec2(1., 1.), |v, r| v.min(vec2(r[0], r[1])));
    let hi = rects
        .iter()
        .fold(Vec2::ZERO, |v, r| v.max(vec2(r[0] + r[2], r[1] + r[3])));
    [lo.x, lo.y, hi.x - lo.x, hi.y - lo.y]
}
// Form geometry stays explicit beside each property, as in the shared UI helpers.
#[allow(clippy::too_many_arguments)]
fn number(
    title: &str,
    value: &mut f32,
    limits: [f32; 2],
    step: f32,
    x: f32,
    y: &mut f32,
    w: f32,
    s: f32,
) {
    slider_value(
        Rect::new(x, *y, w, 31. * s),
        title,
        value,
        limits,
        step,
        s * 0.74,
        |v| {
            if step >= 1. {
                format!("{v:.0}")
            } else {
                format!("{v:.2}")
            }
        },
    );
    *y += 48. * s;
}
#[allow(clippy::too_many_arguments)]
fn field(
    title: &str,
    value: &mut String,
    focus: &mut bool,
    x: f32,
    y: &mut f32,
    w: f32,
    s: f32,
    max: usize,
) {
    label(title, x, *y, 12. * s, muted_color());
    text_field(
        Rect::new(x, *y + 10. * s, w, 31. * s),
        value,
        focus,
        max,
        s * 0.75,
        false,
    );
    *y += 65. * s;
}
fn cycle<T: Copy + PartialEq>(
    r: Rect,
    current: T,
    options: &[T],
    name: impl Fn(T) -> String,
    s: f32,
) -> T {
    let index = options.iter().position(|v| *v == current).unwrap_or(0);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    if small_button(r, &format!("{}  ›", name(current)), s * 0.78) {
        options[if shift {
            (index + options.len() - 1) % options.len()
        } else {
            (index + 1) % options.len()
        }]
    } else {
        current
    }
}
fn choice_u8(r: Rect, value: u8, lo: u8, hi: u8, s: f32) -> u8 {
    let down = button(
        Rect::new(r.x, r.y, 28. * s, r.h),
        "−",
        false,
        value > lo,
        s * 0.8,
    );
    let up = button(
        Rect::new(r.right() - 28. * s, r.y, 28. * s, r.h),
        "+",
        false,
        value < hi,
        s * 0.8,
    );
    center_label(
        &value.to_string(),
        r.center().x,
        r.y + r.h * 0.7,
        15. * s,
        text_color(),
    );
    if down {
        value.saturating_sub(1).max(lo)
    } else if up {
        value.saturating_add(1).min(hi)
    } else {
        value.clamp(lo, hi)
    }
}
fn color_properties(rgb: &mut [u8; 3], x: f32, mut y: f32, w: f32, s: f32) -> f32 {
    for (i, title) in ["Red", "Green", "Blue"].into_iter().enumerate() {
        let mut value = f32::from(rgb[i]);
        number(title, &mut value, [0., 255.], 1., x, &mut y, w, s);
        rgb[i] = value as u8;
    }
    y
}
fn effect_properties(e: &mut ParticleEffect, x: f32, mut y: f32, w: f32, s: f32) -> f32 {
    let names = ["Embers", "Rain", "Dust", "Sparks"];
    for (i, title) in names.into_iter().enumerate() {
        if small_button(
            Rect::new(
                x + (i % 2) as f32 * w * 0.51,
                y + (i / 2) as f32 * 33. * s,
                w * 0.49,
                28. * s,
            ),
            title,
            s * 0.7,
        ) {
            *e = match i {
                0 => ParticleEffect {
                    shape: ParticleShape::Square,
                    gravity: -18.,
                    spin: 80.,
                    ..Default::default()
                },
                1 => ParticleEffect {
                    shape: ParticleShape::Spark,
                    direction: 90.,
                    spread: 10.,
                    speed: 260.,
                    rate: 28.,
                    color: [126, 174, 211],
                    end_color: [103, 148, 190],
                    size: 9.,
                    end_size: 5.,
                    ..Default::default()
                },
                2 => ParticleEffect {
                    speed: 18.,
                    spread: 360.,
                    rate: 5.,
                    size: 12.,
                    end_size: 18.,
                    color: [164, 156, 140],
                    end_color: [164, 156, 140],
                    ..Default::default()
                },
                _ => ParticleEffect {
                    shape: ParticleShape::Spark,
                    speed: 240.,
                    spread: 75.,
                    gravity: 170.,
                    rate: 14.,
                    lifetime: 1.2,
                    ..Default::default()
                },
            };
        }
    }
    y += 82. * s;
    e.shape = cycle(
        Rect::new(x, y, w, 30. * s),
        e.shape,
        &ParticleShape::ALL,
        |v| format!("{v:?}"),
        s,
    );
    y += 45. * s;
    for (title, value, limits, step) in [
        ("Rate", &mut e.rate, [0.2, 60.], 0.2),
        ("Lifetime", &mut e.lifetime, [0.1, 8.], 0.1),
        ("Speed", &mut e.speed, [0., 800.], 5.),
        ("Direction", &mut e.direction, [0., 360.], 5.),
        ("Spread", &mut e.spread, [0., 360.], 5.),
        ("Gravity", &mut e.gravity, [-800., 800.], 10.),
        ("Drag", &mut e.drag, [0., 8.], 0.1),
        ("Start size", &mut e.size, [1., 120.], 1.),
        ("End size", &mut e.end_size, [0., 120.], 1.),
        ("Spin", &mut e.spin, [-1080., 1080.], 15.),
    ] {
        number(title, value, limits, step, x, &mut y, w, s);
    }
    e.rate = e.rate.min(240. / e.lifetime);
    toggle(
        Rect::new(x, y, w, 30. * s),
        "Fade out",
        &mut e.fade,
        s * 0.8,
    );
    y += 48. * s;
    label("Start colour", x, y, 13. * s, text_color());
    y += 20. * s;
    y = color_properties(&mut e.color, x, y, w, s);
    label("End colour", x, y, 13. * s, text_color());
    y += 20. * s;
    color_properties(&mut e.end_color, x, y, w, s)
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
