//! Apply display preferences through the window that actually owns this game.
use crate::{
    settings::Settings,
    window_control::{Area, WindowControl},
};
use macroquad::prelude::*;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
thread_local! { static CURRENT: Cell<Option<(u32,u32)>> = const { Cell::new(None) }; }
pub fn current() -> Option<(u32, u32)> {
    CURRENT.with(Cell::get)
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Monitor {
    id: u32,
    area: Area,
    pixels: (u32, u32),
    scale: f32,
}
#[cfg(target_os = "linux")]
fn displays() -> Vec<Monitor> {
    use xcb::{Xid, randr};
    let Ok((conn, index)) = xcb::Connection::connect(None) else {
        return vec![];
    };
    let Some(screen) = conn.get_setup().roots().nth(index as usize) else {
        return vec![];
    };
    let Ok(reply) = conn.wait_for_reply(conn.send_request(&randr::GetMonitors {
        window: screen.root(),
        get_active: true,
    })) else {
        return vec![];
    };
    // XRandR gives exact native pixels. display-info divides X11 coordinates
    // by Xft.dpi, which cannot be compared to native window coordinates.
    reply
        .monitors()
        .filter_map(|monitor| {
            let pixels = (u32::from(monitor.width()), u32::from(monitor.height()));
            ((640..=15360).contains(&pixels.0) && (480..=8640).contains(&pixels.1)).then_some(
                Monitor {
                    id: monitor.name().resource_id(),
                    area: (
                        i32::from(monitor.x()),
                        i32::from(monitor.y()),
                        pixels.0,
                        pixels.1,
                    ),
                    pixels,
                    scale: screen_dpi_scale().max(1.),
                },
            )
        })
        .collect()
}
#[cfg(not(target_os = "linux"))]
fn displays() -> Vec<Monitor> {
    display_info::DisplayInfo::all()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|d| {
            let scale = if d.scale_factor.is_finite() {
                d.scale_factor.clamp(0.5, 8.)
            } else {
                1.
            };
            let pixels = (d.width, d.height);
            #[cfg(target_os = "macos")]
            let pixels = (
                (d.width as f32 * scale).round() as u32,
                (d.height as f32 * scale).round() as u32,
            );
            ((640..=15360).contains(&pixels.0) && (480..=8640).contains(&pixels.1)).then_some(
                Monitor {
                    id: d.id,
                    area: (d.x, d.y, d.width, d.height),
                    pixels,
                    scale,
                },
            )
        })
        .collect()
}
fn overlap_area(a: Area, b: Area) -> u64 {
    let w = (i64::from(a.0) + i64::from(a.2)).min(i64::from(b.0) + i64::from(b.2))
        - i64::from(a.0.max(b.0));
    let h = (i64::from(a.1) + i64::from(a.3)).min(i64::from(b.1) + i64::from(b.3))
        - i64::from(a.1.max(b.1));
    w.max(0) as u64 * h.max(0) as u64
}
fn select(displays: &[Monitor], area: Area) -> Option<Monitor> {
    displays
        .iter()
        .enumerate()
        .max_by_key(|(index, d)| (overlap_area(area, d.area), std::cmp::Reverse(*index)))
        .map(|(_, d)| *d)
}
fn windowed_area(current: Area, monitor: Monitor, pixels: (u32, u32)) -> Area {
    let (mw, mh) = monitor.pixels;
    let width = pixels.0.min(mw.saturating_sub(32).max(320));
    let height = pixels.1.min(mh.saturating_sub(64).max(240));
    #[cfg(target_os = "macos")]
    let (aw, ah) = (
        (width as f32 / monitor.scale) as u32,
        (height as f32 / monitor.scale) as u32,
    );
    #[cfg(not(target_os = "macos"))]
    let (aw, ah) = (width, height);
    let x = i64::from(current.0).clamp(
        i64::from(monitor.area.0),
        i64::from(monitor.area.0) + i64::from(monitor.area.2.saturating_sub(aw)),
    );
    let y = i64::from(current.1).clamp(
        i64::from(monitor.area.1) + 24,
        i64::from(monitor.area.1) + i64::from(monitor.area.3.saturating_sub(ah)).max(24),
    );
    (x as i32, y as i32, width, height)
}
#[derive(Clone, Copy, PartialEq)]
struct Choice {
    native: bool,
    fullscreen: bool,
    size: (u32, u32),
    revision: u64,
    monitor: Option<u32>,
}
pub struct NativeResolution {
    window: WindowControl,
    monitor: Option<Monitor>,
    applied: Option<Choice>,
    next_query: Instant,
    windowed: Option<Area>,
    pending: Option<(Instant, Area)>,
}
impl NativeResolution {
    pub fn new(settings: &Settings) -> Self {
        let window = WindowControl::new();
        if let Some([x, y]) = settings.window_position {
            let area = window.area();
            let desired = (x, y, area.2, area.3);
            if displays()
                .iter()
                .any(|monitor| overlap_area(desired, monitor.area) > 0)
            {
                window.geometry(desired);
            }
        }
        let mut value = Self {
            window,
            monitor: None,
            applied: None,
            next_query: Instant::now(),
            windowed: None,
            pending: None,
        };
        value.update(settings, 0);
        value
    }
    pub fn remember(&self, settings: &mut Settings) {
        let area = if settings.fullscreen {
            self.windowed.unwrap_or_else(|| self.window.area())
        } else {
            self.window.area()
        };
        settings.window_position = Some([area.0, area.1]);
    }
    pub fn update(&mut self, settings: &Settings, revision: u64) {
        if self.pending.is_some_and(|(at, _)| Instant::now() >= at)
            && let Some((_, area)) = self.pending.take()
        {
            self.window.geometry(area);
        }
        let changed = self.applied.is_none_or(|old| {
            old.native != settings.native_resolution
                || old.fullscreen != settings.fullscreen
                || (!settings.native_resolution && old.size != (settings.width, settings.height))
                || old.revision != revision
        });
        if !changed && Instant::now() < self.next_query {
            return;
        }
        self.next_query = Instant::now() + Duration::from_secs(1);
        let actual = self.window.area();
        // Keep the selected display while fullscreen; never follow an unwanted
        // backend move onto the primary display. Re-select if a display vanishes.
        let area = if settings.fullscreen && self.applied.is_some_and(|old| old.fullscreen) {
            self.monitor.map_or(actual, |m| m.area)
        } else {
            actual
        };
        let previous = self.monitor;
        self.monitor = select(&displays(), area);
        if previous != self.monitor {
            self.window.cursor(
                self.monitor
                    .map_or(screen_dpi_scale(), |m| m.scale.max(screen_dpi_scale())),
            );
        }
        CURRENT.with(|value| value.set(self.monitor.map(|m| m.pixels)));
        let size = if settings.native_resolution {
            self.monitor
                .map_or((settings.width, settings.height), |m| m.pixels)
        } else {
            (settings.width, settings.height)
        };
        let choice = Choice {
            native: settings.native_resolution,
            fullscreen: settings.fullscreen,
            size,
            revision,
            monitor: if settings.fullscreen || settings.native_resolution {
                self.monitor.map(|m| m.id)
            } else {
                None
            },
        };
        if self.applied == Some(choice) {
            return;
        }
        crate::ui::cancel_pointer_capture();
        let was_fullscreen = self.applied.is_some_and(|old| old.fullscreen);
        if settings.fullscreen {
            if !was_fullscreen {
                self.windowed = Some(actual);
            }
            let monitor = self.monitor.map_or(actual, |m| m.area);
            self.window.fullscreen(true, monitor);
        } else {
            if was_fullscreen {
                self.window
                    .fullscreen(false, self.monitor.map_or(actual, |m| m.area));
            }
            let anchor = if was_fullscreen {
                self.windowed.take().unwrap_or(actual)
            } else {
                actual
            };
            let area = self
                .monitor
                .map_or((anchor.0, anchor.1, size.0, size.1), |m| {
                    windowed_area(anchor, m, size)
                });
            self.window.geometry(area);
            // Some WMs process fullscreen removal asynchronously. One bounded
            // retry restores the requested client area after decorations return.
            if was_fullscreen {
                self.pending = Some((Instant::now() + Duration::from_millis(180), area));
            }
        }
        log::info!(
            "Display applied: fullscreen={}, native={}, requested={}x{}, monitor={:?}",
            choice.fullscreen,
            choice.native,
            size.0,
            size.1,
            choice.monitor
        );
        self.applied = Some(choice);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn monitor(id: u32, x: i32, y: i32, w: u32, h: u32) -> Monitor {
        Monitor {
            id,
            area: (x, y, w, h),
            pixels: (w, h),
            scale: 1.,
        }
    }
    #[test]
    fn monitor_selection_keeps_negative_and_secondary_displays() {
        let displays = [
            monitor(1, 0, 0, 1920, 1080),
            monitor(2, -2560, -100, 2560, 1440),
        ];
        assert_eq!(select(&displays, (-2200, 0, 1280, 800)).unwrap().id, 2);
        assert_eq!(select(&displays, (0, 0, 1280, 800)).unwrap().id, 1);
        assert_eq!(
            overlap_area((-1800, 100, 1280, 800), (-1920, 0, 1920, 1080)),
            1280 * 800
        );
        assert_eq!(
            overlap_area((0, 0, 15360, 8640), (0, 0, 15360, 8640)),
            15360 * 8640
        );
    }
    #[test]
    fn windowed_sizes_fit_the_chosen_display_and_preserve_negative_origins() {
        let m = monitor(2, -1920, 0, 1920, 1080);
        let area = windowed_area((-1800, 100, 1280, 800), m, (7680, 4320));
        assert_eq!((area.2, area.3), (1888, 1016));
        assert!(area.0 < 0 && area.0 >= -1920);
        assert_eq!(
            windowed_area((-1800, 100, 1280, 800), m, (1280, 800)),
            (-1800, 100, 1280, 800)
        );
    }
}
