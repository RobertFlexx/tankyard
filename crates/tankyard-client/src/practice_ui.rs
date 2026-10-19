use crate::ui::{RED, *};
use macroquad::prelude::*;
use tankyard_client::practice::{Goal, Record, Run, Status, clock};

pub enum Action {
    None,
    Retry(usize),
    Browse,
}

pub fn draw(run: &Run, records: &[Record], s: f32, top: f32) -> Action {
    let challenge = run.challenge();
    let remaining = challenge.seconds * 60 - run.ticks;
    let goal = match challenge.goal {
        Goal::Score(target) => format!("Score  {} / {}", run.progress, target),
        Goal::Kills(target) => format!("Tanks  {} / {}", run.progress, target),
        Goal::Survive => format!("Survived  {}", clock(run.ticks)),
        Goal::Boss => "Defeat the Sentinel".into(),
    };
    if !run.done() {
        let r = Rect::new(screen_width() * 0.5 - 180.0 * s, top, 360.0 * s, 59.0 * s);
        panel(r);
        label(
            &goal,
            r.x + 14.0 * s,
            r.y + 24.0 * s,
            15.0 * s,
            text_color(),
        );
        right_label(
            &clock(remaining.saturating_add(59)),
            r.x + r.w - 14.0 * s,
            r.y + 24.0 * s,
            17.0 * s,
            if remaining < 15 * 60 {
                RED
            } else {
                text_color()
            },
        );
        progress(
            Rect::new(r.x + 14.0 * s, r.y + 37.0 * s, r.w - 28.0 * s, 8.0 * s),
            run.progress as f32 / challenge.target() as f32,
            CYAN,
        );
        return Action::None;
    }

    draw_rectangle(
        0.,
        0.,
        screen_width(),
        screen_height(),
        alpha(bg_color(), 0.94),
    );
    let r = Rect::new(
        screen_width() * 0.5 - 255.0 * s,
        screen_height() * 0.5 - 156.0 * s,
        510.0 * s,
        312.0 * s,
    );
    panel(r);
    let color = if run.status == Status::Completed {
        CYAN
    } else {
        RED
    };
    draw_rectangle(r.x, r.y, r.w, 5.0 * s, color);
    let center = r.x + r.w * 0.5;
    center_label(
        challenge.title,
        center,
        r.y + 40.0 * s,
        18.0 * s,
        muted_color(),
    );
    center_label(
        if run.status == Status::Completed {
            "Challenge complete"
        } else {
            "Try again"
        },
        center,
        r.y + 83.0 * s,
        30.0 * s,
        text_color(),
    );
    center_label(
        &format!("{}  ·  Time {}", goal, clock(run.ticks)),
        center,
        r.y + 119.0 * s,
        16.0 * s,
        text_color(),
    );
    if let Some(record) = records.iter().find(|record| record.id == challenge.id) {
        let text = if record.completed > 0 {
            format!(
                "Best {}  ·  {} completed / {} attempts",
                clock(record.best_ticks),
                record.completed,
                record.attempts
            )
        } else {
            format!("{} attempts", record.attempts)
        };
        let width = label_width(&text, 14.0 * s);
        let size = 14.0 * s * ((r.w - 56.0 * s) / width.max(1.0)).min(1.0);
        center_label(&text, center, r.y + 149.0 * s, size, muted_color());
    }
    wrap(
        challenge.tip,
        r.x + 28.0 * s,
        r.y + 187.0 * s,
        r.w - 56.0 * s,
        15.0 * s,
        muted_color(),
    );
    if button(
        Rect::new(r.x + 28.0 * s, r.y + 247.0 * s, 216.0 * s, 38.0 * s),
        "Retry [R]",
        true,
        true,
        s * 0.9,
    ) {
        return Action::Retry(run.index);
    }
    if button(
        Rect::new(r.x + 266.0 * s, r.y + 247.0 * s, 216.0 * s, 38.0 * s),
        "Practice [Esc]",
        false,
        true,
        s * 0.9,
    ) {
        return Action::Browse;
    }
    Action::None
}
