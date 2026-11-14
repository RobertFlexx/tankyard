//! Bacteria's isolated office maze. Its colliders come from the simulation;
//! only the wallpaper, carpet, ceiling fixtures and atmosphere live here.
use crate::{
    render::{self, View},
    settings::Settings,
    ui::*,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use tankyard_content::Content;
use tankyard_core::{POCKET_SECONDS, Snapshot, Tank, V2};

struct Room {
    carpet: Texture2D,
    wallpaper: Texture2D,
    light: Texture2D,
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

pub fn draw(
    snapshot: &Snapshot,
    tank: &Tank,
    content: &Content,
    view: &View,
    settings: &Settings,
    time: f32,
) {
    prepare();
    clear_background(Color::new(0.08, 0.075, 0.045, 1.));
    let z = view.zoom;
    let mut fixtures = Vec::new();
    let bounds = snapshot.config.bounds();
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
        // Fixtures follow the room grid, and their soft pools sit on the carpet.
        let top = view.world(vec2(0., 0.));
        let bottom = view.world(vec2(screen_width(), screen_height()));
        for y in ((top.y - bounds.y) / 400.).floor() as i32
            ..=((bottom.y - bounds.y) / 400.).ceil() as i32
        {
            for x in ((top.x - bounds.x) / 400.).floor() as i32
                ..=((bottom.x - bounds.x) / 400.).ceil() as i32
            {
                let here = V2::new(
                    bounds.x + x as f32 * 400. + 185.,
                    bounds.y + y as f32 * 400. + 160.,
                );
                let p = view.screen(here);
                let wavering = if settings.reduced_motion {
                    1.
                } else {
                    0.98 + (time * 2.1 + x as f32 * 1.7 + y as f32).sin() * 0.02
                };
                for ring in (1..=5).rev() {
                    draw_circle(
                        p.x,
                        p.y,
                        (46. + ring as f32 * 22.) * z,
                        Color::new(1., 0.94, 0.60, 0.018 * wavering),
                    );
                }
                fixtures.push(p);
            }
        }
        for wall in tankyard_core::pocket_walls(&snapshot.config) {
            if !view.visible(wall.center(), wall.w.max(wall.h) + 20.) {
                continue;
            }
            let p = view.screen(V2::new(wall.x, wall.y));
            draw_rectangle(
                p.x + 10. * z,
                p.y + 13. * z,
                wall.w * z,
                wall.h * z,
                Color::new(0.12, 0.105, 0.05, 0.30),
            );
            tiles(
                &room.wallpaper,
                wall,
                128.,
                view,
                Color::new(1., 0.99, 0.82, 1.),
            );
            draw_rectangle_lines(
                p.x,
                p.y,
                wall.w * z,
                wall.h * z,
                2. * z,
                Color::new(0.37, 0.33, 0.18, 1.),
            );
            draw_line(
                p.x,
                p.y + wall.h * z - 3. * z,
                p.x + wall.w * z,
                p.y + wall.h * z - 3. * z,
                4. * z,
                Color::new(0.40, 0.34, 0.21, 1.),
            );
        }
    });
    if let Some(class) = content.class(&tank.class) {
        let p = view.screen(tank.pos);
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
    }
    // The ceiling is a foreground layer: tanks pass beneath the fixtures.
    // A little parallax separates them from the pools of light on the carpet.
    ROOM.with(|slot| {
        let room = slot.borrow();
        let Some(room) = room.as_ref() else {
            return;
        };
        let center = vec2(screen_width(), screen_height()) * 0.5;
        for floor in fixtures {
            let p = center + (floor - center) * 1.035;
            draw_rectangle(
                p.x - 50. * z,
                p.y - 23. * z,
                100. * z,
                46. * z,
                Color::new(0.38, 0.37, 0.27, 1.),
            );
            draw_texture_ex(
                &room.light,
                p.x - 48. * z,
                p.y - 21. * z,
                Color::new(1., 1., 0.90, 0.96),
                DrawTextureParams {
                    dest_size: Some(vec2(96., 42.) * z),
                    ..Default::default()
                },
            );
        }
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
    if tank.pocket < 1.5 {
        center_label(
            "RETURNING TO THE ARENA",
            screen_width() * 0.5,
            72. * s,
            14. * s,
            Color::new(0.97, 0.94, 0.72, 0.85),
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
