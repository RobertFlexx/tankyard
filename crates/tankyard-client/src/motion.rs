//! Per-pixel motion for temporal upscalers (DLSS): where each pixel of this
//! frame was on screen in the last one, in render pixels.
//!
//! The arena is flat, so motion is drawn the cheap way: the camera's motion
//! over the whole screen, then each moving thing carrying its own. Shapes
//! and bullets are round enough for a disc; tanks use their real silhouette
//! (from the same geometry the renderer draws), so the arena behind them
//! keeps its own motion. Motion travels through vertex colors, 16 bits per
//! axis, so the world draws in a handful of batches and the shader writes it
//! out as floats.
use crate::{render::View, tank_model::Primitive};
use macroquad::prelude::*;
use std::collections::HashMap;
use tankyard_content::Content;
use tankyard_core::{EntityId, Snapshot, V2};

/// Motion beyond this many render pixels per frame is a cut, not movement.
pub const RANGE: f32 = 256.;

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying highp vec4 bytes;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    // macroquad passes colors as bytes, 0 to 255.
    bytes = color0;
}"#;

const FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp vec4 bytes;
void main() {
    vec2 high = floor(vec2(bytes.r, bytes.b) + 0.5);
    vec2 low = floor(vec2(bytes.g, bytes.a) + 0.5);
    vec2 unit = (high * 256.0 + low) / 65535.0;
    gl_FragColor = vec4((unit * 2.0 - 1.0) * 256.0, 0.0, 1.0);
}"#;

thread_local! {
    static MATERIAL: Option<Material> = load_material(
        ShaderSource::Glsl { vertex: VERTEX, fragment: FRAGMENT },
        MaterialParams::default(),
    )
    .map_err(|error| log::warn!("Motion vectors unavailable: {error}"))
    .ok();
}

/// A color carrying `motion` (render pixels) for the motion shader.
pub fn encode(motion: Vec2) -> Color {
    let unit = (motion / RANGE * 0.5 + 0.5).clamp(Vec2::ZERO, Vec2::ONE);
    let q = (unit * 65535.).round();
    let (x, y) = (q.x as u32, q.y as u32);
    // Half a step up, so the conversion to bytes can't round down a level.
    let byte = |v: u32| (v as f32 + 0.5) / 255.;
    Color::new(byte(x >> 8), byte(x & 255), byte(y >> 8), byte(y & 255))
}

/// What the motion is measured against: the last frame's camera and the
/// positions things were drawn at.
#[derive(Default)]
pub struct History {
    camera: Option<Camera>,
    pocket: bool,
    /// When motion was last drawn; a gap means the arena wasn't on screen.
    drawn_at: f64,
    positions: HashMap<(u8, EntityId), V2>,
    seen: HashMap<(u8, EntityId), V2>,
}

#[derive(Clone, Copy, PartialEq)]
struct Camera {
    center: V2,
    zoom: f32,
    shake: Vec2,
    screen: Vec2,
}

impl Camera {
    fn of(view: &View) -> Self {
        Self {
            center: view.center,
            zoom: view.zoom,
            shake: view.shake,
            screen: vec2(screen_width(), screen_height()),
        }
    }
    fn screen(&self, p: V2) -> Vec2 {
        vec2(
            (p.x - self.center.x) * self.zoom + self.screen.x * 0.5,
            (p.y - self.center.y) * self.zoom + self.screen.y * 0.5,
        ) + self.shake
    }
    fn world(&self, p: Vec2) -> V2 {
        let p = p - self.shake - self.screen * 0.5;
        V2::new(
            p.x / self.zoom + self.center.x,
            p.y / self.zoom + self.center.y,
        )
    }
}

impl History {
    /// Draw this frame's motion into the selected target and remember it.
    /// `render` is render pixels per window unit. Returns true when the view
    /// cut (a new match, a return from the menus, a respawn across the map,
    /// a resize), so the upscaler should drop its history.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        snapshot: &Snapshot,
        content: &Content,
        local: EntityId,
        view: &View,
        previous_positions: &HashMap<EntityId, (V2, f32)>,
        interpolation: f32,
        render: Vec2,
    ) -> bool {
        let now = Camera::of(view);
        let before = self.camera.replace(now);
        let time = get_time();
        let gap = time - std::mem::replace(&mut self.drawn_at, time) > 0.25;
        let pocket = snapshot.tanks.iter().any(|tank| {
            tank.id == local
                && tank.alive
                && tank.pocket > 0.
                && tankyard_core::POCKET_SECONDS - tank.pocket >= 0.12
        });
        let changed_layer = std::mem::replace(&mut self.pocket, pocket) != pocket;
        let cut = gap
            || changed_layer
            || before.is_none_or(|b| {
                b.screen != now.screen
                    || b.center.distance(now.center) > 400.
                    || (b.zoom / now.zoom - 1.).abs() > 0.08
            });
        let before = if cut { now } else { before.unwrap_or(now) };
        let Some(()) = MATERIAL.with(|material| {
            let material = material.as_ref()?;
            gl_use_material(material);
            Some(())
        }) else {
            return true;
        };
        // Everything fixed in the world moves with the camera.
        let middle = now.screen * 0.5;
        let camera_motion = before.screen(now.world(middle)) - middle;
        draw_rectangle(
            0.,
            0.,
            now.screen.x,
            now.screen.y,
            encode(camera_motion * render),
        );
        std::mem::swap(&mut self.positions, &mut self.seen);
        self.positions.clear();
        let seen = &self.seen;
        let positions = &mut self.positions;
        // Where `position` is on screen now, and the motion to draw there;
        // None when it's off screen.
        let mut motion = |kind: u8, id: EntityId, position: V2, radius: f32| {
            let here = now.screen(position);
            let r = radius * now.zoom;
            if here.x + r < 0.
                || here.y + r < 0.
                || here.x - r > now.screen.x
                || here.y - r > now.screen.y
            {
                return None;
            }
            positions.insert((kind, id), position);
            // Things first seen this frame have no past; treat them as fixed.
            let motion = match (cut, seen.get(&(kind, id))) {
                (false, Some(&last)) => before.screen(last) - here,
                _ => camera_motion,
            };
            Some((here, encode(motion * render)))
        };
        let at = |id: EntityId, current: V2| {
            previous_positions
                .get(&id)
                .map_or(current, |&(previous, _)| {
                    crate::session::interpolate_position(previous, current, interpolation)
                })
        };
        if !pocket {
            for target in &snapshot.targets {
                let radius = target.radius * 1.25 + 4.;
                if let Some((here, color)) = motion(0, target.id, at(target.id, target.pos), radius)
                {
                    draw_circle(here.x, here.y, radius * now.zoom, color);
                }
            }
            for projectile in &snapshot.projectiles {
                let position = projectile.previous.lerp(projectile.pos, interpolation);
                let radius = projectile.radius + 3.;
                if let Some((here, color)) = motion(1, projectile.id, position, radius) {
                    draw_circle(here.x, here.y, radius * now.zoom, color);
                }
            }
            for boss in &snapshot.bosses {
                let radius = boss.radius * 1.7;
                if let Some((here, color)) = motion(2, boss.id, at(boss.id, boss.pos), radius) {
                    draw_circle(here.x, here.y, radius * now.zoom, color);
                }
            }
        }
        // Tanks last, as they're drawn on top.
        let z = now.zoom;
        if pocket && let Some(visitor) = snapshot.tanks.iter().find(|t| t.id == local) {
            for (index, echo) in visitor.pocket_echoes.iter().enumerate() {
                let position = echo.previous.lerp(echo.pos, interpolation);
                let Some((here, color)) = motion(4, index as EntityId, position, echo.radius * 4.)
                else {
                    continue;
                };
                if let Some(class) = content.class(&echo.class) {
                    crate::tank_model::class_model_geometry(
                        class,
                        here,
                        echo.radius * z,
                        echo.aim,
                        0.,
                        0.,
                        color,
                        0.,
                        None,
                        None,
                        Some(z),
                        &mut |primitive| silhouette(primitive, color, 1.5 * z),
                    );
                }
            }
        }
        for tank in snapshot
            .tanks
            .iter()
            .filter(|t| t.alive && (t.id == local || (!pocket && t.pocket <= 0.)))
        {
            let Some((here, color)) = motion(3, tank.id, at(tank.id, tank.pos), tank.radius * 4.)
            else {
                continue;
            };
            let r = tank.radius * z;
            match content.class(&tank.class).filter(|_| !tank.creator_boss) {
                Some(class) => crate::tank_model::class_model_geometry(
                    class,
                    here,
                    r,
                    tank.aim,
                    0.,
                    0.,
                    color,
                    0.,
                    None,
                    Some(&tank.barrel_aims),
                    Some(z),
                    &mut |primitive| silhouette(primitive, color, 1.5 * z),
                ),
                None => draw_circle(here.x, here.y, r * 1.9, color),
            }
            // The name above and health bar below move with the tank.
            let bar = (r * 1.8).max(28. * z);
            draw_rectangle(
                here.x - bar * 0.5 - z,
                here.y + r + 7. * z,
                bar + 2. * z,
                8. * z,
                color,
            );
            if local != 0 && tank.id != local {
                let name = crate::ui::label_width(&tank.name, 15. * z) + 40. * z;
                draw_rectangle(
                    here.x - name * 0.5,
                    here.y - r - 30. * z,
                    name,
                    20. * z,
                    color,
                );
            }
        }
        if pocket {
            // Roof fixtures move against the floor. Their dominant translucent
            // pixels need the roof's motion rather than the carpet's vectors.
            let reduced = crate::accessibility::reduced_motion();
            let size = crate::pocket_view::LIGHT_SIZE * z;
            tankyard_core::with_pocket_layout(&snapshot.config, |layout| {
                for light in &layout.lights {
                    let light = light.pos;
                    let here = crate::pocket_view::ceiling_position(
                        now.screen(light),
                        now.screen,
                        reduced,
                    );
                    let last = crate::pocket_view::ceiling_position(
                        before.screen(light),
                        before.screen,
                        reduced,
                    );
                    if here.x + size.x * 0.5 < 0.
                        || here.y + size.y * 0.5 < 0.
                        || here.x - size.x * 0.5 > now.screen.x
                        || here.y - size.y * 0.5 > now.screen.y
                    {
                        continue;
                    }
                    draw_rectangle(
                        here.x - size.x * 0.5,
                        here.y - size.y * 0.5,
                        size.x,
                        size.y,
                        encode((last - here) * render),
                    );
                }
            });
        }
        gl_use_default_material();
        cut
    }
}

/// A model primitive filled with motion, grown by `outline` so antialiased
/// edges carry it too.
fn silhouette(primitive: Primitive, color: Color, outline: f32) {
    match primitive {
        Primitive::Polygon {
            center,
            sides,
            radius,
            angle,
            ..
        } => {
            // Fewer than three sides is the renderer's circle.
            if sides < 3 {
                draw_circle(center.x, center.y, radius + outline, color);
            } else {
                draw_poly(
                    center.x,
                    center.y,
                    sides,
                    radius + outline,
                    angle.to_degrees(),
                    color,
                );
            }
        }
        Primitive::Triangle { vertices, .. } => {
            draw_triangle(vertices[0], vertices[1], vertices[2], color);
        }
        Primitive::Quad { vertices, .. } => {
            draw_triangle(vertices[0], vertices[1], vertices[2], color);
            draw_triangle(vertices[0], vertices[2], vertices[3], color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(color: Color) -> Vec2 {
        let byte = |v: f32| (v * 255.) as u32 as f32;
        let high = vec2(byte(color.r), byte(color.b));
        let low = vec2(byte(color.g), byte(color.a));
        ((high * 256. + low) / 65535. * 2. - 1.) * RANGE
    }

    #[test]
    fn motion_survives_the_trip_through_vertex_colors() {
        for motion in [
            Vec2::ZERO,
            vec2(0.37, -0.12),
            vec2(-12.5, 3.25),
            vec2(255., -255.),
            vec2(1000., -1000.),
        ] {
            let back = decode(encode(motion));
            let expected = motion.clamp(Vec2::splat(-RANGE), Vec2::splat(RANGE));
            assert!(
                (back - expected).abs().max_element() < 0.01,
                "{motion} -> {back}"
            );
        }
    }
}
