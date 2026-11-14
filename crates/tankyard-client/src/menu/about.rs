//! Credits and player achievements.
use super::*;

impl Menu {
    pub(super) fn credits(&self, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 177.0 * s;
        let width = screen_width() - 80.0 * s;
        label("Credits", x, y, 32.0 * s, text_color());
        label(
            "Made by RobertFlexx.",
            x,
            y + 52.0 * s,
            26.0 * s,
            text_color(),
        );
        wrap(
            "Inspired by Diep.io and Arras.io.",
            x,
            y + 94.0 * s,
            width.min(860.0 * s),
            20.0 * s,
            muted_color(),
        );
        label("BUILT WITH", x, y + 168.0 * s, 13.0 * s, accent());
        label(
            "Rust  /  Macroquad  /  Renet + Netcode  /  CPAL audio",
            x,
            y + 204.0 * s,
            23.0 * s,
            text_color(),
        );
        label("LICENSE", x, y + 264.0 * s, 13.0 * s, accent());
        label(
            "MIT or Apache-2.0",
            x,
            y + 300.0 * s,
            23.0 * s,
            text_color(),
        );
        let mut notices = String::new();
        if cfg!(tankyard_dlss) {
            notices +=
                "Upscaling by AMD FidelityFX Super Resolution 1, and NVIDIA DLSS on GeForce RTX. ";
        }
        if cfg!(target_vendor = "apple") {
            notices += "Rendering with Metal and upscaling with MetalFX by Apple. ";
        }
        notices += "DejaVu Sans Bold and Mono fonts; Lucide microphone icons (ISC). Backrooms textures by Amini Allight; fluorescent hum by theonlycanvas (CC0). Third-party licenses are in NOTICE.md.";
        if cfg!(tankyard_dlss) {
            notices += " NVIDIA, GeForce, RTX and DLSS are trademarks and/or registered trademarks of NVIDIA Corporation in the U.S. and other countries.";
        }
        if cfg!(target_vendor = "apple") {
            notices += " Apple, Metal and MetalFX are trademarks of Apple Inc., registered in the U.S. and other countries and regions.";
        }
        let below = wrap(
            &notices,
            x,
            y + 355.0 * s,
            width.min(900.0 * s),
            18.0 * s,
            muted_color(),
        );
        let affiliation = wrap(
            "Tank Yard is not affiliated with Diep.io or Arras.io.",
            x,
            (y + 411.0 * s).max(below + 8.0 * s),
            width.min(900.0 * s),
            18.0 * s,
            muted_color(),
        );
        // The credit below moves down when the notices above wrap further.
        let y = y + (affiliation + 40. * s - (y + 480. * s)).max(0.);
        tankyard_client::boss_model::boss_model_geometry(
            tankyard_core::BossKind::Bastion,
            crate::ui_layout::position(vec2(x + 28. * s, y + 488. * s)),
            22. * s,
            -0.2,
            crate::accessibility::motion_time(get_time() as f32),
            0,
            None,
            &mut render::draw_model_primitive,
        );
        label(
            &fitted_label(
                &format!(
                    "Preventing cheaters thanks to {}",
                    tankyard_server::bastion::NAME
                ),
                13. * s,
                (width - 80. * s).max(1.),
            ),
            x + 68. * s,
            y + 480. * s,
            13. * s,
            text_color(),
        );
        label(
            tankyard_server::bastion::TAGLINE,
            x + 68. * s,
            y + 505. * s,
            11. * s,
            muted_color(),
        );
        Action::None
    }
    pub(super) fn achievements(
        &self,
        settings: &mut Settings,
        match_claims: &[String],
        s: f32,
    ) -> Action {
        let x = 40.0 * s;
        let y = 169.0 * s;
        label("Achievements", x, y, 30.0 * s, text_color());
        for (i, (mode, name)) in [
            (AchievementMode::PerMatch, "Per match"),
            (AchievementMode::Global, "Global"),
            (AchievementMode::Off, "Off"),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(x + i as f32 * 137.0 * s, y + 20.0 * s, 125.0 * s, 32.0 * s),
                name,
                settings.achievement_mode == mode,
                true,
                s,
            ) && settings.achievement_mode != mode
            {
                settings.achievement_mode = mode;
                return Action::Save;
            }
        }
        let mode = settings.achievement_mode;
        let unlocked = if mode == AchievementMode::Global {
            settings.unlocked_achievements.as_slice()
        } else {
            match_claims
        };
        right_label(
            &format!(
                "{} / {} unlocked",
                if mode == AchievementMode::Off {
                    0
                } else {
                    unlocked.len()
                },
                ACHIEVEMENTS.iter().filter(|a| a.available(mode)).count()
            ),
            screen_width() - x,
            y,
            18.0 * s,
            muted_color(),
        );
        label(
            match mode {
                AchievementMode::PerMatch => {
                    "Reset each match. Sandbox and modded games do not count."
                }
                AchievementMode::Global => {
                    "Saved on this device across matches. Sandbox and modded games do not count."
                }
                AchievementMode::Off => "Achievements are off. Your saved global records are kept.",
            },
            x,
            y + 77.0 * s,
            15.0 * s,
            muted_color(),
        );
        let gap = 12.0 * s;
        let width = (screen_width() - x * 2.0 - gap * 2.0) / 3.0;
        let height = ((screen_height() - y - 183.0 * s) / 5.0 - gap).min(85.0 * s);
        for (i, a) in ACHIEVEMENTS
            .iter()
            .filter(|a| a.available(mode))
            .enumerate()
        {
            let earned = unlocked.iter().any(|id| id == a.id);
            let r = Rect::new(
                x + (i % 3) as f32 * (width + gap),
                y + 96.0 * s + (i / 3) as f32 * (height + gap),
                width,
                height,
            );
            let color = if earned { card_color(i) } else { line_color() };
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                mix(surface_color(), color, if earned { 0.22 } else { 0.12 }),
            );
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                2.0 * s,
                if earned {
                    mix(color, text_color(), 0.25)
                } else {
                    line_color()
                },
            );
            achievement_badge(
                vec2(r.x + 29.0 * s, r.y + r.h * 0.5),
                19.0 * s,
                color,
                earned,
            );
            label(
                a.title,
                r.x + 58.0 * s,
                r.y + 27.0 * s,
                18.0 * s,
                if earned { text_color() } else { muted_color() },
            );
            wrap(
                a.description,
                r.x + 58.0 * s,
                r.y + 49.0 * s,
                r.w - 71.0 * s,
                12.0 * s,
                muted_color(),
            );
        }
        Action::None
    }
}
