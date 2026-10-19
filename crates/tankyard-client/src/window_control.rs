//! Operations target this process's window only. Native coordinates are pixels
//! on X11/Windows and desktop points on macOS; rendering stays in Macroquad units.
use std::sync::OnceLock;
pub type Area = (i32, i32, u32, u32);
static TITLE: OnceLock<String> = OnceLock::new();
#[cfg(target_os = "linux")]
static BOOTSTRAP: OnceLock<String> = OnceLock::new();
pub fn bootstrap_title(title: &str) -> String {
    TITLE.get_or_init(|| title.to_owned());
    #[cfg(target_os = "linux")]
    {
        use std::hash::BuildHasher;
        BOOTSTRAP
            .get_or_init(|| {
                format!(
                    "TankYard-startup-{:016x}",
                    std::collections::hash_map::RandomState::new().hash_one(std::process::id())
                )
            })
            .clone()
    }
    #[cfg(not(target_os = "linux"))]
    {
        title.to_owned()
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use xcb::{Xid, x};
    pub struct Window {
        conn: xcb::Connection,
        root: x::Window,
        window: x::Window,
        restore: Option<Area>,
        cursor: Option<ThemedCursor>,
    }
    impl Window {
        fn atom(conn: &xcb::Connection, name: &[u8]) -> Option<x::Atom> {
            conn.wait_for_reply(conn.send_request(&x::InternAtom {
                only_if_exists: false,
                name,
            }))
            .ok()
            .map(|r| r.atom())
        }
        pub fn new() -> Option<Self> {
            let (conn, screen) = xcb::Connection::connect(None).ok()?;
            let root = conn.get_setup().roots().nth(screen as usize)?.root();
            let title = BOOTSTRAP.get()?;
            let mut pending = vec![root];
            let mut found = None;
            for _ in 0..2048 {
                let Some(window) = pending.pop() else {
                    break;
                };
                if let Ok(name) = conn.wait_for_reply(conn.send_request(&x::GetProperty {
                    delete: false,
                    window,
                    property: x::ATOM_WM_NAME,
                    r#type: x::ATOM_ANY,
                    long_offset: 0,
                    long_length: 256,
                })) && name.format() == 8
                    && name.value::<u8>() == title.as_bytes()
                {
                    found = Some(window);
                    break;
                }
                if let Ok(tree) = conn.wait_for_reply(conn.send_request(&x::QueryTree { window })) {
                    pending.extend(
                        tree.children()
                            .iter()
                            .copied()
                            .take(2048 - pending.len().min(2048)),
                    );
                }
            }
            let window = found?;
            let real_title = TITLE.get()?.as_bytes();
            conn.send_request(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: x::ATOM_WM_NAME,
                r#type: x::ATOM_STRING,
                data: real_title,
            });
            if let (Some(property), Some(utf8)) = (
                Self::atom(&conn, b"_NET_WM_NAME"),
                Self::atom(&conn, b"UTF8_STRING"),
            ) {
                conn.send_request(&x::ChangeProperty {
                    mode: x::PropMode::Replace,
                    window,
                    property,
                    r#type: utf8,
                    data: real_title,
                });
            }
            let _ = conn.flush();
            Some(Self {
                conn,
                root,
                window,
                restore: None,
                cursor: None,
            })
        }
        pub fn area(&self) -> Option<Area> {
            let geometry = self
                .conn
                .wait_for_reply(self.conn.send_request(&x::GetGeometry {
                    drawable: x::Drawable::Window(self.window),
                }))
                .ok()?;
            let position = self
                .conn
                .wait_for_reply(self.conn.send_request(&x::TranslateCoordinates {
                    src_window: self.window,
                    dst_window: self.root,
                    src_x: 0,
                    src_y: 0,
                }))
                .ok()?;
            Some((
                i32::from(position.dst_x()),
                i32::from(position.dst_y()),
                u32::from(geometry.width()),
                u32::from(geometry.height()),
            ))
        }
        pub fn geometry(&self, area: Area) {
            self.conn.send_request(&x::ConfigureWindow {
                window: self.window,
                value_list: &[
                    x::ConfigWindow::X(area.0),
                    x::ConfigWindow::Y(area.1),
                    x::ConfigWindow::Width(area.2),
                    x::ConfigWindow::Height(area.3),
                ],
            });
            let _ = self.conn.flush();
        }
        pub fn fullscreen(&mut self, enabled: bool, monitor: Area) {
            if enabled && self.restore.is_none() {
                self.restore = self.area();
            }
            let wm = Self::atom(&self.conn, b"_NET_SUPPORTING_WM_CHECK")
                .and_then(|property| {
                    self.conn
                        .wait_for_reply(self.conn.send_request(&x::GetProperty {
                            delete: false,
                            window: self.root,
                            property,
                            r#type: x::ATOM_ANY,
                            long_offset: 0,
                            long_length: 1,
                        }))
                        .ok()
                })
                .is_some_and(|reply| {
                    reply.format() == 32 && !reply.value::<x::Window>().is_empty()
                });
            if wm {
                if let (Some(state), Some(full)) = (
                    Self::atom(&self.conn, b"_NET_WM_STATE"),
                    Self::atom(&self.conn, b"_NET_WM_STATE_FULLSCREEN"),
                ) {
                    let event = x::ClientMessageEvent::new(
                        self.window,
                        state,
                        x::ClientMessageData::Data32([
                            u32::from(enabled),
                            full.resource_id(),
                            0,
                            1,
                            0,
                        ]),
                    );
                    // EWMH changes state without Miniquad's unmap/remap, which
                    // can move the window to the WM's primary monitor.
                    self.conn.send_request(&x::SendEvent {
                        propagate: false,
                        destination: x::SendEventDest::Window(self.root),
                        event_mask: x::EventMask::SUBSTRUCTURE_REDIRECT
                            | x::EventMask::SUBSTRUCTURE_NOTIFY,
                        event: &event,
                    });
                }
            } else if enabled {
                self.geometry(monitor);
            }
            if !enabled && let Some(area) = self.restore.take() {
                self.geometry(area);
            }
            let _ = self.conn.flush();
        }
        pub fn cursor(&mut self, dpi: f32) {
            self.cursor = ThemedCursor::new(self.window.resource_id(), dpi);
            if self.cursor.is_none() {
                // Inherit the desktop cursor instead of X's tiny bitmap font.
                self.conn.send_request(&x::ChangeWindowAttributes {
                    window: self.window,
                    value_list: &[x::Cw::Cursor(x::Cursor::none())],
                });
                let _ = self.conn.flush();
            }
        }
    }
    struct ThemedCursor {
        _x11: libloading::Library,
        _xcursor: libloading::Library,
        display: *mut std::ffi::c_void,
        cursor: std::ffi::c_ulong,
        close: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
        free: unsafe extern "C" fn(*mut std::ffi::c_void, std::ffi::c_ulong) -> i32,
    }
    impl ThemedCursor {
        fn new(window: u32, dpi: f32) -> Option<Self> {
            // Libraries and function pointers stay alive together. Xlib owns
            // this separate connection; the cursor is assigned only to our XID.
            unsafe {
                let x11 = libloading::Library::new("libX11.so.6").ok()?;
                let xcursor = libloading::Library::new("libXcursor.so.1").ok()?;
                let open = *x11
                    .get::<unsafe extern "C" fn(*const std::ffi::c_char) -> *mut std::ffi::c_void>(
                        b"XOpenDisplay\0",
                    )
                    .ok()?;
                let close = *x11
                    .get::<unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>(b"XCloseDisplay\0")
                    .ok()?;
                let free = *x11
                    .get::<unsafe extern "C" fn(*mut std::ffi::c_void, std::ffi::c_ulong) -> i32>(
                        b"XFreeCursor\0",
                    )
                    .ok()?;
                let define = *x11
                    .get::<unsafe extern "C" fn(
                        *mut std::ffi::c_void,
                        std::ffi::c_ulong,
                        std::ffi::c_ulong,
                    ) -> i32>(b"XDefineCursor\0")
                    .ok()?;
                let flush = *x11
                    .get::<unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>(b"XFlush\0")
                    .ok()?;
                let load = *xcursor
                    .get::<unsafe extern "C" fn(
                        *mut std::ffi::c_void,
                        *const std::ffi::c_char,
                    ) -> std::ffi::c_ulong>(b"XcursorLibraryLoadCursor\0")
                    .ok()?;
                let set_size = *xcursor
                    .get::<unsafe extern "C" fn(*mut std::ffi::c_void, i32) -> i32>(
                        b"XcursorSetDefaultSize\0",
                    )
                    .ok()?;
                let get_size = *xcursor
                    .get::<unsafe extern "C" fn(*mut std::ffi::c_void) -> i32>(
                        b"XcursorGetDefaultSize\0",
                    )
                    .ok()?;
                let display = open(std::ptr::null());
                if display.is_null() {
                    return None;
                }
                if std::env::var_os("XCURSOR_SIZE").is_none() {
                    set_size(
                        display,
                        get_size(display).max((24. * dpi.clamp(1., 4.)).round() as i32),
                    );
                }
                let cursor = load(display, c"left_ptr".as_ptr());
                if cursor == 0 {
                    close(display);
                    return None;
                }
                define(display, window.into(), cursor);
                flush(display);
                Some(Self {
                    _x11: x11,
                    _xcursor: xcursor,
                    display,
                    cursor,
                    close,
                    free,
                })
            }
        }
    }
    impl Drop for ThemedCursor {
        fn drop(&mut self) {
            unsafe {
                (self.free)(self.display, self.cursor);
                (self.close)(self.display);
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::*,
    };
    use windows_sys::core::BOOL;
    pub struct Window {
        hwnd: HWND,
        restore: Option<(Area, isize)>,
    }
    unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> BOOL {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == std::process::id() && IsWindowVisible(hwnd) != 0 {
                *(data as *mut HWND) = hwnd;
                return 0;
            }
        }
        1
    }
    impl Window {
        pub fn new() -> Option<Self> {
            let mut hwnd: HWND = std::ptr::null_mut();
            unsafe {
                EnumWindows(Some(find), std::ptr::addr_of_mut!(hwnd) as LPARAM);
            }
            (!hwnd.is_null()).then_some(Self {
                hwnd,
                restore: None,
            })
        }
        pub fn area(&self) -> Option<Area> {
            unsafe {
                let mut rect: RECT = std::mem::zeroed();
                let mut point = POINT { x: 0, y: 0 };
                if GetClientRect(self.hwnd, &mut rect) == 0
                    || ClientToScreen(self.hwnd, &mut point) == 0
                {
                    return None;
                }
                Some((
                    point.x,
                    point.y,
                    (rect.right - rect.left).max(1) as u32,
                    (rect.bottom - rect.top).max(1) as u32,
                ))
            }
        }
        pub fn geometry(&self, area: Area) {
            unsafe {
                let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32;
                let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) as u32;
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: area.2 as i32,
                    bottom: area.3 as i32,
                };
                if AdjustWindowRectEx(&mut rect, style, 0, ex) == 0 {
                    return;
                }
                SetWindowPos(
                    self.hwnd,
                    std::ptr::null_mut(),
                    area.0 + rect.left,
                    area.1 + rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
        pub fn fullscreen(&mut self, enabled: bool, monitor: Area) {
            unsafe {
                if enabled {
                    if self.restore.is_none() {
                        self.restore = self
                            .area()
                            .map(|area| (area, GetWindowLongPtrW(self.hwnd, GWL_STYLE)));
                    }
                    let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32;
                    SetWindowLongPtrW(
                        self.hwnd,
                        GWL_STYLE,
                        ((style & !WS_OVERLAPPEDWINDOW) | WS_POPUP) as isize,
                    );
                    self.geometry(monitor);
                } else if let Some((area, style)) = self.restore.take() {
                    SetWindowLongPtrW(self.hwnd, GWL_STYLE, style);
                    self.geometry(area);
                }
            }
        }
        pub fn cursor(&mut self, _dpi: f32) { /* Windows supplies its themed DPI-aware cursor. */
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSScreen, NSWindow};
    use objc2_foundation::{NSPoint, NSSize};
    pub struct Window;
    fn window() -> Option<objc2::rc::Retained<NSWindow>> {
        let app = NSApplication::sharedApplication(MainThreadMarker::new()?);
        app.keyWindow().or_else(|| app.mainWindow())
    }
    impl Window {
        pub fn new() -> Option<Self> {
            window().map(|_| Self)
        }
        pub fn area(&self) -> Option<Area> {
            let frame = window()?.frame();
            let screens = NSScreen::screens(MainThreadMarker::new()?);
            let primary = screens.firstObject()?.frame();
            Some((
                frame.origin.x as i32,
                (primary.size.height - frame.origin.y - frame.size.height) as i32,
                frame.size.width as u32,
                frame.size.height as u32,
            ))
        }
        pub fn geometry(&self, area: Area) {
            if let Some(window) = window() {
                let dpi = macroquad::prelude::screen_dpi_scale().max(1.);
                window.setContentSize(NSSize::new(
                    f64::from(area.2) / f64::from(dpi),
                    f64::from(area.3) / f64::from(dpi),
                ));
                if let Some(marker) = MainThreadMarker::new() {
                    let screens = NSScreen::screens(marker);
                    if let Some(primary) = screens.firstObject() {
                        window.setFrameTopLeftPoint(NSPoint::new(
                            f64::from(area.0),
                            primary.frame().size.height - f64::from(area.1),
                        ));
                    }
                }
            }
        }
        pub fn fullscreen(&mut self, enabled: bool, _monitor: Area) {
            macroquad::prelude::set_fullscreen(enabled);
        }
        pub fn cursor(&mut self, _dpi: f32) {}
    }
}

pub struct WindowControl {
    native: Option<platform::Window>,
}
impl WindowControl {
    pub fn new() -> Self {
        Self {
            native: platform::Window::new(),
        }
    }
    pub fn area(&self) -> Area {
        if let Some(area) = self.native.as_ref().and_then(|w| w.area()) {
            return area;
        }
        let (width, height) = macroquad::miniquad::window::screen_size();
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let (x, y) = macroquad::miniquad::window::get_window_position();
        #[cfg(target_os = "macos")]
        let (x, y) = (0u32, 0u32);
        (
            x as i32,
            y as i32,
            width.max(1.) as u32,
            height.max(1.) as u32,
        )
    }
    pub fn geometry(&self, area: Area) {
        if let Some(native) = &self.native {
            native.geometry(area);
        } else {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            macroquad::miniquad::window::set_window_position(area.0 as u32, area.1 as u32);
            macroquad::miniquad::window::set_window_size(area.2, area.3);
        }
    }
    pub fn fullscreen(&mut self, enabled: bool, monitor: Area) {
        if let Some(native) = &mut self.native {
            native.fullscreen(enabled, monitor);
        } else {
            macroquad::prelude::set_fullscreen(enabled);
        }
    }
    pub fn cursor(&mut self, dpi: f32) {
        if let Some(native) = &mut self.native {
            native.cursor(dpi);
        }
    }
}
