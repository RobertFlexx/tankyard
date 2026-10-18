//! Monitor queries stay off the render thread except AppKit's main-thread API.
use crate::settings::Settings;
use macroquad::prelude::*;
#[cfg(not(target_os = "macos"))]
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

thread_local! { static CURRENT: Cell<Option<(u32, u32)>> = const { Cell::new(None) }; }
pub fn current() -> Option<(u32, u32)> {
    CURRENT.with(Cell::get)
}

#[derive(Clone, Copy, PartialEq)]
struct Monitor {
    id: u32,
    width: u32,
    height: u32,
}

type WindowArea = (i32, i32, u32, u32);
fn query(area: WindowArea) -> Option<Monitor> {
    let displays = display_info::DisplayInfo::all().ok()?;
    let display = displays.iter().max_by_key(|d| {
        let overlap = overlap_area(area, (d.x, d.y, d.width, d.height));
        (overlap, d.is_primary)
    })?;
    let (width, height) = (display.width, display.height);
    // CoreGraphics reports points; other backends report pixels.
    #[cfg(target_os = "macos")]
    let (width, height) = (
        (width as f32 * display.scale_factor).round() as u32,
        (height as f32 * display.scale_factor).round() as u32,
    );
    ((640..=15360).contains(&width) && (480..=8640).contains(&height)).then_some(Monitor {
        id: display.id,
        width,
        height,
    })
}
fn overlap_area(a: WindowArea, b: WindowArea) -> u64 {
    let width = (i64::from(a.0) + i64::from(a.2)).min(i64::from(b.0) + i64::from(b.2))
        - i64::from(a.0.max(b.0));
    let height = (i64::from(a.1) + i64::from(a.3)).min(i64::from(b.1) + i64::from(b.3))
        - i64::from(a.1.max(b.1));
    width.max(0) as u64 * height.max(0) as u64
}

pub struct NativeResolution {
    monitor: Option<Monitor>,
    applied: Option<(bool, bool, u32, u32)>,
    next_query: Instant,
    #[cfg(not(target_os = "macos"))]
    request: Option<SyncSender<WindowArea>>,
    #[cfg(not(target_os = "macos"))]
    results: Receiver<Option<Monitor>>,
}
impl NativeResolution {
    pub fn new() -> Self {
        #[cfg(not(target_os = "macos"))]
        let (request, results) = {
            let (request, requests) = mpsc::sync_channel(1);
            let (result, results) = mpsc::sync_channel(1);
            let worker = std::thread::Builder::new()
                .name("display-info".into())
                .spawn(move || {
                    while let Ok(area) = requests.recv() {
                        if result.send(query(area)).is_err() {
                            break;
                        }
                    }
                });
            (worker.ok().map(|_| request), results)
        };
        Self {
            monitor: None,
            applied: None,
            next_query: Instant::now(),
            #[cfg(not(target_os = "macos"))]
            request,
            #[cfg(not(target_os = "macos"))]
            results,
        }
    }
    pub fn update(&mut self, settings: &Settings) {
        if Instant::now() >= self.next_query {
            self.next_query = Instant::now() + Duration::from_secs(1);
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            let (x, y) = macroquad::miniquad::window::get_window_position();
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            let (x, y) = (0_u32, 0_u32);
            let dpi = screen_dpi_scale().max(1.);
            let area = (
                x as i32,
                y as i32,
                (screen_width() * dpi) as u32,
                (screen_height() * dpi) as u32,
            );
            #[cfg(target_os = "macos")]
            {
                self.monitor = query(area);
            }
            #[cfg(not(target_os = "macos"))]
            if let Some(request) = &self.request {
                let _ = request.try_send(area);
            }
        }
        #[cfg(not(target_os = "macos"))]
        while let Ok(monitor) = self.results.try_recv() {
            self.monitor = monitor;
        }
        CURRENT.with(|value| value.set(self.monitor.map(|m| (m.width, m.height))));
        let size = if settings.native_resolution {
            let Some(monitor) = self.monitor else {
                return;
            };
            (monitor.width, monitor.height)
        } else {
            (settings.width, settings.height)
        };
        let applied = (
            settings.native_resolution,
            settings.fullscreen,
            size.0,
            size.1,
        );
        if self.applied == Some(applied) {
            return;
        }
        let initial = self.applied.is_none();
        self.applied = Some(applied);
        if settings.fullscreen || (initial && !settings.native_resolution) {
            return;
        }
        let dpi = screen_dpi_scale().max(1.);
        let (width, height) = if settings.native_resolution {
            (size.0 as f32 / dpi, size.1 as f32 / dpi)
        } else {
            (size.0 as f32, size.1 as f32)
        };
        #[cfg(target_os = "macos")]
        macroquad::miniquad::window::set_window_size(width as u32, height as u32);
        #[cfg(not(target_os = "macos"))]
        request_new_screen_size(width, height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_selection_handles_negative_coordinates_and_large_desktops() {
        assert_eq!(
            overlap_area((-1800, 100, 1280, 800), (-1920, 0, 1920, 1080)),
            1280 * 800
        );
        assert_eq!(overlap_area((-1800, 100, 1280, 800), (0, 0, 1920, 1080)), 0);
        assert_eq!(
            overlap_area((0, 0, 15360, 8640), (0, 0, 15360, 8640)),
            15360 * 8640
        );
    }
}
