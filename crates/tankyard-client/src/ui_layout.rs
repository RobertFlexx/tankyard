//! Scroll enlarged menus without changing the game's camera or input units.
use macroquad::prelude::*;
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Canvas {
    height: f32,
    offset: f32,
    clip: Rect,
}
thread_local! {
    static CANVAS: Cell<Option<Canvas>> = const { Cell::new(None) };
    static WHEEL_CLAIM: Cell<Option<Rect>> = const { Cell::new(None) };
    /// The lowest point the current page has drawn, in page units.
    static REACH: Cell<f32> = const { Cell::new(0.) };
}
/// Record that the page shows text or an image down to `y` (page units), so
/// the panel scrolls far enough to show it. Panels and outlines may tuck under
/// the footer by design, so only `ui::label` and `ui::ui_texture` call this.
pub fn reach(y: f32) {
    if y.is_finite() && CANVAS.with(|c| c.get().is_some()) {
        REACH.with(|r| r.set(r.get().max(y)));
    }
}
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

/// Whether this frame's scroll came from a trackpad or another precise
/// source rather than a wheel's steps. Windows has no flag for it, but its
/// wheels move in whole notches of 120.
pub fn precise_scroll() -> bool {
    if macroquad::miniquad::window::mouse_wheel_in_pixels() {
        return true;
    }
    let (x, y) = mouse_wheel();
    cfg!(target_os = "windows") && (x % 120. != 0. || y % 120. != 0.)
}

/// Scroll in consistent wheel steps, preserving high-resolution deltas.
/// One step moves an ordinary list by 56 logical pixels.
pub fn wheel_lines() -> Vec2 {
    let (x, y) = mouse_wheel();
    if x == 0. && y == 0. {
        return Vec2::ZERO;
    }
    let pixels = macroquad::miniquad::window::mouse_wheel_in_pixels();
    let unit = if pixels {
        1. / 56.
    } else if cfg!(target_os = "windows") {
        1. / 120.
    } else {
        1.
    };
    // Windows reports positive horizontal movement to the right; every other
    // backend reports it to the left.
    let horizontal = if cfg!(target_os = "windows") {
        -unit
    } else {
        unit
    };
    normalize_scroll(vec2(x, y), vec2(horizontal, unit))
}

fn normalize_scroll(raw: Vec2, units: Vec2) -> Vec2 {
    let axis = |value: f32| {
        if value.is_finite() {
            value.clamp(-8., 8.)
        } else {
            0.
        }
    };
    vec2(axis(raw.x * units.x), axis(raw.y * units.y))
}

/// Carry fractional movement between frames for lists with discrete rows.
pub fn scroll_steps(remainder: &mut f32, delta: f32) -> i32 {
    *remainder += delta;
    let steps = remainder.trunc() as i32;
    *remainder -= steps as f32;
    steps
}
/// A list that scrolls itself keeps the wheel from also scrolling the page
/// while the pointer is over it. The claim holds for the next frame.
/// Regions drawn over what came before them in a frame, in screen space and
/// in the order they came. The pointer over one belongs to it and to what
/// is drawn after it, never to what lies beneath. Drawing order is only
/// known once a frame is drawn, so each frame checks the last frame's
/// covers, matched by how many came before.
#[derive(Default)]
struct Covers {
    last: Vec<Rect>,
    now: Vec<Rect>,
}
thread_local! {
    static COVERS: std::cell::RefCell<Covers> = std::cell::RefCell::new(Covers::default());
}
/// Starts a frame's covers. The main loop calls it before drawing.
pub fn begin_covers() {
    COVERS.with(|covers| {
        let covers = &mut *covers.borrow_mut();
        std::mem::swap(&mut covers.last, &mut covers.now);
        covers.now.clear();
    });
}
/// Declares that `rect`, in the current page's units, is drawn from here on
/// over everything before it this frame. Call it before drawing what the
/// region holds, so its own buttons come after it.
pub fn cover(rect: Rect) {
    let top = position(rect.point());
    let mut screen = Rect::new(top.x, top.y, rect.w, rect.h);
    // Inside a scrolled region only the part showing covers anything; one
    // scrolled out of sight still keeps its place in the order.
    if let Some(canvas) = CANVAS.with(Cell::get) {
        screen = screen.intersect(canvas.clip).unwrap_or_default();
    }
    COVERS.with(|covers| covers.borrow_mut().now.push(screen));
}
/// Whether a region declared later than this point in the last frame lies
/// under the pointer and reaches beyond `rect`, so `rect` is beneath it. A
/// region wholly inside `rect` is taken for `rect`'s own content, such as a
/// card in a list, and leaves it be.
pub fn covered(rect: Rect) -> bool {
    let top = position(rect.point());
    let own = Rect::new(top.x, top.y, rect.w, rect.h);
    let (x, y) = mouse_position();
    let point = vec2(x, y);
    let inside = |cover: &Rect| {
        cover.x >= own.x - 0.5
            && cover.y >= own.y - 0.5
            && cover.right() <= own.right() + 0.5
            && cover.bottom() <= own.bottom() + 0.5
    };
    COVERS.with(|covers| {
        let covers = covers.borrow();
        let drawn = covers.now.len();
        covers.last.get(drawn..).is_some_and(|later| {
            later
                .iter()
                .any(|cover| cover.contains(point) && !inside(cover))
        })
    })
}
pub fn claim_wheel(rect: Rect) {
    let top = position(rect.point());
    WHEEL_CLAIM.with(|c| c.set(Some(Rect::new(top.x, top.y, rect.w, rect.h))));
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
        REACH.with(|r| r.set(0.));
    }
    pub fn begin(&mut self, s: f32, top: f32, bottom: f32, anchor: Option<f32>) -> CanvasGuard {
        let height = screen_height().max(800. * s);
        // Pages are laid out on an 800-unit canvas, but text can run past it
        // (long credits, wrapped notices): the last frame's lowest text
        // extends the range, with a little room below it.
        let drawn = REACH.with(|r| r.replace(0.));
        let maximum = (height - 79. * s - bottom)
            .max(drawn + 6. * s - bottom)
            .max(0.);
        if self.previous_scale > 0.
            && let Some(anchor) = anchor
        {
            self.offset += anchor * (s - self.previous_scale);
        }
        self.previous_scale = s;
        let clip = Rect::new(0., top, screen_width(), (bottom - top).max(1.));
        let (mx, my) = mouse_position();
        // Overlays above the page, and lists that scroll themselves, keep the wheel.
        let interactive = crate::ui::interactive();
        let claimed = WHEEL_CLAIM
            .with(Cell::take)
            .is_some_and(|rect| rect.contains(vec2(mx, my)));
        if interactive && !claimed && clip.contains(vec2(mx, my)) && !crate::ui::pointer_captured()
        {
            self.offset -= wheel_lines().y * 56. * s;
        }
        if let Some(focus) = crate::ui::numeric_focus_rect() {
            if focus.y < top + 8. * s {
                self.offset -= top + 8. * s - focus.y;
            } else if focus.bottom() > bottom - 8. * s {
                self.offset += focus.bottom() - bottom + 8. * s;
            }
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
            if interactive
                && is_mouse_button_pressed(MouseButton::Left)
                && track.contains(vec2(mx, my))
            {
                self.dragging = true;
            }
            if !is_mouse_button_down(MouseButton::Left) {
                self.dragging = false;
            }
            if self.dragging {
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
        scissor(Some(clip));
        CanvasGuard
    }
}
pub struct CanvasGuard;
/// An independently scrolled tool panel, with drawing and hit testing in the
/// same coordinates. The caller measures its own content and scroll range.
pub fn region(clip: Rect, height: f32, offset: f32) -> CanvasGuard {
    CANVAS.with(|canvas| {
        assert!(canvas.get().is_none(), "nested menu canvas");
        canvas.set(Some(Canvas {
            height,
            offset,
            clip,
        }));
    });
    scissor(Some(clip));
    CanvasGuard
}
impl Drop for CanvasGuard {
    fn drop(&mut self) {
        scissor(None);
        CANVAS.with(|c| c.set(None));
    }
}

/// Run `draw` with no clip, then put back the clip of the canvas or region
/// being drawn, if any: for work that draws into a target of its own.
pub fn unclipped<R>(draw: impl FnOnce() -> R) -> R {
    scissor(None);
    let result = draw();
    if let Some(canvas) = CANVAS.with(Cell::get) {
        scissor(Some(canvas.clip));
    }
    result
}

/// Clip following draws to `rect`, in window coordinates, on whichever
/// target they draw into.
fn scissor(rect: Option<Rect>) {
    let gl = unsafe { get_internal_gl() };
    let Some(rect) = rect else {
        gl.quad_gl.scissor(None);
        return;
    };
    let pass = gl.quad_gl.get_active_render_pass();
    let physical = pass.map_or_else(miniquad::window::screen_size, |pass| {
        let (w, h) = gl
            .quad_context
            .texture_size(gl.quad_context.render_pass_texture(pass));
        (w as f32, h as f32)
    });
    let sx = physical.0 / screen_width().max(1.);
    let sy = physical.1 / screen_height().max(1.);
    let (x, y) = ((rect.x * sx).floor() as i32, (rect.y * sy).floor() as i32);
    let (w, h) = (
        (rect.w * sx).ceil().max(1.) as i32,
        (rect.h * sy).ceil().max(1.) as i32,
    );
    // On Metal a resolution target keeps the window's top at its last row
    // (see `Target::select`), but macroquad places clips as on the screen.
    let y = if pass.is_some() && crate::graphics::metal() {
        physical.1 as i32 - (y + h)
    } else {
        y
    };
    gl.quad_gl.scissor(Some((x, y, w, h)));
}

/// Clip a self-contained area of the page, inside the page's own clip.
pub struct Scissor;
impl Scissor {
    pub fn new(rect: Rect) -> Self {
        let top = position(rect.point());
        let rect = Rect::new(rect.x, top.y, rect.w, rect.h);
        let page = CANVAS.with(|c| c.get().map(|c| c.clip));
        scissor(Some(page.map_or(rect, |page| {
            page.intersect(rect)
                .unwrap_or(Rect::new(rect.x, page.y, rect.w, 0.))
        })));
        Self
    }
}
impl Drop for Scissor {
    fn drop(&mut self) {
        scissor(CANVAS.with(|c| c.get().map(|c| c.clip)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_units_preserve_precision_and_reject_nonfinite_axes() {
        let windows = normalize_scroll(vec2(15., 30.), vec2(-1. / 120., 1. / 120.));
        assert_eq!(windows, vec2(-0.125, 0.25));
        let pixels = normalize_scroll(vec2(2.8, 5.6), Vec2::splat(1. / 56.));
        assert!((pixels.x - 0.05).abs() < 0.00001);
        assert!((pixels.y - 0.1).abs() < 0.00001);
        assert_eq!(
            normalize_scroll(vec2(f32::NAN, 1.), Vec2::ONE),
            vec2(0., 1.)
        );
        assert_eq!(
            normalize_scroll(vec2(1000., -1000.), Vec2::ONE),
            vec2(8., -8.)
        );
    }

    #[test]
    fn small_scrolls_accumulate_into_rows_without_rounding_up_every_frame() {
        let mut remainder = 0.;
        for _ in 0..3 {
            assert_eq!(scroll_steps(&mut remainder, 0.25), 0);
        }
        assert_eq!(scroll_steps(&mut remainder, 0.25), 1);
        assert_eq!(remainder, 0.);
        assert_eq!(scroll_steps(&mut remainder, -1.25), -1);
        assert_eq!(scroll_steps(&mut remainder, -0.75), -1);
        assert_eq!(remainder, 0.);
        assert_eq!(scroll_steps(&mut remainder, 3.), 3);
    }
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
