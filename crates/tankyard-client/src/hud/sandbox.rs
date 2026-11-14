//! Centred Sandbox toolbox, with a searchable catalogue and immediate controls.
use super::*;

const PAGE_SIZE: usize = 12;

fn catalogue<'a>(content: &'a Content, query: &str, tier: Option<u8>) -> Vec<&'a ClassDef> {
    let query = query.trim().to_lowercase();
    let mut classes: Vec<_> = content
        .classes
        .iter()
        .filter(|class| {
            tier.is_none_or(|tier| class.tier == tier)
                && (class.name.to_lowercase().contains(&query)
                    || class.id.to_lowercase().contains(&query))
        })
        .collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    classes
}

impl Hud {
    pub fn sandbox_typing(&self) -> bool {
        self.sandbox && self.sandbox_search_active
    }
    pub fn close_sandbox(&mut self) {
        self.sandbox = false;
        self.sandbox_search_active = false;
        self.sandbox_level_pending = None;
    }

    pub(super) fn sandbox_overlay(
        &mut self,
        tank: &Tank,
        content: &Content,
        actions: &mut Vec<HudAction>,
        s: f32,
    ) {
        let (r, s) = sandbox_layout(screen_width(), screen_height(), s);
        let interactive = set_interactive(false);
        set_interactive(interactive);
        draw_rectangle(
            0.,
            0.,
            screen_width(),
            screen_height(),
            alpha(bg_color(), 0.72),
        );
        panel(r);
        let left = r.x + 24. * s;
        let right = r.x + 600. * s;
        let left_w = 548. * s;
        let right_w = 296. * s;
        label("Sandbox", left, r.y + 38. * s, 27. * s, accent());
        if small_button(
            Rect::new(r.x + r.w - 120. * s, r.y + 17. * s, 96. * s, 32. * s),
            "Close",
            s * 0.85,
        ) {
            self.close_sandbox();
            return;
        }
        label("Choose tank", left, r.y + 78. * s, 14. * s, muted_color());
        let old_query = self.sandbox_search.clone();
        text_field(
            Rect::new(left, r.y + 92. * s, left_w - 84. * s, 38. * s),
            &mut self.sandbox_search,
            &mut self.sandbox_search_active,
            48,
            s * 0.80,
            false,
        );
        if self.sandbox_search.is_empty() && !self.sandbox_search_active {
            label(
                "Search by name or ID",
                left + 10. * s,
                r.y + 117. * s,
                15. * s,
                muted_color(),
            );
        }
        if small_button(
            Rect::new(left + left_w - 76. * s, r.y + 92. * s, 76. * s, 38. * s),
            if self.sandbox_search.is_empty() {
                "Current"
            } else {
                "Clear"
            },
            s * 0.8,
        ) {
            if self.sandbox_search.is_empty() {
                self.sandbox_search = content
                    .class(&tank.class)
                    .map_or_else(|| tank.class.clone(), |c| c.name.clone());
                self.sandbox_tier = None;
            } else {
                self.sandbox_search.clear();
            }
        }
        if old_query != self.sandbox_search {
            self.sandbox_page = 0;
            self.sandbox_scroll = 0.;
        }
        let chip = (left_w - 40. * s) / 6.;
        for (i, (title, tier)) in [
            ("All", None),
            ("Base", Some(0)),
            ("Tier 1", Some(1)),
            ("Tier 2", Some(2)),
            ("Tier 3", Some(3)),
            ("Tier 4", Some(4)),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(
                    left + i as f32 * (chip + 8. * s),
                    r.y + 142. * s,
                    chip,
                    28. * s,
                ),
                title,
                self.sandbox_tier == tier,
                true,
                s * 0.72,
            ) {
                self.sandbox_tier = tier;
                self.sandbox_page = 0;
            }
        }
        let classes = catalogue(content, &self.sandbox_search, self.sandbox_tier);
        let pages = classes.len().div_ceil(PAGE_SIZE).max(1);
        self.sandbox_page = self.sandbox_page.min(pages - 1);
        let card_w = (left_w - 24. * s) / 3.;
        for (i, class) in classes
            .iter()
            .skip(self.sandbox_page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .enumerate()
        {
            let card = Rect::new(
                left + (i % 3) as f32 * (card_w + 12. * s),
                r.y + 188. * s + (i / 3) as f32 * 84. * s,
                card_w,
                74. * s,
            );
            if button(card, "", class.id == tank.class, self.sandbox_allowed, s) {
                actions.push(HudAction::SandboxClass(class.id.clone()));
            }
            render::class_preview(
                class,
                Rect::new(card.x + 8. * s, card.y + 5. * s, card.w - 16. * s, 42. * s),
                13. * s,
                0.,
                player_color(),
            );
            center_label(
                &fitted_label(&class.name, 13. * s, card.w - 12. * s),
                card.center().x,
                card.y + 62. * s,
                13. * s,
                if self.sandbox_allowed {
                    text_color()
                } else {
                    muted_color()
                },
            );
        }
        if classes.is_empty() {
            center_label(
                "No tanks match this search",
                left + left_w * 0.5,
                r.y + 336. * s,
                16. * s,
                muted_color(),
            );
        }
        let pager_y = r.y + 536. * s;
        if button(
            Rect::new(left, pager_y, 92. * s, 32. * s),
            "Previous",
            false,
            self.sandbox_page > 0,
            s * 0.7,
        ) {
            self.sandbox_page = self.sandbox_page.saturating_sub(1);
        }
        center_label(
            &format!(
                "{} {} · {} / {}",
                classes.len(),
                if classes.len() == 1 { "tank" } else { "tanks" },
                self.sandbox_page + 1,
                pages
            ),
            left + left_w * 0.5,
            pager_y + 22. * s,
            13. * s,
            muted_color(),
        );
        if button(
            Rect::new(left + left_w - 92. * s, pager_y, 92. * s, 32. * s),
            "Next",
            false,
            self.sandbox_page + 1 < pages,
            s * 0.7,
        ) {
            self.sandbox_page = (self.sandbox_page + 1).min(pages - 1);
        }
        // Both wheel types page the catalogue; no modifier or tiny scrollbar
        // is required on a laptop touchpad.
        let area = Rect::new(left, r.y + 174. * s, left_w, 398. * s);
        if contains(area) {
            let steps = crate::ui_layout::scroll_steps(
                &mut self.sandbox_scroll,
                crate::ui_layout::wheel_lines().y,
            );
            self.sandbox_page = if steps >= 0 {
                self.sandbox_page.saturating_sub(steps as usize)
            } else {
                (self.sandbox_page + steps.unsigned_abs() as usize).min(pages - 1)
            };
        }
        set_interactive(interactive && self.sandbox_allowed);
        label("PLAYER", right, r.y + 78. * s, 12. * s, muted_color());
        if self.sandbox_level_pending == Some(tank.level) {
            self.sandbox_level_pending = None;
        }
        let mut level = f32::from(self.sandbox_level_pending.unwrap_or(tank.level));
        if slider_value(
            Rect::new(right, r.y + 96. * s, right_w, 48. * s),
            "Level",
            &mut level,
            [1., f32::from(tankyard_core::MAX_LEVEL)],
            1.,
            s * 0.9,
            |v| format!("{v:.0}"),
        ) {
            self.sandbox_level_pending = Some(level.round() as u8);
        }
        let step = right_w / tankyard_core::UPGRADE_LEVELS.len() as f32;
        for (i, level) in tankyard_core::UPGRADE_LEVELS.into_iter().enumerate() {
            if small_button(
                Rect::new(
                    right + i as f32 * step,
                    r.y + 156. * s,
                    step - 6. * s,
                    30. * s,
                ),
                &level.to_string(),
                s * 0.8,
            ) {
                self.sandbox_level_pending = Some(level);
            }
        }
        if let Some(level) = self.sandbox_level_pending
            && get_time() - self.sandbox_level_at >= 0.11
            && self.sandbox_allowed
        {
            self.sandbox_level_at = get_time();
            actions.push(HudAction::SandboxLevel(level));
        }
        if toggle(
            Rect::new(right, r.y + 204. * s, right_w, 32. * s),
            "Invulnerability",
            &mut self.invulnerable,
            s * 0.9,
        ) {
            actions.push(HudAction::Invulnerable(self.invulnerable));
        }
        label("SIMULATION", right, r.y + 272. * s, 12. * s, muted_color());
        set_interactive(interactive && self.local_simulation);
        let previous = self.speed;
        if slider_value(
            Rect::new(right, r.y + 286. * s, right_w, 48. * s),
            if self.local_simulation {
                "Speed"
            } else {
                "Server controls speed"
            },
            &mut self.speed,
            [0.25, 2.],
            0.25,
            s * 0.9,
            |v| format!("{v:.2}×"),
        ) && previous != self.speed
        {
            actions.push(HudAction::Speed(self.speed));
        }
        if small_button(
            Rect::new(right, r.y + 344. * s, right_w, 32. * s),
            "Reset to 1×",
            s * 0.85,
        ) {
            self.speed = 1.;
            actions.push(HudAction::Speed(1.));
        }
        set_interactive(interactive && self.sandbox_allowed);
        label("SPAWN", right, r.y + 416. * s, 12. * s, muted_color());
        if small_button(
            Rect::new(right, r.y + 430. * s, right_w, 32. * s),
            "Add AI tank",
            s * 0.85,
        ) {
            actions.push(HudAction::Bot);
        }
        let bosses = BossKind::ALL;
        self.boss_index %= bosses.len();
        if small_button(Rect::new(right, r.y + 474. * s, 32. * s, 32. * s), "‹", s) {
            self.boss_index = (self.boss_index + bosses.len() - 1) % bosses.len();
        }
        center_label(
            bosses[self.boss_index].name(),
            right + right_w * 0.5,
            r.y + 496. * s,
            14. * s,
            text_color(),
        );
        if small_button(
            Rect::new(right + right_w - 32. * s, r.y + 474. * s, 32. * s, 32. * s),
            "›",
            s,
        ) {
            self.boss_index = (self.boss_index + 1) % bosses.len();
        }
        if small_button(
            Rect::new(right, r.y + 518. * s, right_w, 34. * s),
            "Spawn selected boss",
            s * 0.85,
        ) {
            actions.push(HudAction::Boss(bosses[self.boss_index]));
        }
        set_interactive(interactive);
        draw_line(
            left,
            r.y + 590. * s,
            r.x + r.w - 24. * s,
            r.y + 590. * s,
            s,
            line_color(),
        );
        label(
            if self.local_simulation {
                "Changes apply immediately to this offline game."
            } else if self.sandbox_allowed {
                "Level, tank and spawn changes are applied by the server."
            } else {
                "Sandbox tools are disabled by this server."
            },
            left,
            r.y + 615. * s,
            12. * s,
            muted_color(),
        );
        right_label(
            "PgUp / PgDn: level · Shift: 15 · T / Esc: close",
            r.x + r.w - 24. * s,
            r.y + 633. * s,
            11. * s,
            muted_color(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tank_search_matches_names_ids_and_tiers_in_modded_catalogues() {
        let mut content = Content::builtin();
        let mut class = content.classes[0].clone();
        class.id = "custom_isolation".into();
        class.name = "Example Isolation".into();
        class.tier = 4;
        content.classes.push(class);
        assert_eq!(catalogue(&content, "  bAcTeRiA ", None)[0].id, "bacteria");
        assert!(catalogue(&content, "bacteria", Some(0)).is_empty());
        assert_eq!(
            catalogue(&content, "custom_isolation", Some(4))[0].name,
            "Example Isolation"
        );
        assert!(catalogue(&content, "does not exist", None).is_empty());
    }
}
