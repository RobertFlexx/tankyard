use crate::render::polygon;
use crate::ui::{CYAN, RED, VIOLET};
use crate::{render, settings::Settings, ui::*};
use macroquad::prelude::*;
use tankyard_content::Content;

pub fn panels(width: f32, height: f32) -> [Rect; 2] {
    [
        Rect::new(0., 0., width * 0.5, height),
        Rect::new(width * 0.5, 0., width * 0.5, height),
    ]
}

/// Runs before joining a server or starting a simulation, so taking time to
/// choose cannot time out a first connection or consume spawn protection.
pub async fn choose_theme(automated: bool) -> Option<bool> {
    let mut settings = Settings::load();
    if settings.theme_chosen || automated {
        return Some(settings.dark_arena);
    }
    let content = Content::builtin();
    let mut focus_dark = settings.dark_arena;
    let mut committed = None;
    loop {
        if is_quit_requested() {
            return None;
        }
        if let Some(dark) = committed
            && !is_mouse_button_down(MouseButton::Left)
            && !is_key_down(KeyCode::Enter)
            && !is_key_down(KeyCode::Space)
        {
            set_theme(dark);
            return Some(dark);
        }
        if is_key_pressed(KeyCode::Tab) {
            focus_dark = !focus_dark;
        }
        if is_key_pressed(KeyCode::Left) {
            focus_dark = true;
        }
        if is_key_pressed(KeyCode::Right) {
            focus_dark = false;
        }
        set_interactive(true);
        let w = screen_width();
        let h = screen_height();
        let s = (w / 1280.).min(h / 800.).clamp(0.5, 2.75);
        let mut choice = None;
        for (rect, dark) in panels(w, h).into_iter().zip([true, false]) {
            set_theme(dark);
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg_color());
            let grid = if dark {
                Color::new(0.17, 0.20, 0.23, 1.)
            } else {
                Color::new(0.765, 0.765, 0.765, 1.)
            };
            let step = (32. * s).max(16.) as usize;
            for x in (0..rect.w as usize).step_by(step) {
                draw_line(rect.x + x as f32, 0., rect.x + x as f32, h, 1., grid);
            }
            for y in (0..h as usize).step_by(step) {
                draw_line(rect.x, y as f32, rect.x + rect.w, y as f32, 1., grid);
            }
            let center = rect.x + rect.w * 0.5;
            let arena_y = h * 0.48;
            for (nx, ny, sides, radius, color) in [
                (0.22, 0.3, 5, 24., VIOLET),
                (0.72, 0.27, 3, 18., RED),
                (0.2, 0.68, 4, 15., AMBER),
                (0.76, 0.66, 5, 30., VIOLET),
            ] {
                let p = vec2(rect.x + rect.w * nx, h * ny);
                polygon(
                    p,
                    sides,
                    (radius + 2.5) * s,
                    0.3,
                    Color::new(0.33, 0.33, 0.33, 1.),
                );
                polygon(p, sides, radius * s, 0.3, color);
            }
            if let Some(class) = content.class("anchor") {
                render::class_model(
                    class,
                    vec2(center, arena_y),
                    32. * s,
                    -0.35,
                    0.,
                    0.,
                    CYAN,
                    0.,
                );
            }
            arcade_track(
                Rect::new(center - 34. * s, arena_y + 43. * s, 68. * s, 7. * s),
                0.76,
                Color::new(0.5, 0.85, 0.4, 1.),
            );
            arcade_center("Trapper", center, arena_y - 53. * s, 17. * s);
            let button_width = (rect.w - 48. * s).min(260. * s);
            let button_rect = Rect::new(
                center - button_width / 2.,
                h - 106. * s,
                button_width,
                52. * s,
            );
            if button(
                button_rect,
                if dark { "Dark" } else { "Light" },
                focus_dark == dark,
                committed.is_none(),
                s,
            ) {
                choice = Some(dark);
            }
        }
        set_theme(false);
        arcade_center("Choose your theme", w * 0.5, 67. * s, 30. * s);
        draw_line(
            w * 0.5,
            110. * s,
            w * 0.5,
            h - 140. * s,
            2. * s,
            alpha(CYAN, 0.45),
        );
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            choice = Some(focus_dark);
        }
        if let Some(dark) = choice.filter(|_| committed.is_none()) {
            settings.dark_arena = dark;
            settings.theme_chosen = true;
            match settings.save() {
                Ok(()) => {
                    set_theme(dark);
                    committed = Some(dark);
                }
                Err(message) => {
                    log::warn!("Could not save theme: {message}");
                    set_theme(dark);
                    committed = Some(dark);
                }
            }
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_split_the_window_exactly_at_all_supported_sizes() {
        for (w, h) in [(800., 600.), (1280., 800.), (3840., 2160.)] {
            let [dark, light] = panels(w, h);
            assert_eq!(dark.x + dark.w, light.x);
            assert_eq!(light.x + light.w, w);
            assert_eq!(dark.h, h);
            assert_eq!(light.h, h);
        }
    }
}
