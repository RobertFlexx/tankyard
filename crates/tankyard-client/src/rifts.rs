//! Rifts on the arena floor: windows down into deep space. The space sits
//! far below the floor in three layers, so it slides past the rim as the
//! camera moves, the far stars slowest. Entries wind their light in toward
//! the heart and exits pour it out; a rift still waiting for its other end
//! shows a broken rim. Each player sees their own side's rifts in violet and
//! teal and an opponent's in crimson and amber, ringed with thorns.
use crate::graphics::draw_circle;
use crate::render::View;
use crate::ui::mix;
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use tankyard_core::{
    EntityId, RIFT_CLOSING, RIFT_RADIUS, RIFT_SECONDS, Rift, RiftEnd, RiftShot, Snapshot, V2,
};

/// Your own side's rifts: entries violet, exits teal.
pub const ENTRY: Color = Color::new(0.72, 0.45, 1.0, 1.);
pub const EXIT: Color = Color::new(0.27, 0.96, 0.84, 1.);
/// An opponent's rifts: entries crimson, exits amber.
pub const HOSTILE_ENTRY: Color = Color::new(1.0, 0.27, 0.36, 1.);
pub const HOSTILE_EXIT: Color = Color::new(1.0, 0.64, 0.2, 1.);
/// How long a rift takes to open, overshooting a little.
const OPENING: f32 = 0.42;
/// The drawn quad reaches past the rim, for the rim's soft edge.
const QUAD: f32 = 1.25;

pub fn color(end: RiftEnd, hostile: bool) -> Color {
    match (end, hostile) {
        (RiftEnd::Entry, false) => ENTRY,
        (RiftEnd::Exit, false) => EXIT,
        (RiftEnd::Entry, true) => HOSTILE_ENTRY,
        (RiftEnd::Exit, true) => HOSTILE_EXIT,
    }
}
/// Whether a rift `owner` on `team` made is an opponent's to the player
/// `viewer` on `viewer_team`: one they can't travel through.
pub fn hostile(owner: EntityId, team: u8, viewer: EntityId, viewer_team: u8) -> bool {
    owner != viewer && (team == 0 || team != viewer_team)
}
/// The team of the player `local`, or 0.
pub fn team_of(snapshot: &Snapshot, local: EntityId) -> u8 {
    snapshot
        .tanks
        .iter()
        .find(|tank| tank.id == local)
        .map_or(0, |tank| tank.team)
}

/// Each layer of deep space: how fast it slides compared with the floor,
/// and how many star cells span one rift radius.
const LAYERS: [(f32, f32); 3] = [(0.12, 4.6), (0.33, 3.1), (0.58, 1.9)];

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
uniform mat4 Model;
uniform mat4 Projection;
varying mediump vec2 uv;
void main() { gl_Position = Projection * Model * vec4(position, 1.0); uv = texcoord; }
"#;

// Deep: far layer .xy, near layer .zw; Mid: middle layer .xy. Each is the
// rift's heart in that layer's star cells. Shape: star cells per rim
// radius for the far and near layers, +1 entry or -1 exit, and the size of
// a pixel in rim radii. Look: rim color and opacity. Clock: seconds,
// whether linked, how far collapsed, and 1 for an opponent's rift.
const FRAGMENT: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying mediump vec2 uv;
uniform vec4 Deep;
uniform vec4 Mid;
uniform vec4 Shape;
uniform vec4 Look;
uniform vec4 Clock;
float hash(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}
vec2 hash2(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.xx + q.yz) * q.zy);
}
float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
               mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}
float clouds(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * noise(p);
        p = p * 2.07 + vec2(13.7, 7.1);
        a *= 0.5;
    }
    return v;
}
vec3 stars(vec2 p, float density, float size, float t) {
    vec2 cell = floor(p);
    float h = hash(cell);
    if (h > density) { return vec3(0.0); }
    vec2 at = 0.2 + hash2(cell + 7.0) * 0.6;
    float d = length(fract(p) - at);
    float glint = 0.62 + 0.38 * sin(t * (1.3 + h * 4.0) + h * 61.0);
    float light = smoothstep(size, 0.0, d) + smoothstep(size * 2.6, 0.0, d) * 0.3;
    vec3 hue = mix(vec3(0.72, 0.84, 1.0), vec3(1.0, 0.87, 0.74), fract(h * 13.7));
    return hue * light * glint;
}
vec2 turn(vec2 p, float a) {
    float c = cos(a);
    float s = sin(a);
    return vec2(c * p.x - s * p.y, s * p.x + c * p.y);
}
void main() {
    vec2 local = (uv * 2.0 - 1.0) * 1.25;
    float r = length(local);
    float t = Clock.x;
    float dir = Shape.z;
    float linked = Clock.y;
    float collapse = Clock.z;
    float a = atan(local.y, local.x);
    float rim = 1.0 + 0.016 * sin(a * 5.0 + t * 2.2 * dir) + 0.009 * sin(a * 11.0 - t * 3.1);
    float aa = Shape.w * 1.5;
    float inside = 1.0 - smoothstep(rim - aa, rim, r);
    float heart = 1.0 - min(r, 1.0);
    float spin = dir * (t * 0.25 + heart * (0.9 + collapse * 7.0));
    float midSpan = (Shape.x + Shape.y) * 0.5;
    vec2 far = Deep.xy + turn(local, spin * 0.35) * Shape.x;
    vec2 middle = Mid.xy + turn(local, spin * 0.65) * midSpan;
    vec2 near = Deep.zw + turn(local, spin) * Shape.y;
    vec3 color = vec3(0.012, 0.008, 0.04) + Look.rgb * 0.05;
    float haze = clouds(far * 0.42 + vec2(t * 0.02, -t * 0.013));
    float drift = clouds(near * 0.55 - vec2(t * 0.03, t * 0.02));
    color += Look.rgb * pow(haze, 3.0) * 1.15;
    color += mix(vec3(0.55, 0.22, 0.85), vec3(0.8, 0.2, 0.16), Clock.w) * pow(drift, 4.0) * 0.9;
    color += stars(far, 0.5, 0.075, t) * 0.75;
    color += stars(middle + 31.0, 0.34, 0.085, t * 1.4);
    color += stars(near, 0.24, 0.095, t * 1.9) * 1.25;
    color *= mix(0.3, 1.0, smoothstep(rim, 0.15, r));
    float arms = sin(a * 3.0 - dir * (r * 8.0 + t * 3.5));
    color += Look.rgb * smoothstep(0.55, 1.0, arms) * heart * smoothstep(0.05, 0.35, r)
        * (0.25 + 0.35 * linked);
    color += mix(Look.rgb, vec3(1.0), 0.5) * smoothstep(0.42, 0.0, r)
        * (0.18 + 0.22 * linked + collapse * 1.4);
    float band = smoothstep(aa * 2.5 + 0.06, 0.0, abs(r - rim));
    float broken = mix(0.35 + 0.65 * step(0.0, sin(a * 10.0 + t * 4.0 * dir)), 1.0, linked);
    float ring = band * broken;
    vec3 edge = mix(Look.rgb, vec3(1.0), 0.55);
    gl_FragColor = vec4(mix(edge, mix(color, edge, ring), inside), max(inside, ring) * Look.a);
}
"#;

const MSL: &str = r#"#include <metal_stdlib>
using namespace metal;
struct Uniforms {
    float4x4 Projection; float4x4 Model; float4 _Time;
    float4 Deep; float4 Mid; float4 Shape; float4 Look; float4 Clock;
};
struct Vertex { float3 position [[attribute(0)]]; float2 texcoord [[attribute(1)]]; float4 color0 [[attribute(2)]]; };
struct Varyings { float4 position [[position]]; float2 uv; };
vertex Varyings vertexShader(Vertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    Varyings o; o.position = u.Projection * u.Model * float4(v.position, 1.0); o.uv = v.texcoord; return o;
}
static float hash1(float2 p) {
    float3 q = fract(float3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}
static float2 hash2(float2 p) {
    float3 q = fract(float3(p.xyx) * float3(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.xx + q.yz) * q.zy);
}
static float noise1(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash1(i), hash1(i + float2(1.0, 0.0)), u.x),
               mix(hash1(i + float2(0.0, 1.0)), hash1(i + float2(1.0, 1.0)), u.x), u.y);
}
static float clouds(float2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * noise1(p);
        p = p * 2.07 + float2(13.7, 7.1);
        a *= 0.5;
    }
    return v;
}
static float3 stars(float2 p, float density, float size, float t) {
    float2 cell = floor(p);
    float h = hash1(cell);
    if (h > density) { return float3(0.0); }
    float2 at = 0.2 + hash2(cell + 7.0) * 0.6;
    float d = length(fract(p) - at);
    float glint = 0.62 + 0.38 * sin(t * (1.3 + h * 4.0) + h * 61.0);
    float light = smoothstep(size, 0.0, d) + smoothstep(size * 2.6, 0.0, d) * 0.3;
    float3 hue = mix(float3(0.72, 0.84, 1.0), float3(1.0, 0.87, 0.74), fract(h * 13.7));
    return hue * light * glint;
}
static float2 turn(float2 p, float a) {
    float c = cos(a);
    float s = sin(a);
    return float2(c * p.x - s * p.y, s * p.x + c * p.y);
}
fragment float4 fragmentShader(Varyings in [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    float2 local = (in.uv * 2.0 - 1.0) * 1.25;
    float r = length(local);
    float t = u.Clock.x;
    float dir = u.Shape.z;
    float linked = u.Clock.y;
    float collapse = u.Clock.z;
    float a = atan2(local.y, local.x);
    float rim = 1.0 + 0.016 * sin(a * 5.0 + t * 2.2 * dir) + 0.009 * sin(a * 11.0 - t * 3.1);
    float aa = u.Shape.w * 1.5;
    float inside = 1.0 - smoothstep(rim - aa, rim, r);
    float heart = 1.0 - min(r, 1.0);
    float spin = dir * (t * 0.25 + heart * (0.9 + collapse * 7.0));
    float midSpan = (u.Shape.x + u.Shape.y) * 0.5;
    float2 far = u.Deep.xy + turn(local, spin * 0.35) * u.Shape.x;
    float2 middle = u.Mid.xy + turn(local, spin * 0.65) * midSpan;
    float2 near = u.Deep.zw + turn(local, spin) * u.Shape.y;
    float3 color = float3(0.012, 0.008, 0.04) + u.Look.rgb * 0.05;
    float haze = clouds(far * 0.42 + float2(t * 0.02, -t * 0.013));
    float drift = clouds(near * 0.55 - float2(t * 0.03, t * 0.02));
    color += u.Look.rgb * pow(haze, 3.0) * 1.15;
    color += mix(float3(0.55, 0.22, 0.85), float3(0.8, 0.2, 0.16), u.Clock.w) * pow(drift, 4.0) * 0.9;
    color += stars(far, 0.5, 0.075, t) * 0.75;
    color += stars(middle + 31.0, 0.34, 0.085, t * 1.4);
    color += stars(near, 0.24, 0.095, t * 1.9) * 1.25;
    color *= mix(0.3, 1.0, smoothstep(rim, 0.15, r));
    float arms = sin(a * 3.0 - dir * (r * 8.0 + t * 3.5));
    color += u.Look.rgb * smoothstep(0.55, 1.0, arms) * heart * smoothstep(0.05, 0.35, r)
        * (0.25 + 0.35 * linked);
    color += mix(u.Look.rgb, float3(1.0), 0.5) * smoothstep(0.42, 0.0, r)
        * (0.18 + 0.22 * linked + collapse * 1.4);
    float band = smoothstep(aa * 2.5 + 0.06, 0.0, abs(r - rim));
    float broken = mix(0.35 + 0.65 * step(0.0, sin(a * 10.0 + t * 4.0 * dir)), 1.0, linked);
    float ring = band * broken;
    float3 edge = mix(u.Look.rgb, float3(1.0), 0.55);
    return float4(mix(edge, mix(color, edge, ring), inside), max(inside, ring) * u.Look.a);
}
"#;

thread_local! {
    static MATERIAL: RefCell<Option<Option<Material>>> = const { RefCell::new(None) };
}

fn load() -> Option<Material> {
    load_material(
        if crate::graphics::metal() {
            ShaderSource::Msl { program: MSL }
        } else {
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: FRAGMENT,
            }
        },
        MaterialParams {
            uniforms: ["Deep", "Mid", "Shape", "Look", "Clock"]
                .into_iter()
                .map(|name| UniformDesc::new(name, UniformType::Float4))
                .collect(),
            pipeline_params: PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                alpha_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::One,
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .map_err(|e| log::warn!("Rift shader unavailable, drawing flat rifts: {e}"))
    .ok()
}

/// Compile the shader ahead of the first rift, so opening one never stalls.
pub fn prepare() {
    MATERIAL.with(|slot| {
        if slot.borrow().is_none() {
            *slot.borrow_mut() = Some(load());
        }
    });
}

/// Each rift's age and time left, run smoothly between snapshots.
#[derive(Default)]
pub struct Clocks(HashMap<EntityId, (f32, f32)>);

impl Clocks {
    pub fn note(&mut self, rifts: &[Rift], dt: f32) {
        if rifts.is_empty() {
            self.0.clear();
            return;
        }
        self.0
            .retain(|id, _| rifts.iter().any(|rift| rift.id == *id));
        for rift in rifts {
            let clock = self.0.entry(rift.id).or_insert((rift.age, rift.life));
            clock.0 += dt;
            // As on the host, a waiting end keeps its time.
            if rift.partner != 0 || rift.life <= 0. {
                clock.1 -= dt;
            }
            // Snapshots arrive in steps; follow them without stuttering.
            if (clock.0 - rift.age).abs() > 0.15 {
                clock.0 = rift.age;
            }
            if (clock.1 - rift.life).abs() > 0.15 {
                clock.1 = rift.life;
            }
            clock.1 = clock.1.clamp(-RIFT_CLOSING, RIFT_SECONDS);
        }
    }
    /// Age and time left for `rift`.
    pub fn of(&self, rift: &Rift) -> (f32, f32) {
        self.0
            .get(&rift.id)
            .copied()
            .unwrap_or((rift.age, rift.life))
    }
}

fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0., 1.) - 1.;
    1. + t * t * (2.7 * t + 1.7)
}

fn ease_in_back(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (2.7 * t - 1.7)
}

/// How big a rift is drawn, 0 to a little over 1: it springs open, then at
/// the end gathers itself and is sucked away to nothing.
pub fn scale(age: f32, life: f32) -> f32 {
    let open = if age < OPENING {
        ease_out_back(age / OPENING)
    } else {
        1.
    };
    let collapse = (-life / RIFT_CLOSING).clamp(0., 1.);
    (open * (1. - ease_in_back(collapse))).max(0.)
}

/// Where a rift's heart lies in one layer of deep space, in star cells.
fn deep(view: &View, pos: V2, (follow, cells): (f32, f32), drift: f32) -> Vec2 {
    let cell = RIFT_RADIUS / cells;
    vec2(
        (pos.x - view.center.x * (1. - follow)) / cell + drift * 0.37,
        (pos.y - view.center.y * (1. - follow)) / cell - drift * 0.21,
    )
}

/// Draw every rift in view, on the floor under tanks and shots.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    snapshot: &Snapshot,
    view: &View,
    clocks: &Clocks,
    local: EntityId,
    time: f32,
    reduced_motion: bool,
    light: &mut crate::emission::Emission,
) {
    if snapshot.rifts.is_empty() {
        return;
    }
    let local_team = team_of(snapshot, local);
    // The thread between a pair shows the owner and their team which way
    // their rift leads.
    for entry in snapshot
        .rifts
        .iter()
        .filter(|rift| rift.end == RiftEnd::Entry && rift.partner != 0 && rift.life > 0.)
    {
        if entry.owner != local && (local_team == 0 || entry.team != local_team) {
            continue;
        }
        if let Some(exit) = snapshot.rifts.iter().find(|r| r.id == entry.partner) {
            thread(view, entry, exit, clocks, time, reduced_motion);
        }
    }
    let shader = MATERIAL.with(|slot| {
        if slot.borrow().is_none() {
            *slot.borrow_mut() = Some(load());
        }
        slot.borrow().clone().flatten()
    });
    let t = if reduced_motion { 0. } else { time };
    for rift in &snapshot.rifts {
        let (age, life) = clocks.of(rift);
        let size = scale(age, life);
        let radius = RIFT_RADIUS * view.zoom * size;
        if radius < 0.5 || !view.visible(rift.pos, RIFT_RADIUS * 2.5) {
            continue;
        }
        let at = view.screen(rift.pos);
        let foe = hostile(rift.owner, rift.team, local, local_team);
        let tint = color(rift.end, foe);
        let linked = rift.partner != 0;
        let collapse = (-life / RIFT_CLOSING).clamp(0., 1.);
        // Light spills onto the floor around the rim.
        if crate::glow::begin(crate::glow::Blend::Glow) {
            crate::glow::spot(
                at,
                radius * 2.3,
                Color {
                    a: if linked { 0.42 } else { 0.24 } * (1. + collapse),
                    ..tint
                },
            );
            crate::glow::end();
        }
        match &shader {
            Some(material) => {
                let layer = |index: usize| deep(view, rift.pos, LAYERS[index], t);
                let far = layer(0);
                let middle = layer(1);
                let near = layer(2);
                gl_use_material(material);
                material.set_uniform("Deep", vec4(far.x, far.y, near.x, near.y));
                material.set_uniform("Mid", vec4(middle.x, middle.y, 0., 0.));
                material.set_uniform(
                    "Shape",
                    vec4(
                        LAYERS[0].1,
                        LAYERS[2].1,
                        if rift.end == RiftEnd::Entry { 1. } else { -1. },
                        1. / radius.max(1.),
                    ),
                );
                material.set_uniform("Look", vec4(tint.r, tint.g, tint.b, 1.));
                material.set_uniform(
                    "Clock",
                    vec4(
                        t + rift.id as f32 * 0.37 % 60.,
                        f32::from(u8::from(linked)),
                        collapse,
                        f32::from(u8::from(foe)),
                    ),
                );
                let half = radius * QUAD;
                draw_texture_ex(
                    &Texture2D::empty(),
                    at.x - half,
                    at.y - half,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(half * 2., half * 2.)),
                        rotation: rift.angle,
                        ..Default::default()
                    },
                );
                gl_use_default_material();
            }
            None => flat(at, radius, tint, linked, rift.end, t),
        }
        motes(at, radius, tint, rift.end, linked, t, rift.id);
        if foe {
            thorns(at, radius, tint, t, collapse);
        }
        // The rim forming: a bright ring springs out as a rift opens.
        if age < 0.4 {
            let fade = 1. - age / 0.4;
            let rim = RIFT_RADIUS * view.zoom * (1. + age * 2.2);
            light.ring(
                crate::emission::Layer::Wash,
                at,
                rim,
                (9. * fade + 2.) * view.zoom,
                Color {
                    a: 0.4 * fade,
                    ..tint
                },
            );
            light.ring(
                crate::emission::Layer::Core,
                at,
                rim,
                (1.6 * fade + 0.7) * view.zoom,
                Color {
                    a: 0.9 * fade,
                    ..light.hot(tint, 0.55)
                },
            );
        }
    }
}

/// Without shaders: a dark disc, a few fixed stars and a solid rim.
fn flat(at: Vec2, radius: f32, tint: Color, linked: bool, end: RiftEnd, t: f32) {
    crate::render::polygon(at, 0, radius, 0., Color::new(0.03, 0.02, 0.08, 1.));
    crate::render::polygon(at, 0, radius * 0.55, 0., Color { a: 0.22, ..tint });
    let turn = if end == RiftEnd::Entry { t } else { -t } * 0.3;
    for star in 0..14 {
        let angle = star as f32 * 2.399_963 + turn;
        let reach = ((star * 37 % 11) as f32 / 11.) * 0.85;
        draw_circle(
            at.x + angle.cos() * radius * reach,
            at.y + angle.sin() * radius * reach,
            (radius * 0.035).max(1.),
            Color::new(0.9, 0.92, 1., 0.85),
        );
    }
    draw_poly_lines(
        at.x,
        at.y,
        crate::graphics::circle_sides(radius),
        radius,
        0.,
        (radius * 0.07).max(1.5),
        if linked {
            mix(tint, WHITE, 0.4)
        } else {
            Color { a: 0.55, ..tint }
        },
    );
}

/// Motes circling a rift: drawn in to an entry's heart, flung out of an exit.
fn motes(at: Vec2, radius: f32, tint: Color, end: RiftEnd, linked: bool, t: f32, seed: EntityId) {
    if radius < 4. || !crate::glow::begin(crate::glow::Blend::Add) {
        return;
    }
    let count = if linked { 16 } else { 8 };
    let spin = if end == RiftEnd::Entry { 1. } else { -1. };
    for index in 0..count {
        let phase = (t * 0.55 + index as f32 / count as f32 + (seed % 97) as f32 * 0.013).fract();
        let travel = if end == RiftEnd::Entry {
            1.55 - phase * 1.35
        } else {
            0.25 + phase * 1.35
        };
        let angle = index as f32 * 2.399_963 + spin * (t * 0.8 + travel * 2.4);
        let fade = (phase * std::f32::consts::PI).sin();
        let p = at + vec2(angle.cos(), angle.sin()) * radius * travel;
        draw_circle(
            p.x,
            p.y,
            (radius * 0.045).max(1.),
            Color {
                a: 0.75 * fade,
                ..mix(tint, WHITE, 0.35)
            },
        );
    }
    crate::glow::end();
}

/// A ring of short thorns just outside an opponent's rim, turning slowly,
/// so it reads as theirs without color.
fn thorns(at: Vec2, radius: f32, tint: Color, t: f32, collapse: f32) {
    if radius < 6. {
        return;
    }
    let width = (radius * 0.05).clamp(1., 3.);
    let color = Color {
        a: 0.8 * (1. - collapse),
        ..mix(tint, WHITE, 0.2)
    };
    for index in 0..12 {
        let angle = index as f32 * std::f32::consts::TAU / 12. + t * 0.35;
        let dir = vec2(angle.cos(), angle.sin());
        let from = at + dir * radius * 1.08;
        let to = at + dir * radius * 1.24;
        draw_line(from.x, from.y, to.x, to.y, width, color);
    }
}

/// Dots drifting from an entry to its exit.
fn thread(view: &View, entry: &Rift, exit: &Rift, clocks: &Clocks, time: f32, reduced: bool) {
    let (entry_age, _) = clocks.of(entry);
    let (exit_age, _) = clocks.of(exit);
    let shown = (entry_age.min(exit_age) / 0.6).clamp(0., 1.);
    let from = view.screen(entry.pos);
    let to = view.screen(exit.pos);
    let span = to - from;
    let length = span.length();
    let rim = RIFT_RADIUS * view.zoom;
    if shown <= 0. || length < rim * 2.4 {
        return;
    }
    let dir = span / length;
    let spacing = 26.;
    let offset = if reduced { 0. } else { (time * 46.) % spacing };
    let size = vec2(screen_width(), screen_height());
    let mut along = rim + offset;
    let mut drawn = 0;
    while along < length - rim && drawn < 400 {
        let p = from + dir * along;
        along += spacing;
        if p.x < -8. || p.y < -8. || p.x > size.x + 8. || p.y > size.y + 8. {
            continue;
        }
        drawn += 1;
        let k = along / length;
        let tint = mix(ENTRY, EXIT, k.clamp(0., 1.));
        draw_circle(
            p.x,
            p.y,
            1.8 * view.zoom.clamp(0.6, 1.4),
            Color {
                a: 0.32 * shown,
                ..tint
            },
        );
    }
}

/// Which end `owner`'s next rift shot opens.
pub fn next_end(snapshot: &Snapshot, owner: EntityId) -> RiftEnd {
    let entry = snapshot
        .rifts
        .iter()
        .any(|rift| rift.owner == owner && rift.end == RiftEnd::Entry && rift.life > 0.)
        || snapshot
            .rift_shots
            .iter()
            .any(|shot| shot.owner == owner && shot.end == RiftEnd::Entry);
    if entry { RiftEnd::Exit } else { RiftEnd::Entry }
}

/// An armed rift gun glows at its lens and core in the color of the end
/// it opens next, as the viewer sees that player's rifts.
pub fn armed(
    core: Vec2,
    radius: f32,
    lens: Vec2,
    (end, foe): (RiftEnd, bool),
    zoom: f32,
    time: f32,
) {
    let tint = color(end, foe);
    let pulse = 0.8 + 0.2 * (time * 6.).sin();
    if crate::glow::begin(crate::glow::Blend::Glow) {
        crate::glow::spot(lens, 30. * zoom * pulse, Color { a: 0.95, ..tint });
        crate::glow::spot(core, radius * 0.9, Color { a: 0.45, ..tint });
        crate::glow::end();
    }
    draw_circle(lens.x, lens.y, 4.5 * zoom, mix(tint, WHITE, 0.45));
    draw_circle(core.x, core.y, radius * 0.13, mix(tint, WHITE, 0.3));
    draw_poly_lines(
        core.x,
        core.y,
        crate::graphics::circle_sides(radius * 0.46),
        radius * 0.46,
        0.,
        1.8 * zoom,
        tint,
    );
}

/// Rift shots in flight: bright bolts with a short tail. `viewer` is the
/// player and their team.
pub fn draw_shots(shots: &[RiftShot], view: &View, interpolation: f32, viewer: (EntityId, u8)) {
    if shots.is_empty() {
        return;
    }
    let tint_of =
        |shot: &RiftShot| color(shot.end, hostile(shot.owner, shot.team, viewer.0, viewer.1));
    // A snapshot holds where a shot is; draw it on the way there from where
    // it was a tick before, as other rounds are.
    let place = |shot: &RiftShot| {
        shot.pos + shot.vel * (tankyard_core::DT * (interpolation.clamp(0., 1.) - 1.))
    };
    let glowing = crate::glow::begin(crate::glow::Blend::Glow);
    for shot in shots {
        let at = place(shot);
        if !view.visible(at, 80.) {
            continue;
        }
        let p = view.screen(at);
        let tint = tint_of(shot);
        let back = shot.vel.normalized() * (-34. * view.zoom);
        if glowing {
            crate::glow::stretched_spot(
                p + vec2(back.x, back.y) * 0.5,
                vec2(back.x, back.y) * 0.9,
                9. * view.zoom,
                Color { a: 0.8, ..tint },
            );
            crate::glow::spot(p, 22. * view.zoom, Color { a: 0.75, ..tint });
        }
    }
    if glowing {
        crate::glow::end();
    }
    for shot in shots {
        let at = place(shot);
        if !view.visible(at, 40.) {
            continue;
        }
        let p = view.screen(at);
        draw_circle(p.x, p.y, 6.5 * view.zoom, mix(tint_of(shot), WHITE, 0.3));
        draw_circle(p.x, p.y, 3. * view.zoom, WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rifts_spring_open_then_shrink_to_nothing() {
        assert_eq!(scale(0., RIFT_SECONDS), 0.);
        let peak = (1..40)
            .map(|step| scale(step as f32 * 0.01, RIFT_SECONDS))
            .fold(0f32, f32::max);
        assert!(peak > 1.02 && peak < 1.2, "a slight overshoot: {peak}");
        assert!((scale(1., 5.) - 1.).abs() < 1e-6);
        assert!(scale(1., -RIFT_CLOSING * 0.5) < 1.1);
        assert_eq!(scale(1., -RIFT_CLOSING), 0.);
    }

    #[test]
    fn deep_layers_slide_slower_than_the_floor() {
        let mut view = View::new();
        let pos = V2::new(1000., 800.);
        let before: Vec<Vec2> = LAYERS.iter().map(|&l| deep(&view, pos, l, 0.)).collect();
        view.center = V2::new(100., 0.);
        for (index, &layer) in LAYERS.iter().enumerate() {
            let after = deep(&view, pos, layer, 0.);
            // In cells, then back to world units.
            let moved = (after.x - before[index].x) * RIFT_RADIUS / layer.1;
            assert!((moved + 100. * (1. - layer.0)).abs() < 1e-3);
        }
    }
}
