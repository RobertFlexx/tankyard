//! World post-processing: the arena renders at its own scale, then FXAA,
//! AMD FidelityFX Super Resolution 1 (EASU upscale and RCAS sharpening) or a
//! plain filter bring it to the output. Menus and the HUD draw afterwards at
//! full resolution, so text stays crisp at any render scale.
//!
//! The FSR 1 passes follow AMD's published algorithm (MIT licensed) and run on
//! GLSL ES 1.0, so they work wherever the game does. A driver that rejects a
//! shader falls back to bilinear filtering.
use crate::settings::Settings;
use macroquad::prelude::*;

pub const UPSCALERS: [&str; 2] = ["Bilinear", "FSR 1"];

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
}
"#;

const PRECISION: &str = "#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying vec2 uv;
uniform sampler2D Texture;
uniform vec2 InputSize;
";

/// Edge-adaptive spatial upsampling: a 12-tap, direction-aware Lanczos-like
/// filter, clamped to the local 2x2 range to avoid ringing.
const EASU: &str = r#"
float luma(vec3 c) { return c.g + 0.5 * (c.r + c.b); }
vec3 tap(vec2 texel) { return texture2D(Texture, (texel + 0.5) / InputSize).rgb; }
void setDir(inout vec2 dir, inout float len, float w, float a, float b, float c, float d, float e) {
    float dirX = d - b;
    float lenX = clamp(abs(dirX) / max(max(abs(d - c), abs(c - b)), 1.0 / 65536.0), 0.0, 1.0);
    float dirY = e - a;
    float lenY = clamp(abs(dirY) / max(max(abs(e - c), abs(c - a)), 1.0 / 65536.0), 0.0, 1.0);
    dir += vec2(dirX, dirY) * w;
    len += (lenX * lenX + lenY * lenY) * w;
}
void accumulate(inout vec3 color, inout float weight, vec2 off, vec2 dir, vec2 len2, float lob, float clp, vec3 c) {
    vec2 v = vec2(dot(off, dir), dot(off, vec2(-dir.y, dir.x))) * len2;
    float d2 = min(dot(v, v), clp);
    float wB = 0.4 * d2 - 1.0;
    float wA = lob * d2 - 1.0;
    wB *= wB;
    wA *= wA;
    wB = 1.5625 * wB - 0.5625;
    float w = wB * wA;
    color += c * w;
    weight += w;
}
void main() {
    vec2 pp = uv * InputSize - 0.5;
    vec2 fp = floor(pp);
    pp -= fp;
    vec3 b = tap(fp + vec2(0.0, -1.0)); vec3 c = tap(fp + vec2(1.0, -1.0));
    vec3 e = tap(fp + vec2(-1.0, 0.0)); vec3 f = tap(fp);
    vec3 g = tap(fp + vec2(1.0, 0.0)); vec3 h = tap(fp + vec2(2.0, 0.0));
    vec3 i = tap(fp + vec2(-1.0, 1.0)); vec3 j = tap(fp + vec2(0.0, 1.0));
    vec3 k = tap(fp + vec2(1.0, 1.0)); vec3 l = tap(fp + vec2(2.0, 1.0));
    vec3 n = tap(fp + vec2(0.0, 2.0)); vec3 o = tap(fp + vec2(1.0, 2.0));
    float bL = luma(b), cL = luma(c), eL = luma(e), fL = luma(f), gL = luma(g), hL = luma(h);
    float iL = luma(i), jL = luma(j), kL = luma(k), lL = luma(l), nL = luma(n), oL = luma(o);
    vec2 dir = vec2(0.0);
    float len = 0.0;
    setDir(dir, len, (1.0 - pp.x) * (1.0 - pp.y), bL, eL, fL, gL, jL);
    setDir(dir, len, pp.x * (1.0 - pp.y), cL, fL, gL, hL, kL);
    setDir(dir, len, (1.0 - pp.x) * pp.y, fL, iL, jL, kL, nL);
    setDir(dir, len, pp.x * pp.y, gL, jL, kL, lL, oL);
    float dirR = dot(dir, dir);
    if (dirR < 1.0 / 32768.0) {
        dir = vec2(1.0, 0.0);
    } else {
        dir *= inversesqrt(dirR);
    }
    len = len * 0.5;
    len *= len;
    float stretch = dot(dir, dir) / max(abs(dir.x), abs(dir.y));
    vec2 len2 = vec2(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
    float lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
    float clp = 1.0 / lob;
    vec3 color = vec3(0.0);
    float weight = 0.0;
    accumulate(color, weight, vec2(0.0, -1.0) - pp, dir, len2, lob, clp, b);
    accumulate(color, weight, vec2(1.0, -1.0) - pp, dir, len2, lob, clp, c);
    accumulate(color, weight, vec2(-1.0, 1.0) - pp, dir, len2, lob, clp, i);
    accumulate(color, weight, vec2(0.0, 1.0) - pp, dir, len2, lob, clp, j);
    accumulate(color, weight, vec2(0.0, 0.0) - pp, dir, len2, lob, clp, f);
    accumulate(color, weight, vec2(-1.0, 0.0) - pp, dir, len2, lob, clp, e);
    accumulate(color, weight, vec2(1.0, 1.0) - pp, dir, len2, lob, clp, k);
    accumulate(color, weight, vec2(2.0, 1.0) - pp, dir, len2, lob, clp, l);
    accumulate(color, weight, vec2(2.0, 0.0) - pp, dir, len2, lob, clp, h);
    accumulate(color, weight, vec2(1.0, 0.0) - pp, dir, len2, lob, clp, g);
    accumulate(color, weight, vec2(1.0, 2.0) - pp, dir, len2, lob, clp, o);
    accumulate(color, weight, vec2(0.0, 2.0) - pp, dir, len2, lob, clp, n);
    vec3 lo = min(min(f, g), min(j, k));
    vec3 hi = max(max(f, g), max(j, k));
    gl_FragColor = vec4(clamp(color / weight, lo, hi), 1.0);
}
"#;

/// Robust contrast-adaptive sharpening, limited so it never rings.
const RCAS: &str = r#"
uniform float Sharpness;
vec3 tap(vec2 offset) { return texture2D(Texture, uv + offset / InputSize).rgb; }
void main() {
    vec3 b = tap(vec2(0.0, -1.0));
    vec3 d = tap(vec2(-1.0, 0.0));
    vec3 e = tap(vec2(0.0, 0.0));
    vec3 f = tap(vec2(1.0, 0.0));
    vec3 h = tap(vec2(0.0, 1.0));
    vec3 mn = min(min(min(b, d), min(f, h)), e);
    vec3 mx = max(max(max(b, d), max(f, h)), e);
    vec3 hitMin = mn / max(4.0 * mx, vec3(1.0 / 65536.0));
    vec3 hitMax = (1.0 - mx) / min(4.0 * mn - 4.0, vec3(-1.0 / 65536.0));
    vec3 lobeRGB = max(-hitMin, hitMax);
    float lobe = max(-0.1875, min(max(lobeRGB.r, max(lobeRGB.g, lobeRGB.b)), 0.0)) * Sharpness;
    vec3 color = (lobe * (b + d + f + h) + e) / (4.0 * lobe + 1.0);
    gl_FragColor = vec4(clamp(color, 0.0, 1.0), 1.0);
}
"#;

/// Fast approximate antialiasing for the remaining stair steps on edges.
const FXAA: &str = r#"
vec3 tap(vec2 offset) { return texture2D(Texture, uv + offset / InputSize).rgb; }
void main() {
    const vec3 weights = vec3(0.299, 0.587, 0.114);
    vec3 nw = tap(vec2(-1.0, -1.0));
    vec3 ne = tap(vec2(1.0, -1.0));
    vec3 sw = tap(vec2(-1.0, 1.0));
    vec3 se = tap(vec2(1.0, 1.0));
    vec3 m = tap(vec2(0.0));
    float lNW = dot(nw, weights), lNE = dot(ne, weights), lSW = dot(sw, weights);
    float lSE = dot(se, weights), lM = dot(m, weights);
    float lo = min(lM, min(min(lNW, lNE), min(lSW, lSE)));
    float hi = max(lM, max(max(lNW, lNE), max(lSW, lSE)));
    vec2 dir = vec2(-((lNW + lNE) - (lSW + lSE)), (lNW + lSW) - (lNE + lSE));
    float reduce = max((lNW + lNE + lSW + lSE) * (0.25 / 8.0), 1.0 / 128.0);
    float scale = 1.0 / (min(abs(dir.x), abs(dir.y)) + reduce);
    dir = clamp(dir * scale, vec2(-8.0), vec2(8.0));
    vec3 a = 0.5 * (tap(dir * (1.0 / 3.0 - 0.5)) + tap(dir * (2.0 / 3.0 - 0.5)));
    vec3 b = a * 0.5 + 0.25 * (tap(dir * -0.5) + tap(dir * 0.5));
    float lB = dot(b, weights);
    gl_FragColor = vec4((lB < lo || lB > hi) ? a : b, 1.0);
}
"#;

struct Materials {
    easu: Material,
    rcas: Material,
    fxaa: Material,
}

fn material(body: &str, sharpness: bool) -> Result<Material, String> {
    let mut uniforms = vec![UniformDesc::new("InputSize", UniformType::Float2)];
    if sharpness {
        uniforms.push(UniformDesc::new("Sharpness", UniformType::Float1));
    }
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: &format!("{PRECISION}{body}"),
        },
        MaterialParams {
            uniforms,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())
}

fn target(size: (u32, u32), samples: u8) -> RenderTarget {
    let target = render_target_ex(
        size.0,
        size.1,
        RenderTargetParams {
            sample_count: i32::from(samples),
            ..Default::default()
        },
    );
    target.texture.set_filter(FilterMode::Linear);
    target
}

fn bounded(size: (f32, f32)) -> (u32, u32) {
    let factor = (4096. / size.0.max(1.)).min(4096. / size.1.max(1.)).min(1.);
    (
        (size.0 * factor).round().max(1.) as u32,
        (size.1 * factor).round().max(1.) as u32,
    )
}

/// What the world needs this frame, decided from settings and sizes alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub scene: (u32, u32),
    pub fxaa: bool,
    pub easu: bool,
    pub sharpness: f32,
}

impl Plan {
    pub fn new(settings: &Settings, output: (u32, u32)) -> Option<Self> {
        let scale = settings.render_scale.clamp(0.5, 2.0);
        let scene = bounded((output.0 as f32 * scale, output.1 as f32 * scale));
        let fsr = settings.upscaler == 1;
        let easu = fsr && scene.0 < output.0;
        // FSR always pairs its upscale with RCAS; elsewhere sharpening is opt-in.
        let sharpness = if fsr {
            settings.sharpness.clamp(0., 1.).max(0.1)
        } else {
            settings.sharpness.clamp(0., 1.)
        };
        let plan = Self {
            scene,
            fxaa: settings.fxaa,
            easu,
            sharpness,
        };
        (scene != output || plan.fxaa || plan.sharpness > 0.).then_some(plan)
    }
}

#[derive(Default)]
pub struct Scaler {
    materials: Option<Result<Materials, String>>,
    scene: Option<((u32, u32, u8), RenderTarget)>,
    smoothed: Option<((u32, u32), RenderTarget)>,
    upscaled: Option<((u32, u32), RenderTarget)>,
    plan: Option<(Plan, (u32, u32))>,
}

fn reuse(slot: &mut Option<((u32, u32), RenderTarget)>, size: (u32, u32)) -> RenderTarget {
    if slot.as_ref().is_none_or(|(s, _)| *s != size) {
        *slot = Some((size, target(size, 1)));
    }
    slot.as_ref().unwrap().1.clone()
}

fn camera_for(target: &RenderTarget) -> Camera2D {
    let mut camera =
        Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height()));
    camera.render_target = Some(target.clone());
    camera
}

/// Draw `source` over the whole current target, optionally through `material`.
fn blit(source: &Texture2D, material: Option<&Material>, input: (u32, u32), sharpness: f32) {
    if let Some(material) = material {
        material.set_uniform("InputSize", vec2(input.0 as f32, input.1 as f32));
        if sharpness > 0. {
            // RCAS takes sharpness in stops: 0 is strongest, each stop halves it.
            material.set_uniform("Sharpness", (-(1. - sharpness) * 2.).exp2());
        }
        gl_use_material(material);
    }
    draw_texture_ex(
        source,
        0.,
        0.,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(screen_width(), screen_height())),
            flip_y: true,
            ..Default::default()
        },
    );
    if material.is_some() {
        gl_use_default_material();
    }
}

impl Scaler {
    /// The reason post-processing is unavailable, if a shader failed.
    pub fn error(&self) -> Option<&str> {
        match &self.materials {
            Some(Err(error)) => Some(error),
            _ => None,
        }
    }

    /// Redirect world drawing into the scene target. `output` is the size of
    /// whatever the world would otherwise draw into.
    pub fn begin(&mut self, settings: &Settings, output: (u32, u32)) {
        self.plan = Plan::new(settings, output).map(|plan| (plan, output));
        let Some((plan, _)) = self.plan else {
            return;
        };
        if self.materials.is_none() && (plan.fxaa || plan.easu || plan.sharpness > 0.) {
            let loaded = (|| {
                Ok(Materials {
                    easu: material(EASU, false)?,
                    rcas: material(RCAS, true)?,
                    fxaa: material(FXAA, false)?,
                })
            })();
            if let Err(error) = &loaded {
                log::warn!("Post-processing unavailable, using bilinear filtering: {error}");
            }
            self.materials = Some(loaded);
        }
        let samples = crate::graphics::samples(settings.anti_aliasing);
        let key = (plan.scene.0, plan.scene.1, samples);
        if self.scene.as_ref().is_none_or(|(k, _)| *k != key) {
            self.scene = Some((key, target(plan.scene, samples)));
        }
        let scene = &self.scene.as_ref().unwrap().1;
        set_camera(&camera_for(scene));
        clear_background(BLANK);
    }

    /// Resolve the world into the output. `restore` reselects the output.
    pub fn finish(&mut self, restore: impl Fn()) {
        let Some((plan, output)) = self.plan.take() else {
            return;
        };
        let Some((_, scene)) = &self.scene else {
            return;
        };
        let materials = match &self.materials {
            Some(Ok(materials)) => Some(materials),
            _ => None,
        };
        let mut source = scene.texture.clone();
        let mut size = plan.scene;
        if let (true, Some(m)) = (plan.fxaa, materials) {
            let smoothed = reuse(&mut self.smoothed, size);
            set_camera(&camera_for(&smoothed));
            blit(&source, Some(&m.fxaa), size, 0.);
            source = smoothed.texture.clone();
        }
        let sharpen = plan.sharpness > 0. && materials.is_some();
        if sharpen && size != output {
            let upscaled = reuse(&mut self.upscaled, output);
            set_camera(&camera_for(&upscaled));
            blit(
                &source,
                materials.filter(|_| plan.easu).map(|m| &m.easu),
                size,
                0.,
            );
            source = upscaled.texture.clone();
            size = output;
        }
        restore();
        let final_pass = match materials {
            Some(m) if sharpen => Some(&m.rcas),
            Some(m) if plan.easu && size != output => Some(&m.easu),
            _ => None,
        };
        // The scene was cleared transparent; the world covers it fully, so draw
        // it opaque over the arena background.
        blit(&source, final_pass, size, plan.sharpness);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_skip_work_at_native_scale_and_pair_fsr_with_sharpening() {
        let mut settings = Settings {
            render_scale: 1.,
            upscaler: 0,
            sharpness: 0.,
            fxaa: false,
            ..Default::default()
        };
        assert_eq!(Plan::new(&settings, (1920, 1080)), None);
        settings.render_scale = 0.5;
        settings.upscaler = 1;
        let plan = Plan::new(&settings, (1920, 1080)).unwrap();
        assert_eq!(plan.scene, (960, 540));
        assert!(plan.easu);
        assert!(plan.sharpness > 0.);
        settings.render_scale = 2.;
        let plan = Plan::new(&settings, (3000, 2000)).unwrap();
        assert_eq!(plan.scene, (4096, 2731));
        assert!(!plan.easu);
    }
}
