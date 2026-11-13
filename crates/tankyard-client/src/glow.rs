//! Soft light for lamps and glowing particles. A rectangle drawn while one of
//! these blends is active becomes a round falloff from its center, and the
//! additive blends brighten what lies under them the way light does. Each
//! blend needs one shader; when shaders are unavailable `begin` returns
//! false and callers draw flat shapes instead.
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

// Light from a small source fades with the square of distance; the last term
// takes the remainder smoothly to zero at the edge so no rim shows.
const SOFT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
void main() {
    float d = dot(uv, uv);
    float edge = max(1.0 - d, 0.0);
    float a = edge * edge * (0.25 + 0.75 / (1.0 + 10.0 * d));
    gl_FragColor = vec4(tint.rgb, tint.a * a);
}"#;

const FLAT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
void main() {
    gl_FragColor = tint;
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

const SOFT_MSL: &str = r#"
fragment float4 fragmentShader(Varyings in [[stage_in]]) {
    float d = dot(in.uv, in.uv);
    float edge = max(1.0 - d, 0.0);
    float a = edge * edge * (0.25 + 0.75 / (1.0 + 10.0 * d));
    return float4(in.tint.rgb, in.tint.a * a);
}
"#;

const FLAT_MSL: &str = r#"
fragment float4 fragmentShader(Varyings in [[stage_in]]) {
    return in.tint;
}
"#;

struct Materials {
    soft: Material,
    glow: Material,
    add: Material,
}

fn material(fragment: &str, msl: &str, additive: bool) -> Result<Material, String> {
    let program = format!("{MSL}{msl}");
    let mut pipeline_params = PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    };
    if additive {
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
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment,
            }
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
            soft: material(SOFT, SOFT_MSL, false)?,
            glow: material(SOFT, SOFT_MSL, true)?,
            add: material(FLAT, FLAT_MSL, true)?,
        })
    })()
    .map_err(|error| log::warn!("Soft lighting unavailable: {error}"))
    .ok();
}

/// Draw with `blend` until `end`. False when shaders are unavailable, and
/// nothing changed.
pub fn begin(blend: Blend) -> bool {
    MATERIALS.with(|materials| {
        let Some(materials) = materials else {
            return false;
        };
        gl_use_material(match blend {
            Blend::Soft => &materials.soft,
            Blend::Glow => &materials.glow,
            Blend::Add => &materials.add,
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
