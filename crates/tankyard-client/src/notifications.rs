use crate::ui::*;
use macroquad::prelude::*;
use tankyard_client::achievements::{ACHIEVEMENTS, Feed};

pub fn draw(feed: &Feed, s: f32, reduced_motion: bool) {
    let Some((achievement, age)) = feed.current() else {
        return;
    };
    let fade = if reduced_motion {
        1.0
    } else {
        (age / 0.2).min(1.0) * ((4.5 - age) / 0.3).clamp(0.0, 1.0)
    };
    let settle = if reduced_motion {
        1.0
    } else {
        1.0 - (-age * 16.0).exp()
    };
    let r = Rect::new(
        (screen_width() - 374.0 * s) * 0.5,
        (119.0 - (1.0 - settle) * 16.0) * s,
        374.0 * s,
        83.0 * s,
    );
    let index = ACHIEVEMENTS
        .iter()
        .position(|a| a.id == achievement.id)
        .unwrap_or(0);
    let color = card_color(index);
    draw_rectangle(r.x, r.y, r.w, r.h, alpha(surface_color(), fade * 0.97));
    draw_rectangle_lines(
        r.x,
        r.y,
        r.w,
        r.h,
        2.5 * s,
        alpha(Color::new(0.333, 0.333, 0.333, 1.0), fade),
    );
    draw_rectangle(
        r.x + 2.0 * s,
        r.y + 2.0 * s,
        r.w - 4.0 * s,
        5.0 * s,
        alpha(color, fade),
    );
    achievement_badge(
        vec2(r.x + 29.0 * s, r.y + 44.0 * s),
        18.0 * s,
        alpha(color, fade),
        true,
    );
    label(
        "Achievement unlocked",
        r.x + 57.0 * s,
        r.y + 26.0 * s,
        12.0 * s,
        alpha(muted_color(), fade),
    );
    label(
        achievement.title,
        r.x + 57.0 * s,
        r.y + 49.0 * s,
        20.0 * s,
        alpha(text_color(), fade),
    );
    label(
        achievement.description,
        r.x + 57.0 * s,
        r.y + 69.0 * s,
        11.0 * s,
        alpha(muted_color(), fade),
    );
}
