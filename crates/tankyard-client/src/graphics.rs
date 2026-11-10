use macroquad::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
};
use tankyard_mods::TextureData;
thread_local! {
    static TEXTURES:RefCell<HashMap<String,Texture2D>>=RefCell::new(HashMap::new());
    static QUALITY:Cell<u8>=const {Cell::new(2)};
    static FILTER:Cell<u8>=const {Cell::new(1)};
    static MAX_ANISOTROPY:Cell<u8>=const {Cell::new(1)};
    static MAX_SAMPLES:Cell<u8>=const {Cell::new(1)};
    static MIPMAPS_READY:Cell<bool>=const {Cell::new(false)};
}
/// The most MSAA samples worth offering. Apple GPUs resolve up to 4 samples
/// in tile memory; their drivers emulate 8, which costs several times more
/// with no visible gain on flat shapes, so those Macs offer 4.
pub const MSAA_LIMIT: u8 = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    4
} else {
    8
};
/// The current screen image. Metal reads the presented frame; call
/// `prepare_screen_capture` a frame earlier so the screen is readable.
pub fn screen_image() -> Option<Image> {
    if metal() {
        unsafe {
            let mut gl = get_internal_gl();
            gl.flush();
            let (width, height, top_down) = gl.quad_context.read_screen()?;
            // Images here are bottom-up, as OpenGL stores the screen.
            let row = width as usize * 4;
            let bytes = top_down
                .chunks_exact(row)
                .rev()
                .flatten()
                .copied()
                .collect();
            Some(Image {
                bytes,
                width: width.min(u32::from(u16::MAX)) as u16,
                height: height.min(u32::from(u16::MAX)) as u16,
            })
        }
    } else {
        // macroquad reads the framebuffer bottom-up, as OpenGL stores it.
        Some(get_screen_data())
    }
}
/// Whether this Mac has a Metal GPU, checked before the window opens so a
/// Mac without one starts with OpenGL.
#[cfg(target_os = "macos")]
pub fn metal_device_present() -> bool {
    // Listing devices, unlike creating the default one, doesn't switch a Mac
    // with two GPUs over to the discrete GPU.
    #[link(name = "Metal", kind = "framework")]
    unsafe extern "C" {
        fn MTLCopyAllDevices() -> *const std::ffi::c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(array: *const std::ffi::c_void) -> isize;
        fn CFRelease(object: *const std::ffi::c_void);
    }
    unsafe {
        let devices = MTLCopyAllDevices();
        if devices.is_null() {
            return false;
        }
        let count = CFArrayGetCount(devices);
        CFRelease(devices);
        count > 0
    }
}
#[cfg(not(target_os = "macos"))]
pub fn metal_device_present() -> bool {
    false
}
/// Whether MetalFX upscaling is available (Metal on macOS 13 or later).
pub fn metalfx_available() -> bool {
    unsafe { get_internal_gl().quad_context.spatial_upscaler_available() }
}
/// Upscale `input` into `output` with MetalFX after the draws queued so far.
pub fn metalfx_upscale(input: &Texture2D, output: &Texture2D) -> bool {
    unsafe {
        let mut gl = get_internal_gl();
        gl.flush();
        gl.quad_context
            .spatial_upscale(input.raw_miniquad_id(), output.raw_miniquad_id())
    }
}
pub fn prepare_screen_capture() {
    unsafe { get_internal_gl().quad_context.prepare_screen_reads() };
}
/// Whether the game renders with Metal (Macs) rather than OpenGL.
pub fn metal() -> bool {
    unsafe { get_internal_gl().quad_context.info().backend == miniquad::Backend::Metal }
}
pub fn set_quality(quality: u8) {
    QUALITY.with(|q| q.set(quality.min(2)));
}
pub fn circle_sides(radius: f32) -> u8 {
    QUALITY.with(|q| sides_for(radius, q.get()))
}
fn sides_for(radius: f32, quality: u8) -> u8 {
    let error = [0.55_f32, 0.25, 0.10][usize::from(quality.min(2))];
    let count = (std::f32::consts::PI * (radius.max(1.0) / (2.0 * error)).sqrt()).ceil() as u16;
    (count.clamp(24, 128).div_ceil(8) * 8).min(128) as u8
}
pub fn load(textures: &[TextureData], filter: u8) {
    MIPMAPS_READY.with(|ready| ready.set(false));
    TEXTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.clear();
        for data in textures {
            let texture = Texture2D::from_rgba8(data.width, data.height, &data.rgba);
            texture.set_filter(FilterMode::Linear);
            cache.insert(data.id.clone(), texture);
        }
    });
    FILTER.with(|value| value.set(0));
    set_filter(filter);
}
pub fn unload() {
    TEXTURES.with(|cache| cache.borrow_mut().clear());
    MENU_BACKGROUND.with(|slot| slot.borrow_mut().take());
    MIPMAPS_READY.with(|ready| ready.set(false));
}

pub struct Resources;
impl Drop for Resources {
    fn drop(&mut self) {
        // Thread-local fonts and mod textures must be freed while OpenGL is loaded.
        unsafe {
            let mut gl = get_internal_gl();
            gl.flush();
            gl.quad_gl.reset();
        }
        unload();
        crate::ui::unload_font();
    }
}
pub fn max_anisotropy() -> u8 {
    MAX_ANISOTROPY.with(Cell::get)
}
pub fn texture_count() -> usize {
    TEXTURES.with(|cache| cache.borrow().len())
}
/// The OpenGL extensions the driver reports. Call only on OpenGL.
pub unsafe fn gl_extensions() -> Vec<String> {
    use miniquad::graphics::raw_gl::*;
    let text = |name: *const u8| unsafe {
        std::ffi::CStr::from_ptr(name.cast())
            .to_string_lossy()
            .into_owned()
    };
    unsafe {
        if is_gl2() {
            let extensions = glGetString(GL_EXTENSIONS);
            if extensions.is_null() {
                return Vec::new();
            }
            text(extensions.cast())
                .split_whitespace()
                .map(str::to_owned)
                .collect()
        } else {
            let mut count = 0;
            glGetIntegerv(GL_NUM_EXTENSIONS, &mut count);
            (0..count.clamp(0, 4096) as u32)
                .filter_map(|index| {
                    let extension = glGetStringi(GL_EXTENSIONS, index);
                    (!extension.is_null()).then(|| text(extension.cast()))
                })
                .collect()
        }
    }
}
pub fn refresh_capabilities(filter: u8) {
    let samples = unsafe {
        let gl = get_internal_gl();
        if let Some(maximum) = gl.quad_context.max_render_samples() {
            maximum.clamp(1, MSAA_LIMIT.into()) as u8
        } else if gl.quad_context.info().backend == miniquad::Backend::OpenGl
            && gl.quad_context.info().features.resolve_attachments
        {
            let mut maximum = 1;
            miniquad::graphics::raw_gl::glGetIntegerv(0x8D57, &mut maximum);
            maximum.clamp(1, MSAA_LIMIT.into()) as u8
        } else {
            1
        }
    };
    MAX_SAMPLES.with(|s| s.set(samples));
    let maximum = unsafe {
        let gl = get_internal_gl();
        if let Some(maximum) = gl.quad_context.max_anisotropy() {
            maximum.clamp(1, 16)
        } else if gl.quad_context.info().backend != miniquad::Backend::OpenGl {
            1
        } else {
            if gl_extensions().iter().any(|e| anisotropy_extension(e)) {
                let mut limit = 1;
                miniquad::graphics::raw_gl::glGetIntegerv(0x84FF, &mut limit);
                limit.clamp(1, 16) as u8
            } else {
                1
            }
        }
    };
    MAX_ANISOTROPY.with(|v| v.set(maximum));
    log::info!(
        "Renderer: {}; MSAA up to {samples}x, filtering up to {maximum}x, MetalFX {}",
        if metal() { "Metal" } else { "OpenGL" },
        if metalfx_available() {
            "available"
        } else {
            "unavailable"
        },
    );
    FILTER.with(|v| v.set(0));
    set_filter(filter);
}
pub fn samples(requested: u8) -> u8 {
    let maximum = MAX_SAMPLES.with(Cell::get);
    [8, 4, 1]
        .into_iter()
        .find(|&n| n <= requested && n <= maximum)
        .unwrap_or(1)
}
pub fn supports_samples(samples: u8) -> bool {
    samples == 1 || samples <= MAX_SAMPLES.with(Cell::get)
}
fn anisotropy_extension(name: &str) -> bool {
    matches!(
        name,
        "GL_EXT_texture_filter_anisotropic" | "GL_ARB_texture_filter_anisotropic"
    )
}
pub fn set_filter(filter: u8) {
    let maximum = max_anisotropy();
    let filter = filter.clamp(1, maximum);
    let initialize = maximum > 1 && MIPMAPS_READY.with(|ready| !ready.replace(true));
    if FILTER.with(|v| v.replace(filter) == filter) && !initialize {
        return;
    }
    if maximum <= 1 {
        return;
    }
    TEXTURES.with(|cache| {
        log::debug!(
            "Texture filtering: {filter}x for {} textures; initialize mipmaps: {initialize}",
            cache.borrow().len()
        );
        for texture in cache.borrow().values() {
            // Restore the binding so Miniquad's cached GL state stays correct.
            unsafe {
                let gl = get_internal_gl();
                let id = texture.raw_miniquad_id();
                if initialize {
                    gl.quad_context.texture_generate_mipmaps(id);
                    gl.quad_context.texture_set_min_filter(
                        id,
                        miniquad::FilterMode::Linear,
                        miniquad::MipmapFilterMode::Linear,
                    );
                }
                // Metal sets filtering itself; OpenGL goes through raw calls.
                if gl.quad_context.texture_set_anisotropy(id, filter) {
                    continue;
                }
                #[cfg(not(target_vendor = "apple"))]
                let miniquad::RawId::OpenGl(id) =
                    gl.quad_context.texture_raw_id(texture.raw_miniquad_id());
                #[cfg(target_vendor = "apple")]
                let id = match gl.quad_context.texture_raw_id(texture.raw_miniquad_id()) {
                    miniquad::RawId::OpenGl(id) => id,
                    miniquad::RawId::Metal(_) => continue,
                };
                {
                    use miniquad::graphics::raw_gl::*;
                    let mut previous = 0;
                    glGetIntegerv(0x8069, &mut previous);
                    glBindTexture(GL_TEXTURE_2D, id);
                    glTexParameterf(GL_TEXTURE_2D, 0x84FE, f32::from(filter.clamp(1, maximum)));
                    glBindTexture(GL_TEXTURE_2D, previous as u32);
                }
            }
        }
    });
}
thread_local! {
    /// A pack's main menu background; kept apart from game textures, which
    /// a server's packs replace.
    static MENU_BACKGROUND: std::cell::RefCell<Option<Texture2D>> = const { std::cell::RefCell::new(None) };
}
pub fn set_menu_background(data: Option<&TextureData>) {
    let texture = data.map(|data| {
        let texture = Texture2D::from_rgba8(data.width, data.height, &data.rgba);
        texture.set_filter(FilterMode::Linear);
        texture
    });
    MENU_BACKGROUND.with(|slot| *slot.borrow_mut() = texture);
}
/// Tile the menu background across the screen in `cell`-sized squares,
/// drifting slowly with `time`. Returns false when no pack provides one.
pub fn menu_background(cell: f32, time: f32) -> bool {
    MENU_BACKGROUND.with(|slot| {
        let slot = slot.borrow();
        let Some(texture) = slot.as_ref() else {
            return false;
        };
        let cell = cell.max(16.0);
        let offset = (time * 12.0).rem_euclid(cell);
        let (w, h) = (screen_width(), screen_height());
        let mut y = -cell + offset;
        while y < h {
            let mut x = -cell + offset;
            while x < w {
                draw_texture_ex(
                    texture,
                    x,
                    y,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(Vec2::splat(cell)),
                        ..Default::default()
                    },
                );
                x += cell;
            }
            y += cell;
        }
        true
    })
}
pub fn body(id: &str, center: Vec2, radius: f32, angle: f32, opacity: f32) {
    let center = crate::ui_layout::position(center);
    TEXTURES.with(|cache| {
        if let Some(texture) = cache.borrow().get(id) {
            draw_texture_ex(
                texture,
                center.x - radius,
                center.y - radius,
                Color::new(1.0, 1.0, 1.0, opacity),
                DrawTextureParams {
                    dest_size: Some(Vec2::splat(radius * 2.0)),
                    rotation: angle,
                    pivot: Some(center),
                    ..Default::default()
                },
            );
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn circle_geometry_keeps_subpixel_edges_at_high_dpi_without_excess_vertices() {
        for radius in [5.0_f32, 22.0, 50.0, 120.0, 200.0] {
            let sides = sides_for(radius, 2);
            let error = radius * (1.0 - (std::f32::consts::PI / f32::from(sides)).cos());
            assert!(error <= 0.11);
            assert!((24..=128).contains(&sides));
            assert!(sides_for(radius, 0) <= sides);
        }
    }
}
