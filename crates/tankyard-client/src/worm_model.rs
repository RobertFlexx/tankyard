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

/// A tapering strip through `points`, `width` wide at its root and coming to
/// a point: a bristle, a feeler or a strand of torn flesh.
fn strand(emit: &mut impl FnMut(Primitive), points: &[Vec2], width: f32, color: Color) {
    let count = points.len();
    if count < 2 {
        return;
    }
    let side = |n: usize| {
        let ahead = points[(n + 1).min(count - 1)] - points[n.saturating_sub(1)];
        let ahead = ahead.normalize_or_zero();
        let half = width * 0.5 * (1. - n as f32 / (count - 1) as f32);
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
    if segment.depth >= SEEN
        || !segment.at.is_finite()
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
    let mut drawn: Vec<Option<(Vec2, f32, f32)>> = segments
        .iter()
        .enumerate()
        .map(|(k, segment)| shown(segment, pose.first + k, pose))
        .collect();
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
        let lit = mix(plate, SHEEN, 0.32 * dim);
        let center = at + along * radius * 0.08;
        oval(
            emit,
            center,
            along,
            (radius * 0.58, radius * 0.9),
            14,
            plate,
        );
        // The lit front edge of the plate and a wet highlight on it.
        let toward = LIGHT - along * LIGHT.dot(along) * 0.5;
        oval(
            emit,
            center + along * radius * 0.16 + toward * radius * 0.18,
            along,
            (radius * 0.3, radius * 0.6),
            12,
            lit,
        );
        oval(
            emit,
            center + along * radius * 0.2 + toward * radius * 0.4,
            along,
            (radius * 0.07, radius * 0.2),
            8,
            faded(SHEEN, 0.7 * dim),
        );
        if sink > 0.5 {
            continue;
        }
        // A pore on each side that weeps as it weakens.
        for side in [-1., 1.] {
            let pore = at + across * side * radius * 0.7 - along * radius * 0.05;
            let size = radius * (0.07 + 0.03 * grain(ring, 5));
            disc(emit, pore, size * 1.35, 10, faded(shade, dim));
            disc(emit, pore, size, 10, mix(EYE, BLOOD, rage));
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
            disc(emit, spot, size * 1.4, 12, faded(shade, 0.9));
            let open = (1. - pose.blink.clamp(0., 1.)).max(0.12);
            disc(emit, spot, size * open, 12, EYE);
            disc(
                emit,
                spot + LIGHT * size * 0.4 * open,
                size * 0.32 * open,
                6,
                faded(SHEEN, 0.9),
            );
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
        oval(emit, center, along, (maw * 1.1, maw), 28, GUM);
        oval(
            emit,
            center + along * maw * 0.08,
            along,
            (maw * 0.91, maw * 0.82),
            28,
            mix(MAW, BILE, pose.bile * 0.4),
        );
        // The throat contracts back toward its dark center. Teeth stay rooted
        // in their gums; only the soft palate moves as it swallows or roars.
        for (ring, count, reach) in [(0, 14, 0.86), (1, 11, 0.59), (2, 8, 0.35)] {
            let contraction = 1. + 0.035 * (pose.time * 5. - ring as f32 * 1.4).sin() * pose.motion;
            for n in 0..count {
                let a = n as f32 / count as f32 * TAU + ring as f32 * 0.31;
                let out = along * a.cos() + across * a.sin();
                let side = out.perp();
                let base = center + (out + along * (a.cos() * 0.1)) * maw * reach * contraction;
                let width = maw * (0.048 - ring as f32 * 0.009);
                let length =
                    maw * (0.21 - ring as f32 * 0.035) * (0.72 + 0.5 * grain(n, 40 + ring));
                let hook = base - out * length * 0.58 + side * length * 0.13;
                let tip = base - out * length;
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
                    mix(BONE, BONE_SHADE, ring as f32 * 0.13),
                );
            }
        }
        oval(
            emit,
            center + along * maw * 0.08,
            along,
            (maw * 0.22, maw * 0.18),
            16,
            Color::new(0.035, 0.004, 0.01, 1.),
        );
        if pose.bile > 0.05 {
            oval(
                emit,
                center,
                along,
                (maw * 0.27, maw * 0.19),
                16,
                faded(BILE, pose.bile * 0.55),
            );
        }
    }
    const LOBES: usize = 5;
    let mut tips = [center; LOBES];
    for (n, saved_tip) in tips.iter_mut().enumerate() {
        let phase = n as f32 * 1.71;
        let flex = (opening + 0.045 * (pose.time * 3.4 - phase).sin() * opening * pose.motion)
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
                * (1. + 0.12 * (u * PI).sin());
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
                    quad(
                        emit,
                        [
                            before.0,
                            now.0,
                            now.0 - now.1 * now.2 * 0.75,
                            before.0 - before.1 * before.2 * 0.75,
                        ],
                        mix(color, GUM, opening * 0.8),
                    );
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
            let sag = radius * (0.12 + 0.07 * (pose.time * 2.1 + n as f32).sin() * pose.motion);
            let middle = a.lerp(b, 0.5) + vec2(0., sag);
            let mut points = [a; 7];
            for (k, p) in points.iter_mut().enumerate() {
                let u = k as f32 / 6.;
                *p = a * (1. - u).powi(2) + middle * (2. * u * (1. - u)) + b * u * u;
            }
            strand(
                emit,
                &points,
                (radius * 0.009).max(0.5),
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

/// A torn end, facing `out`: the raw cut, the severed spine in the middle,
/// and ragged flaps of skin around it.
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
    // Flaps of skin, of uneven lengths, hanging past the cut.
    for n in 0..9 {
        let u = n as f32 / 8. * 2. - 1.;
        let root = face + across * u * radius * 0.86 - out * radius * 0.12 * u * u;
        let length = radius * (0.2 + 0.32 * grain(n, salt * 13));
        let sway = (pose.time * 2.4 + n as f32 * 1.7).sin() * 0.25 * pose.motion;
        let way = (out + across * (u * 0.5 + sway)).normalize_or_zero();
        let color = if n % 3 == 1 { RAW } else { FLESH };
        strand(
            emit,
            &[root, root + way * length * 0.6, root + way * length],
            radius * 0.28,
            OUTLINE,
        );
        strand(
            emit,
            &[root, root + way * length * 0.6, root + way * length],
            radius * 0.2,
            color,
        );
    }
    oval(emit, face, out, (radius * 0.4, radius * 0.92), 16, OUTLINE);
    oval(emit, face, out, (radius * 0.34, radius * 0.86), 16, BLOOD);
    oval(
        emit,
        face + out * radius * 0.04,
        out,
        (radius * 0.26, radius * 0.7),
        16,
        RAW,
    );
    // Rings of muscle round the cut, and the spine through the middle.
    oval(
        emit,
        face + out * radius * 0.06,
        out,
        (radius * 0.16, radius * 0.44),
        12,
        mix(RAW, BLOOD, 0.5),
    );
    disc(
        emit,
        face + out * radius * 0.08,
        radius * 0.2,
        12,
        BONE_SHADE,
    );
    disc(emit, face + out * radius * 0.08, radius * 0.15, 12, BONE);
    disc(emit, face + out * radius * 0.08, radius * 0.06, 8, BLOOD);
    // Wet where the blood still runs.
    let pulse = 0.5 + 0.5 * (pose.time * 4.).sin();
    oval(
        emit,
        face + out * radius * 0.1 + LIGHT * radius * 0.2,
        out,
        (radius * 0.05, radius * 0.24),
        8,
        faded(Color::new(1., 0.6, 0.6, 1.), 0.35 + 0.25 * pulse),
    );
}

/// The worm curled up small, for its card and the model sheet: a coil of
/// its own rings with its mouth open toward the viewer's left.
pub fn worm_card_geometry(center: Vec2, size: f32, time: f32, emit: &mut impl FnMut(Primitive)) {
    if !size.is_finite() || size <= 0. {
        return;
    }
    let rings = tankyard_core::sunderworm::WORM_RINGS;
    let scale = size / tankyard_core::sunderworm::WORM_RADIUS * 0.26;
    let mut segments = Vec::with_capacity(rings);
    for k in 0..rings {
        let u = k as f32 / (rings - 1) as f32;
        // A spiral, tightest at the tail.
        let angle = PI + u * TAU * 1.35;
        let reach = size * (0.72 - 0.5 * u);
        let at = center + vec2(angle.cos(), angle.sin()) * reach;
        let ahead = vec2(-(angle).sin(), angle.cos()) * -1.;
        segments.push(Segment {
            at,
            radius: tankyard_core::sunderworm::ring_radius(k) * scale,
            depth: 0.,
            along: ahead,
        });
    }
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
                Primitive::Quad { vertices, .. } => vertices.iter().all(|v| v.is_finite()),
            };
            assert!(finite, "{primitive:?}");
            colors.push(primitive.color());
        });
        (drawn, colors)
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
