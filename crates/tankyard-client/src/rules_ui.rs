//! One rules form shared by the editor and live server controls.
use crate::ui::*;
use macroquad::prelude::*;
use tankyard_mods::GameRules;

pub fn controls(rules: &mut GameRules, rect: Rect, s: f32) {
    let gap = 24. * s;
    let narrow = rect.w < 500. * s;
    let width = if narrow { rect.w } else { (rect.w - gap) * 0.5 };
    let fields = [
        ("Movement", &mut rules.movement_multiplier, [0.1, 3.], 0.05),
        ("Acceleration", &mut rules.acceleration, [0.1, 30.], 0.1),
        ("Coasting", &mut rules.coast, [0.1, 30.], 0.1),
        ("Damage", &mut rules.damage_multiplier, [0., 5.], 0.05),
        ("Health", &mut rules.health_multiplier, [0.1, 5.], 0.05),
        ("Regeneration", &mut rules.regen_multiplier, [0., 5.], 0.05),
        ("Experience", &mut rules.xp_multiplier, [0., 10.], 0.1),
        ("Score", &mut rules.score_multiplier, [0., 10.], 0.1),
        (
            "Respawn seconds",
            &mut rules.respawn_seconds,
            [0.5, 60.],
            0.5,
        ),
    ];
    let rows = if narrow { fields.len() } else { 5 };
    for (index, (title, value, limits, step)) in fields.into_iter().enumerate() {
        let x = rect.x + (index / rows) as f32 * (width + gap);
        let y = rect.y + (index % rows) as f32 * 54. * s;
        slider_value(
            Rect::new(x, y, width, 32. * s),
            title,
            value,
            limits,
            step,
            s * 0.85,
            |v| format!("{v:.2}"),
        );
    }
}

pub fn presets(rules: &mut GameRules, rect: Rect, s: f32) -> bool {
    let presets = [
        ("Standard", GameRules::default()),
        (
            "Arcade",
            GameRules {
                movement_multiplier: 1.15,
                xp_multiplier: 2.,
                respawn_seconds: 1.,
                ..Default::default()
            },
        ),
        (
            "Endurance",
            GameRules {
                health_multiplier: 1.5,
                regen_multiplier: 0.5,
                xp_multiplier: 1.5,
                ..Default::default()
            },
        ),
        (
            "Drift",
            GameRules {
                movement_multiplier: 1.3,
                acceleration: 3.,
                coast: 1.5,
                ..Default::default()
            },
        ),
    ];
    let columns = if rect.w < 500. * s { 2 } else { 4 };
    let w = (rect.w - (columns - 1) as f32 * 8. * s) / columns as f32;
    for (index, (title, preset)) in presets.into_iter().enumerate() {
        if button(
            Rect::new(
                rect.x + (index % columns) as f32 * (w + 8. * s),
                rect.y + (index / columns) as f32 * 38. * s,
                w,
                30. * s,
            ),
            title,
            *rules == preset,
            true,
            s * 0.8,
        ) {
            *rules = preset;
            return true;
        }
    }
    false
}
