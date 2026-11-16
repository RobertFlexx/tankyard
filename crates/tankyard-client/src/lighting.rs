//! Darkness and the light that cuts through it. A map can start dark, and a
//! Lighting trigger can darken it during a round. Light gathers in a light
//! map at half the window's resolution, cleared to the ambient level, and
//! the light map is multiplied over the arena, so lamps, spotlights, beams,
//! tanks and shots reveal the floor and cover under them rather than paint
//! over them.
use crate::glow::{self, Blend};
use macroquad::prelude::*;
use std::cell::{Cell, RefCell};

/// One source of light, in window coordinates.
#[derive(Clone, Copy, Debug)]
pub enum Light {
    /// A round light fading from its middle.
    Spot { at: Vec2, radius: f32, color: Color },
    /// A cone from `at` along `angle` radians, `spread` radians either side
    /// and `reach` long.
    Cone {
        at: Vec2,
        angle: f32,
        spread: f32,
        reach: f32,
        color: Color,
    },
    /// A straight shaft from `from` to `to`, `width` across.
    Shaft {
        from: Vec2,
        to: Vec2,
        width: f32,
        color: Color,
    },
}

/// Most lights one frame gathers; the rest are left dark rather than slow
/// the frame.
pub const MOST_LIGHTS: usize = 768;

thread_local! {
    static LIGHT_MAP: RefCell<Option<((u32, u32), RenderTarget)>> = const { RefCell::new(None) };
    /// The lighting last shown and when, for [`steady`].
    static SHOWN: Cell<Option<([f32; 4], f64)>> = const { Cell::new(None) };
}

/// Fastest the shown lighting changes with Reduced motion on, per second:
/// a flash from dark to full light takes about half a second.
const STEADY_RATE: f32 = 2.;

/// The arena's lighting as players see it. With `reduced` set, sudden
/// changes such as a lightning flash ease in at [`STEADY_RATE`] instead;
/// slow changes pass through unchanged.
pub fn steady(lighting: [f32; 4], reduced: bool) -> [f32; 4] {
    let now = get_time();
    SHOWN.with(|cell| {
        let shown = match cell.get() {
            Some((last, at)) if reduced => {
                let most = (now - at).clamp(0., 0.25) as f32 * STEADY_RATE;
                ease_toward(last, lighting, most)
            }
            _ => lighting,
        };
        cell.set(Some((shown, now)));
        shown
    })
}

fn ease_toward(mut from: [f32; 4], to: [f32; 4], most: f32) -> [f32; 4] {
    for (value, target) in from.iter_mut().zip(to) {
        if target.is_finite() {
            *value += (target - *value).clamp(-most, most);
        }
    }
    from
}

/// The light map's color where no light falls, for `lighting` as red,
/// green, blue and darkness.
pub fn ambient(lighting: [f32; 4]) -> Color {
    let d = lighting[3].clamp(0., 1.);
    Color::new(
        1. - d + lighting[0] * d,
        1. - d + lighting[1] * d,
        1. - d + lighting[2] * d,
        1.,
    )
}

/// Darken what has been drawn so far inside `area` (the whole window when
/// None) by `lighting`, letting `lights` through. Nothing happens when the
/// arena isn't dark. Call it with no clip set: the light map is drawn
/// whole and only the result is fitted to `area`.
pub fn darken(lighting: [f32; 4], lights: &[Light], area: Option<Rect>) {
    let darkness = lighting[3];
    if !darkness.is_finite() || darkness <= 0.004 {
        return;
    }
    let (w, h) = (screen_width(), screen_height());
    let area = area
        .and_then(|a| a.intersect(Rect::new(0., 0., w, h)))
        .unwrap_or(Rect::new(0., 0., w, h));
    if area.w < 1. || area.h < 1. {
        return;
    }
    if !glow::available() {
        // Without shaders: a plain veil, with the lights as flat washes.
        draw_rectangle(
            area.x,
            area.y,
            area.w,
            area.h,
            Color::new(
                lighting[0] * 0.3,
                lighting[1] * 0.3,
                lighting[2] * 0.3,
                darkness * 0.8,
            ),
        );
        for light in lights.iter().take(MOST_LIGHTS) {
            if let Light::Spot { at, radius, color } = *light {
                glow::fallback_spot(
                    at,
                    radius,
                    Color {
                        a: color.a * darkness * 0.5,
                        ..color
                    },
                );
            }
        }
        return;
    }
    let size = (
        (w * 0.5).clamp(1., 2048.) as u32,
        (h * 0.5).clamp(1., 2048.) as u32,
    );
    LIGHT_MAP.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.as_ref().is_none_or(|(made, _)| *made != size) {
            let target = render_target(size.0, size.1);
            target.texture.set_filter(FilterMode::Linear);
            *slot = Some((size, target));
        }
        let Some((_, target)) = slot.as_ref() else {
            return;
        };
        // A clip set for the window would cut the light map wrongly.
        crate::ui_layout::unclipped(|| {
            push_camera_state();
            // The same camera the frame's own targets use, so the light map
            // reads back the same way on OpenGL and Metal.
            let mut camera = Camera2D::from_display_rect(Rect::new(0., 0., w, h));
            if !crate::graphics::metal() {
                camera.zoom.y = -camera.zoom.y;
            }
            set_camera(&camera);
            unsafe {
                get_internal_gl()
                    .quad_gl
                    .render_pass(Some(target.render_pass.raw_miniquad_id()));
            }
            clear_background(ambient(lighting));
            let lights = &lights[..lights.len().min(MOST_LIGHTS)];
            if glow::begin(Blend::Glow) {
                for light in lights {
                    if let Light::Spot { at, radius, color } = *light
                        && on_screen(at, radius, w, h)
                    {
                        glow::spot(at, radius, color);
                    }
                }
                glow::end();
            }
            if glow::begin(Blend::Add) {
                for light in lights {
                    match *light {
                        Light::Cone {
                            at,
                            angle,
                            spread,
                            reach,
                            color,
                        } if on_screen(at, reach, w, h) => cone(at, angle, spread, reach, color),
                        Light::Shaft {
                            from,
                            to,
                            width,
                            color,
                        } if on_screen(
                            (from + to) * 0.5,
                            from.distance(to) * 0.5 + width,
                            w,
                            h,
                        ) =>
                        {
                            shaft(from, to, width, color)
                        }
                        _ => {}
                    }
                }
                glow::end();
            }
            pop_camera_state();
        });
        if glow::begin(Blend::Multiply) {
            // The light map is stored bottom-up, so `area` is found from
            // the bottom and the flip puts it the right way round.
            let (sx, sy) = (size.0 as f32 / w, size.1 as f32 / h);
            draw_texture_ex(
                &target.texture,
                area.x,
                area.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(area.w, area.h)),
                    source: Some(Rect::new(
                        area.x * sx,
                        (h - area.bottom()) * sy,
                        area.w * sx,
                        area.h * sy,
                    )),
                    flip_y: true,
                    ..Default::default()
                },
            );
            glow::end();
        }
    });
}

fn on_screen(at: Vec2, radius: f32, w: f32, h: f32) -> bool {
    at.x + radius > 0. && at.y + radius > 0. && at.x - radius < w && at.y - radius < h
}

fn vertex(p: Vec2, color: Color) -> Vertex {
    Vertex::new(p.x, p.y, 0., 0., 0., color)
}

/// A cone of light, brightest at its tip and along its middle and fading to
/// nothing at its far end and sides, in the current blend.
pub fn cone(at: Vec2, angle: f32, spread: f32, reach: f32, color: Color) {
    const STEPS: usize = 14;
    if reach < 0.5 || color.a <= 0.004 {
        return;
    }
    let spread = spread.clamp(0.02, 1.5);
    let mut vertices = Vec::with_capacity(1 + STEPS * 2 + 2);
    vertices.push(vertex(at, color));
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32 * 2. - 1.;
        let a = angle + t * spread;
        let direction = vec2(a.cos(), a.sin());
        // Soft sides: full in the middle, none at the edge.
        let side = (1. - t * t).max(0.);
        vertices.push(vertex(
            at + direction * reach * 0.45,
            Color {
                a: color.a * 0.65 * side,
                ..color
            },
        ));
        vertices.push(vertex(at + direction * reach, Color { a: 0., ..color }));
    }
    let mut indices = Vec::with_capacity(STEPS * 9);
    for i in 0..STEPS as u16 {
        let (near, far) = (1 + i * 2, 2 + i * 2);
        let (next_near, next_far) = (near + 2, far + 2);
        indices.extend_from_slice(&[0, near, next_near]);
        indices.extend_from_slice(&[near, far, next_far]);
        indices.extend_from_slice(&[near, next_far, next_near]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// A shaft of light with a bright core and soft edges, in the current blend.
pub fn shaft(from: Vec2, to: Vec2, width: f32, color: Color) {
    let along = to - from;
    if along.length() < 0.5 || width < 0.2 || color.a <= 0.004 {
        return;
    }
    let across = vec2(-along.y, along.x).normalize() * width * 0.5;
    let clear = Color { a: 0., ..color };
    let vertices = vec![
        vertex(from - across, clear),
        vertex(from, color),
        vertex(from + across, clear),
        vertex(to - across, clear),
        vertex(to, color),
        vertex(to + across, clear),
    ];
    draw_mesh(&Mesh {
        vertices,
        indices: vec![0, 1, 4, 0, 4, 3, 1, 2, 5, 1, 5, 4],
        texture: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_lighting_eases_flashes_at_a_bounded_rate() {
        let dark = [0.1, 0.1, 0.3, 0.8];
        let flash = [0.1, 0.1, 0.3, 0.];
        let once = ease_toward(dark, flash, 0.25 * STEADY_RATE);
        assert!((once[3] - 0.3).abs() < 1e-5, "{once:?}");
        assert_eq!(ease_toward(once, flash, 1.), flash);
        let bad = [f32::NAN, 0., 0., 0.];
        assert_eq!(ease_toward(dark, bad, 1.)[0], dark[0]);
    }

    #[test]
    fn ambient_light_falls_with_darkness_toward_its_color() {
        assert_eq!(ambient([0., 0., 0., 0.]), WHITE);
        let night = ambient([0.1, 0.1, 0.4, 0.8]);
        assert!((night.r - 0.28).abs() < 1e-5);
        assert!((night.b - 0.52).abs() < 1e-5);
        assert!(night.b > night.r, "the dark takes the map's tint");
    }
}
