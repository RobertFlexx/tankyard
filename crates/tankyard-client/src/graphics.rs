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
    // The rendering backend stays fixed for the lifetime of the window.
    // Asking the driver for ContextInfo clones its version string.
    static METAL:Cell<Option<bool>>=const {Cell::new(None)};
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
    METAL.with(|backend| match backend.get() {
        Some(metal) => metal,
        None => {
            let metal = unsafe {
                get_internal_gl().quad_context.info().backend == miniquad::Backend::Metal
            };
            backend.set(Some(metal));
            metal
        }
    })
}
pub fn set_quality(quality: u8) {
    QUALITY.with(|q| q.set(quality.min(2)));
}
/// Macroquad's `draw_poly`, point for point, minus the two vectors it
/// allocates for every shape. A busy frame draws hundreds of shapes.
pub fn draw_poly(x: f32, y: f32, sides: u8, radius: f32, rotation: f32, color: Color) {
    thread_local! {
        static SHAPE: RefCell<(Vec<macroquad::models::Vertex>, Vec<u16>)> =
            const { RefCell::new((Vec::new(), Vec::new())) };
    }
    SHAPE.with(|shape| {
        let (vertices, indices) = &mut *shape.borrow_mut();
        poly_into(vertices, indices, (x, y), sides, radius, rotation, color);
        crate::lighting::geometry(vertices, indices);
    });
}
/// Macroquad's `draw_circle`: a 20-sided polygon.
pub fn draw_circle(x: f32, y: f32, radius: f32, color: Color) {
    draw_poly(x, y, CIRCLE_SIDES, radius, 0., color);
}
const CIRCLE_SIDES: u8 = 20;
/// The unit vector to corner `i` of a polygon turned by `turned` radians.
fn spoke(i: u8, sides: u8, turned: f32) -> (f32, f32) {
    let angle = f32::from(i) / f32::from(sides) * std::f32::consts::PI * 2. + turned;
    (angle.cos(), angle.sin())
}
/// The spokes of an unturned circle.
fn circle_spokes() -> &'static [(f32, f32); CIRCLE_SIDES as usize + 1] {
    static SPOKES: std::sync::OnceLock<[(f32, f32); CIRCLE_SIDES as usize + 1]> =
        std::sync::OnceLock::new();
    SPOKES.get_or_init(|| std::array::from_fn(|i| spoke(i as u8, CIRCLE_SIDES, 0.)))
}
fn poly_into(
    vertices: &mut Vec<macroquad::models::Vertex>,
    indices: &mut Vec<u16>,
    (x, y): (f32, f32),
    sides: u8,
    radius: f32,
    rotation: f32,
    color: Color,
) {
    vertices.clear();
    indices.clear();
    let turned = rotation.to_radians();
    // Most shapes are unturned circles, so their spokes are looked up rather
    // than worked out twenty-one times each.
    let spokes = (sides == CIRCLE_SIDES && rotation == 0.).then(circle_spokes);
    vertices.push(macroquad::models::Vertex::new(x, y, 0., 0., 0., color));
    for i in 0..=sides {
        let (rx, ry) =
            spokes.map_or_else(|| spoke(i, sides, turned), |table| table[usize::from(i)]);
        vertices.push(macroquad::models::Vertex::new(
            x + radius * rx,
            y + radius * ry,
            0.,
            rx,
            ry,
            color,
        ));
        if i != sides {
            let i = u16::from(i);
            indices.extend_from_slice(&[0, i + 1, i + 2]);
        }
    }
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

#[cfg(test)]
mod polygon_tests {
    use super::*;

    #[test]
    fn a_looked_up_circle_is_the_worked_out_one_to_the_bit() {
        let (mut fast, mut indices) = (Vec::new(), Vec::new());
        poly_into(
            &mut fast,
            &mut indices,
            (12.5, -3.25),
            CIRCLE_SIDES,
            9.75,
            0.,
            WHITE,
        );
        assert_eq!(fast.len(), usize::from(CIRCLE_SIDES) + 2);
        for (i, vertex) in fast[1..].iter().enumerate() {
            let (rx, ry) = spoke(i as u8, CIRCLE_SIDES, 0_f32.to_radians());
            assert_eq!(vertex.uv.x.to_bits(), rx.to_bits(), "{i}");
            assert_eq!(vertex.uv.y.to_bits(), ry.to_bits(), "{i}");
            assert_eq!(vertex.position.x.to_bits(), (12.5 + 9.75 * rx).to_bits());
            assert_eq!(vertex.position.y.to_bits(), (-3.25 + 9.75 * ry).to_bits());
        }
    }

    #[test]
    fn a_polygon_laps_back_to_its_first_point_and_reuses_its_buffers() {
        let (mut vertices, mut indices) = (Vec::new(), Vec::new());
        poly_into(&mut vertices, &mut indices, (10., 20.), 20, 5., 0., WHITE);
        // One center, then every corner with the first repeated to close it.
        assert_eq!(vertices.len(), 22);
        assert_eq!(indices.len(), 60);
        assert_eq!(indices[57..], [0, 20, 21]);
        assert!(vertices[1].position.distance(vertices[21].position) < 1e-3);
        assert_eq!(vertices[0].position, vec3(10., 20., 0.));
        // A smaller shape leaves nothing of the last one behind.
        poly_into(&mut vertices, &mut indices, (0., 0.), 3, 1., 90., WHITE);
        assert_eq!((vertices.len(), indices.len()), (5, 9));
        assert!(vertices[1].position.distance(vec3(0., 1., 0.)) < 1e-5);
        // The largest shape still indexes inside its own vertices.
        poly_into(
            &mut vertices,
            &mut indices,
            (0., 0.),
            u8::MAX,
            1.,
            0.,
            WHITE,
        );
        assert!(indices.iter().all(|&i| usize::from(i) < vertices.len()));
    }
}
