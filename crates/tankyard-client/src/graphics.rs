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
pub fn refresh_capabilities(filter: u8) {
    let maximum = unsafe {
        let gl = get_internal_gl();
        if gl.quad_context.info().backend != miniquad::Backend::OpenGl {
            1
        } else {
            use miniquad::graphics::raw_gl::*;
            let extensions = glGetString(GL_EXTENSIONS);
            if extensions.is_null() {
                1
            } else {
                let extensions = std::ffi::CStr::from_ptr(extensions.cast()).to_string_lossy();
                if extensions.split_whitespace().any(|name| {
                    matches!(
                        name,
                        "GL_EXT_texture_filter_anisotropic" | "GL_ARB_texture_filter_anisotropic"
                    )
                }) {
                    let mut limit = 1;
                    glGetIntegerv(0x84FF, &mut limit);
                    limit.clamp(1, 16) as u8
                } else {
                    1
                }
            }
        }
    };
    MAX_ANISOTROPY.with(|v| v.set(maximum));
    FILTER.with(|v| v.set(0));
    set_filter(filter);
}
pub fn set_filter(filter: u8) {
    if FILTER.with(|v| {
        if v.get() == filter {
            true
        } else {
            v.set(filter);
            false
        }
    }) {
        return;
    }
    let maximum = max_anisotropy();
    if maximum <= 1 {
        return;
    }
    TEXTURES.with(|cache| {
        for texture in cache.borrow().values() {
            // Restore the binding so Miniquad's cached GL state stays correct.
            unsafe {
                let gl = get_internal_gl();
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
                    gl.quad_context
                        .texture_generate_mipmaps(texture.raw_miniquad_id());
                    gl.quad_context.texture_set_min_filter(
                        texture.raw_miniquad_id(),
                        miniquad::FilterMode::Linear,
                        miniquad::MipmapFilterMode::Linear,
                    );
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
pub fn body(id: &str, center: Vec2, radius: f32, angle: f32, opacity: f32) {
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
