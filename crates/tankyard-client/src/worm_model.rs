//! The Sunderworm, drawn from the rings the game lays along its path: a pale
//! body of overlapping plates that swells in slow waves, bristles rowing
//! along its sides, a mouth of folding petals lined with turning rings of
//! teeth, and a cluster of small wet eyes. Torn in two, both ends are raw.
//!
//! Like the other models it only emits primitives, so the game, its menus and
//! the model sheet all draw it the same way.
use crate::tank_model::{OUTLINE, Primitive};
use macroquad::prelude::{Color, Vec2, vec2};
use std::f32::consts::{PI, TAU};
use tankyard_core::sunderworm::WORM_RINGS;

/// Its skin, a pale sickly pink.
pub const FLESH: Color = Color::new(0.79, 0.64, 0.6, 1.);
/// The bruised shade between its plates and on its shadowed side.
const BRUISE: Color = Color::new(0.36, 0.2, 0.27, 1.);
/// Where its wet skin catches the light.
const SHEEN: Color = Color::new(0.98, 0.92, 0.88, 1.);
const BONE: Color = Color::new(0.9, 0.85, 0.71, 1.);
const BONE_SHADE: Color = Color::new(0.55, 0.48, 0.37, 1.);
const MAW: Color = Color::new(0.2, 0.02, 0.04, 1.);
const GUM: Color = Color::new(0.62, 0.1, 0.15, 1.);
/// Torn flesh.
pub const RAW: Color = Color::new(0.76, 0.07, 0.11, 1.);
/// Blood, darker where it pools.
pub const BLOOD: Color = Color::new(0.42, 0.02, 0.05, 1.);
const EYE: Color = Color::new(0.03, 0.02, 0.03, 1.);
/// The bottom of its throat.
const DEEP: Color = Color::new(0.045, 0.004, 0.014, 1.);
/// The pale layer under its skin, seen where it is cut.
const FAT: Color = Color::new(0.86, 0.62, 0.55, 1.);
const BRISTLE: Color = Color::new(0.2, 0.13, 0.14, 1.);
/// Bile, in its throat before it sprays.
pub const BILE: Color = Color::new(0.62, 0.78, 0.18, 1.);
/// Where the light comes from on screen: up and to the left.
const LIGHT: Vec2 = vec2(-0.5547, -0.8321);
/// How deep a ring can be and still be seen, matching the game.
const SEEN: f32 = tankyard_core::sunderworm::SURFACED;
/// Rings this deep or deeper are sinking into the floor.
const SINKING: f32 = 0.16;

/// One ring of its body as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub at: Vec2,
    pub radius: f32,
    /// How deep it is, from 0 on the surface.
    pub depth: f32,
    /// The way the body runs here, toward the head.
    pub along: Vec2,
}

/// How it is moving and what it is doing, for drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WormPose {
    pub time: f32,
    /// How wide its mouth is open, 0 to 1, a little past while it snaps.
    pub jaw: f32,
    /// How far it has reared back to strike or scream, 0 to 1.
    pub rear: f32,
    /// The flush of being hurt, 0 to 1.
    pub hurt: f32,
    /// How far through its phases it is, 0 to 1: it grows rawer.
    pub rage: f32,
    /// What is in its throat: 0 nothing, 1 bile.
    pub bile: f32,
    /// How far its eyes are shut, 0 to 1.
    pub blink: f32,
    /// Its first ring is its head, or its last its tail.
    pub head: bool,
    pub tail: bool,
    /// Its first or last ring is a torn end.
    pub torn_front: bool,
    pub torn_back: bool,
    /// The number of its first ring along the whole worm, so its markings
    /// stay put on each half after the tear.
    pub first: usize,
    /// How far its plates swell in waves: 1 as designed, 0 holding still.
    pub motion: f32,
}

impl Default for WormPose {
    fn default() -> Self {
        Self {
            time: 0.,
            jaw: 0.,
            rear: 0.,
            hurt: 0.,
            rage: 0.,
            bile: 0.,
            blink: 0.,
            head: true,
            tail: true,
            torn_front: false,
            torn_back: false,
            first: 0,
            motion: 1.,
        }
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn faded(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha.clamp(0., 1.),
        ..color
    }
}

fn disc(emit: &mut impl FnMut(Primitive), center: Vec2, radius: f32, sides: u8, color: Color) {
    if radius > 0.05 {
        emit(Primitive::Polygon {
            center,
            sides,
            radius,
            angle: 0.,
            color,
        });
    }
}

fn tri(emit: &mut impl FnMut(Primitive), a: Vec2, b: Vec2, c: Vec2, color: Color) {
    emit(Primitive::Triangle {
        vertices: [a, b, c],
        color,
    });
}

fn quad(emit: &mut impl FnMut(Primitive), vertices: [Vec2; 4], color: Color) {
    emit(Primitive::Quad { vertices, color });
}

/// An oval `long` along `axis` and `wide` across it, as a fan of triangles.
fn oval(
    emit: &mut impl FnMut(Primitive),
    center: Vec2,
    axis: Vec2,
    (long, wide): (f32, f32),
    sides: usize,
    color: Color,
) {
    if long <= 0.05 || wide <= 0.05 {
        return;
    }
    let across = vec2(-axis.y, axis.x);
    let point = |n: usize| {
        let a = n as f32 / sides as f32 * TAU;
        center + axis * (a.cos() * long) + across * (a.sin() * wide)
    };
    let mut last = point(0);
    for n in 1..=sides {
        let next = point(n);
        tri(emit, center, last, next, color);
        last = next;
    }
}

/// An oval with a torn, uneven edge: each corner falls short by up to `jag`
/// of the way in, the same corners every frame.
fn ragged(
    emit: &mut impl FnMut(Primitive),
    center: Vec2,
    axis: Vec2,
    (long, wide): (f32, f32),
    salt: u32,
    jag: f32,
    color: Color,
) {
    const CORNERS: usize = 22;
    if long <= 0.05 || wide <= 0.05 {
        return;
    }
    let across = vec2(-axis.y, axis.x);
    let point = |n: usize| {
        let a = n as f32 / CORNERS as f32 * TAU;
        let short = 1. - jag * grain(n % CORNERS, salt);
        center + axis * (a.cos() * long * short) + across * (a.sin() * wide * short)
    };
    let mut last = point(0);
    for n in 1..=CORNERS {
        let next = point(n);
        tri(emit, center, last, next, color);
        last = next;
    }
}

/// A strand held at both ends that sags in the middle, thick where it clings
/// and thin where it stretches: saliva, bile or sinew.
fn sagging(
    emit: &mut impl FnMut(Primitive),
    a: Vec2,
    b: Vec2,
    sag: Vec2,
    width: f32,
    color: Color,
) {
    let middle = a.lerp(b, 0.5) + sag * 2.;
    let mut points = [a; 7];
    for (k, p) in points.iter_mut().enumerate() {
        let u = k as f32 / 6.;
        *p = a * (1. - u).powi(2) + middle * (2. * u * (1. - u)) + b * u * u;
    }
    strand(emit, &points[..4], width, color);
    let mut back = [b; 4];
    for (k, p) in back.iter_mut().enumerate() {
        *p = points[6 - k];
    }
    strand(emit, &back, width, color);
}

/// A tapering strip through `points`, `width` wide at its root and coming to
/// a point: a bristle, a feeler or a strand of torn flesh.
fn strand(emit: &mut impl FnMut(Primitive), points: &[Vec2], width: f32, color: Color) {
    strip(emit, points, width, 0., color);
}

/// A strip through `points`, `width` wide at its root and `end` times that at
/// its last point.
fn strip(emit: &mut impl FnMut(Primitive), points: &[Vec2], width: f32, end: f32, color: Color) {
    let count = points.len();
    if count < 2 {
        return;
    }
    let side = |n: usize| {
        let ahead = points[(n + 1).min(count - 1)] - points[n.saturating_sub(1)];
        let ahead = ahead.normalize_or_zero();
        let u = n as f32 / (count - 1) as f32;
        let half = width * 0.5 * (1. - u * (1. - end));
        vec2(-ahead.y, ahead.x) * half
    };
    for n in 0..count - 1 {
        let (a, b) = (side(n), side(n + 1));
        quad(
            emit,
            [
                points[n] + a,
                points[n + 1] + b,
                points[n + 1] - b,
                points[n] - a,
            ],
            color,
        );
    }
}

/// A small hash for markings that stay put: the same ring always gets the
/// same pores and scars.
fn grain(n: usize, salt: u32) -> f32 {
    let mut x = (n as u32).wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_535.
}

/// The ring as drawn: sinking rings shrink and darken into the floor, and a
/// slow wave of swelling runs down the body from the head.
fn shown(segment: &Segment, k: usize, pose: &WormPose) -> Option<(Vec2, f32, f32)> {
    if !segment.depth.is_finite()
        || segment.depth >= SEEN
        || !segment.at.is_finite()
        || !segment.along.is_finite()
        || !segment.radius.is_finite()
        || segment.radius <= 0.
    {
        return None;
    }
    let sink = ((segment.depth - SINKING) / (SEEN - SINKING)).clamp(0., 1.);
    let wave = (pose.time * (3.2 + 2. * pose.rage) - k as f32 * 0.62).sin();
    let swell = 1. + 0.05 * wave * pose.motion;
    let radius = segment.radius * swell * (1. - 0.38 * sink);
    Some((segment.at, radius, sink))
}

/// A curved plate catches the same upper-left light however the worm bends.
/// Parallel chords follow the light across the elliptical shell. Their shared
/// edges have identical positions and colors, including on a tight turn.
fn scute(
    emit: &mut impl FnMut(Primitive),
    center: Vec2,
    along: Vec2,
    radius: f32,
    skin: Color,
    shade: Color,
    sink: f32,
) {
    const STEPS: usize = 14;
    static CHORDS: std::sync::OnceLock<[(f32, f32); STEPS + 1]> = std::sync::OnceLock::new();
    let chords = CHORDS.get_or_init(|| {
        std::array::from_fn(|i| {
            let (half, u) = (PI * i as f32 / STEPS as f32).sin_cos();
            (-u, if i == 0 || i == STEPS { 0. } else { half })
        })
    });
    // Callers pass a direction, but the model is public: an unnormalised or
    // degenerate axis must not overflow into non-finite geometry.
    let along = along.normalize_or_zero();
    let across = along.perp();
    let sun = vec2(LIGHT.dot(along), LIGHT.dot(across));
    let to_screen = |p: Vec2| radius * (along * (0.61 * p.x) + across * (0.91 * p.y));
    let climb = to_screen(sun);
    let span = to_screen(sun.perp());
    let mut last = None;
    for &(u, half) in chords {
        // The rolled shoulder sweeps back where the plate is broadest.
        let mid = center + climb * u - along * (radius * 0.12 * half * half);
        let left = mid - span * half;
        let right = mid + span * half;
        let diffuse = (0.74 * half + 0.53 * u).clamp(0., 1.);
        let specular = (0.92 * half + 0.39 * u).clamp(0., 1.).powi(26);
        let surface = mix(
            mix(shade, skin, 0.15 + 0.85 * diffuse),
            SHEEN,
            (0.07 * diffuse + 0.24 * specular) * (1. - sink),
        );
        if let Some((prev_left, prev_right, color)) = last {
            emit(Primitive::ShadedQuad {
                vertices: [prev_left, left, right, prev_right],
                color,
                end: surface,
            });
        }
        last = Some((left, right, surface));
    }
}

/// The worm's body. `segments` run head first; a half passes only its own.
pub fn worm_geometry(segments: &[Segment], pose: &WormPose, emit: &mut impl FnMut(Primitive)) {
    worm_geometry_tagged(segments, pose, &mut |_, primitive| emit(primitive));
}

/// The same model with each primitive tagged by its body ring, so temporal
/// upscalers can follow the actual silhouette and each bend independently.
pub fn worm_geometry_tagged(
    segments: &[Segment],
    pose: &WormPose,
    emit: &mut impl FnMut(usize, Primitive),
) {
    if segments.is_empty() || !pose.time.is_finite() {
        return;
    }
    let count = segments.len();
    let rage = pose.rage.clamp(0., 1.);
    let hurt = pose.hurt.clamp(0., 1.);
    // Raw and blotched as it weakens, flushing pale when struck.
    let skin = mix(
        mix(FLESH, Color::new(0.74, 0.47, 0.46, 1.), rage),
        SHEEN,
        hurt * 0.5,
    );
    let shade = mix(BRUISE, Color::new(0.34, 0.07, 0.12, 1.), rage * 0.7);
    let mut stack = [None; WORM_RINGS];
    let mut overflow;
    let drawn = if count <= stack.len() {
        &mut stack[..count]
    } else {
        overflow = vec![None; count];
        &mut overflow[..]
    };
    for (k, segment) in segments.iter().enumerate() {
        drawn[k] = shown(segment, pose.first + k, pose);
    }
    // Rearing, its head lifts and draws back over its neck.
    if pose.head
        && let Some((at, radius, sink)) = drawn[0]
    {
        let rear = pose.rear.clamp(0., 1.2);
        drawn[0] = Some((
            at - segments[0].along * radius * 0.22 * rear,
            radius * (1. + 0.12 * rear),
            sink,
        ));
    }
    // Its shadow, lifting away from the head as it rears.
    for (k, item) in drawn.iter().enumerate().rev() {
        let Some((at, radius, sink)) = *item else {
            continue;
        };
        let emit = &mut |primitive| emit(k, primitive);
        let lift = if k == 0 && pose.head {
            0.2 + 0.3 * pose.rear.clamp(0., 1.)
        } else {
            0.16
        };
        let fade = 1. - sink;
        let offset = vec2(0.55, 0.83) * radius * lift;
        disc(
            emit,
            at + offset,
            radius * 1.08,
            24,
            faded(Color::new(0., 0., 0., 1.), 0.13 * fade),
        );
        disc(
            emit,
            at + offset * 0.8,
            radius * 0.98,
            24,
            faded(Color::new(0., 0., 0., 1.), 0.18 * fade),
        );
    }
    // Bristles first, so the body covers their roots. They row in a wave
    // that runs back down the body.
    for (k, item) in drawn.iter().enumerate().rev() {
        let Some((at, radius, sink)) = *item else {
            continue;
        };
        let emit = &mut |primitive| emit(k, primitive);
        if sink > 0.6 {
            continue;
        }
        let along = segments[k].along;
        let across = vec2(-along.y, along.x);
        let ring = pose.first + k;
        for side in [-1., 1.] {
            for n in 0..2 {
                let stroke = (pose.time * 7. - ring as f32 * 0.8 + n as f32 * 1.4 + side * 0.6)
                    .sin()
                    * pose.motion;
                let root =
                    at + across * side * radius * 0.9 - along * radius * (0.12 + 0.34 * n as f32);
                let out = (across * side * (0.8 + 0.2 * grain(ring * 2 + n, 3))
                    - along * (0.55 + 0.35 * stroke))
                    .normalize_or_zero();
                let length = radius * (0.34 + 0.12 * grain(ring * 2 + n, 7));
                let bend = vec2(-out.y, out.x) * side * length * 0.18 * stroke;
                strand(
                    emit,
                    &[root, root + out * length * 0.55 + bend, root + out * length],
                    (radius * 0.09).max(0.8),
                    faded(BRISTLE, 1. - sink),
                );
            }
        }
    }
    // Its outline, then its flesh, as one smooth tube.
    let edge = |radius: f32| (radius * 0.05).max(0.6);
    for pass in 0..2 {
        for (k, item) in drawn.iter().enumerate().rev() {
            let Some((at, radius, sink)) = *item else {
                continue;
            };
            let emit = &mut |primitive| emit(k, primitive);
            let (grow, color) = if pass == 0 {
                (edge(radius), OUTLINE)
            } else {
                (0., mix(shade, OUTLINE, sink * 0.6))
            };
            disc(emit, at, radius + grow, 28, color);
            if let Some(Some((next, next_radius, _))) = drawn.get(k + 1).copied() {
                let way = (next - at).normalize_or_zero();
                let across = vec2(-way.y, way.x);
                let next_grow = if pass == 0 { edge(next_radius) } else { 0. };
                quad(
                    emit,
                    [
                        at + across * (radius + grow),
                        next + across * (next_radius + next_grow),
                        next - across * (next_radius + next_grow),
                        at - across * (radius + grow),
                    ],
                    color,
                );
            }
        }
    }
    // Its plates, tail first so each overlaps the one behind it, with the
    // groove between them left dark.
    for (k, item) in drawn.iter().enumerate().rev() {
        let Some((at, radius, sink)) = *item else {
            continue;
        };
        let emit = &mut |primitive| emit(k, primitive);
        let along = segments[k].along;
        let across = vec2(-along.y, along.x);
        let ring = pose.first + k;
        let dim = 1. - 0.55 * sink;
        let blotch = grain(ring, 11) * rage;
        let plate = mix(mix(skin, shade, 0.12 + 0.3 * blotch), OUTLINE, sink * 0.5);
        let center = at + along * radius * 0.08;
        scute(
            emit,
            center,
            along,
            radius,
            plate,
            mix(shade, OUTLINE, sink * 0.5),
            sink,
        );
        // A vein under the skin, forking on every other plate, darker as it
        // is worn down. Too small to see on a far-off worm.
        if sink < 0.5 && radius > 8. {
            let bend = if grain(ring, 67) < 0.5 { -1. } else { 1. };
            let root =
                center - along * radius * 0.42 + across * radius * (grain(ring, 61) - 0.5) * 0.9;
            let fork = root + along * radius * 0.3 + across * bend * radius * 0.12;
            let vein = faded(
                mix(shade, BLOOD, 0.35 + 0.45 * rage),
                (0.2 + 0.45 * rage) * dim,
            );
            strand(
                emit,
                &[
                    root,
                    fork,
                    fork + along * radius * 0.26 - across * bend * radius * 0.08,
                ],
                radius * 0.05,
                vein,
            );
            if ring.is_multiple_of(2) {
                strand(
                    emit,
                    &[
                        fork,
                        fork + along * radius * 0.08 + across * bend * radius * 0.12,
                        fork + along * radius * 0.12 + across * bend * radius * 0.24,
                    ],
                    radius * 0.032,
                    vein,
                );
            }
        }
        // A broken, narrow glint follows the lip of the plate.
        let toward = LIGHT - along * LIGHT.dot(along) * 0.5;
        if radius > 7. && sink < 0.65 {
            let gleam = center + toward * radius * 0.35 + along * radius * 0.09;
            let length = radius * (0.15 + 0.12 * grain(ring, 17));
            strip(
                emit,
                &[
                    gleam - across * length,
                    gleam + along * radius * 0.055,
                    gleam + across * length * 0.8,
                ],
                radius * 0.038,
                0.,
                faded(SHEEN, (0.38 + 0.14 * grain(ring, 19)) * dim),
            );
        }
        if sink > 0.5 {
            continue;
        }
        // Recessed breathing slits open along the flanks. Their lips follow
        // the curvature of each plate and retain their shape after the tear.
        for (n, side) in [-1f32, 1.].into_iter().enumerate() {
            let mark = ring * 2 + n;
            let pore = at + across * side * radius * (0.69 + 0.06 * grain(mark, 13))
                - along * radius * (0.04 + 0.07 * grain(mark, 3));
            let size = radius * (0.06 + 0.03 * grain(mark, 5));
            let front = pore + along * (2.1 * size) - across * (side * size * 0.6);
            let back = pore - along * (1.75 * size) - across * (side * size * 0.22);
            let breath = (pose.time * 2.7 - ring as f32 * 0.54).sin() * pose.motion;
            let lip = across * (side * size);
            let outside = pore + lip * (0.7 + 0.18 * breath);
            let inside = pore - lip * 0.35;
            quad(
                emit,
                [front, outside + lip * 0.27, back, inside],
                mix(plate, SHEEN, 0.37 * dim),
            );
            quad(
                emit,
                [front, outside, back, inside],
                mix(shade, BLOOD, 0.24 + 0.56 * rage),
            );
            if radius > 8. && rage > 0.35 && grain(mark, 47) < (rage - 0.35) * 1.6 {
                // A tear of blood running back from it.
                let length = radius * (0.2 + 0.25 * grain(mark, 53)) * rage;
                let from = pore - along * size * 0.5;
                strand(
                    emit,
                    &[
                        from,
                        from - along * length * 0.5 + across * side * size * 0.25,
                        from - along * length,
                    ],
                    size * 1.1,
                    faded(BLOOD, 0.85 * dim),
                );
            }
        }
        // A ridge of bone down its back, on every other plate.
        if (ring + 1).is_multiple_of(2) && !(pose.head && k == 0) {
            let base = at - along * radius * 0.05;
            let tip = base - along * radius * 0.5 + toward * radius * 0.04;
            tri(
                emit,
                base + across * radius * 0.16,
                tip,
                base - across * radius * 0.16,
                BONE_SHADE,
            );
            tri(
                emit,
                base + across * radius * 0.08 + along * radius * 0.03,
                tip + along * radius * 0.05,
                base - across * radius * 0.08 + along * radius * 0.03,
                BONE,
            );
        }
        // Cracks of raw flesh opening up across it as it is worn down.
        if rage > 0.25 && grain(ring, 23) < rage * 0.8 {
            let tilt = (grain(ring, 29) - 0.5) * 1.2;
            let from = center + across * radius * (0.62 * grain(ring, 31) - 0.31);
            let way = (across * tilt.cos() + along * tilt.sin()).normalize_or_zero();
            let length = radius * (0.3 + 0.3 * grain(ring, 37));
            strand(
                emit,
                &[from - way * length * 0.5, from, from + way * length * 0.5],
                (radius * 0.11).max(0.8),
                mix(BLOOD, RAW, 0.4 + 0.3 * (pose.time * 2. + ring as f32).sin()),
            );
        }
    }
    if pose.tail
        && !pose.torn_back
        && let Some(Some((at, radius, sink))) = drawn.last().copied()
        && sink < 0.5
    {
        stinger(
            &mut |p| emit(count - 1, p),
            at,
            radius,
            segments[count - 1].along,
            pose,
        );
    }
    if pose.torn_back
        && let Some(Some((at, radius, sink))) = drawn.last().copied()
        && sink < 0.8
    {
        wound(
            &mut |p| emit(count - 1, p),
            at,
            radius,
            segments[count - 1].along * -1.,
            pose,
            1,
        );
    }
    if pose.torn_front
        && let Some((at, radius, sink)) = drawn[0]
        && sink < 0.8
    {
        wound(&mut |p| emit(0, p), at, radius, segments[0].along, pose, 2);
    }
    if pose.head
        && let Some((at, radius, sink)) = drawn[0]
        && sink < 0.85
    {
        head(
            &mut |p| emit(0, p),
            at,
            radius,
            segments[0].along,
            pose,
            skin,
            shade,
        );
    }
}

/// Its head: eyes along the brow, feelers, and the mouth.
fn head(
    emit: &mut impl FnMut(Primitive),
    at: Vec2,
    radius: f32,
    along: Vec2,
    pose: &WormPose,
    skin: Color,
    shade: Color,
) {
    let across = vec2(-along.y, along.x);
    let jaw = pose.jaw.clamp(0., 1.15);
    let time = pose.time;
    // Feelers that taste the air either side of the mouth.
    for side in [-1., 1.] {
        let root = at + along * radius * 0.6 + across * side * radius * 0.62;
        let mut points = [root; 6];
        for (n, point) in points.iter_mut().enumerate().skip(1) {
            let u = n as f32 / 5.;
            let sway = (time * 3.1 + u * 2.6 + side).sin() * 0.5 * pose.motion;
            let out = (along * (0.9 - 0.25 * jaw) + across * side * (0.55 + 0.3 * jaw + sway * u))
                .normalize_or_zero();
            *point = root + out * radius * 1.05 * u;
        }
        strand(emit, &points, (radius * 0.1).max(0.8), OUTLINE);
        strand(
            emit,
            &points,
            (radius * 0.06).max(0.6),
            mix(skin, shade, 0.4),
        );
    }
    // Eyes in two crescents on its brow: small, black, wet, and too many.
    for side in [-1., 1.] {
        for n in 0..4 {
            let angle = side * (0.62 + n as f32 * 0.24);
            let place = along * angle.cos() + across * angle.sin();
            let size = radius * (0.11 - 0.017 * n as f32);
            let spot = at + place * radius * (0.56 + 0.04 * n as f32);
            let lid = place.perp();
            let open = 1. - pose.blink.clamp(0., 1.);
            oval(
                emit,
                spot,
                lid,
                (size * 1.34, size * 0.98),
                10,
                mix(skin, shade, 0.67),
            );
            // The lids meet across the eye, leaving a crease when it dies.
            oval(
                emit,
                spot,
                lid,
                (size, size * (0.09 + 0.82 * open)),
                10,
                EYE,
            );
            if open > 0.12 {
                disc(
                    emit,
                    spot + LIGHT * size * 0.33 * open,
                    size * 0.27 * open,
                    8,
                    faded(SHEEN, 0.87 * open),
                );
            }
        }
    }
    mouth(emit, at, radius, along, pose, skin, shade);
}

/// Fixed gums and flexible lobes, rather than rotating rings. Each lobe has
/// its own resting length and a small delay under load, like living tissue.
fn mouth(
    emit: &mut impl FnMut(Primitive),
    at: Vec2,
    radius: f32,
    along: Vec2,
    pose: &WormPose,
    skin: Color,
    shade: Color,
) {
    let across = along.perp();
    let jaw = pose.jaw.clamp(0., 1.15);
    let opening = jaw.min(1.);
    let center = at + along * radius * (0.58 + 0.06 * opening);
    let maw = radius * 0.64 * opening;
    if maw > 0.5 {
        oval(
            emit,
            center,
            along,
            (maw * 1.1 + radius * 0.035, maw + radius * 0.035),
            28,
            OUTLINE,
        );
        // Wet gums, catching the light along one edge.
        oval(
            emit,
            center + LIGHT * maw * 0.03,
            along,
            (maw * 1.1, maw),
            28,
            mix(GUM, SHEEN, 0.3),
        );
        oval(
            emit,
            center - LIGHT * maw * 0.02,
            along,
            (maw * 1.08, maw * 0.98),
            28,
            GUM,
        );
        // The throat folds away from the lips, each fold darker and deeper
        // than the last, and squeezes in waves as it swallows or roars.
        let bile = pose.bile.clamp(0., 1.);
        for (n, (size, depth, color)) in [
            (0.93, 0.03, mix(GUM, MAW, 0.5)),
            (0.72, 0.07, mix(MAW, GUM, 0.12)),
            (0.48, 0.105, mix(MAW, DEEP, 0.4)),
            (0.24, 0.13, DEEP),
        ]
        .into_iter()
        .enumerate()
        {
            let squeeze = 1. + 0.035 * (pose.time * 5. - n as f32 * 1.1).sin() * pose.motion;
            oval(
                emit,
                center + along * maw * depth,
                along,
                (maw * size * 1.05 * squeeze, maw * size * 0.95 * squeeze),
                20,
                mix(color, mix(BILE, DEEP, 0.6), bile * 0.2 * (n as f32 / 3.)),
            );
        }
        // Ridges of muscle running down into it.
        for n in 0..9 {
            let a = (n as f32 + 0.5) / 9. * TAU + 0.2;
            let out = along * a.cos() + across * a.sin();
            let rim = center + along * maw * 0.03 + out * maw * 0.9;
            let deep = center + along * maw * 0.12 + out * maw * 0.24;
            strand(
                emit,
                &[rim, rim.lerp(deep, 0.5), deep],
                maw * 0.065,
                faded(DEEP, 0.36),
            );
        }
        // Teeth stay rooted in their gums, a fang now and then, and darker the
        // deeper they sit; only the soft palate moves.
        for (ring, count, reach) in [(0, 14, 0.86), (1, 11, 0.59), (2, 8, 0.35)] {
            let contraction = 1. + 0.035 * (pose.time * 5. - ring as f32 * 1.4).sin() * pose.motion;
            let sunk = along * maw * (0.03 + 0.035 * ring as f32);
            for n in 0..count {
                let a = n as f32 / count as f32 * TAU + ring as f32 * 0.31;
                let out = along * a.cos() + across * a.sin();
                let side = out.perp();
                let base =
                    center + sunk + (out + along * (a.cos() * 0.1)) * maw * reach * contraction;
                let fang: f32 = if ring == 0 && n % 4 == 1 { 1.4 } else { 1. };
                let width = maw * (0.048 - ring as f32 * 0.009) * fang.sqrt();
                let length =
                    maw * (0.21 - ring as f32 * 0.035) * (0.72 + 0.5 * grain(n, 40 + ring)) * fang;
                let hook = base - out * length * 0.58 + side * length * 0.13;
                let tip = base - out * length;
                disc(emit, base, width * 1.25, 6, mix(GUM, DEEP, 0.35));
                tri(
                    emit,
                    base + side * width,
                    tip,
                    base - side * width,
                    BONE_SHADE,
                );
                quad(
                    emit,
                    [
                        base + side * width * 0.7,
                        hook + side * width * 0.35,
                        tip,
                        base - side * width * 0.25,
                    ],
                    mix(BONE, BONE_SHADE, ring as f32 * 0.22),
                );
            }
        }
        if bile > 0.05 {
            // Bile welling up the throat: a glossy pool with bubbles in it,
            // and strings of it reaching up over the teeth.
            let pool = center + along * maw * 0.11;
            let size = maw * (0.14 + 0.2 * bile);
            for n in 0..3 {
                let a = grain(n, 71) * TAU;
                let out = along * a.cos() + across * a.sin();
                let reach = center + along * maw * 0.04 + out * maw * (0.5 + 0.3 * bile);
                strand(
                    emit,
                    &[pool + out * size * 0.7, pool.lerp(reach, 0.6), reach],
                    size * 0.3,
                    faded(BILE, 0.8 * bile),
                );
            }
            ragged(
                emit,
                pool,
                along,
                (size * 1.08, size),
                41,
                0.2,
                faded(mix(BILE, DEEP, 0.55), bile),
            );
            ragged(
                emit,
                pool - LIGHT * size * 0.08,
                along,
                (size * 0.9, size * 0.8),
                43,
                0.18,
                faded(BILE, bile),
            );
            for n in 0..3 {
                let rise = (pose.time * (1.3 + 0.4 * n as f32) + n as f32 * 2.1).fract();
                let a = grain(n, 73) * TAU;
                let at = pool + (along * a.cos() + across * a.sin()) * size * 0.5 * rise;
                let r = size * (0.06 + 0.08 * grain(n, 79)) * (rise * PI).sin();
                disc(emit, at, r, 8, faded(mix(BILE, SHEEN, 0.35), bile));
                disc(
                    emit,
                    at + LIGHT * r * 0.4,
                    r * 0.35,
                    6,
                    faded(SHEEN, 0.8 * bile),
                );
            }
            oval(
                emit,
                pool + LIGHT * size * 0.45,
                along,
                (size * 0.24, size * 0.11),
                10,
                faded(SHEEN, 0.6 * bile),
            );
        }
        // Saliva stretched across the opening, thinning as the mouth gapes.
        if opening > 0.5 && radius > 24. {
            let thin = 1. - 0.6 * ((opening - 0.5) / 0.5).clamp(0., 1.);
            for (n, (from, to)) in [(0.9f32, 3.8f32), (2.3, 5.5)].into_iter().enumerate() {
                let rim = |a: f32| {
                    center
                        + along * maw * 0.03
                        + (along * a.cos() + across * a.sin() * 0.92) * maw * 0.86
                };
                let sag =
                    maw * (0.07 + 0.04 * (pose.time * 1.7 + n as f32 * 2.).sin() * pose.motion);
                sagging(
                    emit,
                    rim(from),
                    rim(to),
                    vec2(0., sag),
                    (maw * 0.03).max(0.6),
                    faded(SHEEN, 0.34 * thin),
                );
            }
        }
    }
    const LOBES: usize = 5;
    let mut tips = [center; LOBES];
    for (n, saved_tip) in tips.iter_mut().enumerate() {
        let phase = n as f32 * 1.71;
        // The shorter folds start drawing back first. They all meet at the
        // same bite and settle fully open regardless of frame timing.
        let lead = (grain(n, 69) - 0.5) * opening * (1. - opening) * 0.56;
        let flex =
            (opening + lead + 0.045 * (pose.time * 3.4 - phase).sin() * opening * pose.motion)
                .clamp(0., 1.04);
        let a = n as f32 / LOBES as f32 * TAU + PI / LOBES as f32;
        let out = along * a.cos() + across * a.sin();
        let side = out.perp();
        let root = center + out * radius * (0.2 + 0.39 * flex);
        let shut = center + along * radius * (0.5 + 0.04 * grain(n, 63));
        let tip = shut.lerp(center + out * radius * (0.91 + 0.1 * grain(n, 64)), flex);
        *saved_tip = tip;
        let control = root.lerp(tip, 0.54) + side * radius * (0.055 + 0.025 * grain(n, 65));
        let sample = |u: f32| {
            let p = root * (1. - u).powi(2) + control * (2. * u * (1. - u)) + tip * u * u;
            let tangent = ((control - root) * (1. - u) + (tip - control) * u).normalize_or_zero();
            let normal = if tangent == Vec2::ZERO {
                side
            } else {
                tangent.perp()
            };
            // A broad muscular base, a rounded fold, and a narrow lip.
            let width = radius
                * (0.19 + 0.045 * grain(n, 66))
                * (1. - u).sqrt()
                * (1. + (0.12 + 0.18 * flex) * (u * PI).sin());
            (p, normal, width)
        };
        let steps = if radius > 45. { 9 } else { 6 };
        for pass in 0..2 {
            let mut before = sample(0.);
            for k in 1..=steps {
                let u = k as f32 / steps as f32;
                let now = sample(u);
                let extra = if pass == 0 {
                    (radius * 0.022).max(0.45)
                } else {
                    0.
                };
                let color = if pass == 0 {
                    OUTLINE
                } else {
                    mix(skin, shade, 0.16 + 0.25 * u)
                };
                quad(
                    emit,
                    [
                        before.0 + before.1 * (before.2 + extra),
                        now.0 + now.1 * (now.2 + extra),
                        now.0 - now.1 * (now.2 + extra),
                        before.0 - before.1 * (before.2 + extra),
                    ],
                    color,
                );
                if pass == 1 {
                    // The inside unfolds continuously; it doesn't pop into view.
                    emit(Primitive::ShadedQuad {
                        vertices: [
                            before.0,
                            before.0 - before.1 * before.2 * 0.85,
                            now.0 - now.1 * now.2 * 0.85,
                            now.0,
                        ],
                        color,
                        end: mix(color, GUM, opening * 0.88),
                    });
                    if k < steps - 1 {
                        quad(
                            emit,
                            [
                                before.0 + before.1 * before.2 * 0.45,
                                now.0 + now.1 * now.2 * 0.45,
                                now.0 + now.1 * now.2 * 0.55,
                                before.0 + before.1 * before.2 * 0.55,
                            ],
                            mix(color, SHEEN, 0.28),
                        );
                    }
                }
                before = now;
            }
        }
        // Hooked teeth on the inner lip move with the fold they're attached to.
        for u in [0.55, 0.77] {
            let (p, normal, width) = sample(u);
            let root = p - normal * width * 0.66;
            let tooth = radius * 0.12 * opening;
            if tooth < 0.05 {
                continue;
            }
            tri(
                emit,
                root - out * tooth * 0.34,
                root - normal * tooth,
                root + out * tooth * 0.34,
                BONE,
            );
        }
    }
    // Fine saliva bridges stretch between the lower lips and sag under their
    // own weight. These are reflective strands, drawn with the lit body.
    if opening > 0.46 && radius > 24. {
        for n in [0, 3] {
            let a = tips[n].lerp(center, 0.17);
            let b = tips[(n + 1) % LOBES].lerp(center, 0.17);
            let sag = radius * (0.06 + 0.035 * (pose.time * 2.1 + n as f32).sin() * pose.motion);
            sagging(
                emit,
                a,
                b,
                vec2(0., sag),
                (radius * 0.016).max(0.5),
                faded(SHEEN, 0.38 * opening),
            );
        }
    }
}

/// The bone barb at the tip of its tail.
fn stinger(emit: &mut impl FnMut(Primitive), at: Vec2, radius: f32, along: Vec2, pose: &WormPose) {
    let back = along * -1.;
    let across = vec2(-back.y, back.x);
    let curl = (pose.time * 2.3).sin() * 0.35 * pose.motion;
    let mut points = [at; 5];
    for (n, point) in points.iter_mut().enumerate() {
        let u = n as f32 / 4.;
        *point = at + back * radius * (0.6 + 1.5 * u) + across * radius * curl * u * u;
    }
    strand(emit, &points, radius * 1.05, OUTLINE);
    strand(emit, &points, radius * 0.85, BONE_SHADE);
    strand(emit, &points[1..], radius * 0.45, BONE);
}

/// A torn end, facing `out`: a ragged cut through skin, the pale layer under
/// it and the muscle, vessels that still pump, the snapped spine, and flaps of
/// skin and sinew hanging past it.
fn wound(
    emit: &mut impl FnMut(Primitive),
    at: Vec2,
    radius: f32,
    out: Vec2,
    pose: &WormPose,
    salt: u32,
) {
    let across = vec2(-out.y, out.x);
    let face = at + out * radius * 0.42;
    let time = pose.time;
    // Flaps of skin round the rim of the cut: unevenly spaced and sized,
    // some curling away and some folded back over the body, raw beneath, and
    // ending blunt and torn rather than sharp.
    for n in 0..7 {
        let a = ((n as f32 + 0.5 + (grain(n, salt * 23) - 0.5) * 0.8) / 7. * 2. - 1.) * 1.4;
        let root = face + out * (a.cos() * radius * 0.3) + across * (a.sin() * radius * 0.84);
        let length = radius * (0.1 + 0.26 * grain(n, salt * 13));
        let sway = (time * 2.4 + n as f32 * 1.7).sin() * 0.25 * pose.motion;
        let folded = grain(n, salt * 29) < 0.3;
        let way = if folded {
            -out * 0.55 + across * a.sin().signum() * 0.85
        } else {
            out * a.cos() + across * (a.sin() * 0.8 + sway * 0.4)
        }
        .normalize_or_zero();
        let curl = way.perp() * length * 0.35 * (grain(n, salt * 17) - 0.5);
        let tip = root + way * length + curl * 1.8;
        let points = [root, root + way * length * 0.55 + curl, tip];
        let skin = mix(FLESH, RAW, 0.12 + 0.28 * grain(n, salt * 19));
        let width = radius * (0.13 + 0.13 * grain(n, salt * 31));
        strip(emit, &points, width + radius * 0.05, 0.5, OUTLINE);
        disc(emit, tip, width * 0.3 + radius * 0.025, 8, OUTLINE);
        strip(emit, &points, width, 0.45, skin);
        disc(emit, tip, width * 0.24, 8, skin);
        strip(emit, &points, width * 0.4, 0.4, mix(RAW, BLOOD, 0.35));
    }
    // The cut itself, edged with blood, then the layers it went through.
    ragged(
        emit,
        face,
        out,
        (radius * 0.42, radius * 0.95),
        salt,
        0.1,
        OUTLINE,
    );
    ragged(
        emit,
        face,
        out,
        (radius * 0.37, radius * 0.89),
        salt,
        0.1,
        BLOOD,
    );
    ragged(
        emit,
        face + out * radius * 0.02,
        out,
        (radius * 0.31, radius * 0.77),
        salt + 1,
        0.07,
        FAT,
    );
    ragged(
        emit,
        face + out * radius * 0.04,
        out,
        (radius * 0.28, radius * 0.71),
        salt + 2,
        0.09,
        RAW,
    );
    // Bundles of muscle, cut across.
    let middle = face + out * radius * 0.06;
    for n in 0..14 {
        let a = (n as f32 + grain(n, salt * 7) * 0.4) / 14. * TAU;
        let way = out * (a.cos() * 0.4) + across * a.sin();
        let from = middle + way * radius * 0.24;
        let to = middle + way * radius * (0.6 + 0.08 * grain(n, salt * 11));
        strand(
            emit,
            &[to, to.lerp(from, 0.5), from],
            radius * 0.05,
            mix(RAW, BLOOD, 0.5),
        );
    }
    oval(
        emit,
        middle,
        out,
        (radius * 0.16, radius * 0.42),
        14,
        mix(RAW, BLOOD, 0.4),
    );
    // Severed vessels, welling up with each beat.
    let beat = (0.5 + 0.5 * (time * 4.).sin()).powi(3);
    for n in 0..6 {
        let a = grain(n, salt * 31) * TAU;
        let reach = 0.3 + 0.34 * grain(n, salt * 37);
        let spot = middle + (out * (a.cos() * 0.4) + across * a.sin()) * radius * reach;
        let size = radius * (0.03 + 0.03 * grain(n, salt * 41));
        disc(emit, spot, size * 1.6, 8, mix(RAW, SHEEN, 0.18));
        disc(emit, spot, size, 8, Color::new(0.14, 0., 0.02, 1.));
        disc(emit, spot, size * 0.85 * beat, 6, RAW);
    }
    // The spine, snapped, with splinters standing out of it.
    let spine = face + out * radius * 0.08;
    for n in 0..4 {
        let a = (n as f32 + 0.3 * grain(n, salt * 43)) / 4. * TAU + 0.4;
        let way = (out * 0.6 + (out * a.cos() * 0.4 + across * a.sin())).normalize_or_zero();
        let side = way.perp();
        let base = spine + way * radius * 0.12;
        let tip = base + way * radius * (0.1 + 0.1 * grain(n, salt * 47));
        tri(
            emit,
            base + side * radius * 0.05,
            tip,
            base - side * radius * 0.05,
            BONE_SHADE,
        );
        tri(emit, base + side * radius * 0.025, tip, base, BONE);
    }
    ragged(
        emit,
        spine,
        out,
        (radius * 0.2, radius * 0.2),
        salt + 3,
        0.16,
        BONE_SHADE,
    );
    ragged(
        emit,
        spine + LIGHT * radius * 0.015,
        out,
        (radius * 0.15, radius * 0.15),
        salt + 4,
        0.14,
        BONE,
    );
    disc(emit, spine, radius * 0.07, 8, BLOOD);
    disc(emit, spine, radius * 0.04, 6, RAW);
    // Sinew trailing from the cut, curling as it sways, a bead of blood
    // gathering at the end of some.
    for n in 0..3 {
        let u = grain(n, salt * 53) * 2. - 1.;
        let root = face + across * u * radius * 0.55 + out * radius * 0.1;
        let sway = (time * 1.9 + n as f32 * 2.3).sin() * 0.35 * pose.motion;
        let way = (out + across * (u * 0.3 + sway)).normalize_or_zero();
        let length = radius * (0.16 + 0.18 * grain(n, salt * 59));
        let bow = way.perp() * length * (0.25 + 0.2 * sway);
        let end = root + way * length - bow * 0.5;
        strand(
            emit,
            &[root, root.lerp(end, 0.5) + bow, end],
            radius * 0.045,
            faded(mix(FAT, RAW, 0.6), 0.9),
        );
        if n != 1 {
            disc(emit, end, radius * 0.026, 8, BLOOD);
            disc(
                emit,
                end + LIGHT * radius * 0.01,
                radius * 0.009,
                6,
                faded(SHEEN, 0.6),
            );
        }
    }
    // Wet where the blood still runs.
    oval(
        emit,
        face + out * radius * 0.1 + LIGHT * radius * 0.22,
        out,
        (radius * 0.05, radius * 0.24),
        8,
        faded(Color::new(1., 0.6, 0.6, 1.), 0.3 + 0.3 * beat),
    );
    oval(
        emit,
        face + out * radius * 0.05 + LIGHT * radius * 0.5,
        out,
        (radius * 0.03, radius * 0.12),
        6,
        faded(SHEEN, 0.35),
    );
}

/// The worm curled up small, for its card and the model sheet: a coil of
/// its own rings with its mouth open toward the viewer's left.
pub fn worm_card_geometry(center: Vec2, size: f32, time: f32, emit: &mut impl FnMut(Primitive)) {
    if !size.is_finite() || size <= 0. {
        return;
    }
    let scale = size / tankyard_core::sunderworm::WORM_RADIUS * 0.26;
    let segments: [_; WORM_RINGS] = std::array::from_fn(|k| {
        let u = k as f32 / (WORM_RINGS - 1) as f32;
        // A spiral, tightest at the tail.
        let angle = PI + u * TAU * 1.35;
        let reach = size * (0.72 - 0.5 * u);
        let at = center + vec2(angle.cos(), angle.sin()) * reach;
        let ahead = vec2(-(angle).sin(), angle.cos()) * -1.;
        Segment {
            at,
            radius: tankyard_core::sunderworm::ring_radius(k) * scale,
            depth: 0.,
            along: ahead,
        }
    });
    worm_geometry(
        &segments,
        &WormPose {
            time,
            jaw: 0.55 + 0.15 * (time * 1.7).sin(),
            rage: 0.2,
            ..WormPose::default()
        },
        emit,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(depth: f32) -> Vec<Segment> {
        (0..tankyard_core::sunderworm::WORM_RINGS)
            .map(|k| Segment {
                at: vec2(600. - k as f32 * 30., 400. + (k as f32 * 0.4).sin() * 40.),
                radius: tankyard_core::sunderworm::ring_radius(k) * 0.45,
                depth,
                along: vec2(1., 0.),
            })
            .collect()
    }

    fn count(segments: &[Segment], pose: &WormPose) -> (usize, Vec<Color>) {
        let mut drawn = 0;
        let mut colors = Vec::new();
        worm_geometry(segments, pose, &mut |primitive| {
            drawn += 1;
            let finite = match primitive {
                Primitive::Polygon { center, radius, .. } => {
                    center.is_finite() && radius.is_finite()
                }
                Primitive::Triangle { vertices, .. } => vertices.iter().all(|v| v.is_finite()),
                Primitive::Artwork { vertices, .. }
                | Primitive::Quad { vertices, .. }
                | Primitive::ShadedQuad { vertices, .. } => vertices.iter().all(|v| v.is_finite()),
            };
            assert!(finite, "{primitive:?}");
            colors.push(primitive.color());
        });
        (drawn, colors)
    }

    #[test]
    fn scute_strips_share_positions_and_light_at_every_seam() {
        for angle in [0., 0.7, 1.6, PI, 4.8] {
            let mut last = None;
            let mut strips = 0;
            scute(
                &mut |part| {
                    let Primitive::ShadedQuad {
                        vertices: [a, b, c, d],
                        color,
                        end,
                    } = part
                    else {
                        panic!("expected a shaded strip")
                    };
                    assert!([a, b, c, d].into_iter().all(Vec2::is_finite));
                    assert_eq!(color.a, end.a);
                    if let Some((left, right, shade)) = last {
                        assert_eq!((a, d, color), (left, right, shade));
                    } else {
                        assert_eq!(a, d);
                    }
                    last = Some((b, c, end));
                    strips += 1;
                },
                Vec2::ZERO,
                Vec2::from_angle(angle),
                60.,
                FLESH,
                BRUISE,
                0.1,
            );
            let (b, c, _) = last.unwrap();
            assert_eq!(b, c);
            assert_eq!(strips, 14);
        }
        for along in [Vec2::ZERO, Vec2::splat(1e30)] {
            scute(
                &mut |part| {
                    assert!(part.color().a.is_finite());
                    match part {
                        Primitive::ShadedQuad {
                            vertices,
                            color,
                            end,
                        } => {
                            assert!(vertices.into_iter().all(Vec2::is_finite));
                            assert!(color.a.is_finite() && end.a.is_finite());
                        }
                        _ => panic!("expected a shaded strip"),
                    }
                },
                Vec2::ZERO,
                along,
                60.,
                FLESH,
                BRUISE,
                0.1,
            );
        }
    }

    #[test]
    fn the_whole_worm_draws_within_budget_and_nothing_shows_underground() {
        let pose = WormPose {
            jaw: 1.,
            rage: 1.,
            rear: 1.,
            ..WormPose::default()
        };
        let (drawn, _) = count(&body(0.), &pose);
        assert!(drawn > 400 && drawn < 2400, "{drawn}");
        let (buried, _) = count(&body(1.), &pose);
        assert_eq!(buried, 0);
        // Half under: only the rings above ground.
        let mut half = body(0.);
        for segment in half.iter_mut().skip(14) {
            segment.depth = 0.9;
        }
        let (some, _) = count(&half, &pose);
        assert!(some > 0 && some < drawn);
    }

    #[test]
    fn an_open_mouth_shows_teeth_and_a_torn_end_shows_raw_flesh() {
        let shut = count(&body(0.), &WormPose::default()).1;
        let open = count(
            &body(0.),
            &WormPose {
                jaw: 1.,
                ..WormPose::default()
            },
        )
        .1;
        assert!(
            !shut.contains(&BONE)
                || open.iter().filter(|&&c| c == BONE).count()
                    > shut.iter().filter(|&&c| c == BONE).count() + 20
        );
        assert!(!shut.contains(&RAW));
        let torn = count(
            &body(0.),
            &WormPose {
                torn_back: true,
                tail: false,
                ..WormPose::default()
            },
        )
        .1;
        assert!(torn.contains(&RAW));
    }

    #[test]
    fn motion_tags_follow_the_body_including_the_mouth_and_torn_ends() {
        let full = body(0.);
        for (start, end) in [(0, 28), (0, 13), (13, 28)] {
            let segments = &full[start..end];
            let pose = WormPose {
                jaw: 1.,
                rear: 0.8,
                first: start,
                head: start == 0,
                tail: end == 28,
                torn_front: start != 0,
                torn_back: end != 28,
                ..Default::default()
            };
            let mut opaque = vec![0; segments.len()];
            worm_geometry_tagged(segments, &pose, &mut |ring, primitive| {
                assert!(ring < segments.len());
                if primitive.color().a < 0.6 {
                    return;
                }
                opaque[ring] += 1;
                let bounds = primitive.bounds();
                let middle = vec2(bounds.x + bounds.w * 0.5, bounds.y + bounds.h * 0.5);
                assert!(middle.distance(segments[ring].at) < segments[ring].radius * 3.2 + 66.);
            });
            assert!(
                opaque.iter().all(|&count| count > 1),
                "every surfaced ring carries its own motion"
            );
        }
    }

    #[test]
    fn the_card_fits_its_box() {
        let mut lo = Vec2::splat(f32::INFINITY);
        let mut hi = Vec2::splat(f32::NEG_INFINITY);
        worm_card_geometry(vec2(100., 100.), 60., 1.3, &mut |primitive| {
            let bounds = primitive.bounds();
            lo = lo.min(vec2(bounds.x, bounds.y));
            hi = hi.max(vec2(bounds.x + bounds.w, bounds.y + bounds.h));
        });
        assert!(
            lo.x > 20. && lo.y > 20. && hi.x < 180. && hi.y < 180.,
            "{lo} {hi}"
        );
    }
}
