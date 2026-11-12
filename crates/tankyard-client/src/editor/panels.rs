//! Panel sizes and view switches the workshop remembers between sessions,
//! and the panel edges that drag to resize the inspector and the dock.
use super::{Editor, Layout, Mode, directory};
use crate::ui::*;
use macroquad::{miniquad::CursorIcon, miniquad::window::set_mouse_cursor, prelude::*};
use serde::{Deserialize, Serialize};

/// Inspector widths and dock heights, before UI scaling.
pub const INSPECTOR: [f32; 2] = [260., 760.];
pub const DOCK: [f32; 2] = [112., 300.];
const INSPECTOR_DEFAULT: f32 = 352.;
const DOCK_DEFAULT: f32 = 152.;
/// How far from an edge, in pixels, the pointer can grab it.
const GRAB: f32 = 7.;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub inspector: f32,
    pub dock: f32,
    /// Canvas only: the dock and inspector are out of the way.
    pub hide_panels: bool,
    pub grid: f32,
    pub snap: bool,
    pub step: usize,
    pub show_groups: bool,
    pub show_links: bool,
    /// Plugins whose tools are put away in the Tools tab, by pack id.
    pub folded: Vec<String>,
    /// What scrolling over the canvas does.
    pub scroll: ScrollMode,
    pub pan_speed: f32,
    pub zoom_speed: f32,
}

/// Whether scrolling over the canvas pans or zooms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollMode {
    /// Pan with precise trackpad scrolling, zoom with a wheel's steps.
    #[default]
    Auto,
    /// Scrolling pans; Ctrl or Cmd zooms.
    Trackpad,
    /// Scrolling zooms.
    Mouse,
}
impl ScrollMode {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Trackpad, Self::Mouse];
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Trackpad => "Trackpad",
            Self::Mouse => "Mouse",
        }
    }
    /// Whether this frame's scroll pans rather than zooms.
    pub fn pans(self) -> bool {
        match self {
            Self::Auto => crate::ui_layout::precise_scroll(),
            Self::Trackpad => true,
            Self::Mouse => false,
        }
    }
}
impl Default for Prefs {
    fn default() -> Self {
        Self {
            inspector: INSPECTOR_DEFAULT,
            dock: DOCK_DEFAULT,
            hide_panels: false,
            grid: 40.,
            snap: true,
            step: 2,
            show_groups: true,
            show_links: false,
            folded: Vec::new(),
            scroll: ScrollMode::Auto,
            pan_speed: 1.,
            zoom_speed: 1.,
        }
    }
}
impl Prefs {
    pub fn load() -> Self {
        let mut prefs: Self = directory()
            .ok()
            .and_then(|dir| std::fs::read(dir.join("prefs.json")).ok())
            .filter(|bytes| bytes.len() < 16 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        // A hand-edited file still has to give a usable workshop.
        let fallback = Self::default();
        prefs.inspector = finite_in(prefs.inspector, INSPECTOR, fallback.inspector);
        prefs.dock = finite_in(prefs.dock, DOCK, fallback.dock);
        prefs.pan_speed = finite_in(prefs.pan_speed, [0.25, 3.], 1.);
        prefs.zoom_speed = finite_in(prefs.zoom_speed, [0.25, 3.], 1.);
        if !super::dock::GRIDS.contains(&prefs.grid) {
            prefs.grid = fallback.grid;
        }
        if prefs.step >= super::dock::STEPS.len() {
            prefs.step = fallback.step;
        }
        prefs.folded.retain(|id| !id.is_empty() && id.len() <= 64);
        prefs.folded.truncate(64);
        prefs
    }
    fn save(&self) -> Result<(), String> {
        let path = directory()?.join("prefs.json");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
}
fn finite_in(value: f32, [lo, hi]: [f32; 2], fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_preferences_get_navigation_defaults() {
        let prefs: Prefs =
            serde_json::from_str(r#"{"inspector":300,"grid":80,"snap":false}"#).unwrap();
        assert_eq!(prefs.inspector, 300.);
        assert_eq!(prefs.grid, 80.);
        assert!(!prefs.snap);
        assert_eq!(prefs.scroll, ScrollMode::Auto);
        assert_eq!(prefs.pan_speed, 1.);
        assert_eq!(prefs.zoom_speed, 1.);
    }
}

/// A panel edge that drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Inspector,
    Dock,
}

impl Editor {
    pub(super) fn layout(&self, s: f32) -> Layout {
        let (w, h) = (screen_width(), screen_height());
        let top = 54. * s;
        let (dock, right) = if self.prefs.hide_panels {
            (0., 0.)
        } else {
            (
                (self.prefs.dock * s).min((h - top) * 0.5),
                (self.prefs.inspector * s).min(w * 0.6),
            )
        };
        let middle = (h - top - dock).max(80.);
        Layout {
            top: Rect::new(0., 0., w, top),
            canvas: Rect::new(0., top, (w - right).max(80.), middle),
            dock: Rect::new(0., h - dock, w, dock),
            inspector: Rect::new(w - right, top, right, middle),
            s,
        }
    }

    /// The edge under the pointer, if one could be dragged now.
    fn edge_at(&self, layout: &Layout, p: Vec2) -> Option<Edge> {
        if self.prefs.hide_panels || self.help {
            return None;
        }
        let inspector = layout.inspector;
        if (p.x - inspector.x).abs() <= GRAB && p.y > inspector.y && p.y < inspector.bottom() {
            Some(Edge::Inspector)
        } else if (p.y - layout.dock.y).abs() <= GRAB {
            Some(Edge::Dock)
        } else {
            None
        }
    }

    /// Drag panel edges; a double click puts one back. Returns whether the
    /// pointer belongs to an edge this frame, so nothing under it reacts.
    pub(super) fn panel_edges(&mut self, layout: &Layout) -> bool {
        let p = crate::ui_layout::pointer();
        let busy = self.gesture.is_some() || self.panning.is_some();
        let edge = match self.resizing {
            Some(edge) if is_mouse_button_down(MouseButton::Left) => {
                let s = layout.s;
                match edge {
                    Edge::Inspector => {
                        self.prefs.inspector =
                            ((screen_width() - p.x) / s).clamp(INSPECTOR[0], INSPECTOR[1]);
                    }
                    Edge::Dock => {
                        self.prefs.dock = ((screen_height() - p.y) / s).clamp(DOCK[0], DOCK[1]);
                    }
                }
                Some(edge)
            }
            Some(_) => {
                self.resizing = None;
                None
            }
            // A button held since before reaching the edge belongs elsewhere.
            None if busy
                || (is_mouse_button_down(MouseButton::Left)
                    && !is_mouse_button_pressed(MouseButton::Left)) =>
            {
                None
            }
            None => {
                let over = self.edge_at(layout, p);
                if let Some(edge) = over
                    && is_mouse_button_pressed(MouseButton::Left)
                {
                    let now = get_time();
                    if now - self.edge_click < 0.35 {
                        let fallback = Prefs::default();
                        match edge {
                            Edge::Inspector => self.prefs.inspector = fallback.inspector,
                            Edge::Dock => self.prefs.dock = fallback.dock,
                        }
                    } else {
                        self.resizing = Some(edge);
                    }
                    self.edge_click = now;
                }
                over
            }
        };
        let cursor = match edge {
            Some(Edge::Inspector) => CursorIcon::EWResize,
            Some(Edge::Dock) => CursorIcon::NSResize,
            None if self.mode == Mode::Pan && layout.canvas.contains(p) => CursorIcon::Move,
            None => CursorIcon::Default,
        };
        if self.cursor != cursor {
            self.cursor = cursor;
            set_mouse_cursor(cursor);
        }
        self.edge_lit = edge;
        edge.is_some()
    }

    /// Put the pointer back to normal on the way out of the workshop.
    pub(super) fn release_cursor(&mut self) {
        if self.cursor != CursorIcon::Default {
            self.cursor = CursorIcon::Default;
            set_mouse_cursor(CursorIcon::Default);
        }
    }

    /// Highlight the edge being dragged or about to be.
    pub(super) fn draw_edges(&self, layout: &Layout) {
        let Some(edge) = self.edge_lit else {
            return;
        };
        let color = alpha(accent(), if self.resizing.is_some() { 0.9 } else { 0.6 });
        let s = layout.s;
        match edge {
            Edge::Inspector => {
                let r = layout.inspector;
                draw_line(r.x, r.y, r.x, r.bottom(), 3., color);
                let mid = r.center().y;
                for dy in [-8., 0., 8.] {
                    draw_circle(r.x, mid + dy * s, 2.2 * s, color);
                }
            }
            Edge::Dock => {
                let r = layout.dock;
                draw_line(r.x, r.y, r.right(), r.y, 3., color);
                let mid = layout.canvas.center().x;
                for dx in [-8., 0., 8.] {
                    draw_circle(mid + dx * s, r.y, 2.2 * s, color);
                }
            }
        }
    }

    pub(super) fn prefs_now(&self) -> Prefs {
        Prefs {
            grid: self.grid,
            snap: self.snap,
            step: self.step,
            show_groups: self.show_groups,
            show_links: self.show_links,
            ..self.prefs.clone()
        }
    }

    /// Write the preferences a second after they settle.
    pub(super) fn keep_prefs(&mut self) {
        if self.resizing.is_some() || get_time() - self.prefs_checked < 1. {
            return;
        }
        self.prefs_checked = get_time();
        let now = self.prefs_now();
        if now != self.prefs_saved {
            if let Err(error) = now.save() {
                log::warn!("workshop preferences were not saved: {error}");
            }
            self.prefs_saved = now;
        }
    }

    pub(super) fn toggle_panels(&mut self) {
        self.prefs.hide_panels = !self.prefs.hide_panels;
        self.resizing = None;
        self.say(if self.prefs.hide_panels {
            "Panels hidden. Press \\ to bring them back."
        } else {
            "Panels shown. Drag their edges to resize them."
        });
    }
}
