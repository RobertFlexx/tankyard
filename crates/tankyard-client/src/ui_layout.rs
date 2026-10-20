//! Scroll enlarged menus without changing the game's camera or input units.
use macroquad::prelude::*;
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Canvas {
    height: f32,
    offset: f32,
    clip: Rect,
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
pub fn visible_y(top: f32, bottom: f32) -> bool {
    CANVAS.with(|c| {
        c.get()
            .is_none_or(|canvas| bottom >= canvas.clip.y && top <= canvas.clip.bottom())
    })
}
pub fn scale(width: f32, height: f32, preference: f32) -> f32 {
    ((width / 1280.).min(height / 800.) * preference).clamp(0.1, 8.)
}

pub fn staff_button(width: f32, height: f32, s: f32) -> Rect {
    Rect::new(
        width - 176. * s,
        (height - 306. * s).max(296. * s),
        156. * s,
        28. * s,
    )
}

/// Keep the layout fixed while its own scale slider is being dragged.
#[derive(Default)]
pub struct ScaleEdit {
    pending: Option<f32>,
}
impl ScaleEdit {
    pub fn preview(&self, applied: f32) -> f32 {
        self.pending.unwrap_or(applied)
    }
    pub fn stage(&mut self, value: f32) {
        self.pending = Some(value.clamp(0.8, 1.4));
    }
    pub fn commit(&mut self, applied: &mut f32, holding: bool) -> bool {
        if holding {
            return false;
        }
        let Some(value) = self.pending.take() else {
            return false;
        };
        let changed = *applied != value;
        *applied = value;
        changed
    }
    pub fn cancel(&mut self) {
        self.pending = None;
    }
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

/// Clip a self-contained canvas using the active render target's real dimensions.
pub struct Scissor;
impl Scissor {
    pub fn new(rect: Rect) -> Self {
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
            let sx = physical.0 / screen_width().max(1.);
            let sy = physical.1 / screen_height().max(1.);
            gl.quad_gl.scissor(Some((
                (rect.x * sx).floor() as i32,
                (rect.y * sy).floor() as i32,
                (rect.w * sx).ceil() as i32,
                (rect.h * sy).ceil() as i32,
            )));
        }
        Self
    }
}
impl Drop for Scissor {
    fn drop(&mut self) {
        unsafe {
            get_internal_gl().quad_gl.scissor(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staff_toggle_fits_between_a_full_leaderboard_and_the_minimap() {
        for (w, h) in [(800., 600.), (1280., 800.), (1920., 1080.), (3840., 2160.)] {
            for preference in [0.8, 1., 1.4] {
                let s = scale(w, h, preference);
                let r = staff_button(w, h, s);
                assert!(r.y >= 295. * s);
                assert!(r.bottom() <= h - 236. * s);
                assert!(r.x > 0. && r.right() < w);
            }
        }
    }
    #[test]
    fn scale_drag_commits_once_on_release_and_cancellation_keeps_the_applied_scale() {
        let mut edit = ScaleEdit::default();
        let mut applied = 1.;
        for step in 80..=140 {
            edit.stage(step as f32 / 100.);
            assert!(!edit.commit(&mut applied, true));
            assert_eq!(applied, 1.);
        }
        assert!(edit.commit(&mut applied, false));
        assert_eq!(applied, 1.4);
        assert!(!edit.commit(&mut applied, false));
        edit.stage(0.8);
        edit.cancel();
        assert!(!edit.commit(&mut applied, false));
        assert_eq!(edit.preview(applied), 1.4);
        edit.stage(0.8);
        assert!(edit.commit(&mut applied, false), "fast clicks also apply");
        assert_eq!(applied, 0.8);
    }
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
