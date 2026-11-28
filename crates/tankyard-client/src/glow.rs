//! Soft light for lamps and glowing particles. A rectangle drawn while one of
//! these blends is active becomes a round falloff from its center, and the
//! additive blends brighten what lies under them the way light does. Each
//! blend needs one shader; when shaders are unavailable `begin` returns
//! false and callers draw flat shapes instead.
use crate::graphics::draw_circle;
use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Equation, UniformDesc, UniformType,
};
use macroquad::prelude::*;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// A round falloff, blended like any other shape: haze and smoke.
    Soft,
    /// A round falloff added to what is under it: light.
    Glow,
    /// Shapes as drawn, added to what is under them: glowing sparks.
    Add,
    /// Textured shapes added to what is under them: glowing particle
    /// sprites.
    Light,
    /// A texture multiplied into what is under it: a light map over the
    /// arena, darkening where no light falls.
    Multiply,
    /// [`Blend::Soft`], held back wherever the cover map is set.
    CoveredSoft,
    /// [`Blend::Glow`], held back wherever the cover map is set.
    CoveredGlow,
    /// Solid white whatever the shape's color: what the cover map is made of.
    Solid,
}

impl Blend {
    fn emits(self) -> bool {
        matches!(
            self,
            Self::Glow | Self::CoveredGlow | Self::Add | Self::Light
        )
    }
}

thread_local! {
    static DAYLIGHT: Cell<f32> = const { Cell::new(0.) };
}

/// Set the light response for one scene or pass, restoring the caller's
/// response on return. Illumination maps use zero to accumulate raw light.
#[must_use]
pub struct Daylight(f32);

pub fn daylight(amount: f32) -> Daylight {
    Daylight(DAYLIGHT.with(|slot| slot.replace(amount.clamp(0., 1.))))
}

impl Drop for Daylight {
    fn drop(&mut self) {
        DAYLIGHT.with(|slot| slot.set(self.0));
    }
}

// Premultiplied color lets one blend preserve additive light at night and
// reveal a colored falloff across a pale floor. The hotter center can still
// be brighter than its surroundings. Neutral light keeps a quiet cool edge.
const EMIT_LIGHT: &str = r#"
VEC4 emit_light(VEC3 rgb, float intensity, float daylight) {
    if (daylight <= 0.001) {
        return VEC4(rgb * intensity, 0.0);
    }
    float peak = max(rgb.r, max(rgb.g, rgb.b));
    float chroma = peak - min(rgb.r, min(rgb.g, rgb.b));
    float neutral = 1.0 - smoothstep(0.06, 0.48, chroma);
    float core = smoothstep(0.09, 0.48, intensity);
    VEC3 fringe = mix(rgb * 0.76, VEC3(0.35, 0.46, 0.60), neutral * 0.3);
    VEC3 color = mix(fringe, mix(rgb, VEC3(1.0), 0.65), core);
    float coverage = clamp(intensity, 0.0, 1.0) * daylight * mix(0.64, 0.41, core);
    float radiance = mix(1.0, mix(0.24, 0.51, core), daylight);
    return VEC4(rgb * intensity * radiance + color * coverage, coverage);
}
"#;

fn emitter_source(vec3: &str, vec4: &str) -> String {
    EMIT_LIGHT.replace("VEC3", vec3).replace("VEC4", vec4)
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec4 tint;
varying mediump vec2 uv;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    tint = color0 / 255.0;
    uv = texcoord * 2.0 - 1.0;
}"#;

/// The same, carrying where this point falls on the cover map.
const COVERED_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
varying lowp vec4 tint;
varying mediump vec2 uv;
varying mediump vec2 cover;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    tint = color0 / 255.0;
    uv = texcoord * 2.0 - 1.0;
    cover = normal.xy;
}"#;

// Light from a small source fades with the square of distance; the last term
// takes the remainder smoothly to zero at the edge so no rim shows. A wide,
// faint glow would still step visibly through an 8-bit frame, so it is nudged
// by under one step of fixed noise, which is scaled to the brightest channel
// and fades out with the light so that dark floor stays clean. The noise also
// takes its phase from where the pixel falls inside this shape: dozens of puffs
// can lie on one pixel, and if each nudged it the same way the nudges would add
// up into visible stripes instead of averaging out.
macro_rules! falloff {
    ($at:expr, $position:expr) => {
        concat!(
            "float d = dot(",
            $at,
            ", ",
            $at,
            ");
    float edge = max(1.0 - d, 0.0);
    float a = edge * edge * (0.25 + 0.75 / (1.0 + 10.0 * d));
    float grain = fract(52.9829189 * fract(dot(",
            $position,
            ", VEC(0.06711056, 0.00583715)) + dot(",
            $at,
            ", VEC(1.618034, 2.414214))));
    float light = TINT.a * a;
    float peak = max(max(TINT.r, TINT.g), max(TINT.b, 0.2));
    light = max(light + (grain - 0.5) * min(light * 64.0, 1.0) / (255.0 * peak), 0.0);"
        )
    };
}

fn soft_glsl(covered: bool, emits: bool) -> String {
    let body = falloff!("uv", "gl_FragCoord.xy")
        .replace("VEC", "vec2")
        .replace("TINT", "tint");
    let cover = if covered {
        "varying mediump vec2 cover;
uniform sampler2D Texture;"
    } else {
        ""
    };
    let mask = if covered {
        "light *= 1.0 - texture2D(Texture, cover).r;"
    } else {
        ""
    };
    let (emitter, color) = if emits {
        (
            format!(
                "uniform float Daylight;
{}",
                emitter_source("vec3", "vec4")
            ),
            "emit_light(tint.rgb, light, Daylight)",
        )
    } else {
        (String::new(), "vec4(tint.rgb, light)")
    };
    format!(
        "#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
{cover}
{emitter}
void main() {{
    {body}
    {mask}
    gl_FragColor = {color};
}}"
    )
}

fn soft_msl(covered: bool, emits: bool) -> String {
    let body = falloff!("in.uv", "in.position.xy")
        .replace("VEC", "float2")
        .replace("TINT", "in.tint");
    let (input, mask) = if covered {
        (
            "CoveredVaryings in [[stage_in]], texture2d<float> Texture [[texture(0)]], sampler smp [[sampler(0)]]",
            "light *= 1.0 - Texture.sample(smp, in.cover).r;",
        )
    } else {
        ("Varyings in [[stage_in]]", "")
    };
    let (emitter, uniform, color) = if emits {
        (
            emitter_source("float3", "float4"),
            ", constant Uniforms& u [[buffer(0)]]",
            "emit_light(in.tint.rgb, light, u.Daylight)",
        )
    } else {
        (String::new(), "", "float4(in.tint.rgb, light)")
    };
    format!(
        "
{emitter}
fragment float4 fragmentShader({input}{uniform}) {{
    {body}
    {mask}
    return {color};
}}"
    )
}

fn lit_glsl(textured: bool) -> String {
    let emitter = emitter_source("vec3", "vec4");
    let (texture, color) = if textured {
        (
            "uniform sampler2D Texture;",
            "texture2D(Texture, uv * 0.5 + 0.5) * tint",
        )
    } else {
        ("", "tint")
    };
    format!(
        "#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
uniform float Daylight;
{texture}
{emitter}
void main() {{
    vec4 color = {color};
    gl_FragColor = emit_light(color.rgb, color.a, Daylight);
}}"
    )
}

fn lit_msl(textured: bool) -> String {
    let emitter = emitter_source("float3", "float4");
    let (texture, color) = if textured {
        (
            ", texture2d<float> Texture [[texture(0)]], sampler smp [[sampler(0)]]",
            "Texture.sample(smp, in.uv * 0.5 + 0.5) * in.tint",
        )
    } else {
        ("", "in.tint")
    };
    format!("
{emitter}
fragment float4 fragmentShader(Varyings in [[stage_in]], constant Uniforms& u [[buffer(0)]]{texture}) {{
    float4 color = {color};
    return emit_light(color.rgb, color.a, u.Daylight);
}}")
}

const SOLID: &str = r#"#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
void main() {
    gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);
}"#;

const TEXTURED: &str = r#"#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = texture2D(Texture, uv * 0.5 + 0.5) * tint;
}"#;

const MSL: &str = r#"#include <metal_stdlib>
using namespace metal;
struct Uniforms {
    float4x4 Projection;
    float4x4 Model;
    float4 _Time;
    float Daylight;
};
struct Vertex {
    float3 position [[attribute(0)]];
    float2 texcoord [[attribute(1)]];
    float4 color0 [[attribute(2)]];
};
struct Varyings {
    float4 position [[position]];
    float2 uv;
    float4 tint;
};
vertex Varyings vertexShader(Vertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    Varyings out;
    out.position = u.Projection * u.Model * float4(v.position, 1.0);
    out.uv = v.texcoord * 2.0 - 1.0;
    out.tint = v.color0 / 255.0;
    return out;
}
"#;

/// The same inputs as [`MSL`], with where the point falls on the cover map.
const COVERED_MSL: &str = r#"#include <metal_stdlib>
using namespace metal;
struct Uniforms {
    float4x4 Projection;
    float4x4 Model;
    float4 _Time;
    float Daylight;
};
struct CoveredVertex {
    float3 position [[attribute(0)]];
    float2 texcoord [[attribute(1)]];
    float4 color0 [[attribute(2)]];
    float4 normal [[attribute(3)]];
};
struct CoveredVaryings {
    float4 position [[position]];
    float2 uv;
    float4 tint;
    float2 cover;
};
vertex CoveredVaryings vertexShader(CoveredVertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    CoveredVaryings out;
    out.position = u.Projection * u.Model * float4(v.position, 1.0);
    out.uv = v.texcoord * 2.0 - 1.0;
    out.tint = v.color0 / 255.0;
    out.cover = v.normal.xy;
    return out;
}
"#;

const SOLID_MSL: &str = r#"
fragment float4 fragmentShader(Varyings in [[stage_in]]) {
    return float4(1.0);
}
"#;

const TEXTURED_MSL: &str = r#"
fragment float4 fragmentShader(Varyings in [[stage_in]],
                               texture2d<float> Texture [[texture(0)]], sampler smp [[sampler(0)]]) {
    return Texture.sample(smp, in.uv * 0.5 + 0.5) * in.tint;
}
"#;

struct Materials {
    soft: Material,
    glow: Material,
    add: Material,
    light: Material,
    multiply: Material,
}

/// What keeps light off the tanks standing on it. Loaded apart from the
/// others, so a driver that refuses these loses only that layering.
struct Covering {
    soft: Material,
    glow: Material,
    solid: Material,
}

fn material(fragment: &str, msl: &str, blend: Blend) -> Result<Material, String> {
    shader(VERTEX, MSL, fragment, msl, blend)
}

fn shader(
    vertex: &str,
    msl_head: &str,
    fragment: &str,
    msl: &str,
    blend: Blend,
) -> Result<Material, String> {
    let program = format!("{msl_head}{msl}");
    let mut pipeline_params = PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    };
    if blend == Blend::Multiply {
        pipeline_params.color_blend = Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::DestinationColor),
            BlendFactor::Zero,
        ));
        pipeline_params.alpha_blend = Some(BlendState::new(
            Equation::Add,
            BlendFactor::Zero,
            BlendFactor::One,
        ));
    }
    if blend == Blend::Solid {
        pipeline_params.color_blend = None;
    }
    if blend.emits() {
        pipeline_params.color_blend = Some(BlendState::new(
            Equation::Add,
            BlendFactor::One,
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        ));
        // Light brightens color; it never makes the frame more opaque.
        pipeline_params.alpha_blend = Some(BlendState::new(
            Equation::Add,
            BlendFactor::Zero,
            BlendFactor::One,
        ));
    }
    load_material(
        if crate::graphics::metal() {
            ShaderSource::Msl { program: &program }
        } else {
            ShaderSource::Glsl { vertex, fragment }
        },
        MaterialParams {
            pipeline_params,
            uniforms: if blend.emits() {
                vec![UniformDesc::new("Daylight", UniformType::Float1)]
            } else {
                Vec::new()
            },
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())
}

thread_local! {
    static MATERIALS: Option<Materials> = (|| {
        Ok::<_, String>(Materials {
            soft: material(&soft_glsl(false, false), &soft_msl(false, false), Blend::Soft)?,
            glow: material(&soft_glsl(false, true), &soft_msl(false, true), Blend::Glow)?,
            add: material(&lit_glsl(false), &lit_msl(false), Blend::Add)?,
            light: material(&lit_glsl(true), &lit_msl(true), Blend::Light)?,
            multiply: material(TEXTURED, TEXTURED_MSL, Blend::Multiply)?,
        })
    })()
    .map_err(|error| log::warn!("Soft lighting unavailable: {error}"))
    .ok();
    static COVERING: Option<Covering> = (|| {
        let covered = |blend: Blend| {
            shader(
                COVERED_VERTEX,
                COVERED_MSL,
                &soft_glsl(true, blend.emits()),
                &soft_msl(true, blend.emits()),
                blend,
            )
        };
        Ok::<_, String>(Covering {
            soft: covered(Blend::CoveredSoft)?,
            glow: covered(Blend::CoveredGlow)?,
            solid: material(SOLID, SOLID_MSL, Blend::Solid)?,
        })
    })()
    .map_err(|error| log::warn!("Light will not be kept off tanks: {error}"))
    .ok();
}

/// Whether light can be held back by a cover map on this device.
pub fn can_cover() -> bool {
    available() && COVERING.with(Option::is_some)
}

/// Whether the shaders loaded, so `begin` can work.
pub fn available() -> bool {
    MATERIALS.with(Option::is_some)
}

/// Draw with `blend` until `end`. False when shaders are unavailable, and
/// nothing changed.
pub fn begin(blend: Blend) -> bool {
    if matches!(
        blend,
        Blend::CoveredSoft | Blend::CoveredGlow | Blend::Solid
    ) {
        return COVERING.with(|covering| {
            let Some(covering) = covering else {
                return false;
            };
            apply(
                match blend {
                    Blend::CoveredSoft => &covering.soft,
                    Blend::CoveredGlow => &covering.glow,
                    _ => &covering.solid,
                },
                blend,
            );
            true
        });
    }
    MATERIALS.with(|materials| {
        let Some(materials) = materials else {
            return false;
        };
        apply(
            match blend {
                Blend::Soft => &materials.soft,
                Blend::Glow => &materials.glow,
                Blend::Add => &materials.add,
                Blend::Light => &materials.light,
                _ => &materials.multiply,
            },
            blend,
        );
        true
    })
}

fn apply(material: &Material, blend: Blend) {
    if blend.emits() {
        material.set_uniform("Daylight", DAYLIGHT.with(Cell::get));
    }
    gl_use_material(material);
}

pub fn end() {
    gl_use_default_material();
}

/// A round soft spot of `radius` around `center`, in the current blend.
pub fn spot(center: Vec2, radius: f32, color: Color) {
    draw_rectangle(
        center.x - radius,
        center.y - radius,
        radius * 2.,
        radius * 2.,
        color,
    );
}

/// A soft spot stretched into an oval: `along` is half its length, as a
/// vector it lies along, and `across` half its width, in the current blend.
pub fn stretched_spot(center: Vec2, along: Vec2, across: f32, color: Color) {
    if along.length_squared() < 0.01 || across < 0.1 || color.a <= 0.004 {
        return;
    }
    let side = vec2(-along.y, along.x).normalize() * across;
    let corner = |u: f32, v: f32| {
        let p = center + along * (u * 2. - 1.) + side * (v * 2. - 1.);
        Vertex::new(p.x, p.y, 0., u, v, color)
    };
    let vertices = [
        corner(0., 0.),
        corner(1., 0.),
        corner(1., 1.),
        corner(0., 1.),
    ];
    // SAFETY: rendering runs on the main graphics thread; no context borrow
    // survives this call, which copies the geometry into macroquad's batch.
    unsafe {
        let gl = get_internal_gl().quad_gl;
        gl.texture(None);
        gl.draw_mode(DrawMode::Triangles);
        gl.geometry(&vertices, &[0, 1, 2, 0, 2, 3]);
    }
}

/// Rings stacking to roughly the shader's falloff.
pub fn fallback_spot(center: Vec2, radius: f32, color: Color) {
    for (scale, share) in [(1., 0.10), (0.7, 0.14), (0.42, 0.22), (0.2, 0.38)] {
        draw_circle(
            center.x,
            center.y,
            radius * scale,
            Color {
                a: color.a * share,
                ..color
            },
        );
    }
}
