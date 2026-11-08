//! Offscreen render targets that multisample without resolving per draw call.
//!
//! macroquad ends a render pass after every draw call, and miniquad resolves a
//! multisampled pass each time one ends. A world drawn in dozens of draw calls
//! was resolved dozens of times per frame, which cost tens of milliseconds at
//! high resolutions (and far more where OpenGL is emulated, as on macOS). Here
//! drawing goes into a pass with no resolve attachment, and a second pass over
//! the same samples resolves them once, when the frame needs the result.
//!
//! The target owns every texture and pass it creates. miniquad deletes a
//! pass's attachments with it, so nothing here is shared with macroquad.
use macroquad::{
    miniquad::{self, PassAction},
    prelude::*,
};

pub struct Target {
    size: (u32, u32),
    /// Single-sample color: drawn into directly, or the resolve destination.
    texture: Texture2D,
    /// Draws go here: the color texture, or the multisampled one.
    draw: miniquad::RenderPass,
    /// Resolves the multisampled color into `texture`.
    resolve: Option<miniquad::RenderPass>,
    /// The multisampled color, freed explicitly where passes don't own it.
    samples: Option<miniquad::TextureId>,
    dirty: bool,
}

fn multisampling_available() -> bool {
    unsafe {
        get_internal_gl()
            .quad_context
            .info()
            .features
            .resolve_attachments
    }
}

impl Target {
    pub fn new(size: (u32, u32), samples: u8) -> Self {
        let multisample = samples > 1 && multisampling_available();
        let metal = crate::graphics::metal();
        let (color, draw, resolve, samples) = unsafe {
            let gl = get_internal_gl();
            let ctx = gl.quad_context;
            let color = ctx.new_render_texture(miniquad::TextureParams {
                width: size.0,
                height: size.1,
                ..Default::default()
            });
            if multisample {
                let samples = ctx.new_render_texture(miniquad::TextureParams {
                    width: size.0,
                    height: size.1,
                    sample_count: i32::from(samples),
                    ..Default::default()
                });
                let resolving = ctx.new_render_pass_mrt(&[samples], Some(&[color]), None);
                if metal {
                    // Metal resolves once, when the pass's encoder closes, and
                    // consecutive draws share one encoder.
                    (color, resolving, None, Some(samples))
                } else {
                    let draw = ctx.new_render_pass_mrt(&[samples], None, None);
                    (color, draw, Some(resolving), Some(samples))
                }
            } else {
                (color, ctx.new_render_pass(color, None), None, None)
            }
        };
        let texture = Texture2D::from_miniquad_texture(color);
        texture.set_filter(FilterMode::Linear);
        Self {
            size,
            texture,
            draw,
            resolve,
            samples,
            dirty: false,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Make this the target for following draws, in window coordinates.
    pub fn select(&mut self) {
        let mut camera =
            Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height()));
        // OpenGL stores render targets bottom-up; macroquad flips cameras that
        // name one, and this camera names its pass directly instead. Metal
        // stores them top-down like the screen, so the camera stays as is and
        // every target is read back the same way on both.
        if !crate::graphics::metal() {
            camera.zoom.y = -camera.zoom.y;
        }
        set_camera(&camera);
        unsafe { get_internal_gl().quad_gl.render_pass(Some(self.draw)) };
        self.dirty = true;
    }

    /// The finished image. Draws queued for this target are submitted first.
    pub fn texture(&mut self) -> &Texture2D {
        if let Some(resolve) = self.resolve
            && std::mem::take(&mut self.dirty)
        {
            unsafe {
                let mut gl = get_internal_gl();
                gl.flush();
                gl.quad_context
                    .begin_pass(Some(resolve), PassAction::Nothing);
                gl.quad_context.end_render_pass();
            }
        }
        &self.texture
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        // Deleting an OpenGL pass deletes its attachments. The two passes share
        // the multisampled texture; GL ignores the second delete of a name, and
        // no name can be reused between these two calls.
        unsafe {
            let gl = get_internal_gl();
            gl.quad_context.delete_render_pass(self.draw);
            if let Some(resolve) = self.resolve {
                gl.quad_context.delete_render_pass(resolve);
            }
            // Metal passes don't own their attachments.
            if gl.quad_context.info().backend != miniquad::Backend::OpenGl {
                gl.quad_context
                    .delete_texture(self.texture.raw_miniquad_id());
                if let Some(samples) = self.samples {
                    gl.quad_context.delete_texture(samples);
                }
            }
        }
    }
}
