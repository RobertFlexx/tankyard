//! Bacteria's isolated office maze. Its colliders come from the simulation;
//! only the wallpaper, carpet, ceiling fixtures and atmosphere live here.
mod ceiling;

use crate::{
    render::{self, View},
    settings::Settings,
    ui::*,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use tankyard_content::Content;
use tankyard_core::{LightState, POCKET_SECONDS, PocketLight, Snapshot, Tank, V2};

pub const LIGHT_SIZE: Vec2 = vec2(110., 50.);

/// Shared with the temporal motion pass so the roof never trails the floor.
pub fn ceiling_position(position: Vec2, size: Vec2, reduced: bool) -> Vec2 {
    let center = size * 0.5;
    center + (position - center) * if reduced { 1. } else { 0.925 }
}

/// How brightly a fixture shines now, 0 to 1. Failing tubes drop out at
/// irregular moments, a few times a second at most; reduced motion holds
/// them at a steady low level instead.
fn fixture_level(light: &PocketLight, time: f32, reduced: bool) -> f32 {
    let phase = light.pos.x * 0.007 + light.pos.y * 0.003;
    match light.state {
        LightState::Dead => 0.,
        LightState::Failing if reduced => 0.55,
        LightState::Failing => {
            let slot = (time * 5. + phase * 3.).floor();
            let roll = ((slot * 12.9898 + phase * 78.233).sin() * 43758.547)
                .fract()
                .abs();
            if roll < 0.24 {
                0.18
            } else {
                0.86 + (time * 23. + phase).sin() * 0.06
            }
        }
        LightState::Steady if reduced => 1.,
        LightState::Steady => 0.98 + (time * 2.1 + phase).sin() * 0.02,
    }
}

struct Room {
    carpet: Texture2D,
    wallpaper: Texture2D,
    light: Texture2D,
    ceiling: Option<Material>,
}
thread_local! { static ROOM: RefCell<Option<Room>> = const { RefCell::new(None) }; }

/// Load once, away from the first ability press, so entering never decodes PNGs.
pub fn prepare() {
    ROOM.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        let texture = |bytes| {
            let texture = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
            texture.set_filter(FilterMode::Linear);
            texture
        };
        *slot.borrow_mut() = Some(Room {
            carpet: texture(include_bytes!("../../../assets/pocket/carpet.png")),
            wallpaper: texture(include_bytes!("../../../assets/pocket/wallpaper.png")),
            light: texture(include_bytes!("../../../assets/pocket/fluorescent.png")),
            ceiling: ceiling::material(),
        });
    });
}

fn tiles(texture: &Texture2D, rect: tankyard_core::Rect, tile: f32, view: &View, tint: Color) {
    let top = view.world(vec2(0., 0.));
    let bottom = view.world(vec2(screen_width(), screen_height()));
    let left = rect.x.max(top.x);
    let right = (rect.x + rect.w).min(bottom.x);
    let first = rect.y.max(top.y);
    let last = (rect.y + rect.h).min(bottom.y);
    if right <= left || last <= first {
        return;
    }
    for y in (first / tile).floor() as i32..(last / tile).ceil() as i32 {
        for x in (left / tile).floor() as i32..(right / tile).ceil() as i32 {
            let origin = V2::new(x as f32 * tile, y as f32 * tile);
            let a = V2::new(left.max(origin.x), first.max(origin.y));
            let b = V2::new(right.min(origin.x + tile), last.min(origin.y + tile));
            let screen = view.screen(a);
            draw_texture_ex(
                texture,
                screen.x,
                screen.y,
                tint,
                DrawTextureParams {
                    dest_size: Some(vec2(b.x - a.x, b.y - a.y) * view.zoom),
                    source: Some(Rect::new(
                        (a.x - origin.x) / tile * texture.width(),
                        (a.y - origin.y) / tile * texture.height(),
                        (b.x - a.x) / tile * texture.width(),
                        (b.y - a.y) / tile * texture.height(),
                    )),
                    ..Default::default()
                },
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    snapshot: &Snapshot,
    tank: &Tank,
    content: &Content,
    view: &View,
    settings: &Settings,
    interpolation: f32,
    time: f32,
    position: V2,
) {
    prepare();
    clear_background(Color::new(0.08, 0.075, 0.045, 1.));
    let z = view.zoom;
    tankyard_core::with_pocket_layout(&snapshot.config, |layout| {
        ROOM.with(|slot| {
            let room = slot.borrow();
            let Some(room) = room.as_ref() else {
                return;
            };
            let top_left = view.world(vec2(0., 0.));
            let bottom_right = view.world(vec2(screen_width(), screen_height()));
            tiles(
                &room.carpet,
                tankyard_core::Rect {
                    x: top_left.x,
                    y: top_left.y,
                    w: bottom_right.x - top_left.x,
                    h: bottom_right.y - top_left.y,
                },
                240.,
                view,
                Color::new(0.82, 0.79, 0.67, 1.),
            );
            // Beyond the boundary everything is solid wall, so its outer face
            // never shows as a seam.
            let inner = |edge: &tankyard_core::PocketEdge| {
                let mid = (edge.a + edge.b) * 0.5;
                let b = layout.bounds;
                mid.x >= b.x - 1.
                    && mid.y >= b.y - 1.
                    && mid.x <= b.x + b.w + 1.
                    && mid.y <= b.y + b.h + 1.
            };
            // Cast shadows only from the union's exposed south and east edges.
            for edge in layout.edges.iter().filter(|edge| inner(edge)) {
                if !view.visible((edge.a + edge.b) * 0.5, edge.a.distance(edge.b) * 0.5 + 18.) {
                    continue;
                }
                if edge.normal.x + edge.normal.y <= 0. {
                    continue;
                }
                let a = view.screen(edge.a);
                let b = view.screen(edge.b);
                draw_line(
                    a.x + 7. * z,
                    a.y + 10. * z,
                    b.x + 7. * z,
                    b.y + 10. * z,
                    12. * z,
                    Color::new(0.14, 0.11, 0.05, 0.16),
                );
                draw_line(
                    a.x + 4. * z,
                    a.y + 6. * z,
                    b.x + 4. * z,
                    b.y + 6. * z,
                    7. * z,
                    Color::new(0.14, 0.11, 0.05, 0.12),
                );
            }
            let lit = crate::glow::begin(crate::glow::Blend::Glow);
            for light in &layout.lights {
                let level = fixture_level(light, time, settings.reduced_motion);
                if level <= 0. || !view.visible(light.pos, 180.) {
                    continue;
                }
                let p = view.screen(light.pos);
                if lit {
                    crate::glow::spot(p, 180. * z, Color::new(1., 0.94, 0.62, 0.20 * level));
                } else {
                    draw_circle(
                        p.x,
                        p.y,
                        130. * z,
                        Color::new(1., 0.94, 0.62, 0.035 * level),
                    );
                }
            }
            if lit {
                crate::glow::end();
            }
            let paper = Color::new(1., 0.99, 0.82, 1.);
            for &wall in &layout.walls {
                if view.visible(wall.center(), wall.w.max(wall.h) + 20.) {
                    tiles(&room.wallpaper, wall, 128., view, paper);
                }
            }
            // The paper continues on the same tile grid past the boundary
            // wall, so the maze ends in a solid mass rather than open floor.
            let b = layout.bounds;
            let (near, far) = (top_left, bottom_right);
            for outside in [
                tankyard_core::Rect {
                    x: near.x,
                    y: near.y,
                    w: b.x - near.x,
                    h: far.y - near.y,
                },
                tankyard_core::Rect {
                    x: b.x + b.w,
                    y: near.y,
                    w: far.x - b.x - b.w,
                    h: far.y - near.y,
                },
                tankyard_core::Rect {
                    x: b.x,
                    y: near.y,
                    w: b.w,
                    h: b.y - near.y,
                },
                tankyard_core::Rect {
                    x: b.x,
                    y: b.y + b.h,
                    w: b.w,
                    h: far.y - b.y - b.h,
                },
            ] {
                tiles(&room.wallpaper, outside, 128., view, paper);
            }
            for edge in layout.edges.iter().filter(|edge| inner(edge)) {
                if !view.visible((edge.a + edge.b) * 0.5, edge.a.distance(edge.b) * 0.5 + 4.) {
                    continue;
                }
                let a = view.screen(edge.a);
                let b = view.screen(edge.b);
                let shaded = edge.normal.x + edge.normal.y > 0.;
                draw_line(a.x, a.y, b.x, b.y, 2. * z, Color::new(0.36, 0.32, 0.19, 1.));
                let inset = edge.normal * (-3.);
                let a = view.screen(edge.a + inset);
                let b = view.screen(edge.b + inset);
                draw_line(
                    a.x,
                    a.y,
                    b.x,
                    b.y,
                    if shaded { 3. * z } else { 1. * z },
                    if shaded {
                        Color::new(0.43, 0.37, 0.23, 1.)
                    } else {
                        Color::new(0.78, 0.75, 0.48, 0.65)
                    },
                );
            }
        });
    });
    draw_echoes(tank, content, view, interpolation);
    if let Some(class) = content.class(&tank.class) {
        let p = view.screen(position);
        draw_circle(
            p.x + 4. * z,
            p.y + 7. * z,
            tank.radius * z * 1.1,
            Color::new(0., 0., 0., 0.15),
        );
        render::class_model(
            class,
            p,
            tank.radius * z,
            tank.aim,
            0.,
            0.,
            Color::new(0.76, 0.82, 0.53, 1.),
            if settings.reduced_motion { 0. } else { time },
        );
        render::health_bar(
            p,
            tank.radius * z,
            tank.health / tank.max_health,
            z,
            Color::new(0.84, 0.90, 0.36, 1.),
        );
    }
    // The ceiling is a foreground layer: tanks pass beneath the fixtures.
    // A little parallax separates them from the pools of light on the carpet.
    tankyard_core::with_pocket_layout(&snapshot.config, |layout| {
        ROOM.with(|slot| {
            let room = slot.borrow();
            let Some(room) = room.as_ref() else {
                return;
            };
            let size = vec2(screen_width(), screen_height());
            let fixture_size = LIGHT_SIZE * z;
            if let Some(material) = &room.ceiling {
                gl_use_material(material);
            }
            for light in &layout.lights {
                // Counter-motion towards the view centre separates the roof
                // from the floor, while the transparent housing keeps sight clear.
                let p = ceiling_position(view.screen(light.pos), size, settings.reduced_motion);
                // Cull on the ceiling plane, including on ultrawide displays.
                if p.x + fixture_size.x * 0.5 < 0.
                    || p.y + fixture_size.y * 0.5 < 0.
                    || p.x - fixture_size.x * 0.5 > size.x
                    || p.y - fixture_size.y * 0.5 > size.y
                {
                    continue;
                }
                // A dead tube is grey glass; a failing one dims with its glow.
                let level = fixture_level(light, time, settings.reduced_motion);
                let tint = 0.52 + 0.48 * level;
                draw_texture_ex(
                    &room.light,
                    p.x - LIGHT_SIZE.x * 0.5 * z,
                    p.y - LIGHT_SIZE.y * 0.5 * z,
                    Color::new(tint, tint * 0.99, tint * 0.90, 0.62),
                    DrawTextureParams {
                        dest_size: Some(LIGHT_SIZE * z),
                        ..Default::default()
                    },
                );
            }
            if room.ceiling.is_some() {
                gl_use_default_material();
            }
        });
    });
    // Broad, quiet edge shading rather than static, scanlines or flashing.
    for edge in 0..7 {
        let inset = edge as f32 * 11.;
        draw_rectangle_lines(
            inset,
            inset,
            screen_width() - 2. * inset,
            screen_height() - 2. * inset,
            24.,
            Color::new(0.04, 0.035, 0.01, 0.045),
        );
    }
    let s = crate::ui_layout::scale(screen_width(), screen_height(), settings.ui_scale);
    if POCKET_SECONDS - tank.pocket < 2.8 {
        arcade_center(
            "Keep moving. The doubles can force you out.",
            screen_width() * 0.5,
            72. * s,
            13. * s,
        );
    } else if tank.pocket < 1.5 {
        center_label(
            "RETURNING TO THE ARENA",
            screen_width() * 0.5,
            72. * s,
            14. * s,
            Color::new(0.97, 0.94, 0.72, 0.85),
        );
    }
}

fn draw_echoes(tank: &Tank, content: &Content, view: &View, interpolation: f32) {
    let age = POCKET_SECONDS - tank.pocket;
    let opacity = ((age - 0.25) / 0.35).clamp(0., 1.);
    let z = view.zoom;
    for echo in &tank.pocket_echoes {
        let position = echo.previous.lerp(echo.pos, interpolation);
        if !view.visible(position, echo.radius + 65.) {
            continue;
        }
        let Some(class) = content.class(&echo.class) else {
            continue;
        };
        let p = view.screen(position);
        draw_circle(
            p.x + 4. * z,
            p.y + 7. * z,
            echo.radius * z * 1.1,
            Color::new(0.08, 0.07, 0.04, 0.20 * opacity),
        );
        // A static cast of a player's hull. The sleeping pose has no idle
        // rotation; waking only changes its aim and movement, never its guns.
        render::class_model(
            class,
            p,
            echo.radius * z,
            echo.aim,
            0.,
            0.,
            Color::new(0.55, 0.53, 0.38, opacity),
            0.,
        );
    }
}

pub fn blackout(remaining: f32, reduced: bool) {
    if remaining <= 0. {
        return;
    }
    let age = POCKET_SECONDS - remaining;
    let duration = if reduced { 0.6 } else { 0.4 };
    let opacity = if age < 0.12 {
        age / 0.12
    } else {
        1. - (age - 0.12) / duration
    };
    if opacity > 0. {
        draw_rectangle(
            0.,
            0.,
            screen_width(),
            screen_height(),
            Color::new(0., 0., 0., opacity.clamp(0., 1.)),
        );
    }
}
