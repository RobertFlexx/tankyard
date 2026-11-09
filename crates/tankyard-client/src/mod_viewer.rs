//! The Mods page's file browser: every file in a pack, read straight from
//! its folder or `.zip` without extracting anything. Text opens with line
//! numbers and syntax colors, art and models with a preview, and anything
//! else as a hex dump. Creators can keep files private with `[source]` in
//! `tankyard.toml`; those are listed but not shown.
use crate::syntax::{self, Block, Language, Line, Style, Token};
use crate::ui::{
    ui_line as draw_line, ui_rectangle as draw_rectangle,
    ui_rectangle_lines as draw_rectangle_lines, *,
};
use macroquad::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use tankyard_mods::project::{Project, Source};

const MAX_LINES: usize = 20_000;
const MAX_LINE_CHARS: usize = 2_000;
const HEX_BYTES: usize = 64 * 1024;

pub struct Viewer {
    pub title: String,
    archive: bool,
    files: Vec<(String, Vec<u8>)>,
    access: Source,
    tree: Dir,
    collapsed: HashSet<String>,
    rows: Vec<Row>,
    selected: usize,
    view: View,
    /// Show the source of art that is also text, such as SVG.
    source: bool,
    /// Show Markdown as written instead of rendered.
    markdown_raw: bool,
    /// A line to mark, from a safety review finding.
    mark: Option<usize>,
    scroll: f32,
    hscroll: f32,
    tree_scroll: f32,
    dragging: bool,
    /// Highlighted text in the code view or a rendered code block.
    selection: Option<Selection>,
    selecting: bool,
    /// Which box was last copied from, and when, for the "Copied" note.
    copied: Option<(usize, f64)>,
}

/// The code view counts as box `CODE_VIEW`; code blocks in rendered
/// Markdown are numbered from 0.
const CODE_VIEW: usize = usize::MAX;

/// A highlighted range as (line, column) in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    block: usize,
    anchor: (usize, usize),
    head: (usize, usize),
}

impl Selection {
    fn range(self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    fn is_empty(self) -> bool {
        self.anchor == self.head
    }

    /// Columns highlighted on `line` (of `len` characters), with one extra
    /// for the line break when the selection carries on past it.
    fn columns(self, line: usize, len: usize) -> Option<(usize, usize)> {
        let (start, end) = self.range();
        if self.is_empty() || line < start.0 || line > end.0 {
            return None;
        }
        let from = if line == start.0 { start.1.min(len) } else { 0 };
        let to = if line == end.0 {
            end.1.min(len)
        } else {
            len + 1
        };
        (to > from).then_some((from, to))
    }

    /// The highlighted text from `lines`.
    fn text<S: AsRef<str>>(self, lines: &[S]) -> String {
        let (start, end) = self.range();
        let mut out = String::new();
        for (index, line) in lines.iter().enumerate().take(end.0 + 1).skip(start.0) {
            let line = line.as_ref();
            let len = line.chars().count();
            let from = if index == start.0 {
                start.1.min(len)
            } else {
                0
            };
            let to = if index == end.0 { end.1.min(len) } else { len };
            out.extend(line.chars().skip(from).take(to.saturating_sub(from)));
            if index < end.0 {
                out.push('\n');
            }
        }
        out
    }
}

/// Ctrl, or Cmd on a Mac, without Alt.
fn command_held() -> bool {
    let alt = is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt);
    !alt && [
        KeyCode::LeftControl,
        KeyCode::RightControl,
        KeyCode::LeftSuper,
        KeyCode::RightSuper,
    ]
    .into_iter()
    .any(is_key_down)
}

fn line_text(line: &Line) -> String {
    line.iter().map(|(_, text)| text.as_str()).collect()
}

#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: Vec<(String, usize)>,
}

enum Row {
    Folder {
        path: String,
        name: String,
        depth: usize,
        open: bool,
    },
    File {
        index: usize,
        name: String,
        depth: usize,
    },
}

enum View {
    Text {
        language: &'static str,
        lines: Vec<Line>,
        widest: usize,
        note: Option<String>,
    },
    Image {
        texture: Texture2D,
        note: String,
    },
    /// Rendered Markdown, laid out again when the width or theme changes.
    Markdown {
        blocks: Vec<Block>,
        layout: Option<(u64, Layout)>,
    },
    Message(String),
    Private,
}

/// Rendered Markdown placed for one width: items with offsets from the top.
struct Layout {
    items: Vec<Item>,
    /// Code blocks, which can be highlighted and copied.
    codes: Vec<CodeBox>,
    height: f32,
}

struct CodeBox {
    rect: Rect,
    lines: Vec<String>,
    line_h: f32,
    cell: f32,
    cols: usize,
}

impl CodeBox {
    /// Where text starts inside the box.
    fn origin(&self, s: f32) -> Vec2 {
        vec2(self.rect.x + 14.0 * s, self.rect.y + 8.0 * s)
    }

    fn copy_button(&self, s: f32) -> Rect {
        Rect::new(
            self.rect.right() - 32.0 * s,
            self.rect.y + 5.0 * s,
            26.0 * s,
            26.0 * s,
        )
    }
}

enum Item {
    Text {
        x: f32,
        y: f32,
        size: f32,
        text: String,
        color: Color,
        mono: bool,
    },
    Rect(Rect, Color),
}

impl Viewer {
    /// Read the pack at `path`. `focus` picks the first file and line shown.
    pub fn open(title: &str, path: &str, focus: Option<(&str, usize)>) -> Result<Self, String> {
        let (_, pack) = tankyard_mods::read_pack(Path::new(path))?;
        // An unreadable tankyard.toml is reported on the Mods page; keep
        // everything private rather than guess what the creator wanted.
        let access = match Project::read(&pack) {
            Some(Ok(project)) => project.source,
            Some(Err(_)) => Source {
                viewable: false,
                private: Vec::new(),
            },
            None => Source::default(),
        };
        let files: Vec<(String, Vec<u8>)> = pack
            .files()
            .map(|(name, data)| (name.to_owned(), data.to_vec()))
            .collect();
        let mut tree = Dir::default();
        for (index, (name, _)) in files.iter().enumerate() {
            let mut parts: Vec<&str> = name.split('/').collect();
            let file = parts.pop().unwrap_or_default();
            let mut dir = &mut tree;
            for part in parts {
                dir = dir.dirs.entry(part.to_owned()).or_default();
            }
            dir.files.push((file.to_owned(), index));
        }
        let find = |name: &str| files.iter().position(|(n, _)| n == name);
        let selected = focus
            .and_then(|(file, _)| find(file))
            .or_else(|| find("main.lua"))
            .or_else(|| find("mod.json"))
            .unwrap_or(0);
        let mut viewer = Self {
            title: title.to_owned(),
            archive: pack.archive,
            files,
            access,
            tree,
            collapsed: HashSet::new(),
            rows: Vec::new(),
            selected,
            view: View::Message(String::new()),
            source: false,
            markdown_raw: false,
            selection: None,
            selecting: false,
            copied: None,
            mark: None,
            scroll: 0.0,
            hscroll: 0.0,
            tree_scroll: 0.0,
            dragging: false,
        };
        viewer.rebuild_rows();
        viewer.select(selected);
        if let Some((_, line)) = focus.filter(|&(_, line)| line > 0) {
            viewer.mark = Some(line - 1);
            viewer.scroll = line.saturating_sub(6) as f32;
        }
        Ok(viewer)
    }

    fn rebuild_rows(&mut self) {
        fn walk(
            dir: &Dir,
            prefix: &str,
            depth: usize,
            collapsed: &HashSet<String>,
            rows: &mut Vec<Row>,
        ) {
            for (name, sub) in &dir.dirs {
                let path = format!("{prefix}{name}/");
                let open = !collapsed.contains(&path);
                rows.push(Row::Folder {
                    path: path.clone(),
                    name: name.clone(),
                    depth,
                    open,
                });
                if open {
                    walk(sub, &path, depth + 1, collapsed, rows);
                }
            }
            for (name, index) in &dir.files {
                rows.push(Row::File {
                    index: *index,
                    name: name.clone(),
                    depth,
                });
            }
        }
        self.rows.clear();
        walk(&self.tree, "", 0, &self.collapsed, &mut self.rows);
    }

    fn select(&mut self, index: usize) {
        self.selected = index;
        self.scroll = 0.0;
        self.hscroll = 0.0;
        self.mark = None;
        self.selection = None;
        self.view = self.load();
    }

    fn copy(&mut self, block: usize, text: &str) {
        if !text.is_empty() {
            drop(crate::clipboard::copy(text));
            self.copied = Some((block, get_time()));
        }
    }

    /// True for a moment after copying from `block`.
    fn just_copied(&self, block: usize) -> bool {
        self.copied
            .is_some_and(|(b, at)| b == block && get_time() - at < 1.4)
    }

    fn load(&self) -> View {
        let Some((name, data)) = self.files.get(self.selected) else {
            return View::Message("This pack has no files.".into());
        };
        if !self.access.readable(name) {
            return View::Private;
        }
        let text = std::str::from_utf8(data).ok().filter(|t| !t.contains('\0'));
        if let Some(text) = text
            && Language::for_file(name) == Language::Markdown
            && !self.markdown_raw
        {
            return View::Markdown {
                blocks: syntax::markdown_blocks(text),
                layout: None,
            };
        }
        if tankyard_mods::assets::is_asset(name) && !(self.source && text.is_some()) {
            let files = |other: &str| {
                self.files
                    .iter()
                    .find(|(n, _)| n == other)
                    .map(|(_, d)| d.clone())
            };
            return match tankyard_mods::assets::decode(name, data, &files) {
                Ok(image) => {
                    let texture = Texture2D::from_rgba8(
                        image.width() as u16,
                        image.height() as u16,
                        image.as_raw(),
                    );
                    let model = tankyard_mods::assets::MODEL_EXTENSIONS
                        .contains(&tankyard_mods::assets::extension(name).as_str());
                    View::Image {
                        texture,
                        note: format!(
                            "{} preview · {} × {}",
                            if model { "Top-down" } else { "Image" },
                            image.width(),
                            image.height()
                        ),
                    }
                }
                Err(error) => match text {
                    Some(text) => text_view(name, text, Some(format!("No preview: {error}"))),
                    None => View::Message(format!("This file can't be previewed: {error}")),
                },
            };
        }
        match text {
            Some(text) => text_view(name, text, None),
            None => {
                let shown = &data[..data.len().min(HEX_BYTES)];
                let lines = syntax::hex_dump(shown);
                View::Text {
                    language: "Binary",
                    widest: lines.iter().map(line_chars).max().unwrap_or(0),
                    lines,
                    note: (data.len() > HEX_BYTES).then(|| {
                        format!(
                            "Showing the first {} of {}",
                            size(HEX_BYTES),
                            size(data.len())
                        )
                    }),
                }
            }
        }
    }

    /// Draw the browser over the page. Returns false once it is closed.
    pub fn draw(&mut self, s: f32) -> bool {
        let screen = vec2(screen_width(), crate::ui_layout::height());
        draw_rectangle(0.0, 0.0, screen.x, screen.y, alpha(bg_color(), 0.8));
        let w = (1180.0 * s).min(screen.x - 40.0 * s);
        let h = (screen.y - 80.0 * s).max(320.0 * s);
        let r = Rect::new((screen.x - w) * 0.5, (screen.y - h) * 0.5, w, h);
        panel(r);
        draw_rectangle(r.x, r.y, r.w, 4.0 * s, accent());

        let total: usize = self.files.iter().map(|(_, d)| d.len()).sum();
        label(
            &fitted_label(&self.title, 22.0 * s, r.w - 200.0 * s),
            r.x + 20.0 * s,
            r.y + 38.0 * s,
            22.0 * s,
            text_color(),
        );
        label(
            &format!(
                "{} {} · {} · {}{}",
                self.files.len(),
                if self.files.len() == 1 {
                    "file"
                } else {
                    "files"
                },
                size(total),
                if self.archive {
                    "read from the .zip, nothing extracted"
                } else {
                    "folder"
                },
                self.privacy_note()
            ),
            r.x + 20.0 * s,
            r.y + 60.0 * s,
            12.0 * s,
            muted_color(),
        );
        if small_button(
            Rect::new(r.right() - 120.0 * s, r.y + 20.0 * s, 100.0 * s, 34.0 * s),
            "Close",
            s * 0.85,
        ) || is_key_pressed(KeyCode::Escape)
        {
            return false;
        }

        let body = Rect::new(r.x, r.y + 76.0 * s, r.w, r.h - 76.0 * s);
        draw_line(body.x, body.y, body.right(), body.y, 1.0, line_color());
        let tree_w = (body.w * 0.27).clamp(190.0 * s, 300.0 * s);
        let tree = Rect::new(body.x, body.y, tree_w, body.h);
        draw_line(
            tree.right(),
            tree.y,
            tree.right(),
            tree.bottom(),
            1.0,
            line_color(),
        );
        self.draw_tree(tree, s);
        let content = Rect::new(tree.right(), body.y, body.right() - tree.right(), body.h);
        self.draw_content(content, s);
        self.keys();
        true
    }

    fn privacy_note(&self) -> String {
        if !self.access.viewable {
            return " · the creator keeps these files private".into();
        }
        match self
            .files
            .iter()
            .filter(|(n, _)| !self.access.readable(n))
            .count()
        {
            0 => String::new(),
            1 => " · 1 file is private".into(),
            n => format!(" · {n} files are private"),
        }
    }

    fn keys(&mut self) {
        let step = |viewer: &mut Self, forward: bool| {
            let files: Vec<usize> = viewer
                .rows
                .iter()
                .filter_map(|row| match row {
                    Row::File { index, .. } => Some(*index),
                    Row::Folder { .. } => None,
                })
                .collect();
            let at = files.iter().position(|&i| i == viewer.selected);
            let next = match (at, forward) {
                (Some(at), true) => files.get(at + 1),
                (Some(at), false) => at.checked_sub(1).and_then(|at| files.get(at)),
                (None, _) => files.first(),
            };
            if let Some(&next) = next {
                viewer.select(next);
            }
        };
        if is_key_pressed(KeyCode::Down) {
            step(self, true);
        }
        if is_key_pressed(KeyCode::Up) {
            step(self, false);
        }
        if is_key_pressed(KeyCode::PageDown) {
            self.scroll += 30.0;
        }
        if is_key_pressed(KeyCode::PageUp) {
            self.scroll -= 30.0;
        }
        if is_key_pressed(KeyCode::Home) {
            self.scroll = 0.0;
        }
        if is_key_pressed(KeyCode::End) {
            self.scroll = f32::MAX;
        }
    }

    fn draw_tree(&mut self, r: Rect, s: f32) {
        let row_h = 26.0 * s;
        let top = r.y + 8.0 * s;
        let visible = ((r.bottom() - top - 8.0 * s) / row_h).floor().max(1.0) as usize;
        if contains(r) {
            self.tree_scroll -= mouse_wheel().1.clamp(-3.0, 3.0);
        }
        let max = self.rows.len().saturating_sub(visible) as f32;
        self.tree_scroll = self.tree_scroll.clamp(0.0, max);
        let first = self.tree_scroll.round() as usize;
        let mut clicked = None;
        for (slot, row) in self.rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (slot - first) as f32 * row_h;
            let rect = Rect::new(r.x + 6.0 * s, y, r.w - 12.0 * s, row_h - 2.0 * s);
            let (depth, name, selected) = match row {
                Row::Folder { name, depth, .. } => (*depth, name, false),
                Row::File { index, name, depth } => (*depth, name, *index == self.selected),
            };
            let hovered = contains(rect);
            if selected {
                draw_rectangle(
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    mix(surface_color(), accent(), 0.30),
                );
                draw_rectangle(rect.x, rect.y, 3.0 * s, rect.h, accent());
            } else if hovered {
                draw_rectangle(rect.x, rect.y, rect.w, rect.h, alpha(hover_color(), 0.6));
            }
            let x = rect.x + 10.0 * s + depth as f32 * 14.0 * s;
            let mid = rect.y + rect.h * 0.5;
            let text_x = match row {
                Row::Folder { open, .. } => {
                    let c = muted_color();
                    let (a, b, d) = if *open {
                        (
                            vec2(x, mid - 3.0 * s),
                            vec2(x + 8.0 * s, mid - 3.0 * s),
                            vec2(x + 4.0 * s, mid + 3.0 * s),
                        )
                    } else {
                        (
                            vec2(x + 1.0 * s, mid - 4.5 * s),
                            vec2(x + 1.0 * s, mid + 4.5 * s),
                            vec2(x + 7.0 * s, mid),
                        )
                    };
                    let p = |v: Vec2| crate::ui_layout::position(v);
                    draw_triangle(p(a), p(b), p(d), c);
                    x + 14.0 * s
                }
                Row::File { index, .. } => {
                    let (tag, color) = file_badge(&self.files[*index].0);
                    let bw = 30.0 * s;
                    draw_rectangle(x - 2.0 * s, mid - 7.0 * s, bw, 14.0 * s, alpha(color, 0.22));
                    center_label(tag, x - 2.0 * s + bw * 0.5, mid + 3.5 * s, 8.5 * s, color);
                    x + bw + 6.0 * s
                }
            };
            let private = matches!(row, Row::File { index, .. } if !self.access.readable(&self.files[*index].0));
            let lock_room = if private { 18.0 * s } else { 0.0 };
            if private {
                lock_icon(
                    rect.right() - 12.0 * s,
                    mid + 4.0 * s,
                    s * 0.8,
                    muted_color(),
                );
            }
            label(
                &fitted_label(name, 13.0 * s, rect.right() - text_x - 4.0 * s - lock_room),
                text_x,
                mid + 4.5 * s,
                13.0 * s,
                if selected {
                    text_color()
                } else {
                    mix(text_color(), muted_color(), 0.25)
                },
            );
            if hovered && is_mouse_button_pressed(MouseButton::Left) {
                clicked = Some(slot);
            }
        }
        if self.rows.len() > visible {
            scrollbar(
                Rect::new(r.right() - 5.0 * s, top, 3.0 * s, visible as f32 * row_h),
                first as f32,
                visible as f32,
                self.rows.len() as f32,
            );
        }
        match clicked.and_then(|slot| self.rows.get(slot)) {
            Some(Row::Folder { path, .. }) => {
                let path = path.clone();
                if !self.collapsed.remove(&path) {
                    self.collapsed.insert(path);
                }
                self.rebuild_rows();
            }
            Some(Row::File { index, .. }) => {
                let index = *index;
                if index != self.selected {
                    self.select(index);
                }
            }
            None => {}
        }
    }

    fn draw_content(&mut self, r: Rect, s: f32) {
        let Some((name, data)) = self.files.get(self.selected) else {
            label(
                "This pack has no files.",
                r.x + 20.0 * s,
                r.y + 40.0 * s,
                14.0 * s,
                muted_color(),
            );
            return;
        };
        let px = r.x + 18.0 * s;
        let readable_text = self.access.readable(name)
            && std::str::from_utf8(data).is_ok_and(|t| !t.contains('\0'));
        let markdown = readable_text && Language::for_file(name) == Language::Markdown;
        // Art that is also text (SVG) and Markdown switch between a preview
        // and what's written.
        let switch = if markdown {
            Some(("Rendered", !self.markdown_raw))
        } else if readable_text && tankyard_mods::assets::is_asset(name) {
            Some(("Preview", !self.source))
        } else {
            None
        };
        let mut facts = vec![size(data.len())];
        match &self.view {
            View::Text {
                language, lines, ..
            } => {
                facts.insert(0, (*language).to_owned());
                facts.push(format!(
                    "{} {}",
                    lines.len(),
                    if lines.len() == 1 { "line" } else { "lines" }
                ));
            }
            View::Markdown { .. } => facts.insert(0, "Markdown".into()),
            View::Image { note, .. } => facts.insert(0, note.clone()),
            View::Message(_) => {}
            View::Private => facts.insert(0, "Private".into()),
        }
        let facts = facts.join(" · ");
        let facts_w = label_width(&facts, 12.0 * s);
        // The same switch as the settings pages, at the viewer's scale.
        let k = s * 0.72;
        let switch_w = switch.map_or(0.0, |(text, _)| label_width(text, 18.0 * k) + 76.0 * k);
        let right = r.right()
            - 18.0 * s
            - if switch.is_some() {
                switch_w + 20.0 * s
            } else {
                0.0
            };
        label(
            &fitted_label(name, 14.0 * s, right - px - facts_w - 16.0 * s),
            px,
            r.y + 30.0 * s,
            14.0 * s,
            text_color(),
        );
        right_label(&facts, right, r.y + 30.0 * s, 12.0 * s, muted_color());
        if let Some((text, mut on)) = switch {
            let rect = Rect::new(
                r.right() - 18.0 * s - switch_w,
                r.y + 30.0 * s - 22.0 * k,
                switch_w,
                33.0 * k,
            );
            if toggle(rect, text, &mut on, k) {
                if markdown {
                    self.markdown_raw = !on;
                } else {
                    self.source = !on;
                }
                let mark = self.mark;
                self.view = self.load();
                self.scroll = 0.0;
                self.mark = mark;
                self.selection = None;
            }
        }
        let area = Rect::new(
            r.x + 10.0 * s,
            r.y + 48.0 * s,
            r.w - 20.0 * s,
            r.h - 58.0 * s,
        );
        draw_rectangle(
            area.x,
            area.y,
            area.w,
            area.h,
            mix(surface_color(), bg_color(), 0.55),
        );
        draw_rectangle_lines(area.x, area.y, area.w, area.h, 1.0, line_color());
        match &self.view {
            View::Message(message) => {
                wrap(
                    message,
                    area.x + 16.0 * s,
                    area.y + 30.0 * s,
                    area.w - 32.0 * s,
                    13.0 * s,
                    muted_color(),
                );
            }
            View::Image { texture, .. } => image(area, texture, s),
            View::Private => {
                let c = area.center();
                lock_icon(c.x, c.y - 30.0 * s, s * 2.2, muted_color());
                center_label(
                    "The creator keeps this file private.",
                    c.x,
                    c.y + 14.0 * s,
                    15.0 * s,
                    text_color(),
                );
                center_label(
                    "The safety review still checked it before the pack could run.",
                    c.x,
                    c.y + 38.0 * s,
                    12.0 * s,
                    muted_color(),
                );
            }
            View::Text { .. } => self.draw_code(area, s),
            View::Markdown { .. } => self.draw_markdown(area, s),
        }
    }

    fn draw_code(&mut self, area: Rect, s: f32) {
        let View::Text {
            lines,
            widest,
            note,
            ..
        } = &self.view
        else {
            return;
        };
        let size = 13.0 * s;
        let line_h = (size * 1.5).round();
        let cell = code_advance(size).max(1.0);
        let note_h = if note.is_some() {
            line_h + 4.0 * s
        } else {
            0.0
        };
        let digits = lines.len().max(1).to_string().len().max(3);
        let gutter = digits as f32 * cell + 22.0 * s;
        let text_x = area.x + gutter + 10.0 * s;
        let top = area.y + 8.0 * s;
        let rows = ((area.bottom() - top - note_h - 6.0 * s) / line_h)
            .floor()
            .max(1.0) as usize;
        let cols = ((area.right() - text_x - 14.0 * s) / cell).floor().max(1.0) as usize;

        if contains(area) {
            let (wheel_x, wheel_y) = mouse_wheel();
            let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
            if shift {
                self.hscroll -= wheel_y.clamp(-3.0, 3.0) * 6.0;
            } else {
                self.scroll -= wheel_y.clamp(-3.0, 3.0) * 3.0;
            }
            self.hscroll -= wheel_x.clamp(-3.0, 3.0) * 6.0;
        }
        if is_key_pressed(KeyCode::Right) {
            self.hscroll += 8.0;
        }
        if is_key_pressed(KeyCode::Left) {
            self.hscroll -= 8.0;
        }
        let max_scroll = lines.len().saturating_sub(rows) as f32;
        // Dragging the scrollbar.
        let track = Rect::new(area.right() - 9.0 * s, top, 6.0 * s, rows as f32 * line_h);
        if lines.len() > rows {
            let grab = Rect::new(track.x - 6.0 * s, track.y, track.w + 12.0 * s, track.h);
            if contains(grab) && is_mouse_button_pressed(MouseButton::Left) {
                self.dragging = true;
            }
            if self.dragging {
                let y = crate::ui_layout::pointer().y;
                self.scroll = ((y - track.y) / track.h) * lines.len() as f32 - rows as f32 * 0.5;
            }
        }
        // Highlighting: press on the text and drag. Past the edges, the view
        // scrolls along.
        let text_area = Rect::new(
            area.x + 1.0,
            top,
            track.x - 6.0 * s - area.x - 1.0,
            rows as f32 * line_h,
        );
        let pointer = crate::ui_layout::pointer();
        let line_len = |index: usize| lines.get(index).map_or(0, |l| line_text(l).chars().count());
        let position = |first: usize, skip: usize| -> (usize, usize) {
            let row = ((pointer.y - top) / line_h).floor() as i64;
            let line = (first as i64 + row).clamp(0, lines.len().saturating_sub(1) as i64) as usize;
            let col = skip + ((pointer.x - text_x) / cell).round().max(0.0) as usize;
            (line, col.min(line_len(line)))
        };
        if !self.dragging
            && !lines.is_empty()
            && contains(text_area)
            && is_mouse_button_pressed(MouseButton::Left)
        {
            let at = position(self.scroll.round() as usize, self.hscroll.round() as usize);
            let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
            let anchor = match self.selection {
                Some(sel) if shift && sel.block == CODE_VIEW => sel.anchor,
                _ => at,
            };
            self.selection = Some(Selection {
                block: CODE_VIEW,
                anchor,
                head: at,
            });
            self.selecting = true;
        }
        if self.selecting && is_mouse_button_down(MouseButton::Left) {
            if pointer.y < text_area.y {
                self.scroll -= 0.5;
            } else if pointer.y > text_area.bottom() {
                self.scroll += 0.5;
            }
            if pointer.x < text_x {
                self.hscroll -= 1.0;
            } else if pointer.x > text_area.right() {
                self.hscroll += 1.0;
            }
        }
        if !is_mouse_button_down(MouseButton::Left) {
            self.dragging = false;
            self.selecting = false;
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        self.hscroll = self.hscroll.clamp(0.0, widest.saturating_sub(cols) as f32);
        let first = self.scroll.round() as usize;
        let skip = self.hscroll.round() as usize;
        if self.selecting
            && let Some(sel) = &mut self.selection
        {
            sel.head = position(first, skip);
        }
        if command_held() && is_key_pressed(KeyCode::A) && !lines.is_empty() {
            let last = lines.len() - 1;
            self.selection = Some(Selection {
                block: CODE_VIEW,
                anchor: (0, 0),
                head: (last, line_len(last)),
            });
        }
        let mut copy = None;
        if command_held()
            && is_key_pressed(KeyCode::C)
            && let Some(sel) = self
                .selection
                .filter(|s| s.block == CODE_VIEW && !s.is_empty())
        {
            let (start, end) = sel.range();
            let text: Vec<String> = lines[start.0..=end.0].iter().map(line_text).collect();
            let shifted = Selection {
                block: CODE_VIEW,
                anchor: (0, start.1),
                head: (end.0 - start.0, end.1),
            };
            copy = Some(shifted.text(&text));
        }

        draw_rectangle(
            area.x + 1.0,
            area.y + 1.0,
            gutter,
            area.h - 2.0,
            alpha(line_color(), 0.22),
        );
        for (slot, line) in lines.iter().enumerate().skip(first).take(rows) {
            let y = top + (slot - first) as f32 * line_h;
            let baseline = y + line_h * 0.72;
            if self.mark == Some(slot) {
                draw_rectangle(
                    area.x + 1.0,
                    y,
                    area.w - 2.0,
                    line_h,
                    alpha(warm_text_color(), 0.18),
                );
                draw_rectangle(area.x + 1.0, y, 3.0 * s, line_h, warm_text_color());
            }
            if let Some((from, to)) = self
                .selection
                .filter(|s| s.block == CODE_VIEW)
                .and_then(|sel| sel.columns(slot, line_len(slot)))
                .map(|(from, to)| (from.max(skip), to.min(skip + cols)))
                .filter(|(from, to)| to > from)
            {
                draw_rectangle(
                    text_x + (from - skip) as f32 * cell,
                    y,
                    (to - from) as f32 * cell,
                    line_h,
                    alpha(accent(), 0.28),
                );
            }
            let number = (slot + 1).to_string();
            code_label(
                &number,
                area.x + gutter - 8.0 * s - number.len() as f32 * cell,
                baseline,
                size,
                alpha(muted_color(), 0.75),
            );
            let mut col = 0;
            for (token, text) in line {
                let len = text.chars().count();
                if col + len > skip && col < skip + cols {
                    let from = skip.saturating_sub(col);
                    let shown: String = text
                        .chars()
                        .skip(from)
                        .take(skip + cols - col.max(skip))
                        .collect();
                    let x = text_x + (col + from - skip) as f32 * cell;
                    code_label(&shown, x, baseline, size, token_color(*token));
                }
                col += len;
                if col >= skip + cols {
                    break;
                }
            }
        }
        if lines.len() > rows {
            scrollbar(track, first as f32, rows as f32, lines.len() as f32);
        }
        if *widest > cols {
            let bar = Rect::new(
                text_x,
                top + rows as f32 * line_h + 2.0 * s,
                area.right() - text_x - 14.0 * s,
                4.0 * s,
            );
            scrollbar_horizontal(bar, skip as f32, cols as f32, *widest as f32);
        }
        if let Some(note) = note {
            label(
                note,
                area.x + 12.0 * s,
                area.bottom() - 10.0 * s,
                11.0 * s,
                muted_color(),
            );
        }
        if let Some(text) = copy {
            self.copy(CODE_VIEW, &text);
        }
        if self.just_copied(CODE_VIEW) {
            copied_note(area.right() - 20.0 * s, area.bottom() - 22.0 * s, s);
        }
    }

    fn draw_markdown(&mut self, area: Rect, s: f32) {
        let View::Markdown { blocks, layout } = &mut self.view else {
            return;
        };
        let pad = 22.0 * s;
        let width = area.w - pad * 2.0 - 10.0 * s;
        let key = ((width.round() as u64) << 32)
            | ((s * 1000.0).round() as u64 * 2)
            | u64::from(is_dark());
        if layout.as_ref().is_none_or(|(k, _)| *k != key) {
            *layout = Some((key, layout_markdown(blocks, width, s)));
        }
        let Some((_, layout)) = layout.as_ref() else {
            return;
        };
        // `scroll` counts steps of 20 px, like lines in the code view.
        let step = 20.0 * s;
        if contains(area) {
            self.scroll -= mouse_wheel().1.clamp(-3.0, 3.0) * 3.0;
        }
        let max = ((layout.height + pad * 2.0 - area.h) / step).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
        let offset = self.scroll * step;
        let x0 = area.x + pad;
        let y0 = area.y + pad - offset;
        let (top, bottom) = (area.y + 1.0, area.bottom() - 1.0);
        for item in &layout.items {
            match item {
                Item::Rect(rect, color) => {
                    let y = (y0 + rect.y).max(top);
                    let end = (y0 + rect.bottom()).min(bottom);
                    if end > y {
                        draw_rectangle(x0 + rect.x, y, rect.w, end - y, *color);
                    }
                }
                Item::Text {
                    x,
                    y,
                    size,
                    text,
                    color,
                    mono,
                } => {
                    let baseline = y0 + y;
                    if baseline - size < top || baseline + size * 0.3 > bottom {
                        continue;
                    }
                    if *mono {
                        code_label(text, x0 + x, baseline, *size, *color);
                    } else {
                        label(text, x0 + x, baseline, *size, *color);
                    }
                }
            }
        }
        let origin = vec2(x0, y0);
        let visible = |rect: Rect| rect.y >= top && rect.bottom() <= bottom;
        let pointer = crate::ui_layout::pointer();
        let pressed = contains(area) && is_mouse_button_pressed(MouseButton::Left);
        let mut copy = None;
        let mut hit = None;
        for (index, code) in layout.codes.iter().enumerate() {
            let rect = Rect::new(
                origin.x + code.rect.x,
                origin.y + code.rect.y,
                code.rect.w,
                code.rect.h,
            );
            let button = code.copy_button(s).offset(origin);
            if pressed && visible(button) && contains(button) {
                copy = Some((index, code.lines.join("\n")));
            } else if pressed && contains(rect) {
                hit = Some(index);
            }
        }
        // Where the pointer falls in a code box, clamped to its text.
        let position = |code: &CodeBox| -> (usize, usize) {
            let start = code.origin(s) + origin;
            let row = ((pointer.y - start.y) / code.line_h).floor() as i64;
            let line = row.clamp(0, code.lines.len().saturating_sub(1) as i64) as usize;
            let len = code.lines.get(line).map_or(0, |l| l.chars().count());
            let col = ((pointer.x - start.x) / code.cell).round().max(0.0) as usize;
            // Past the cut-off edge means the whole line.
            (line, if col >= code.cols { len } else { col.min(len) })
        };
        if pressed && copy.is_none() {
            self.selection = hit.map(|index| {
                let at = position(&layout.codes[index]);
                Selection {
                    block: index,
                    anchor: at,
                    head: at,
                }
            });
            self.selecting = hit.is_some();
        }
        if !is_mouse_button_down(MouseButton::Left) {
            self.selecting = false;
        }
        if self.selecting
            && let Some(sel) = &mut self.selection
            && let Some(code) = layout.codes.get(sel.block)
        {
            sel.head = position(code);
            if pointer.y < area.y {
                self.scroll -= 0.5;
            } else if pointer.y > area.bottom() {
                self.scroll += 0.5;
            }
        }
        if command_held()
            && is_key_pressed(KeyCode::C)
            && let Some(sel) = self.selection.filter(|s| !s.is_empty())
            && let Some(code) = layout.codes.get(sel.block)
        {
            copy = Some((sel.block, sel.text(&code.lines)));
        }
        for (index, code) in layout.codes.iter().enumerate() {
            let start = code.origin(s) + origin;
            if let Some(sel) = self.selection.filter(|s| s.block == index) {
                for (line, text) in code.lines.iter().enumerate() {
                    let len = text.chars().count();
                    let Some((from, to)) = sel.columns(line, len) else {
                        continue;
                    };
                    let to = to.min(code.cols);
                    let y = start.y + line as f32 * code.line_h;
                    if to <= from || y < top || y + code.line_h > bottom {
                        continue;
                    }
                    draw_rectangle(
                        start.x + from as f32 * code.cell,
                        y,
                        (to - from) as f32 * code.cell,
                        code.line_h,
                        alpha(accent(), 0.28),
                    );
                }
            }
            let button = code.copy_button(s).offset(origin);
            if visible(button) {
                let done = self
                    .copied
                    .is_some_and(|(b, at)| b == index && get_time() - at < 1.4);
                copy_icon(button, done, s);
            }
        }
        if max > 0.0 {
            let track = Rect::new(
                area.right() - 9.0 * s,
                area.y + 8.0 * s,
                6.0 * s,
                area.h - 16.0 * s,
            );
            scrollbar(track, offset, area.h, layout.height + pad * 2.0);
        }
        if let Some((index, text)) = copy {
            self.copy(index, &text);
        }
    }
}

/// Two overlapping pages, or a tick once copied.
fn copy_icon(r: Rect, done: bool, s: f32) {
    let hover = contains(r);
    if hover || done {
        draw_rectangle(
            r.x,
            r.y,
            r.w,
            r.h,
            alpha(line_color(), if hover { 0.8 } else { 0.5 }),
        );
    }
    let ink = if done {
        accent()
    } else if hover {
        text_color()
    } else {
        muted_color()
    };
    let c = r.center();
    if done {
        draw_line(
            c.x - 6.0 * s,
            c.y,
            c.x - 2.0 * s,
            c.y + 4.0 * s,
            2.0 * s,
            ink,
        );
        draw_line(
            c.x - 2.0 * s,
            c.y + 4.0 * s,
            c.x + 6.0 * s,
            c.y - 5.0 * s,
            2.0 * s,
            ink,
        );
        right_label("Copied", r.x - 8.0 * s, c.y + 4.0 * s, 11.0 * s, accent());
    } else {
        let w = 9.0 * s;
        let h = 11.0 * s;
        draw_rectangle_lines(
            c.x - w * 0.5 - 2.0 * s,
            c.y - h * 0.5 - 2.0 * s,
            w,
            h,
            1.5 * s,
            ink,
        );
        draw_rectangle(
            c.x - w * 0.5 + 2.0 * s,
            c.y - h * 0.5 + 2.0 * s,
            w,
            h,
            mix(surface_color(), bg_color(), 0.55),
        );
        draw_rectangle_lines(
            c.x - w * 0.5 + 2.0 * s,
            c.y - h * 0.5 + 2.0 * s,
            w,
            h,
            1.5 * s,
            ink,
        );
    }
}

/// "Copied" on a small tag, right-aligned at `x`.
fn copied_note(x: f32, y: f32, s: f32) {
    let w = label_width("Copied", 11.0 * s) + 16.0 * s;
    draw_rectangle(x - w, y - 14.0 * s, w, 20.0 * s, surface_color());
    draw_rectangle_lines(x - w, y - 14.0 * s, w, 20.0 * s, 1.0, line_color());
    right_label("Copied", x - 8.0 * s, y, 11.0 * s, accent());
}

/// Lay out rendered Markdown `width` pixels wide.
fn layout_markdown(blocks: &[Block], width: f32, s: f32) -> Layout {
    let mut items = Vec::new();
    let mut codes = Vec::new();
    let mut y = 0.0;
    let body = mix(text_color(), muted_color(), 0.15);
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            y += match block {
                Block::Heading(..) => 18.0 * s,
                Block::Item(..) => 3.0 * s,
                _ => 11.0 * s,
            };
        }
        match block {
            Block::Heading(level, spans) => {
                let size =
                    [24.0, 19.0, 16.0, 14.5, 14.0, 13.0][usize::from(*level).clamp(1, 6) - 1] * s;
                flow(&mut items, spans, 0.0, width, &mut y, size, text_color(), s);
                if *level <= 2 {
                    y += 6.0 * s;
                    items.push(Item::Rect(Rect::new(0.0, y, width, 1.0), line_color()));
                    y += 2.0 * s;
                }
            }
            Block::Paragraph(spans) => {
                flow(&mut items, spans, 0.0, width, &mut y, 14.0 * s, body, s)
            }
            Block::Item(depth, marker, spans) => {
                let indent = 6.0 * s + *depth as f32 * 22.0 * s;
                let size = 14.0 * s;
                items.push(Item::Text {
                    x: indent,
                    y: y + (size * 1.5).round() * 0.72,
                    size,
                    text: marker.clone(),
                    color: muted_color(),
                    mono: false,
                });
                let text_x = indent + label_width(marker, size).max(10.0 * s) + 8.0 * s;
                flow(
                    &mut items,
                    spans,
                    text_x,
                    width - text_x,
                    &mut y,
                    size,
                    body,
                    s,
                );
            }
            Block::Quote(spans) => {
                let start = y;
                flow(
                    &mut items,
                    spans,
                    16.0 * s,
                    width - 16.0 * s,
                    &mut y,
                    14.0 * s,
                    muted_color(),
                    s,
                );
                items.push(Item::Rect(
                    Rect::new(0.0, start + 2.0 * s, 3.0 * s, y - start - 4.0 * s),
                    alpha(accent(), 0.7),
                ));
            }
            Block::Code(lines) => {
                let size = 12.5 * s;
                let line_h = (size * 1.45).round();
                let cell = code_advance(size).max(1.0);
                // Leave room for the copy button on the first line.
                let cols = ((width - 50.0 * s) / cell).floor().max(1.0) as usize;
                let h = lines.len().max(1) as f32 * line_h + 16.0 * s;
                items.push(Item::Rect(
                    Rect::new(0.0, y, width, h),
                    alpha(bg_color(), 0.55),
                ));
                items.push(Item::Rect(Rect::new(0.0, y, 3.0 * s, h), line_color()));
                for (row, line) in lines.iter().enumerate() {
                    let mut text: String = line.chars().take(cols).collect();
                    if line.chars().count() > cols {
                        text.pop();
                        text.push('…');
                    }
                    items.push(Item::Text {
                        x: 14.0 * s,
                        y: y + 8.0 * s + row as f32 * line_h + line_h * 0.72,
                        size,
                        text,
                        color: token_color(Token::String),
                        mono: true,
                    });
                }
                codes.push(CodeBox {
                    rect: Rect::new(0.0, y, width, h),
                    lines: lines.clone(),
                    line_h,
                    cell,
                    cols,
                });
                y += h;
            }
            Block::Table { header, rows } => {
                table(&mut items, rows, *header, width, &mut y, body, s)
            }
            Block::Rule => {
                y += 4.0 * s;
                items.push(Item::Rect(Rect::new(0.0, y, width, 1.0), line_color()));
                y += 4.0 * s;
            }
        }
    }
    Layout {
        items,
        codes,
        height: y,
    }
}

/// A table whose columns share `width` by how much text they hold.
fn table(
    items: &mut Vec<Item>,
    rows: &[Vec<syntax::Spans>],
    header: bool,
    width: f32,
    y: &mut f32,
    color: Color,
    s: f32,
) {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return;
    }
    let size = 13.0 * s;
    let pad = 8.0 * s;
    let measure = |cell: &syntax::Spans| -> f32 {
        cell.iter()
            .map(|(style, text)| match style {
                Style::Code => text.chars().count() as f32 * code_advance(size * 0.92),
                _ => label_width(text, size),
            })
            .sum()
    };
    let mut natural = vec![40.0 * s; columns];
    for row in rows {
        for (column, cell) in row.iter().enumerate() {
            natural[column] = natural[column].max(measure(cell) + pad * 2.0);
        }
    }
    // Shrink wide columns first, so short ones like names stay on one line.
    let mut widths = natural.clone();
    let total: f32 = natural.iter().sum();
    if total > width {
        let fair = width / columns as f32;
        let small: f32 = natural.iter().filter(|&&w| w <= fair).sum();
        let large: f32 = natural.iter().filter(|&&w| w > fair).sum();
        let room = (width - small).max(0.0);
        for w in &mut widths {
            if *w > fair {
                *w = (*w / large * room).max(60.0 * s);
            }
        }
    }
    let table_w: f32 = widths.iter().sum();
    let top = *y;
    for (index, row) in rows.iter().enumerate() {
        let start = *y;
        let mut bottom = start;
        let mut x = 0.0;
        let first = items.len();
        for (column, w) in widths.iter().enumerate() {
            let mut cell_y = start + 5.0 * s;
            if let Some(cell) = row.get(column) {
                let strong: Vec<(Style, String)>;
                let cell = if header && index == 0 {
                    strong = cell
                        .iter()
                        .map(|(style, text)| {
                            (
                                if *style == Style::Text {
                                    Style::Strong
                                } else {
                                    *style
                                },
                                text.clone(),
                            )
                        })
                        .collect();
                    &strong
                } else {
                    cell
                };
                flow(
                    items,
                    cell,
                    x + pad,
                    w - pad * 2.0,
                    &mut cell_y,
                    size,
                    color,
                    s,
                );
            } else {
                cell_y += (size * 1.5).round();
            }
            bottom = bottom.max(cell_y + 5.0 * s);
            x += w;
        }
        let shade = if header && index == 0 {
            Some(alpha(line_color(), 0.55))
        } else {
            (index % 2 == 0).then(|| alpha(bg_color(), 0.35))
        };
        if let Some(shade) = shade {
            items.insert(
                first,
                Item::Rect(Rect::new(0.0, start, table_w, bottom - start), shade),
            );
        }
        items.push(Item::Rect(
            Rect::new(0.0, bottom, table_w, 1.0),
            line_color(),
        ));
        *y = bottom;
    }
    // Column dividers and the outline.
    let mut x = 0.0;
    for w in &widths {
        items.push(Item::Rect(Rect::new(x, top, 1.0, *y - top), line_color()));
        x += w;
    }
    items.push(Item::Rect(
        Rect::new(x - 1.0, top, 1.0, *y - top),
        line_color(),
    ));
    items.push(Item::Rect(Rect::new(0.0, top, table_w, 1.0), line_color()));
}

/// Wrap styled text into lines starting at `x`, moving `y` past them.
#[allow(clippy::too_many_arguments)]
fn flow(
    items: &mut Vec<Item>,
    spans: &[(Style, String)],
    x: f32,
    width: f32,
    y: &mut f32,
    size: f32,
    color: Color,
    s: f32,
) {
    let line_h = (size * 1.5).round();
    let space = label_width(" ", size).max(size * 0.25);
    let cell = code_advance(size * 0.92).max(1.0);
    let measure = |style: Style, text: &str| match style {
        Style::Code => text.chars().count() as f32 * cell,
        _ => label_width(text, size),
    };
    let mut cursor = 0.0;
    let mut placed = false;
    for (style, text) in spans {
        let ink = match style {
            Style::Text => color,
            Style::Strong => text_color(),
            Style::Code => token_color(Token::String),
            Style::Link => accent(),
        };
        // Words keep the space before them, so styles join up naturally.
        let words = text.split(' ');
        let mut first = true;
        for word in words {
            let gap = if first { 0.0 } else { space };
            first = false;
            if word.is_empty() {
                cursor += gap;
                continue;
            }
            let mut rest = word.to_owned();
            while !rest.is_empty() {
                let mut w = measure(*style, &rest);
                if placed && cursor + gap + w > width {
                    *y += line_h;
                    cursor = 0.0;
                } else {
                    cursor += gap;
                }
                // A word longer than the line breaks where it must.
                let mut piece = rest.clone();
                if w > width - cursor {
                    let mut fit = String::new();
                    for c in rest.chars() {
                        fit.push(c);
                        if measure(*style, &fit) > width - cursor && fit.chars().count() > 1 {
                            fit.pop();
                            break;
                        }
                    }
                    piece = fit;
                    w = measure(*style, &piece);
                }
                if *style == Style::Code {
                    items.push(Item::Rect(
                        Rect::new(
                            x + cursor - 2.0 * s,
                            *y + line_h * 0.16,
                            w + 4.0 * s,
                            line_h * 0.72,
                        ),
                        alpha(line_color(), 0.45),
                    ));
                }
                items.push(Item::Text {
                    x: x + cursor,
                    y: *y + line_h * 0.72,
                    size: if *style == Style::Code {
                        size * 0.92
                    } else {
                        size
                    },
                    text: piece.clone(),
                    color: ink,
                    mono: *style == Style::Code,
                });
                cursor += w;
                placed = true;
                rest = rest[piece.len()..].to_owned();
                if !rest.is_empty() {
                    *y += line_h;
                    cursor = 0.0;
                }
            }
        }
    }
    *y += line_h;
}

fn text_view(name: &str, text: &str, note: Option<String>) -> View {
    let language = Language::for_file(name);
    let mut lines = syntax::highlight(language, text);
    let mut note = note;
    if lines.len() > MAX_LINES {
        note = Some(format!(
            "Showing the first {MAX_LINES} of {} lines",
            lines.len()
        ));
        lines.truncate(MAX_LINES);
    }
    for line in &mut lines {
        if line_chars(line) > MAX_LINE_CHARS {
            let mut kept = 0;
            line.retain_mut(|(_, text)| {
                if kept >= MAX_LINE_CHARS {
                    return false;
                }
                let room = MAX_LINE_CHARS - kept;
                if text.chars().count() > room {
                    *text = text
                        .chars()
                        .take(room)
                        .chain(std::iter::once('…'))
                        .collect();
                }
                kept += text.chars().count();
                true
            });
        }
    }
    View::Text {
        language: language.name(),
        widest: lines.iter().map(line_chars).max().unwrap_or(0),
        lines,
        note,
    }
}

fn line_chars(line: &Line) -> usize {
    line.iter().map(|(_, text)| text.chars().count()).sum()
}

fn size(bytes: usize) -> String {
    match bytes {
        0..1024 => format!("{bytes} bytes"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f32 / 1024.0),
        _ => format!("{:.1} MB", bytes as f32 / 1_048_576.0),
    }
}

/// A short tag and color for a file in the tree.
fn file_badge(name: &str) -> (&'static str, Color) {
    let ext = tankyard_mods::assets::extension(name);
    let (tag, color) = match ext.as_str() {
        "lua" => ("LUA", token_color(Token::Keyword)),
        "json" | "gltf" => ("JSON", token_color(Token::Number)),
        "toml" => ("TOML", token_color(Token::Heading)),
        "md" | "markdown" | "txt" => ("TXT", muted_color()),
        "svg" => ("SVG", token_color(Token::String)),
        _ if tankyard_mods::assets::MODEL_EXTENSIONS.contains(&ext.as_str()) => {
            ("3D", token_color(Token::Function))
        }
        _ if tankyard_mods::assets::is_asset(name) => ("IMG", token_color(Token::String)),
        _ => ("BIN", muted_color()),
    };
    (tag, color)
}

/// Syntax colors, tuned separately for the dark and light themes.
fn token_color(token: Token) -> Color {
    let dark = is_dark();
    let pick = |d: [f32; 3], l: [f32; 3]| {
        let [r, g, b] = if dark { d } else { l };
        Color::new(r, g, b, 1.0)
    };
    match token {
        Token::Plain => text_color(),
        Token::Keyword | Token::Tag => pick([0.80, 0.56, 0.98], [0.52, 0.20, 0.72]),
        Token::Constant => pick([0.98, 0.55, 0.60], [0.75, 0.18, 0.28]),
        Token::Builtin => pick([0.40, 0.82, 0.90], [0.05, 0.47, 0.56]),
        Token::Function => pick([0.98, 0.84, 0.46], [0.60, 0.42, 0.00]),
        Token::String => pick([0.64, 0.86, 0.50], [0.23, 0.50, 0.12]),
        Token::Number => pick([0.98, 0.66, 0.40], [0.72, 0.35, 0.02]),
        Token::Comment => alpha(muted_color(), 0.85),
        Token::Key | Token::Heading => pick([0.50, 0.72, 0.99], [0.10, 0.35, 0.75]),
        Token::Punct => mix(text_color(), muted_color(), 0.55),
    }
}

/// A checkerboard with `texture` fitted in the middle. Small pixel art is
/// enlarged by whole steps so it stays crisp.
fn image(area: Rect, texture: &Texture2D, s: f32) {
    let tile = 12.0 * s;
    let light = mix(surface_color(), text_color(), 0.06);
    let mut y = area.y + 1.0;
    let mut row = 0;
    while y < area.bottom() - 1.0 {
        let h = tile.min(area.bottom() - 1.0 - y);
        let mut x = area.x + 1.0 + if row % 2 == 0 { 0.0 } else { tile };
        while x < area.right() - 1.0 {
            draw_rectangle(x, y, tile.min(area.right() - 1.0 - x), h, light);
            x += tile * 2.0;
        }
        y += tile;
        row += 1;
    }
    let room = vec2(area.w - 48.0 * s, area.h - 48.0 * s);
    let natural = vec2(texture.width(), texture.height());
    let fit = (room.x / natural.x).min(room.y / natural.y);
    let scale = if fit >= 2.0 {
        fit.floor()
    } else {
        fit.min(1.0)
    };
    texture.set_filter(if scale >= 2.0 {
        FilterMode::Nearest
    } else {
        FilterMode::Linear
    });
    let shown = natural * scale;
    let p = crate::ui_layout::position(vec2(
        area.center().x - shown.x * 0.5,
        area.center().y - shown.y * 0.5,
    ));
    draw_texture_ex(
        texture,
        p.x.round(),
        p.y.round(),
        WHITE,
        DrawTextureParams {
            dest_size: Some(shown),
            ..Default::default()
        },
    );
}

fn scrollbar(track: Rect, first: f32, visible: f32, total: f32) {
    draw_rectangle(
        track.x,
        track.y,
        track.w,
        track.h,
        alpha(line_color(), 0.35),
    );
    let h = (visible / total * track.h).max(track.w * 3.0);
    let y = track.y + first / (total - visible).max(1.0) * (track.h - h);
    draw_rectangle(track.x, y, track.w, h, alpha(muted_color(), 0.8));
}

fn scrollbar_horizontal(track: Rect, first: f32, visible: f32, total: f32) {
    draw_rectangle(
        track.x,
        track.y,
        track.w,
        track.h,
        alpha(line_color(), 0.35),
    );
    let w = (visible / total * track.w).max(track.h * 3.0);
    let x = track.x + first / (total - visible).max(1.0) * (track.w - w);
    draw_rectangle(x, track.y, w, track.h, alpha(muted_color(), 0.8));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const MANIFEST: &str = r#"{"api_version": 2, "id": "viewer_test", "name": "Viewer test", "version": "1.0", "scripts": ["main.lua"]}"#;

    /// A pack folder with `extra` files beside its manifest and script.
    fn folder(name: &str, extra: &[(&str, &[u8])]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("tankyard-viewer-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut files: Vec<(&str, &[u8])> = vec![
            ("mod.json", MANIFEST.as_bytes()),
            (
                "main.lua",
                b"local util = require(\"lib.util\")\nreturn util\n",
            ),
        ];
        files.extend_from_slice(extra);
        for (path, data) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, data).unwrap();
        }
        root
    }

    fn zipped(name: &str, extra: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("tankyard-viewer-{name}-{}.zip", std::process::id()));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let mut files: Vec<(&str, &[u8])> = vec![
            ("mod.json", MANIFEST.as_bytes()),
            ("main.lua", b"return 1\n"),
        ];
        files.extend_from_slice(extra);
        for (name, data) in files {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn open(path: &std::path::Path, focus: Option<(&str, usize)>) -> Viewer {
        Viewer::open("Viewer test", path.to_str().unwrap(), focus).unwrap()
    }

    fn select(viewer: &mut Viewer, name: &str) {
        let index = viewer.files.iter().position(|(n, _)| n == name).unwrap();
        viewer.select(index);
    }

    fn row_names(viewer: &Viewer) -> Vec<String> {
        viewer
            .rows
            .iter()
            .map(|row| match row {
                Row::Folder { path, .. } => path.clone(),
                Row::File { index, .. } => viewer.files[*index].0.clone(),
            })
            .collect()
    }

    #[test]
    fn folder_packs_open_on_their_script_with_folders_listed_first() {
        let root = folder(
            "tree",
            &[
                ("lib/util.lua", b"return {}\n"),
                ("data/blob.bin", &[0, 1, 2, 255]),
            ],
        );
        let viewer = open(&root, None);
        assert!(!viewer.archive);
        assert_eq!(viewer.files[viewer.selected].0, "main.lua");
        assert_eq!(
            row_names(&viewer),
            [
                "data/",
                "data/blob.bin",
                "lib/",
                "lib/util.lua",
                "main.lua",
                "mod.json"
            ]
        );
        let View::Text {
            language, lines, ..
        } = &viewer.view
        else {
            panic!("main.lua should open as text");
        };
        assert_eq!(*language, "Lua");
        assert_eq!(lines[0][0], (Token::Keyword, "local".to_owned()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn zip_packs_are_read_without_extracting() {
        let path = zipped("zip", &[("lib/util.lua", b"return {}\n")]);
        let mut viewer = open(&path, None);
        assert!(viewer.archive);
        select(&mut viewer, "lib/util.lua");
        assert!(matches!(
            &viewer.view,
            View::Text {
                language: "Lua",
                ..
            }
        ));
        // Nothing was unpacked next to the archive.
        assert!(!path.with_extension("").exists());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn binary_files_open_as_a_hex_dump() {
        let root = folder("hex", &[("data/blob.bin", &[0, 1, 2, 255])]);
        let mut viewer = open(&root, None);
        select(&mut viewer, "data/blob.bin");
        let View::Text {
            language, lines, ..
        } = &viewer.view
        else {
            panic!("binary should be a hex dump");
        };
        assert_eq!(*language, "Binary");
        assert!(lines[0][1].1.starts_with("00 01 02 ff"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_review_finding_opens_its_file_at_the_line() {
        let root = folder("focus", &[("lib/util.lua", b"a\nb\nc\nd\n")]);
        let viewer = open(&root, Some(("lib/util.lua", 3)));
        assert_eq!(viewer.files[viewer.selected].0, "lib/util.lua");
        assert_eq!(viewer.mark, Some(2));
        // A finding for a file the pack doesn't have still opens the pack.
        let viewer = open(&root, Some(("missing.lua", 1)));
        assert_eq!(viewer.files[viewer.selected].0, "main.lua");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn collapsed_folders_hide_their_files() {
        let root = folder(
            "collapse",
            &[("lib/util.lua", b""), ("lib/more/deep.lua", b"")],
        );
        let mut viewer = open(&root, None);
        assert!(row_names(&viewer).contains(&"lib/more/deep.lua".to_owned()));
        viewer.collapsed.insert("lib/".into());
        viewer.rebuild_rows();
        assert_eq!(row_names(&viewer), ["lib/", "main.lua", "mod.json"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn private_files_are_listed_but_not_shown() {
        let toml: &[u8] = b"[source]\nprivate = [\"lib/\"]\n";
        let root = folder(
            "private",
            &[("tankyard.toml", toml), ("lib/util.lua", b"return 42\n")],
        );
        let mut viewer = open(&root, None);
        assert!(row_names(&viewer).contains(&"lib/util.lua".to_owned()));
        assert_eq!(viewer.privacy_note(), " · 1 file is private");
        select(&mut viewer, "lib/util.lua");
        assert!(matches!(viewer.view, View::Private));
        // Opening at a finding inside a private file doesn't reveal it either.
        let viewer = open(&root, Some(("lib/util.lua", 1)));
        assert!(matches!(viewer.view, View::Private));
        let mut viewer = open(&root, None);
        select(&mut viewer, "main.lua");
        assert!(matches!(viewer.view, View::Text { .. }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_closed_pack_hides_every_file_including_art() {
        let toml: &[u8] = b"[source]\nviewable = false\n";
        let svg: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'/>";
        let path = zipped("closed", &[("tankyard.toml", toml), ("art/icon.svg", svg)]);
        let mut viewer = open(&path, None);
        assert_eq!(
            viewer.privacy_note(),
            " · the creator keeps these files private"
        );
        for index in 0..viewer.files.len() {
            viewer.select(index);
            assert!(
                matches!(viewer.view, View::Private),
                "{} was shown",
                viewer.files[index].0
            );
        }
        // Switching to the source of private art changes nothing.
        viewer.source = true;
        select(&mut viewer, "art/icon.svg");
        assert!(matches!(viewer.view, View::Private));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_broken_project_file_keeps_everything_private() {
        let root = folder(
            "broken",
            &[("tankyard.toml", b"[source]\nviewable = \"maybe\"\n")],
        );
        let viewer = open(&root, None);
        assert!(matches!(viewer.view, View::Private));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_pack_in_the_repository_opens_and_shows_its_files() {
        let mods = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../mods");
        let mut opened = 0;
        for pack in std::fs::read_dir(mods).unwrap().flatten() {
            let path = pack.path();
            if !path.join("mod.json").is_file() {
                continue;
            }
            let mut viewer = open(&path, None);
            opened += 1;
            for index in 0..viewer.files.len() {
                // Art previews need a graphics context; everything else loads here.
                if tankyard_mods::assets::is_asset(&viewer.files[index].0) {
                    continue;
                }
                viewer.select(index);
                let name = &viewer.files[index].0;
                match &viewer.view {
                    View::Text { lines, .. } => assert!(!lines.is_empty(), "{name}"),
                    View::Markdown { blocks, .. } => assert!(!blocks.is_empty(), "{name}"),
                    View::Private => assert!(!viewer.access.readable(name)),
                    _ => panic!("{name} in {} didn't open", path.display()),
                }
            }
        }
        assert!(opened >= 4, "only {opened} packs found");
    }

    #[test]
    fn selections_copy_exactly_what_is_highlighted() {
        let lines = ["local x = 1", "", "return x"];
        let sel = |anchor, head| Selection {
            block: CODE_VIEW,
            anchor,
            head,
        };
        assert_eq!(sel((0, 6), (0, 11)).text(&lines), "x = 1");
        // Dragging backwards selects the same text.
        assert_eq!(sel((2, 6), (0, 6)).text(&lines), "x = 1\n\nreturn");
        assert_eq!(sel((0, 0), (2, 8)).text(&lines), "local x = 1\n\nreturn x");
        // Columns past the end of a line stop at the line.
        assert_eq!(sel((0, 9), (0, 99)).text(&lines), " 1");
        assert!(sel((1, 0), (1, 0)).is_empty());
        assert_eq!(sel((1, 0), (1, 0)).text(&lines), "");
        // Wide characters count once each.
        assert_eq!(sel((0, 1), (0, 3)).text(&["añb"]), "ñb");
    }

    #[test]
    fn selection_highlights_line_breaks_inside_the_range_only() {
        let sel = Selection {
            block: 0,
            anchor: (0, 6),
            head: (2, 3),
        };
        assert_eq!(sel.columns(0, 11), Some((6, 12)));
        assert_eq!(sel.columns(1, 0), Some((0, 1)));
        assert_eq!(sel.columns(2, 8), Some((0, 3)));
        assert_eq!(sel.columns(3, 8), None);
        let empty = Selection {
            block: 0,
            anchor: (1, 2),
            head: (1, 2),
        };
        assert_eq!(empty.columns(1, 5), None);
    }

    #[test]
    fn markdown_opens_rendered_and_switches_to_raw() {
        let root = folder(
            "markdown",
            &[
                ("README.md", b"# Title\n\nSome `code`.\n"),
                ("art/a.svg", b"<svg/>"),
            ],
        );
        let mut viewer = open(&root, None);
        select(&mut viewer, "README.md");
        let View::Markdown { blocks, .. } = &viewer.view else {
            panic!("README.md should open rendered");
        };
        assert!(matches!(blocks[0], Block::Heading(1, _)));
        // The switch shows what's written, with Markdown colors.
        viewer.markdown_raw = true;
        select(&mut viewer, "README.md");
        let View::Text {
            language, lines, ..
        } = &viewer.view
        else {
            panic!("raw Markdown should be text");
        };
        assert_eq!(*language, "Markdown");
        assert_eq!(lines[0][0], (Token::Heading, "# Title".to_owned()));
        // Markdown and SVG remember their switches separately.
        assert!(!viewer.source);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn private_markdown_stays_private_in_both_modes() {
        let toml: &[u8] = b"[source]\nprivate = [\"*.md\"]\n";
        let root = folder(
            "private-md",
            &[("tankyard.toml", toml), ("NOTES.md", b"# secret\n")],
        );
        let mut viewer = open(&root, None);
        for raw in [false, true] {
            viewer.markdown_raw = raw;
            select(&mut viewer, "NOTES.md");
            assert!(matches!(viewer.view, View::Private));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn packs_that_cannot_be_read_report_why() {
        let missing = std::env::temp_dir().join("tankyard-viewer-does-not-exist.zip");
        assert!(Viewer::open("x", missing.to_str().unwrap(), None).is_err());
    }

    #[test]
    fn long_lines_are_cut_and_marked() {
        let View::Text { lines, widest, .. } =
            text_view("a.lua", &"x".repeat(MAX_LINE_CHARS + 50), None)
        else {
            panic!("expected text");
        };
        assert_eq!(widest, MAX_LINE_CHARS + 1);
        assert!(lines[0].last().unwrap().1.ends_with('…'));
    }

    #[test]
    fn huge_files_show_their_first_lines_with_a_note() {
        let View::Text { lines, note, .. } =
            text_view("a.txt", &"line\n".repeat(MAX_LINES + 10), None)
        else {
            panic!("expected text");
        };
        assert_eq!(lines.len(), MAX_LINES);
        assert!(note.unwrap().contains("first 20000"));
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size(512), "512 bytes");
        assert_eq!(size(2048), "2.0 KB");
        assert_eq!(size(3 * 1_048_576), "3.0 MB");
    }
}
