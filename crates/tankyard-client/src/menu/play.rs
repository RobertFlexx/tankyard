//! Modes, parties, practice, controls and connection forms.
use super::*;

impl Menu {
    pub(super) fn modes(&mut self, _settings: &mut Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 152.0 * s;
        label(
            if self.party_active {
                "Party modes · stay together"
            } else {
                "Game modes"
            },
            x,
            y + 12.0 * s,
            25.0 * s,
            text_color(),
        );
        let gap = 6.0 * s;
        let width = (screen_width() - 80.0 * s - gap * 3.0) / 4.0;
        let height = 64.0 * s;
        for (index, mode) in Mode::ALL.into_iter().enumerate() {
            let bx = x + (index % 4) as f32 * (width + gap);
            let by = y + 35.0 * s + (index / 4) as f32 * (height + gap);
            let r = Rect::new(bx, by, width, height);
            let selected = self.mode == mode;
            if button(r, mode.name(), selected, true, s * 0.85) {
                self.select_mode(mode);
            }
            label(
                &fitted_label(mode_blurb(mode), 12. * s, width - 24. * s),
                bx + 16.0 * s,
                by + 56.0 * s,
                12.0 * s,
                muted_color(),
            );
        }
        let by = y + 35.0 * s + Mode::ALL.len().div_ceil(4) as f32 * (height + gap) + 6.0 * s;
        label("ARENA", x, by + 18.0 * s, 13.0 * s, muted_color());
        self.map_page = self.map_page.min(self.maps.len().saturating_sub(1) / 8);
        if self.maps.len() > 8 {
            if small_button(
                Rect::new(screen_width() - 158.0 * s, by - 6.0 * s, 50.0 * s, 24.0 * s),
                "‹",
                s,
            ) {
                self.map_page = self.map_page.saturating_sub(1);
            }
            if small_button(
                Rect::new(screen_width() - 98.0 * s, by - 6.0 * s, 50.0 * s, 24.0 * s),
                "›",
                s,
            ) {
                self.map_page = (self.map_page + 1).min(self.maps.len().saturating_sub(1) / 8);
            }
        }
        let mw = (screen_width() - 80.0 * s - 7.0 * 7.0 * s) / 8.0;
        for (slot, map) in self.maps.iter().skip(self.map_page * 8).take(8).enumerate() {
            let index = self.map_page * 8 + slot;
            let title = if map.name.chars().count() > 18 {
                format!("{}…", map.name.chars().take(16).collect::<String>())
            } else {
                map.name.clone()
            };
            if button(
                Rect::new(
                    x + slot as f32 * (mw + 7.0 * s),
                    by + 31.0 * s,
                    mw,
                    34.0 * s,
                ),
                &title,
                self.map == index,
                true,
                s * 0.70,
            ) {
                self.map = index;
            }
        }
        let total = screen_width() - 80. * s;
        let column = (total - 36. * s) / 3.;
        let mut bots = self.bots as f32;
        slider_value(
            Rect::new(x, by + 80. * s, column, 48. * s),
            if self.party_active || self.party_setup {
                "Team slots (friends + AI)"
            } else if self.mode == Mode::CoopPve {
                "AI teammates"
            } else {
                "AI tanks"
            },
            &mut bots,
            if self.party_active || self.party_setup {
                [2., 8.]
            } else {
                [0., 64.]
            },
            1.,
            s * 0.8,
            |n| format!("{n:.0}"),
        );
        self.bots = bots as usize;
        let middle = x + column + 18. * s;
        label(
            "STARTING LEVEL",
            middle,
            by + 96. * s,
            13. * s,
            muted_color(),
        );
        for (i, level) in [1, 30, 45].into_iter().enumerate() {
            if button(
                Rect::new(
                    middle + i as f32 * column / 3.,
                    by + 108. * s,
                    column / 3. - 5. * s,
                    28. * s,
                ),
                &format!("Level {level}"),
                self.offline_level == level,
                true,
                s * 0.7,
            ) {
                self.offline_level = level;
            }
        }
        let right = middle + column + 18. * s;
        label("ARENA SIZE", right, by + 96. * s, 13. * s, muted_color());
        for (i, (name, width, height)) in [
            ("Compact", 2400., 1800.),
            ("Standard", 3600., 2600.),
            ("Large", 6000., 4000.),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(
                    right + i as f32 * column / 3.,
                    by + 108. * s,
                    column / 3. - 5. * s,
                    28. * s,
                ),
                name,
                self.width == width,
                true,
                s * 0.65,
            ) {
                self.width = width;
                self.height = height;
            }
        }
        if button(
            Rect::new(x, by + 150. * s, column, 28. * s),
            if self.dynamic_arena {
                "Adaptive borders: On"
            } else {
                "Adaptive borders: Off"
            },
            self.dynamic_arena,
            true,
            s * 0.7,
        ) {
            self.dynamic_arena = !self.dynamic_arena;
        }
        label(
            if self.offline_level > 1 {
                "Preset start · arena achievements are off."
            } else {
                "Classic growth · choose your upgrades as you play."
            },
            middle,
            by + 170. * s,
            12. * s,
            muted_color(),
        );
        Action::None
    }
    pub(super) fn modes_footer(&mut self, settings: &mut Settings, s: f32) -> Action {
        if button(
            Rect::new(180.0 * s, screen_height() - 62.0 * s, 230.0 * s, 34.0 * s),
            &format!("Difficulty: {} >", self.difficulty_name()),
            false,
            true,
            s * 0.8,
        ) {
            self.difficulty = (self.difficulty + 1) % 4;
            settings.bot_difficulty = self.difficulty;
            if let Err(error) = settings.save() {
                self.error = format!("Could not save difficulty: {error}");
            }
        }
        if button(
            Rect::new(
                screen_width() - 300.0 * s,
                screen_height() - 62.0 * s,
                260.0 * s,
                36.0 * s,
            ),
            if self.party_configuring {
                "Starting round…"
            } else if self.party_active {
                if self.party_leader {
                    "Start party round  →"
                } else {
                    "Leader chooses the mode"
                }
            } else if self.party_setup {
                "Create party  →"
            } else {
                "Launch offline  →"
            },
            true,
            (!self.party_active || self.party_leader) && !self.party_configuring,
            s * 0.85,
        ) {
            return if self.party_active {
                Action::ApplyParty
            } else if self.party_setup {
                Action::CreateParty
            } else {
                Action::Offline
            };
        }
        Action::None
    }
    pub(super) fn party(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40. * s;
        let gap = 26. * s;
        let width = (screen_width() - 80. * s - gap) * 0.5;
        let right = x + width + gap;
        label("Play together", x, 169. * s, 27. * s, text_color());
        wrap(
            "Private matches run on the leader's computer. Up to eight friends can join; friendly bots fill empty team slots.",
            x,
            200. * s,
            screen_width() - 80. * s,
            16. * s,
            muted_color(),
        );
        panel(Rect::new(x, 245. * s, width, 360. * s));
        panel(Rect::new(right, 245. * s, width, 360. * s));
        let pad = 18. * s;
        let inner = width - pad * 2.;
        if self.party_invite.is_some() {
            label("YOUR PARTY", x + pad, 276. * s, 17. * s, accent());
            label(
                "Public IP:PORT · editable for manual forwarding",
                x + pad,
                308. * s,
                13. * s,
                muted_color(),
            );
            text_field(
                Rect::new(x + pad, 320. * s, inner, 36. * s),
                &mut self.party_address,
                &mut self.address_active,
                64,
                s * 0.8,
                false,
            );
            if button(
                Rect::new(x + pad, 368. * s, inner, 38. * s),
                "Copy invite link",
                true,
                !self.party_address.is_empty(),
                s * 0.85,
            ) {
                return Action::CopyParty;
            }
            label(
                &format!("PLAYERS · {} / 8", self.party_roster.len()),
                x + pad,
                433. * s,
                11. * s,
                muted_color(),
            );
            let columns = if self.party_roster.len() > 4 { 2 } else { 1 };
            for (index, name) in self.party_roster.iter().take(8).enumerate() {
                let cell = inner / columns as f32;
                label(
                    &fitted_label(name, 12. * s, cell - 8. * s),
                    x + pad + (index % columns) as f32 * cell,
                    (454. + (index / columns) as f32 * 18.) * s,
                    12. * s,
                    text_color(),
                );
            }
            let status = if self.party_network.is_empty() {
                "Use Party modes in the main menu to start another mode with these members."
            } else {
                &self.party_network
            };
            for (index, line) in wrapped_lines(status, inner, 13. * s)
                .into_iter()
                .take(4)
                .enumerate()
            {
                label(
                    &line,
                    x + pad,
                    (534. + index as f32 * 16.) * s,
                    13. * s,
                    muted_color(),
                );
            }
        } else {
            label("HOST A PARTY", x + pad, 276. * s, 17. * s, accent());
            label(
                &fitted_label(self.mode.name(), 18. * s, inner),
                x + pad,
                313. * s,
                18. * s,
                text_color(),
            );
            label(
                &fitted_label(
                    self.maps.get(self.map).map_or("Arena", |m| &m.name),
                    14. * s,
                    inner,
                ),
                x + pad,
                340. * s,
                14. * s,
                muted_color(),
            );
            label(
                &format!(
                    "{} team slots · {}",
                    self.bots.clamp(2, 8),
                    self.difficulty_name()
                ),
                x + pad,
                368. * s,
                14. * s,
                muted_color(),
            );
            if button(
                Rect::new(x + pad, 405. * s, inner, 38. * s),
                "Configure match",
                false,
                true,
                s * 0.85,
            ) {
                self.party_setup = true;
                return Action::Page(Page::Modes);
            }
            if button(
                Rect::new(x + pad, 455. * s, inner, 42. * s),
                "Create party",
                true,
                true,
                s * 0.9,
            ) {
                return Action::CreateParty;
            }
            wrap(
                "Invite access lasts until the leader leaves. Invites grant entry only; the leader keeps moderation controls.",
                x + pad,
                531. * s,
                inner,
                13. * s,
                muted_color(),
            );
        }
        if self.party_active {
            label("STAY CONNECTED", right + pad, 276. * s, 17. * s, accent());
            wrap(
                "Your party stays connected when you return to the main menu. Use the tank icons at the side to open members, local voice mutes and party text chat.",
                right + pad,
                313. * s,
                inner,
                15. * s,
                muted_color(),
            );
            if button(
                Rect::new(right + pad, 415. * s, inner, 40. * s),
                "Enter match",
                true,
                true,
                s * 0.9,
            ) {
                return Action::ResumeParty;
            }
            if button(
                Rect::new(right + pad, 467. * s, inner, 36. * s),
                "Members and party chat",
                false,
                true,
                s * 0.85,
            ) {
                return Action::OpenParty;
            }
            wrap(
                "Change modes from the main menu without leaving your party. The leader starts the next round for everyone.",
                right + pad,
                540. * s,
                inner,
                13. * s,
                muted_color(),
            );
            return Action::None;
        }
        label("JOIN A PARTY", right + pad, 276. * s, 17. * s, text_color());
        wrap(
            "Paste the full invite link from your friend.",
            right + pad,
            313. * s,
            inner,
            15. * s,
            muted_color(),
        );
        text_field(
            Rect::new(right + pad, 360. * s, inner, 40. * s),
            &mut self.party_input,
            &mut self.party_focus,
            256,
            s * 0.8,
            false,
        );
        if button(
            Rect::new(right + pad, 415. * s, inner, 40. * s),
            "Join party",
            true,
            !self.party_input.trim().is_empty(),
            s * 0.9,
        ) {
            return Action::JoinParty;
        }
        wrap(
            "Automatic setup tries an internet relay and a temporary UDP forward. The invite screen shows which route is ready. If neither is available, allow the game through your firewall and forward the displayed UDP port. LAN play works without either.",
            right + pad,
            495. * s,
            inner,
            14. * s,
            muted_color(),
        );
        if small_button(
            Rect::new(x, 625. * s, 130. * s, 30. * s),
            "Manage mods",
            s * 0.75,
        ) {
            return Action::Page(Page::Mods);
        }
        if small_button(
            Rect::new(x + 150. * s, 625. * s, 280. * s, 30. * s),
            if settings.auto_party_network {
                "Internet setup: Automatic"
            } else {
                "Internet setup: Manual / LAN"
            },
            s * 0.75,
        ) {
            settings.auto_party_network = !settings.auto_party_network;
            if let Err(error) = settings.save() {
                self.error = format!("Could not save party preference: {error}");
            }
        }
        Action::None
    }
    pub(super) fn practice(&mut self, settings: &Settings, s: f32) -> Action {
        use tankyard_client::practice::{CHALLENGES, Goal, clock};
        for (index, key) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4]
            .into_iter()
            .enumerate()
        {
            if is_key_pressed(key) {
                let index = self.practice_page * 4 + index;
                if index < CHALLENGES.len() {
                    return Action::Practice(index);
                }
            }
        }
        let margin = 40.0 * s;
        label("Practice", margin, 164.0 * s, 26.0 * s, text_color());
        label(
            "Combat trials and boss encounters. Personal bests stay on this device.",
            margin,
            190.0 * s,
            15.0 * s,
            muted_color(),
        );
        for page in 0..CHALLENGES.len().div_ceil(4) {
            if button(
                Rect::new(
                    screen_width() - (475. - page as f32 * 145.) * s,
                    153. * s,
                    135. * s,
                    32. * s,
                ),
                if page == 0 {
                    "Foundations"
                } else if page == 1 {
                    "Combat trials"
                } else {
                    "Boss encounters"
                },
                self.practice_page == page,
                true,
                s * 0.75,
            ) {
                self.practice_page = page;
            }
        }
        let gap = 18.0 * s;
        let width = (screen_width() - margin * 2.0 - gap) * 0.5;
        let height = 213.0 * s;
        for (slot, challenge) in CHALLENGES
            .iter()
            .skip(self.practice_page * 4)
            .take(4)
            .enumerate()
        {
            let index = self.practice_page * 4 + slot;
            let r = Rect::new(
                margin + (slot % 2) as f32 * (width + gap),
                216.0 * s + (slot / 2) as f32 * (height + gap),
                width,
                height,
            );
            panel(r);
            draw_rectangle(r.x, r.y, 4.0 * s, r.h, card_color(index));
            label(
                challenge.title,
                r.x + 20.0 * s,
                r.y + 32.0 * s,
                23.0 * s,
                text_color(),
            );
            wrap(
                challenge.description,
                r.x + 20.0 * s,
                r.y + 62.0 * s,
                width - 130.0 * s,
                15.0 * s,
                text_color(),
            );
            if let Some(class) = self
                .practice_content
                .class(challenge.class)
                .or_else(|| self.practice_content.class("vector"))
            {
                render::class_model(
                    class,
                    crate::ui_layout::position(vec2(r.x + r.w - 66.0 * s, r.y + 68.0 * s)),
                    26.0 * s,
                    -0.4,
                    0.0,
                    0.0,
                    player_color(),
                    self.motion_time,
                );
            }
            wrap(
                challenge.tip,
                r.x + 20.0 * s,
                r.y + 119.0 * s,
                width - 40.0 * s,
                13.0 * s,
                muted_color(),
            );
            let record = settings
                .practice_records
                .iter()
                .find(|record| record.id == challenge.id);
            let best = record.map_or_else(
                || "No attempts yet".to_owned(),
                |record| {
                    if record.completed > 0 {
                        format!(
                            "Best {}  ·  {} completed",
                            clock(record.best_ticks),
                            record.completed
                        )
                    } else {
                        let progress = match challenge.goal {
                            Goal::Score(target) => {
                                format!("score {}/{}", record.best_progress, target)
                            }
                            Goal::Kills(target) => {
                                format!("{}/{} tanks", record.best_progress, target)
                            }
                            Goal::Survive => clock(record.best_progress),
                            Goal::Boss => "boss defeated".into(),
                        };
                        format!("Best {}  ·  {} attempts", progress, record.attempts)
                    }
                },
            );
            label(
                &best,
                r.x + 20.0 * s,
                r.y + 188.0 * s,
                13.0 * s,
                muted_color(),
            );
            if button(
                Rect::new(
                    r.x + r.w - 142.0 * s,
                    r.y + r.h - 48.0 * s,
                    122.0 * s,
                    32.0 * s,
                ),
                &format!("Start [{}]", slot + 1),
                true,
                true,
                s * 0.85,
            ) {
                return Action::Practice(index);
            }
        }
        label(
            "Practice records are separate from arena achievements.",
            margin,
            700.0 * s,
            14.0 * s,
            muted_color(),
        );
        Action::None
    }
    pub(super) fn controls(&mut self, settings: &Settings, s: f32) -> Action {
        let margin = 40.0 * s;
        let gap = 32.0 * s;
        let width = (screen_width() - 2.0 * margin - gap) * 0.5;
        label(
            "Controls & shortcuts",
            margin,
            164.0 * s,
            26.0 * s,
            text_color(),
        );
        label(
            "Offline games pause in menus. Online games keep running.",
            margin,
            192.0 * s,
            15.0 * s,
            muted_color(),
        );
        let move_keys = format!(
            "{} / {} / {} / {}",
            settings.bindings[0], settings.bindings[2], settings.bindings[1], settings.bindings[3]
        );
        let movement = [
            ("Move", move_keys.as_str()),
            ("Aim / fire", "Mouse / left click"),
            ("Ability", settings.bindings[4].as_str()),
            ("Auto-fire", settings.bindings[5].as_str()),
            ("Recall drones", "Right click"),
            ("Buy a skill / fill it", "1-8 / Shift + 1-8"),
            ("Tank upgrades", "Tab"),
            ("Pause / back", "Esc"),
            ("Respawn", "Enter"),
            ("Sandbox level", "PgUp / PgDn (Shift: 15)"),
        ];
        let shortcuts = [
            ("Controls", "F1"),
            ("FPS / network details", "F3"),
            ("Restart from pause", "F5 (offline)"),
            ("Practice challenges", "F6"),
            ("Settings", "F10"),
            ("Light / dark theme", "F9"),
            ("Fullscreen", "F11"),
            ("Next / previous field", "Tab / Shift + Tab"),
            ("Retry a practice result", "R or F5"),
            ("Microphone mute / unmute", "M or mic button"),
            ("Spectate previous / next", "Left / Right (when dead)"),
        ];
        for (column, rows) in [movement.as_slice(), shortcuts.as_slice()]
            .iter()
            .enumerate()
        {
            let x = margin + column as f32 * (width + gap);
            panel(Rect::new(x, 222.0 * s, width, 440.0 * s));
            for (index, (action, key)) in rows.iter().enumerate() {
                let y = (247.0 + index as f32 * 38.0) * s;
                label(action, x + 18.0 * s, y, 15.0 * s, text_color());
                right_label(
                    key,
                    x + width - 18.0 * s,
                    y + 17.0 * s,
                    13.0 * s,
                    if is_dark() {
                        accent()
                    } else {
                        mix(accent(), text_color(), 0.45)
                    },
                );
            }
        }
        label(
            "Text: arrows move the caret, Shift selects, Ctrl+A/C/X/V edits, and held Backspace repeats.",
            margin,
            677.0 * s,
            14.0 * s,
            muted_color(),
        );
        Action::None
    }
    pub(super) fn controls_footer(&mut self, s: f32) -> Action {
        if button(
            Rect::new(
                screen_width() - 270.0 * s,
                screen_height() - 62.0 * s,
                230.0 * s,
                34.0 * s,
            ),
            "Edit bindings",
            true,
            true,
            s * 0.85,
        ) {
            self.settings_tab = 2;
            return Action::Page(Page::Settings);
        }
        Action::None
    }
    pub(super) fn connection(&mut self, settings: &mut Settings, host: bool, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 166.0 * s;
        let width = (screen_width() * 0.5 - 65.0 * s).max(320.0 * s);
        label(
            if host { "Host game" } else { "Join game" },
            x,
            y,
            25.0 * s,
            text_color(),
        );
        if host {
            let tab_w = 136.0 * s;
            for (i, (title, community)) in [("Host & play", false), ("Community server", true)]
                .into_iter()
                .enumerate()
            {
                if button(
                    Rect::new(
                        x + width - (2 - i) as f32 * (tab_w + 6.0 * s) + 6.0 * s,
                        y - 22.0 * s,
                        tab_w,
                        30.0 * s,
                    ),
                    title,
                    self.host_community == community,
                    true,
                    s * 0.75,
                ) {
                    self.host_community = community;
                    self.clear_focus();
                }
            }
        }
        let community = host && self.host_community;
        label("PLAYER NAME", x, y + 42.0 * s, 13.0 * s, muted_color());
        text_field(
            Rect::new(x, y + 53.0 * s, width, 44.0 * s),
            &mut settings.player_name,
            &mut self.name_active,
            24,
            s,
            false,
        );
        label(
            if community {
                "SERVER PASSWORD  (optional)"
            } else if host {
                "HOST PASSWORD  (optional)"
            } else {
                "SERVER ADDRESS  /  IP:PORT"
            },
            x,
            y + 129.0 * s,
            13.0 * s,
            muted_color(),
        );
        if host {
            text_field(
                Rect::new(x, y + 140.0 * s, width, 44.0 * s),
                if community {
                    &mut settings.community_server.password
                } else {
                    &mut self.password
                },
                &mut self.password_active,
                64,
                s,
                true,
            );
            label(self.mode.name(), x, y + 219.0 * s, 24.0 * s, accent());
            label(
                &if community {
                    format!(
                        "{}  ·  rotation and rules from Configure arena",
                        self.difficulty_name()
                    )
                } else {
                    format!(
                        "{} bot limit  ·  {}  ·  UDP port 5000",
                        self.bots,
                        self.difficulty_name()
                    )
                },
                x,
                y + 248.0 * s,
                18.0 * s,
                muted_color(),
            );
            if small_button(
                Rect::new(x, y + 269.0 * s, 200.0 * s, 35.0 * s),
                "Configure arena",
                s,
            ) {
                return Action::Page(Page::Modes);
            }
            wrap(
                if community {
                    "A community server runs on its own, stays up while you play anywhere, and lists its lobbies, players and mods to everyone who saves it."
                } else {
                    "For LAN games, share your local IP. For internet games, share your public IP and forward UDP port 5000."
                },
                x,
                y + 341.0 * s,
                width,
                18.0 * s,
                muted_color(),
            );
        } else {
            text_field(
                Rect::new(x, y + 140.0 * s, width, 44.0 * s),
                &mut settings.server_address,
                &mut self.address_active,
                128,
                s,
                false,
            );
            label(
                "PASSWORD  (if required)",
                x,
                y + 216.0 * s,
                13.0 * s,
                muted_color(),
            );
            text_field(
                Rect::new(x, y + 227.0 * s, width, 44.0 * s),
                &mut self.password,
                &mut self.password_active,
                64,
                s,
                true,
            );
            if small_button(
                Rect::new(x, y + 294.0 * s, 200.0 * s, 35.0 * s),
                "☆  Save favorite",
                s,
            ) {
                let address = settings.server_address.clone();
                if let Err(error) = settings
                    .add_favorite(&address)
                    .and_then(|()| settings.save())
                {
                    self.error = error;
                }
            }
            wrap(
                "Enter the server's IP address and port, or a web address such as tankyard.example.com, then connect.",
                x,
                y + 370.0 * s,
                width,
                18.0 * s,
                muted_color(),
            );
        }
        let rx = x + width + 48.0 * s;
        let rw = screen_width() - rx - 40.0 * s;
        if community {
            if let Some(action) =
                self.community_panel(settings, Rect::new(rx, y - 16.0 * s, rw, 570.0 * s), s)
            {
                return action;
            }
        } else if host {
            panel(Rect::new(rx, y - 16.0 * s, rw, 438.0 * s));
            label(
                "SERVER DETAILS",
                rx + 22.0 * s,
                y + 20.0 * s,
                15.0 * s,
                accent(),
            );
            wrap(
                "Your friends can join using your IP address and port 5000.",
                rx + 22.0 * s,
                y + 65.0 * s,
                rw - 44.0 * s,
                20.0 * s,
                text_color(),
            );
            label(
                "HOST LISTENS ON",
                rx + 22.0 * s,
                y + 213.0 * s,
                13.0 * s,
                muted_color(),
            );
            label(
                "0.0.0.0:5000",
                rx + 22.0 * s,
                y + 246.0 * s,
                25.0 * s,
                text_color(),
            );
            wrap(
                "Returning to the menu closes the server.",
                rx + 22.0 * s,
                y + 304.0 * s,
                rw - 44.0 * s,
                18.0 * s,
                muted_color(),
            );
        } else if let Err(error) = self.server_browser.draw(
            settings,
            &mut self.servers,
            Rect::new(rx, y - 16.0 * s, rw, 438.0 * s),
            s,
        ) {
            self.error = error;
        }
        let running = self.dedicated.running();
        let primary = if community && running {
            Action::StopCommunity
        } else if community {
            Action::StartCommunity
        } else if host {
            Action::Host
        } else {
            Action::Join
        };
        if button(
            Rect::new(x, screen_height() - 140.0 * s, width, 46.0 * s),
            match primary {
                Action::StopCommunity => "Stop community server",
                Action::StartCommunity => "Start community server [Enter]",
                Action::Host => "Start server & play [Enter]",
                _ => "Connect  →",
            },
            !matches!(primary, Action::StopCommunity),
            true,
            s,
        ) {
            return primary;
        }
        if is_key_pressed(KeyCode::Enter) && !matches!(primary, Action::StopCommunity) {
            return primary;
        }
        Action::None
    }
}
