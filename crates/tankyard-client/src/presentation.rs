//! Render resolution is independent of desktop fullscreen geometry. Input and
//! all UI layout remain in the same logical window coordinates at every scale.
use crate::{settings::Settings, target::Target};
use macroquad::prelude::*;
fn bounded_size(size: (u32, u32)) -> (u32, u32) {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let factor = (4096. / w as f32).min(4096. / h as f32).min(1.);
    (
        (w as f32 * factor).round().max(1.) as u32,
        (h as f32 * factor).round().max(1.) as u32,
    )
}
#[derive(Default)]
pub struct Presentation {
    target: Option<((u32, u32, u8), Target)>,
    active: bool,
    output: (u32, u32),
    world: crate::upscale::Scaler,
}
impl Presentation {
    pub fn begin(&mut self, settings: &Settings) {
        let physical = miniquad::window::screen_size();
        let requested = if settings.native_resolution {
            (physical.0 as u32, physical.1 as u32)
        } else {
            bounded_size((settings.width, settings.height))
        };
        self.active = requested != (physical.0 as u32, physical.1 as u32);
        self.output = requested;
        let key = (
            requested.0,
            requested.1,
            crate::graphics::samples(settings.anti_aliasing),
        );
        if !self.active {
            self.target = None;
            set_default_camera();
            return;
        }
        if self.target.as_ref().is_none_or(|(size, _)| *size != key) {
            // Preserve selected antialiasing when fullscreen resolution uses
            // an intermediate target; GL2 safely falls back to one sample.
            self.target = None;
            self.target = Some((key, Target::new((key.0, key.1), key.2)));
            log::info!(
                "Render resolution: {}x{} (window {}x{})",
                requested.0,
                requested.1,
                physical.0,
                physical.1
            );
        }
        self.select_output();
    }
    /// Draw into the frame's output: the resolution target, or else the window.
    fn select_output(&mut self) {
        match &mut self.target {
            Some((_, target)) if self.active => target.select(),
            _ => set_default_camera(),
        }
    }
    /// The arena draws between `begin_world` and `finish_world`, at its own
    /// render scale and through the chosen upscaler.
    pub fn begin_world(&mut self, settings: &Settings) {
        self.world.begin(settings, self.output);
    }
    pub fn finish_world(&mut self) {
        let mut world = std::mem::take(&mut self.world);
        world.finish(|| self.select_output());
        self.world = world;
    }
    pub fn post_processing_error(&self) -> Option<&str> {
        self.world.error()
    }
    pub fn finish(&mut self) {
        if self.active
            && let Some((_, target)) = &mut self.target
        {
            let texture = target.texture().clone();
            set_default_camera();
            clear_background(BLACK);
            draw_texture_ex(
                &texture,
                0.,
                0.,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    flip_y: true,
                    ..Default::default()
                },
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_sizes_preserve_aspect_and_bound_texture_allocation() {
        assert_eq!(bounded_size((1920, 1080)), (1920, 1080));
        assert_eq!(bounded_size((7680, 4320)), (4096, 2304));
        assert_eq!(bounded_size((0, 0)), (1, 1));
    }
}
