//! Soft light for lamps and glowing particles. A rectangle drawn while one of
//! these blends is active becomes a round falloff from its center, and the
//! additive blends brighten what lies under them the way light does. Each
//! blend needs one shader; when shaders are unavailable `begin` returns
//! false and callers draw flat shapes instead.
use crate::graphics::draw_circle;
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation};
use macroquad::prelude::*;

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

fn soft_glsl(covered: bool) -> String {
    let body = falloff!("uv", "gl_FragCoord.xy")
        .replace("VEC", "vec2")
        .replace("TINT", "tint");
    if covered {
        format!(
            "#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
varying mediump vec2 cover;
uniform sampler2D Texture;
void main() {{
    {body}
    gl_FragColor = vec4(tint.rgb, light * (1.0 - texture2D(Texture, cover).r));
}}"
        )
    } else {
        format!(
            "#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
void main() {{
    {body}
    gl_FragColor = vec4(tint.rgb, light);
}}"
        )
    }
}

fn soft_msl(covered: bool) -> String {
    let body = falloff!("in.uv", "in.position.xy")
        .replace("VEC", "float2")
        .replace("TINT", "in.tint");
    if covered {
        format!(
            "
fragment float4 fragmentShader(CoveredVaryings in [[stage_in]],
                               texture2d<float> Texture [[texture(0)]], sampler smp [[sampler(0)]]) {{
    {body}
    return float4(in.tint.rgb, light * (1.0 - Texture.sample(smp, in.cover).r));
}}
"
        )
    } else {
        format!(
            "
fragment float4 fragmentShader(Varyings in [[stage_in]]) {{
    {body}
    return float4(in.tint.rgb, light);
}}
"
        )
    }
}

const SOLID: &str = r#"#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
void main() {
    gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);
}"#;

const FLAT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
void main() {
    gl_FragColor = tint;
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

const FLAT_MSL: &str = r#"
fragment float4 fragmentShader(Varyings in [[stage_in]]) {
    return in.tint;
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
    if matches!(
        blend,
        Blend::Glow | Blend::CoveredGlow | Blend::Add | Blend::Light
    ) {
        pipeline_params.color_blend = Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::One,
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
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())
}

thread_local! {
    static MATERIALS: Option<Materials> = (|| {
        Ok::<_, String>(Materials {
            soft: material(&soft_glsl(false), &soft_msl(false), Blend::Soft)?,
            glow: material(&soft_glsl(false), &soft_msl(false), Blend::Glow)?,
            add: material(FLAT, FLAT_MSL, Blend::Add)?,
            light: material(TEXTURED, TEXTURED_MSL, Blend::Light)?,
            multiply: material(TEXTURED, TEXTURED_MSL, Blend::Multiply)?,
        })
    })()
    .map_err(|error| log::warn!("Soft lighting unavailable: {error}"))
    .ok();
    static COVERING: Option<Covering> = (|| {
        let covered = |blend| {
            shader(
                COVERED_VERTEX,
                COVERED_MSL,
                &soft_glsl(true),
                &soft_msl(true),
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
            gl_use_material(match blend {
                Blend::CoveredSoft => &covering.soft,
                Blend::CoveredGlow => &covering.glow,
                _ => &covering.solid,
            });
            true
        });
    }
    MATERIALS.with(|materials| {
        let Some(materials) = materials else {
            return false;
        };
        gl_use_material(match blend {
            Blend::Soft => &materials.soft,
            Blend::Glow => &materials.glow,
            Blend::Add => &materials.add,
            Blend::Light => &materials.light,
            _ => &materials.multiply,
        });
        true
    })
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
