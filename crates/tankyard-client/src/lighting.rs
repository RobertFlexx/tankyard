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

impl Light {
    /// Whether any of it falls in the window.
    pub fn on_screen(&self) -> bool {
        let (w, h) = (screen_width(), screen_height());
        match *self {
            Light::Spot { at, radius, .. } => on_screen(at, radius, w, h),
            Light::Cone { at, reach, .. } => on_screen(at, reach, w, h),
            Light::Shaft {
                from, to, width, ..
            } => on_screen((from + to) * 0.5, from.distance(to) * 0.5 + width, w, h),
        }
    }
}

/// Adds `light` to a frame's `lights` if it shows and there's room, so
/// lights off screen never crowd out ones on it.
pub fn gather(lights: &mut Vec<Light>, light: Light) {
    if lights.len() < MOST_LIGHTS && light.on_screen() {
        lights.push(light);
    }
}

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
    let window = Rect::new(0., 0., w, h);
    let area = match area {
        Some(requested) => match requested.intersect(window) {
            Some(visible) => visible,
            None => return,
        },
        None => window,
    };
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

pub(crate) fn vertex(p: Vec2, color: Color) -> Vertex {
    Vertex::new(p.x, p.y, 0., 0., 0., color)
}

/// How far along a cone each row of its mesh lies, and how bright it is
/// there: light falling away from the lamp, faster near it, as it spreads.
const CONE_ROWS: [(f32, f32); 4] = [(0.12, 0.82), (0.38, 0.5), (0.7, 0.18), (1., 0.)];
const CONE_STEPS: usize = 18;
const CONE_VERTICES: usize = 1 + (CONE_STEPS + 1) * CONE_ROWS.len();
const CONE_INDICES: [u16; CONE_STEPS * (3 + (CONE_ROWS.len() - 1) * 6)] = {
    let mut indices = [0; CONE_STEPS * (3 + (CONE_ROWS.len() - 1) * 6)];
    let mut sector = 0;
    let mut cursor = 0;
    while sector < CONE_STEPS {
        let near = (1 + sector * CONE_ROWS.len()) as u16;
        let next = near + CONE_ROWS.len() as u16;
        indices[cursor] = 0;
        indices[cursor + 1] = near;
        indices[cursor + 2] = next;
        cursor += 3;
        let mut row = 0;
        while row + 1 < CONE_ROWS.len() {
            let a = near + row as u16;
            let b = next + row as u16;
            indices[cursor] = a;
            indices[cursor + 1] = a + 1;
            indices[cursor + 2] = b + 1;
            indices[cursor + 3] = a;
            indices[cursor + 4] = b + 1;
            indices[cursor + 5] = b;
            cursor += 6;
            row += 1;
        }
        sector += 1;
    }
    indices
};

/// Submit small light shapes directly to the ordinary batcher. Stack-backed
/// vertices and fixed indices avoid allocating a Mesh for each light each frame.
pub(crate) fn geometry(vertices: &[Vertex], indices: &[u16]) {
    // SAFETY: rendering runs on the main graphics thread; no context borrow
    // survives this call, which copies the geometry into macroquad's batch.
    unsafe {
        let gl = get_internal_gl().quad_gl;
        gl.texture(None);
        gl.draw_mode(DrawMode::Triangles);
        gl.geometry(vertices, indices);
    }
}

/// A cone of light, brightest at its tip and along its middle and fading to
/// nothing at its far end and sides, in the current blend.
pub fn cone(at: Vec2, angle: f32, spread: f32, reach: f32, color: Color) {
    const ROWS: usize = CONE_ROWS.len();
    if reach < 0.5 || color.a <= 0.004 {
        return;
    }
    let spread = spread.clamp(0.02, 1.5);
    let mut vertices = [vertex(at, color); CONE_VERTICES];
    for i in 0..=CONE_STEPS {
        let t = i as f32 / CONE_STEPS as f32 * 2. - 1.;
        let a = angle + t * spread;
        let direction = vec2(a.cos(), a.sin());
        // Soft sides: full in the middle, easing to none at the edge.
        let side = (1. - t * t).max(0.).powf(1.5);
        for (row, (along, bright)) in CONE_ROWS.into_iter().enumerate() {
            vertices[1 + i * ROWS + row] = vertex(
                at + direction * reach * along,
                Color {
                    a: color.a * bright * side,
                    ..color
                },
            );
        }
    }
    geometry(&vertices, &CONE_INDICES);
}

/// Across a shaft and around its rounded ends, how far out each ring of its
/// mesh lies, as a fraction of half its width, and how bright it is there:
/// light falling off smoothly from the middle to nothing, with no rim.
const SHAFT_RINGS: [(f32, f32); 4] = [(0.2, 0.7), (0.45, 0.36), (0.72, 0.12), (1., 0.)];
const RINGS: usize = SHAFT_RINGS.len();
const CAP_STEPS: usize = 8;
/// Two middle points, each side's rings at both ends, then each end's cap.
const SHAFT_VERTICES: usize = 2 + 2 * RINGS * 2 + 2 * RINGS * (CAP_STEPS + 1);
/// Where each side's ring `k` lies at the shaft's start; the next vertex
/// is the same ring at its end.
const fn shaft_ring(side: usize, k: usize) -> u16 {
    (2 + (side * RINGS + k) * 2) as u16
}
/// Point `j` of ring `k` around rounded end `end` (0 at `to`, 1 at `from`).
const fn cap_point(end: usize, k: usize, j: usize) -> u16 {
    (2 + 4 * RINGS + (end * RINGS + k) * (CAP_STEPS + 1) + j) as u16
}
/// Two triangles filling the quad `a`, `b`, `c`, `d`, taken in turn.
const fn quad(indices: &mut [u16], cursor: usize, [a, b, c, d]: [u16; 4]) -> usize {
    indices[cursor] = a;
    indices[cursor + 1] = b;
    indices[cursor + 2] = c;
    indices[cursor + 3] = a;
    indices[cursor + 4] = c;
    indices[cursor + 5] = d;
    cursor + 6
}
const SHAFT_TRIANGLES: usize = 2 * RINGS * 2 + 2 * (CAP_STEPS + (RINGS - 1) * CAP_STEPS * 2);
const SHAFT_INDICES: [u16; 3 * SHAFT_TRIANGLES] = {
    let mut indices = [0; 3 * SHAFT_TRIANGLES];
    let mut cursor = 0;
    // Along the shaft, each side: the middle line out to the first ring,
    // then ring to ring.
    let mut side = 0;
    while side < 2 {
        let first = shaft_ring(side, 0);
        cursor = quad(&mut indices, cursor, [0, 1, first + 1, first]);
        let mut k = 0;
        while k + 1 < RINGS {
            let (inner, outer) = (shaft_ring(side, k), shaft_ring(side, k + 1));
            cursor = quad(&mut indices, cursor, [inner, inner + 1, outer + 1, outer]);
            k += 1;
        }
        side += 1;
    }
    // Each rounded end: a fan from its middle point, then ring to ring.
    let mut end = 0;
    while end < 2 {
        let middle = if end == 0 { 1 } else { 0 };
        let mut j = 0;
        while j < CAP_STEPS {
            indices[cursor] = middle;
            indices[cursor + 1] = cap_point(end, 0, j);
            indices[cursor + 2] = cap_point(end, 0, j + 1);
            cursor += 3;
            j += 1;
        }
        let mut k = 0;
        while k + 1 < RINGS {
            let mut j = 0;
            while j < CAP_STEPS {
                cursor = quad(
                    &mut indices,
                    cursor,
                    [
                        cap_point(end, k, j),
                        cap_point(end, k + 1, j),
                        cap_point(end, k + 1, j + 1),
                        cap_point(end, k, j + 1),
                    ],
                );
                j += 1;
            }
            k += 1;
        }
        end += 1;
    }
    assert!(cursor == indices.len());
    indices
};

/// A shaft of light `width` across from `from` to `to`, brightest along its
/// middle and fading smoothly to nothing at its sides and around its rounded
/// ends, the way light spreads from a tube, in the current blend.
pub fn shaft(from: Vec2, to: Vec2, width: f32, color: Color) {
    let along = to - from;
    let length = along.length();
    if length < 0.5 || width < 0.2 || color.a <= 0.004 {
        return;
    }
    let half = width * 0.5;
    let direction = along / length;
    let across = vec2(-direction.y, direction.x);
    let shade = |bright: f32| Color {
        a: color.a * bright,
        ..color
    };
    let mut vertices = [vertex(from, color); SHAFT_VERTICES];
    vertices[1] = vertex(to, color);
    for (side, sign) in [1., -1.].into_iter().enumerate() {
        for (k, (out, bright)) in SHAFT_RINGS.into_iter().enumerate() {
            let offset = across * (sign * out * half);
            let at = usize::from(shaft_ring(side, k));
            vertices[at] = vertex(from + offset, shade(bright));
            vertices[at + 1] = vertex(to + offset, shade(bright));
        }
    }
    let turn = direction.y.atan2(direction.x);
    for (end, (center, start)) in [
        (to, turn - std::f32::consts::FRAC_PI_2),
        (from, turn + std::f32::consts::FRAC_PI_2),
    ]
    .into_iter()
    .enumerate()
    {
        for (k, (out, bright)) in SHAFT_RINGS.into_iter().enumerate() {
            for j in 0..=CAP_STEPS {
                let a = start + j as f32 / CAP_STEPS as f32 * std::f32::consts::PI;
                vertices[usize::from(cap_point(end, k, j))] = vertex(
                    center + vec2(a.cos(), a.sin()) * (out * half),
                    shade(bright),
                );
            }
        }
    }
    geometry(&vertices, &SHAFT_INDICES);
}

/// Most map lights whose light catches edges in one frame.
const RIM_LIGHTS: usize = 96;
/// How strongly an edge facing a light catches it, at full darkness.
const RIM: f32 = 0.85;

/// Where `light` shines on `p` from, and how strongly for its distance,
/// 0 to 1, if it reaches.
fn shining(light: &Light, p: Vec2) -> Option<(Vec2, f32)> {
    let (from, reach) = match *light {
        Light::Spot { at, radius, .. } => (at, radius),
        Light::Cone {
            at,
            angle,
            spread,
            reach,
            ..
        } => {
            let to = p - at;
            let off = (to.y.atan2(to.x) - angle + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            if off.abs() > spread.clamp(0.02, 1.5) {
                return None;
            }
            (at, reach)
        }
        Light::Shaft {
            from, to, width, ..
        } => {
            let along = to - from;
            let t = ((p - from).dot(along) / along.length_squared().max(1e-6)).clamp(0., 1.);
            (from + along * t, width)
        }
    };
    let distance = from.distance(p);
    (distance < reach).then(|| (from, (1. - distance / reach).powi(2)))
}

/// Light catching what faces it in a dark arena, in its color, added after
/// the light map so lit things stand out from the dark: the edges of
/// `walls`, the sides of `round` things (centers and radii) and the edges
/// of `shapes` (regular polygons: center, sides, radius and turn) toward
/// each of `lights`. `darkness` scales it; all in window coordinates.
pub fn rims(
    lights: &[Light],
    darkness: f32,
    walls: &[Rect],
    round: &[(Vec2, f32)],
    shapes: &[(Vec2, u8, f32, f32)],
    zoom: f32,
) {
    let darkness = darkness.clamp(0., 1.);
    if darkness < 0.05
        || lights.is_empty()
        || (walls.is_empty() && round.is_empty() && shapes.is_empty())
    {
        return;
    }
    if !glow::begin(Blend::Add) {
        return;
    }
    let depth = (4. * zoom).clamp(1.5, 9.);
    for light in lights.iter().take(RIM_LIGHTS) {
        let (color, center, reach) = match *light {
            Light::Spot { color, at, radius } => (color, at, radius),
            Light::Cone {
                color, at, reach, ..
            } => (color, at, reach),
            Light::Shaft {
                color,
                from,
                to,
                width,
            } => (color, (from + to) * 0.5, from.distance(to) * 0.5 + width),
        };
        let strength = color.a * darkness * RIM;
        if strength < 0.01 {
            continue;
        }
        for wall in walls {
            if center.x + reach < wall.x
                || center.x - reach > wall.right()
                || center.y + reach < wall.y
                || center.y - reach > wall.bottom()
            {
                continue;
            }
            // Bands run from each edge to the same edge of the cover shrunk
            // a little, so neighbouring bands meet at the corners.
            let inward = depth.min(wall.w.min(wall.h) * 0.4);
            let outer = [
                vec2(wall.x, wall.y),
                vec2(wall.right(), wall.y),
                vec2(wall.right(), wall.bottom()),
                vec2(wall.x, wall.bottom()),
            ];
            let inner = [
                vec2(wall.x + inward, wall.y + inward),
                vec2(wall.right() - inward, wall.y + inward),
                vec2(wall.right() - inward, wall.bottom() - inward),
                vec2(wall.x + inward, wall.bottom() - inward),
            ];
            let normals = [vec2(0., -1.), vec2(1., 0.), vec2(0., 1.), vec2(-1., 0.)];
            for edge in 0..4 {
                let next = (edge + 1) % 4;
                rim_band(
                    light,
                    [outer[edge], outer[next]],
                    [inner[edge], inner[next]],
                    normals[edge],
                    color,
                    strength,
                );
            }
        }
        for &(at, radius) in round {
            if at.distance(center) <= reach + radius {
                rim_round(light, at, radius, color, strength);
            }
        }
        for &(at, sides, radius, turn) in shapes {
            if sides < 3 || at.distance(center) > reach + radius || radius < 1. {
                continue;
            }
            let shrink = 1. - (radius * 0.22).clamp(1., depth) / radius;
            let corner = |k: u8| {
                let a = turn + f32::from(k) / f32::from(sides) * std::f32::consts::TAU;
                at + vec2(a.cos(), a.sin()) * radius
            };
            for k in 0..sides {
                let (a, b) = (corner(k), corner((k + 1) % sides));
                let normal = ((a + b) * 0.5 - at).normalize_or_zero();
                rim_band(
                    light,
                    [a, b],
                    [at + (a - at) * shrink, at + (b - at) * shrink],
                    normal,
                    color,
                    strength,
                );
            }
        }
    }
    glow::end();
}

/// An edge from `outer[0]` to `outer[1]`, facing out along `normal`,
/// catching `light`: brightest where it is nearest and faces it most,
/// fading to nothing at the matching `inner` edge.
fn rim_band(
    light: &Light,
    outer: [Vec2; 2],
    inner: [Vec2; 2],
    normal: Vec2,
    color: Color,
    strength: f32,
) {
    let shine = |p: Vec2| {
        shining(light, p).map_or(0., |(from, fall)| {
            let to = from - p;
            let distance = to.length();
            if distance < 1e-3 {
                return 0.;
            }
            fall * (to.dot(normal) / distance).max(0.)
        })
    };
    // The point of the edge nearest the light shines most.
    let source = match *light {
        Light::Spot { at, .. } | Light::Cone { at, .. } => at,
        Light::Shaft { from, to, .. } => (from + to) * 0.5,
    };
    let [a, b] = outer;
    let along = b - a;
    let t = ((source - a).dot(along) / along.length_squared().max(1e-6)).clamp(0., 1.);
    let points = [a, a + along * t, b];
    let bright = points.map(shine);
    if bright.iter().all(|&b| b < 0.02) {
        return;
    }
    let inside = [inner[0], inner[0].lerp(inner[1], t), inner[1]];
    let clear = Color { a: 0., ..color };
    let mut vertices = [vertex(a, clear); 6];
    for (i, ((p, b), q)) in points.into_iter().zip(bright).zip(inside).enumerate() {
        vertices[i * 2] = vertex(
            p,
            Color {
                a: (strength * b).min(1.),
                ..color
            },
        );
        vertices[i * 2 + 1] = vertex(q, clear);
    }
    geometry(&vertices, &[0, 2, 3, 0, 3, 1, 2, 4, 5, 2, 5, 3]);
}

/// The side of a round shape at `at`, `radius` across, toward `light`: a
/// crescent of its color just inside the outline, brightest facing it.
fn rim_round(light: &Light, at: Vec2, radius: f32, color: Color, strength: f32) {
    const STEPS: usize = 12;
    const SPAN: f32 = 1.2;
    let Some((from, fall)) = shining(light, at) else {
        return;
    };
    let to = from - at;
    let distance = to.length();
    if distance <= radius * 0.9 || fall * strength < 0.02 {
        return;
    }
    let facing = to.y.atan2(to.x);
    let inward = (radius * 0.22).clamp(1., 8.);
    let clear = Color { a: 0., ..color };
    let mut vertices = [vertex(at, clear); 2 * (STEPS + 1)];
    for k in 0..=STEPS {
        let u = k as f32 / STEPS as f32 * 2. - 1.;
        let a = facing + u * SPAN;
        let d = vec2(a.cos(), a.sin());
        let bright = (1. - u * u).powi(2) * fall * strength;
        vertices[k * 2] = vertex(
            at + d * (radius - 0.5).max(0.),
            Color {
                a: bright.min(1.),
                ..color
            },
        );
        vertices[k * 2 + 1] = vertex(at + d * (radius - inward).max(0.), clear);
    }
    let mut indices = [0_u16; STEPS * 6];
    for k in 0..STEPS {
        let o = (k * 2) as u16;
        indices[k * 6..k * 6 + 6].copy_from_slice(&[o, o + 2, o + 3, o, o + 3, o + 1]);
    }
    geometry(&vertices, &indices);
}

/// A four-pointed glint over a bright source: thin spikes `length` long,
/// turned `turn` radians and fading toward their tips, in the current blend.
pub fn glint(at: Vec2, length: f32, turn: f32, color: Color) {
    if length < 1. || color.a <= 0.004 {
        return;
    }
    let clear = Color { a: 0., ..color };
    let mut vertices = [vertex(at, color); 13];
    let mut indices = [0; 24];
    for k in 0..4u16 {
        let a = turn + f32::from(k) * std::f32::consts::FRAC_PI_2;
        let direction = vec2(a.cos(), a.sin());
        let side = vec2(-direction.y, direction.x) * length * 0.08;
        let first = 1 + k * 3;
        vertices[usize::from(first)..usize::from(first) + 3].copy_from_slice(&[
            vertex(at + side, clear),
            vertex(at + direction * length, clear),
            vertex(at - side, clear),
        ]);
        let cursor = usize::from(k) * 6;
        indices[cursor..cursor + 6].copy_from_slice(&[
            0,
            first,
            first + 1,
            0,
            first + 1,
            first + 2,
        ]);
    }
    geometry(&vertices, &indices);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cones_fade_steadily_from_the_lamp_to_nothing() {
        let mut last = (0., 1.);
        for (along, bright) in CONE_ROWS {
            assert!(along > last.0 && bright < last.1, "{along} {bright}");
            last = (along, bright);
        }
        assert_eq!(last, (1., 0.), "the far end meets the dark without a rim");
    }

    #[test]
    fn shafts_fade_smoothly_to_nothing_across_and_around_their_ends() {
        let mut last = (0., 1.);
        for (out, bright) in SHAFT_RINGS {
            assert!(out > last.0 && bright < last.1, "{out} {bright}");
            last = (out, bright);
        }
        assert_eq!(last, (1., 0.), "the edge meets the dark without a rim");
        // Every index names a vertex, and every vertex is used.
        let mut used = [false; SHAFT_VERTICES];
        for &index in &SHAFT_INDICES {
            used[usize::from(index)] = true;
        }
        assert!(used.iter().all(|&used| used));
    }

    #[test]
    fn light_reaches_what_is_in_range_and_in_its_beam() {
        let spot = Light::Spot {
            at: vec2(100., 100.),
            radius: 50.,
            color: WHITE,
        };
        assert_eq!(shining(&spot, vec2(100., 100.)).unwrap().1, 1.);
        assert!(shining(&spot, vec2(130., 100.)).unwrap().1 < 0.2);
        assert!(shining(&spot, vec2(151., 100.)).is_none());
        let cone = Light::Cone {
            at: vec2(0., 0.),
            angle: 0.,
            spread: 0.3,
            reach: 200.,
            color: WHITE,
        };
        assert!(shining(&cone, vec2(100., 10.)).is_some());
        assert!(shining(&cone, vec2(-100., 0.)).is_none(), "behind the lamp");
        assert!(shining(&cone, vec2(50., 50.)).is_none(), "outside the beam");
        let shaft = Light::Shaft {
            from: vec2(0., 0.),
            to: vec2(400., 0.),
            width: 40.,
            color: WHITE,
        };
        let (from, _) = shining(&shaft, vec2(300., 20.)).unwrap();
        assert_eq!(
            from,
            vec2(300., 0.),
            "a shaft shines from its nearest point"
        );
        assert!(shining(&shaft, vec2(300., 41.)).is_none());
    }

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
