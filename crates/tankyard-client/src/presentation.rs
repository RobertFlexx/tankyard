//! Render resolution is independent of desktop fullscreen geometry. Input and
//! all UI layout remain in the same logical window coordinates at every scale.
use crate::settings::Settings;
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
    target: Option<((u32, u32), RenderTarget)>,
    active: bool,
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
        if !self.active {
            self.target = None;
            set_default_camera();
            return;
        }
        if self
            .target
            .as_ref()
            .is_none_or(|(size, _)| *size != requested)
        {
            // Native MSAA remains on the window. A single-sample intermediate
            // works on GL2 as well and bounds memory use on smaller GPUs.
            let target = render_target(requested.0, requested.1);
            target.texture.set_filter(FilterMode::Linear);
            self.target = Some((requested, target));
            log::info!(
                "Render resolution: {}x{} (window {}x{})",
                requested.0,
                requested.1,
                physical.0,
                physical.1
            );
        }
        if let Some((_, target)) = &self.target {
            let mut camera =
                Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height()));
            camera.render_target = Some(target.clone());
            set_camera(&camera);
        }
    }
    pub fn finish(&self) {
        if self.active
            && let Some((_, target)) = &self.target
        {
            set_default_camera();
            clear_background(BLACK);
            draw_texture_ex(
                &target.texture,
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
