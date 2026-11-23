//! The bosses a Boss Rush fights, and the order they arrive in.
use super::*;
use tankyard_core::{BossKind, CLASSIC_RUSH};

/// For a shorter evening.
const QUICK_RUSH: [BossKind; 3] = [BossKind::Sentinel, BossKind::Prism, BossKind::Foundry];

/// `kind` added to the end of the lineup, or taken out if it's already in.
fn toggled(lineup: &[BossKind], kind: BossKind) -> Vec<BossKind> {
    if lineup.contains(&kind) {
        lineup
            .iter()
            .copied()
            .filter(|&boss| boss != kind)
            .collect()
    } else {
        let mut next = lineup.to_vec();
        next.push(kind);
        next
    }
}

/// The boss at `index` swapped with its neighbor, one later or one earlier.
fn moved(lineup: &[BossKind], index: usize, later: bool) -> Vec<BossKind> {
    let mut next = lineup.to_vec();
    let other = if later {
        index + 1
    } else {
        index.wrapping_sub(1)
    };
    if index < next.len() && other < next.len() {
        next.swap(index, other);
    }
    next
}

/// The lineup cut or grown to `count`, growing with bosses not yet in it.
fn resized(lineup: &[BossKind], count: usize) -> Vec<BossKind> {
    let count = count.clamp(1, BossKind::ALL.len());
    let mut next: Vec<_> = lineup.iter().copied().take(count).collect();
    for kind in BossKind::ALL {
        if next.len() >= count {
            break;
        }
        if !next.contains(&kind) {
            next.push(kind);
        }
    }
    next
}

/// The same bosses in a new order, never the order they came in.
fn shuffled(lineup: &[BossKind], seed: u64) -> Vec<BossKind> {
    let mut next = lineup.to_vec();
    let mut state = seed | 1;
    for index in (1..next.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        next.swap(index, (state % (index as u64 + 1)) as usize);
    }
    if next == lineup && next.len() > 1 {
        next.rotate_left(1);
    }
    next
}

/// The classic six in their usual order are stored as no choice at all, so a
/// player who never touched this keeps following the game's default.
fn stored(lineup: Vec<BossKind>) -> Vec<BossKind> {
    if lineup == CLASSIC_RUSH {
        Vec::new()
    } else {
        lineup
    }
}

/// A lineup read from saved settings.
pub(super) fn lineup_from_ids(ids: &[String]) -> Vec<BossKind> {
    stored(
        ids.iter()
            .filter_map(|id| BossKind::from_id(id))
            .take(BossKind::ALL.len())
            .collect(),
    )
}

impl Menu {
    /// The lineup as this page shows it. Cleared, it shows nothing yet.
    fn shown_rush(&self) -> &[BossKind] {
        if self.rush_blank {
            &[]
        } else if self.boss_rush.is_empty() {
            &CLASSIC_RUSH
        } else {
            &self.boss_rush
        }
    }

    fn set_rush(&mut self, settings: &mut Settings, lineup: Vec<BossKind>) {
        self.rush_blank = false;
        self.boss_rush = stored(lineup);
        settings.boss_rush = self
            .boss_rush
            .iter()
            .map(|kind| kind.id().to_owned())
            .collect();
        if let Err(error) = settings.save() {
            self.error = format!("Could not save the lineup: {error}");
        }
    }

    /// What the Boss Rush will fight, in a few words.
    pub(super) fn rush_line(&self) -> String {
        match self.boss_rush.as_slice() {
            [] => "Classic six".to_owned(),
            [only] => format!("Just the {}", only.name()),
            lineup if lineup.len() == BossKind::ALL.len() => {
                format!("All {} bosses", lineup.len())
            }
            lineup => format!("{} bosses, {} first", lineup.len(), lineup[0].name()),
        }
    }

    pub(super) fn boss_rush_page(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40. * s;
        let y = 166. * s;
        let w = screen_width() - 80. * s;
        let gap = 8. * s;
        label("Boss Rush lineup", x, y, 25. * s, text_color());
        label(
            "Pick bosses in the order you'd like to regret them. Pick one again to drop it.",
            x,
            y + 26. * s,
            14. * s,
            muted_color(),
        );
        self.rush_presets(settings, x + w, y, s);

        let shown = self.shown_rush().to_vec();
        let top = y + 42. * s;
        label("LINEUP", x, top + 18. * s, 13. * s, muted_color());
        let step_x = x + 70. * s;
        if button(
            Rect::new(step_x, top, 30. * s, 26. * s),
            "−",
            false,
            shown.len() > 1,
            s * 0.8,
        ) {
            self.set_rush(settings, resized(&shown, shown.len() - 1));
        }
        center_label(
            &shown.len().to_string(),
            step_x + 52. * s,
            top + 19. * s,
            16. * s,
            text_color(),
        );
        if button(
            Rect::new(step_x + 74. * s, top, 30. * s, 26. * s),
            "+",
            false,
            shown.len() < BossKind::ALL.len(),
            s * 0.8,
        ) {
            self.set_rush(settings, resized(&shown, shown.len() + 1));
        }
        label(
            &fitted_label(&self.rush_note(&shown), 13. * s, w - 200. * s),
            step_x + 118. * s,
            top + 18. * s,
            13. * s,
            muted_color(),
        );

        let order_top = top + 34. * s;
        let chip_h = 34. * s;
        let columns = (((w + gap) / (176. * s + gap)).floor() as usize).clamp(2, 7);
        let chip_w = (w - gap * (columns - 1) as f32) / columns as f32;
        let mut change = None;
        for (index, kind) in shown.iter().enumerate() {
            let chip = Rect::new(
                x + (index % columns) as f32 * (chip_w + gap),
                order_top + (index / columns) as f32 * (chip_h + gap),
                chip_w,
                chip_h,
            );
            panel(chip);
            let number = (index + 1).to_string();
            label(
                &number,
                chip.x + 9. * s,
                chip.y + 22. * s,
                15. * s,
                accent(),
            );
            label(
                &fitted_label(kind.name(), 13.5 * s, chip_w - 92. * s),
                chip.x + 31. * s,
                chip.y + 22. * s,
                13.5 * s,
                text_color(),
            );
            let arrow = |slot: f32| {
                Rect::new(
                    chip.x + chip.w - slot * 27. * s - 3. * s,
                    chip.y + 4. * s,
                    24. * s,
                    chip_h - 8. * s,
                )
            };
            if button(arrow(2.), "‹", false, index > 0, s * 0.7) {
                change = Some(moved(&shown, index, false));
            }
            if button(arrow(1.), "›", false, index + 1 < shown.len(), s * 0.7) {
                change = Some(moved(&shown, index, true));
            }
        }
        let rows = shown.len().div_ceil(columns).max(1);
        let roster_top = order_top + rows as f32 * (chip_h + gap) + 12. * s;
        label("BOSSES", x, roster_top + 8. * s, 13. * s, muted_color());
        let card_h = 66. * s;
        let card_top = roster_top + 18. * s;
        let time = crate::accessibility::motion_time(get_time() as f32);
        for (index, kind) in BossKind::ALL.into_iter().enumerate() {
            let card = Rect::new(
                x + (index % columns) as f32 * (chip_w + gap),
                card_top + (index / columns) as f32 * (card_h + gap),
                chip_w,
                card_h,
            );
            let place = shown.iter().position(|&boss| boss == kind);
            if button(card, "", place.is_some(), true, s) {
                if place.is_some() && shown.len() == 1 {
                    self.info = "A rush needs at least one boss to rush at.".into();
                } else {
                    change = Some(toggled(&shown, kind));
                }
            }
            tankyard_client::boss_model::boss_model_geometry(
                kind,
                crate::ui_layout::position(vec2(card.x + 31. * s, card.center().y)),
                18. * s,
                tankyard_client::boss_model::Pose {
                    aim: -0.5,
                    time,
                    ..Default::default()
                },
                None,
                &mut render::draw_model_primitive,
            );
            // Long names take two lines instead of losing their ends.
            let room = chip_w - 74. * s - if place.is_some() { 24. * s } else { 0. };
            let (x_name, y_name) = (card.x + 68. * s, card.center().y);
            match kind.name().split_once(' ') {
                Some((first, second)) if label_width(kind.name(), 14. * s) > room => {
                    for (line, offset) in [(first, -3.), (second, 13.)] {
                        label(
                            &fitted_label(line, 12.5 * s, room),
                            x_name,
                            y_name + offset * s,
                            12.5 * s,
                            text_color(),
                        );
                    }
                }
                _ => label(
                    &fitted_label(kind.name(), 14. * s, room),
                    x_name,
                    y_name + 5. * s,
                    14. * s,
                    text_color(),
                ),
            }
            if let Some(place) = place {
                let badge = vec2(card.x + card.w - 18. * s, card.y + 18. * s);
                draw_circle(badge.x, badge.y, 11. * s, accent());
                center_label(
                    &(place + 1).to_string(),
                    badge.x,
                    badge.y + 4.5 * s,
                    12.5 * s,
                    WHITE,
                );
            }
        }
        if let Some(lineup) = change {
            self.set_rush(settings, lineup);
        }
        Action::None
    }

    /// The one-click lineups, right-aligned to `right`.
    fn rush_presets(&mut self, settings: &mut Settings, right: f32, y: f32, s: f32) {
        let width = 112. * s;
        let gap = 6. * s;
        let shown = self.shown_rush().to_vec();
        let at = |slot: usize| {
            Rect::new(
                right - (5 - slot) as f32 * (width + gap) + gap,
                y - 22. * s,
                width,
                30. * s,
            )
        };
        let blank = self.rush_blank;
        if button(
            at(0),
            "Classic six",
            !blank && self.boss_rush.is_empty(),
            true,
            s * 0.72,
        ) {
            self.set_rush(settings, Vec::new());
        }
        if button(
            at(1),
            "Quick three",
            self.boss_rush == QUICK_RUSH,
            true,
            s * 0.72,
        ) {
            self.set_rush(settings, QUICK_RUSH.to_vec());
        }
        if button(
            at(2),
            "Full rush",
            self.boss_rush == BossKind::ALL,
            true,
            s * 0.72,
        ) {
            self.set_rush(settings, BossKind::ALL.to_vec());
        }
        if button(at(3), "Shuffle", false, shown.len() > 1, s * 0.72) {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |time| time.as_nanos() as u64);
            self.set_rush(settings, shuffled(&shown, seed));
        }
        if button(at(4), "Clear", false, !blank, s * 0.72) {
            self.set_rush(settings, Vec::new());
            self.rush_blank = true;
        }
    }

    /// The line beside the count, saying what will actually happen.
    fn rush_note(&self, shown: &[BossKind]) -> String {
        if shown.is_empty() {
            "Nothing picked yet. Until you do, a rush plays the classic six.".to_owned()
        } else if self.boss_rush.is_empty() {
            "The classic six, gentlest first.".to_owned()
        } else if shown.len() == BossKind::ALL.len() {
            "Every boss in the yard. Bring snacks.".to_owned()
        } else {
            let last = shown.last().map_or("", |kind| kind.name());
            format!("Ends with the {last}.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::BossKind::{Eclipse, Foundry, Prism, Sentinel, Trapper};

    #[test]
    fn picking_a_boss_adds_it_last_and_picking_it_again_drops_it() {
        let lineup = toggled(&[Prism], Trapper);
        assert_eq!(lineup, [Prism, Trapper]);
        assert_eq!(toggled(&lineup, Prism), [Trapper]);
    }

    #[test]
    fn a_boss_moves_one_place_and_never_off_the_end() {
        let lineup = [Prism, Trapper, Foundry];
        assert_eq!(moved(&lineup, 0, true), [Trapper, Prism, Foundry]);
        assert_eq!(moved(&lineup, 2, false), [Prism, Foundry, Trapper]);
        assert_eq!(moved(&lineup, 0, false), lineup);
        assert_eq!(moved(&lineup, 2, true), lineup);
        assert_eq!(moved(&lineup, 9, true), lineup);
    }

    #[test]
    fn the_count_trims_from_the_end_and_grows_with_new_bosses() {
        let lineup = [Trapper, Prism, Foundry];
        assert_eq!(resized(&lineup, 2), [Trapper, Prism]);
        assert_eq!(resized(&lineup, 0), [Trapper]);
        let grown = resized(&lineup, 4);
        assert_eq!(grown[..3], lineup);
        assert_eq!(grown[3], Sentinel);
        let full = resized(&lineup, 99);
        assert_eq!(full.len(), BossKind::ALL.len());
        assert!(BossKind::ALL.iter().all(|kind| full.contains(kind)));
    }

    #[test]
    fn a_shuffle_keeps_the_bosses_and_always_changes_the_order() {
        for seed in 0..64 {
            for lineup in [&[Prism, Trapper][..], &CLASSIC_RUSH, &BossKind::ALL] {
                let next = shuffled(lineup, seed);
                assert_ne!(next, lineup, "seed {seed}");
                assert_eq!(next.len(), lineup.len());
                assert!(lineup.iter().all(|kind| next.contains(kind)));
            }
        }
        assert_eq!(shuffled(&[Eclipse], 7), [Eclipse]);
        assert!(shuffled(&[], 7).is_empty());
    }

    #[test]
    fn the_classic_order_is_kept_as_no_choice_at_all() {
        assert!(stored(CLASSIC_RUSH.to_vec()).is_empty());
        let reordered = moved(&CLASSIC_RUSH, 0, true);
        assert_eq!(stored(reordered.clone()), reordered);
    }

    #[test]
    fn saved_lineups_ignore_bosses_that_no_longer_exist() {
        let saved = ["prism", "the_landlord", "trapper"].map(str::to_owned);
        assert_eq!(lineup_from_ids(&saved), [Prism, Trapper]);
        assert!(lineup_from_ids(&[]).is_empty());
        let classic: Vec<_> = CLASSIC_RUSH.iter().map(|k| k.id().to_owned()).collect();
        assert!(lineup_from_ids(&classic).is_empty());
    }

    #[test]
    fn the_summary_names_what_will_be_fought() {
        let mut menu = Menu::new();
        assert_eq!(menu.rush_line(), "Classic six");
        menu.boss_rush = vec![Trapper];
        assert_eq!(menu.rush_line(), "Just the Trapper");
        menu.boss_rush = vec![Prism, Trapper];
        assert_eq!(menu.rush_line(), "2 bosses, Prism first");
        menu.boss_rush = BossKind::ALL.to_vec();
        assert_eq!(menu.rush_line(), "All 14 bosses");
    }
}
