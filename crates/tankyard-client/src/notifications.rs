use crate::ui::*;
use macroquad::prelude::*;
use tankyard_client::achievements::{ACHIEVEMENTS, Feed};

pub struct TopOverlays {
    pub server_y: f32,
    pub practice_y: f32,
    pub kill_y: f32,
    pub notice_y: f32,
    pub achievement_y: f32,
}

/// Allocate one centered vertical stack for transient overlays. A new boss,
/// practice objective, kill feed and achievement may all arrive on one frame.
pub fn top_overlays(
    height: f32,
    s: f32,
    boss: bool,
    server_height: f32,
    practice: bool,
    kills: usize,
    notice: bool,
) -> TopOverlays {
    let server_y = if boss { 188. } else { 110. } * s;
    let practice_y = server_y + server_height;
    let mut next = (if boss { 188. } else { 119. } * s) + server_height;
    if practice {
        next = next.max(practice_y + 73. * s);
    }
    let kill_y = if practice {
        practice_y + 91. * s
    } else {
        (if boss { 208. } else { 132. }) * s + server_height
    };
    if kills > 0 {
        next = next.max(kill_y + (kills - 1) as f32 * 28. * s + 26. * s);
    }
    let notice_y = (height * if kills == 0 { 0.25 } else { 0.36 }).max(next + 25. * s);
    if notice {
        next = next.max(notice_y + 26. * s);
    }
    TopOverlays {
        server_y,
        practice_y,
        kill_y,
        notice_y,
        achievement_y: next,
    }
}

pub fn server_notice_rect(notice: &tankyard_protocol::ServerNotice, s: f32, top: f32) -> Rect {
    let width = (620. * s).min(screen_width() - 44. * s);
    let lines = wrapped_lines(&notice.text, width - 32. * s, 16. * s)
        .len()
        .min(6);
    Rect::new(
        (screen_width() - width) * 0.5,
        top,
        width,
        (48. + lines as f32 * 21.) * s,
    )
}

pub fn server_notice(notice: &tankyard_protocol::ServerNotice, rect: Rect, s: f32) -> bool {
    let warning = notice.kind == tankyard_protocol::NoticeKind::Warning;
    panel(rect);
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        3. * s,
        if warning { crate::ui::RED } else { CYAN },
    );
    center_label(
        if warning {
            "SERVER WARNING"
        } else {
            "SERVER ANNOUNCEMENT"
        },
        rect.center().x,
        rect.y + 23. * s,
        12. * s,
        muted_color(),
    );
    for (index, line) in wrapped_lines(&notice.text, rect.w - 32. * s, 16. * s)
        .iter()
        .take(6)
        .enumerate()
    {
        center_label(
            line,
            rect.center().x,
            rect.y + (49. + index as f32 * 21.) * s,
            16. * s,
            text_color(),
        );
    }
    small_button(
        Rect::new(rect.x + rect.w - 30. * s, rect.y + 7. * s, 24. * s, 24. * s),
        "×",
        s * 0.8,
    )
}

pub fn draw(feed: &Feed, s: f32, reduced_motion: bool, top: f32) {
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
    let width = (374. * s)
        .max(label_width(achievement.title, 20. * s) + 72. * s)
        .max(label_width(achievement.description, 11. * s) + 72. * s)
        .min(screen_width() - 44. * s);
    let r = Rect::new(
        (screen_width() - width) * 0.5,
        top + (1.0 - settle) * 16.0 * s,
        width,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boss_practice_kills_notices_and_achievements_share_a_clear_centered_stack() {
        for (height, scale) in [(600., 0.6), (800., 0.97), (1080., 1.32), (1440., 1.4)] {
            for boss in [false, true] {
                for practice in [false, true] {
                    for kills in [0, 1, 3] {
                        for notice in [false, true] {
                            let layout =
                                top_overlays(height, scale, boss, 0., practice, kills, notice);
                            if boss {
                                assert!(layout.achievement_y >= 188. * scale);
                            }
                            if practice {
                                if boss {
                                    assert!(layout.practice_y >= 188. * scale);
                                }
                                assert!(layout.achievement_y >= layout.practice_y + 73. * scale);
                                assert!(layout.kill_y >= layout.practice_y + 80. * scale);
                            }
                            if kills > 0 {
                                assert!(
                                    layout.achievement_y
                                        >= layout.kill_y
                                            + (kills - 1) as f32 * 28. * scale
                                            + 26. * scale
                                );
                            }
                            if notice {
                                assert!(layout.achievement_y >= layout.notice_y + 26. * scale);
                            }
                            assert!(layout.achievement_y + 99. * scale < height - 100. * scale);
                        }
                    }
                }
            }
        }
    }
}
