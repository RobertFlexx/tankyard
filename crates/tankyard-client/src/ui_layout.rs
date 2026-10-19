//! Scroll enlarged menus without changing the game's camera or input units.
use macroquad::prelude::*;
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Canvas {
    height: f32,
    offset: f32,
    clip: Rect,
    scrollable: bool,
}
thread_local! { static CANVAS: Cell<Option<Canvas>> = const { Cell::new(None) }; }
pub fn height() -> f32 {
    CANVAS.with(|c| c.get().map_or_else(screen_height, |c| c.height))
}
pub fn pointer() -> Vec2 {
    let (x, y) = mouse_position();
    vec2(x, y + CANVAS.with(|c| c.get().map_or(0., |c| c.offset)))
}
pub fn position(p: Vec2) -> Vec2 {
    vec2(p.x, p.y - CANVAS.with(|c| c.get().map_or(0., |c| c.offset)))
}
pub fn pointer_visible() -> bool {
    let (x, y) = mouse_position();
    CANVAS.with(|c| c.get().is_none_or(|c| c.clip.contains(vec2(x, y))))
}
pub fn scrolling() -> bool {
    CANVAS.with(|c| c.get().is_some_and(|c| c.scrollable))
}
pub fn scale(width: f32, height: f32, preference: f32) -> f32 {
    ((width / 1280.).min(height / 800.) * preference).clamp(0.1, 8.)
}

#[derive(Default)]
pub struct ScrollPanel {
    offset: f32,
    previous_scale: f32,
    dragging: bool,
}
impl ScrollPanel {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn begin(&mut self, s: f32, top: f32, bottom: f32, anchor: Option<f32>) -> CanvasGuard {
        let height = screen_height().max(800. * s);
        let maximum = (height - 79. * s - bottom).max(0.);
        if self.previous_scale > 0.
            && let Some(anchor) = anchor
        {
            self.offset += anchor * (s - self.previous_scale);
        }
        self.previous_scale = s;
        let clip = Rect::new(0., top, screen_width(), (bottom - top).max(1.));
        let (mx, my) = mouse_position();
        if clip.contains(vec2(mx, my)) && !crate::ui::pointer_captured() {
            self.offset -= mouse_wheel().1 * 56. * s;
        }
        self.offset = self.offset.clamp(0., maximum);
        if maximum > 0. {
            let track = Rect::new(
                screen_width() - 16. * s,
                top + 4. * s,
                8. * s,
                (bottom - top - 8. * s).max(1.),
            );
            let thumb_h =
                (track.h * clip.h / (clip.h + maximum)).clamp((28. * s).min(track.h), track.h);
            if is_mouse_button_pressed(MouseButton::Left) && track.contains(vec2(mx, my)) {
                self.dragging = true;
            }
            if !is_mouse_button_down(MouseButton::Left) {
                self.dragging = false;
            }
            if self.dragging
                || (track.contains(vec2(mx, my)) && is_mouse_button_pressed(MouseButton::Left))
            {
                self.offset = ((my - track.y - thumb_h / 2.) / (track.h - thumb_h).max(1.))
                    .clamp(0., 1.)
                    * maximum;
            }
            draw_rectangle(track.x, track.y, track.w, track.h, crate::ui::line_color());
            draw_rectangle(
                track.x,
                track.y + (track.h - thumb_h) * self.offset / maximum,
                track.w,
                thumb_h,
                crate::ui::accent_color(),
            );
        }
        CANVAS.with(|c| {
            assert!(c.get().is_none(), "nested menu canvas");
            c.set(Some(Canvas {
                height,
                offset: self.offset,
                clip,
                scrollable: maximum > 0.,
            }));
        });
        unsafe {
            let gl = get_internal_gl();
            let physical = gl.quad_gl.get_active_render_pass().map_or_else(
                miniquad::window::screen_size,
                |pass| {
                    let (w, h) = gl
                        .quad_context
                        .texture_size(gl.quad_context.render_pass_texture(pass));
                    (w as f32, h as f32)
                },
            );
            let sy = physical.1 / screen_height().max(1.);
            gl.quad_gl.scissor(Some((
                0,
                (top * sy).floor() as i32,
                physical.0 as i32,
                ((bottom - top) * sy).ceil().max(1.) as i32,
            )));
        }
        CanvasGuard
    }
}
pub struct CanvasGuard;
impl Drop for CanvasGuard {
    fn drop(&mut self) {
        unsafe {
            let gl = get_internal_gl();
            gl.quad_gl.scissor(None);
        }
        CANVAS.with(|c| c.set(None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interface_scale_has_headroom_in_both_directions_at_every_supported_size() {
        for (w, h) in [
            (800., 600.),
            (1280., 800.),
            (1920., 1080.),
            (3840., 2160.),
            (7680., 4320.),
        ] {
            assert!(scale(w, h, 0.8) < scale(w, h, 1.));
            assert!((scale(w, h, 1.4) / scale(w, h, 1.) - 1.4).abs() < 0.0001);
        }
    }
}
