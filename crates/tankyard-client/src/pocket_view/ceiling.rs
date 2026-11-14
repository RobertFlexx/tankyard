//! A small Gaussian filter and a feathered housing soften the overhead plane.
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation};
use macroquad::prelude::*;
const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
uniform mat4 Model;
uniform mat4 Projection;
varying lowp vec4 tint;
varying mediump vec2 uv;
void main() { gl_Position=Projection*Model*vec4(position,1.0); uv=texcoord; tint=color0/255.0; }
"#;
const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 tint;
varying mediump vec2 uv;
uniform sampler2D Texture;
void main() {
    vec2 p=abs(uv*2.0-1.0);
    float edge=(1.0-smoothstep(0.90,1.0,p.x))*(1.0-smoothstep(0.78,1.0,p.y));
    vec2 stepUV=vec2(0.016,0.036);
    vec3 c=texture2D(Texture,uv).rgb*0.25;
    c+=(texture2D(Texture,uv+vec2(stepUV.x,0.0)).rgb+texture2D(Texture,uv-vec2(stepUV.x,0.0)).rgb
       +texture2D(Texture,uv+vec2(0.0,stepUV.y)).rgb+texture2D(Texture,uv-vec2(0.0,stepUV.y)).rgb)*0.125;
    c+=(texture2D(Texture,uv+stepUV).rgb+texture2D(Texture,uv-stepUV).rgb
       +texture2D(Texture,uv+vec2(stepUV.x,-stepUV.y)).rgb+texture2D(Texture,uv+vec2(-stepUV.x,stepUV.y)).rgb)*0.0625;
    float tube=(1.0-smoothstep(0.76,0.84,p.x))*(1.0-smoothstep(0.48,0.65,p.y));
    gl_FragColor=vec4(mix(vec3(0.51,0.50,0.42),c*tint.rgb,tube),tint.a*edge);
}
"#;
const MSL: &str = r#"#include <metal_stdlib>
using namespace metal;
struct Uniforms { float4x4 Projection; float4x4 Model; float4 _Time; };
struct Vertex { float3 position [[attribute(0)]]; float2 texcoord [[attribute(1)]]; float4 color0 [[attribute(2)]]; };
struct Varyings { float4 position [[position]]; float2 uv; float4 tint; };
vertex Varyings vertexShader(Vertex v [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
    Varyings o; o.position=u.Projection*u.Model*float4(v.position,1.0); o.uv=v.texcoord; o.tint=v.color0/255.0; return o;
}
fragment float4 fragmentShader(Varyings in [[stage_in]], texture2d<float> Texture [[texture(0)]], sampler smp [[sampler(0)]]) {
    float2 p=abs(in.uv*2.0-1.0);
    float edge=(1.0-smoothstep(0.90,1.0,p.x))*(1.0-smoothstep(0.78,1.0,p.y));
    float2 d=float2(0.016,0.036);
    float3 c=Texture.sample(smp,in.uv).rgb*0.25;
    c+=(Texture.sample(smp,in.uv+float2(d.x,0.0)).rgb+Texture.sample(smp,in.uv-float2(d.x,0.0)).rgb
       +Texture.sample(smp,in.uv+float2(0.0,d.y)).rgb+Texture.sample(smp,in.uv-float2(0.0,d.y)).rgb)*0.125;
    c+=(Texture.sample(smp,in.uv+d).rgb+Texture.sample(smp,in.uv-d).rgb
       +Texture.sample(smp,in.uv+float2(d.x,-d.y)).rgb+Texture.sample(smp,in.uv+float2(-d.x,d.y)).rgb)*0.0625;
    float tube=(1.0-smoothstep(0.76,0.84,p.x))*(1.0-smoothstep(0.48,0.65,p.y));
    return float4(mix(float3(0.51,0.50,0.42),c*in.tint.rgb,tube),in.tint.a*edge);
}
"#;

pub fn material() -> Option<Material> {
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
    .map_err(|e| log::warn!("Ceiling softness unavailable: {e}"))
    .ok()
}
