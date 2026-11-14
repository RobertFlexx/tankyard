//! Community hosting and the public server browser.
use super::*;

impl Menu {
    pub(super) fn community_panel(
        &mut self,
        settings: &mut Settings,
        rect: Rect,
        s: f32,
    ) -> Option<Action> {
        use crate::server_status::{State, quality};
        panel(rect);
        let px = rect.x + 22.0 * s;
        let pw = rect.w - 44.0 * s;
        let mut y = rect.y + 36.0 * s;
        self.dedicated.poll();
        let server = &mut settings.community_server;
        if !self.dedicated.running() {
            label("COMMUNITY SERVER", px, y, 15.0 * s, accent());
            if let Some(reason) = &self.dedicated.stopped {
                right_label(
                    &fitted_label(reason, 12.0 * s, pw * 0.55),
                    px + pw,
                    y,
                    12.0 * s,
                    muted_color(),
                );
            }
            y += 30.0 * s;
            label("SERVER NAME", px, y, 12.0 * s, muted_color());
            text_field(
                Rect::new(px, y + 8.0 * s, pw, 38.0 * s),
                &mut server.name,
                &mut self.community_fields[0],
                48,
                s * 0.9,
                false,
            );
            y += 64.0 * s;
            label("UDP PORT", px, y, 12.0 * s, muted_color());
            if !self.community_fields[1] {
                self.community_port = server.port.to_string();
            }
            text_field(
                Rect::new(px, y + 8.0 * s, 120.0 * s, 38.0 * s),
                &mut self.community_port,
                &mut self.community_fields[1],
                5,
                s * 0.9,
                false,
            );
            self.community_port.retain(|c| c.is_ascii_digit());
            if let Ok(port) = self.community_port.parse::<u16>() {
                server.port = port.max(1024);
            }
            wrap(
                "Forward this UDP port on your router for internet players.",
                px + 136.0 * s,
                y + 22.0 * s,
                pw - 136.0 * s,
                14.0 * s,
                muted_color(),
            );
            y += 52.0 * s;
            let mut value = f32::from(server.max_players);
            slider_value(
                Rect::new(px, y, pw, 50.0 * s),
                "Max players",
                &mut value,
                [2., 128.],
                1.,
                s,
                |v| format!("{v:.0}"),
            );
            server.max_players = value as u16;
            y += 50.0 * s;
            let mut value = f32::from(server.lobby_size);
            slider_value(
                Rect::new(px, y, pw, 50.0 * s),
                "Lobby size",
                &mut value,
                [0., 64.],
                1.,
                s,
                |v| {
                    if v < 1. {
                        "One lobby".into()
                    } else {
                        format!("{v:.0} per lobby")
                    }
                },
            );
            server.lobby_size = value as u16;
            y += 50.0 * s;
            let mut value = f32::from(server.bots);
            slider_value(
                Rect::new(px, y, pw, 50.0 * s),
                "Bots per lobby",
                &mut value,
                [0., 48.],
                1.,
                s,
                |v| format!("{v:.0}"),
            );
            server.bots = value as u16;
            y += 58.0 * s;
            toggle(
                Rect::new(px, y, pw, 30. * s),
                "Domain listener (WebSocket)",
                &mut server.websocket,
                s * 0.8,
            );
            y += 50. * s;
            y = wrap(
                if server.websocket {
                    "Listens at 127.0.0.1:8080. Put Caddy or a tunnel in front for HTTPS; see docs/hosting-domain.md."
                } else {
                    "A domain listener lets friends join a hostname over HTTPS. Setup: docs/hosting-domain.md."
                },
                px,
                y,
                pw,
                12. * s,
                muted_color(),
            ) + 18. * s;
            server.clean();
            let mods = settings.enabled_mods.len();
            wrap(
                &format!(
                    "{} enabled mod{} will load on the server. Players download nothing; mods run on the server.",
                    mods,
                    if mods == 1 { "" } else { "s" }
                ),
                px,
                y,
                pw,
                14.0 * s,
                muted_color(),
            );
            if small_button(
                Rect::new(px, rect.bottom() - 44.0 * s, pw, 28.0 * s),
                "Copy command for an always-on machine",
                s * 0.7,
            ) {
                let server = server.clone();
                let server_mods = crate::mod_library::ready_paths(settings).paths;
                let saved = crate::hosting::settings(
                    &server,
                    self.config(),
                    server_mods,
                    self.difficulty_name(),
                )
                .and_then(|config| {
                    std::fs::create_dir_all(crate::hosting::server_dir())
                        .map_err(|e| e.to_string())?;
                    config
                        .save(crate::hosting::config_path())
                        .map_err(|e| e.to_string())
                });
                match saved {
                    Ok(()) => {
                        drop(crate::clipboard::copy(&crate::hosting::start_command()));
                        self.info = "Saved the server config and copied its start command.".into();
                    }
                    Err(error) => self.error = error,
                }
            }
            return None;
        }
        let local = format!("127.0.0.1:{}", self.dedicated.port);
        self.servers.watch_every(&local, get_time(), 2.0);
        let state = self.servers.state(&local).cloned();
        label("COMMUNITY SERVER", px, y, 15.0 * s, accent());
        let pulse = if matches!(state, Some(State::Online(_))) {
            SIGNAL_GOOD
        } else {
            SIGNAL_FAIR
        };
        draw_circle(px + pw - 60.0 * s, y - 5.0 * s, 4.0 * s, pulse);
        label(
            if matches!(state, Some(State::Online(_))) {
                "Online"
            } else {
                "Starting"
            },
            px + pw - 50.0 * s,
            y,
            13.0 * s,
            text_color(),
        );
        y += 30.0 * s;
        if let Some(State::Online(probe)) = &state {
            label(
                &fitted_label(&probe.status.name, 20.0 * s, pw),
                px,
                y,
                20.0 * s,
                text_color(),
            );
            y += 24.0 * s;
            let connection = self.servers.connection(&local);
            let summary = format!(
                "{}/{} players  ·  {} lobb{}  ·  {}",
                probe.status.players,
                probe.status.max_players,
                probe.status.lobbies.len(),
                if probe.status.lobbies.len() == 1 {
                    "y"
                } else {
                    "ies"
                },
                if probe.status.mods.is_empty() {
                    "no mods".to_owned()
                } else {
                    probe.status.mods.join(", ")
                }
            );
            label(
                &fitted_label(&summary, 13.0 * s, pw - 90.0 * s),
                px,
                y,
                13.0 * s,
                muted_color(),
            );
            signal(
                px + pw - 80.0 * s,
                y,
                connection.map(|(r, l)| quality(r, l)),
                connection.map(|c| c.0),
                s,
            );
        } else {
            label(
                "Waiting for the server to answer…",
                px,
                y,
                15.0 * s,
                muted_color(),
            );
            y += 24.0 * s;
        }
        y += 28.0 * s;
        label("PLAYERS JOIN WITH", px, y, 12.0 * s, muted_color());
        y += 24.0 * s;
        let address = crate::hosting::lan_address(self.dedicated.port)
            .unwrap_or_else(|| format!("your-ip:{}", self.dedicated.port));
        label(&address, px, y, 19.0 * s, text_color());
        if small_button(
            Rect::new(px + pw - 150.0 * s, y - 20.0 * s, 70.0 * s, 28.0 * s),
            "Copy",
            s * 0.7,
        ) {
            drop(crate::clipboard::copy(&address));
            self.info = "Server address copied.".into();
        }
        if small_button(
            Rect::new(px + pw - 74.0 * s, y - 20.0 * s, 74.0 * s, 28.0 * s),
            "Join",
            s * 0.7,
        ) {
            return Some(Action::JoinCommunity);
        }
        y += 18.0 * s;
        label(
            "Local network address. Internet players use your public IP with the same port.",
            px,
            y,
            11.0 * s,
            muted_color(),
        );
        y += 14.0 * s;
        let console = Rect::new(px, y, pw, rect.bottom() - y - 16.0 * s);
        draw_rectangle(
            console.x,
            console.y,
            console.w,
            console.h,
            alpha(bg_color(), 0.6),
        );
        let _clip = crate::ui_layout::Scissor::new(console);
        let line_h = 15.0 * s;
        let lines = self.dedicated.log();
        let visible = ((console.h - 8.0 * s) / line_h).floor().max(0.0) as usize;
        for (i, line) in lines
            .iter()
            .skip(lines.len().saturating_sub(visible))
            .enumerate()
        {
            label(
                &fitted_label(line, 11.0 * s, console.w - 16.0 * s),
                console.x + 8.0 * s,
                console.y + 15.0 * s + i as f32 * line_h,
                11.0 * s,
                text_color(),
            );
        }
        None
    }

    /// Open the main server's lobby list, as if its card were clicked.
    pub fn open_community(&mut self) {
        self.host_community = true;
    }

    pub fn open_lobbies(&mut self) {
        self.lobbies_open = true;
        self.lobbies_open_amount = 1.0;
    }

    /// Lobbies on the main server, as last probed.
    pub(super) fn lobby_count(&self) -> usize {
        match self.servers.state(crate::settings::main_server()) {
            Some(crate::server_status::State::Online(probe)) => probe.status.lobbies.len(),
            _ => 0,
        }
    }

    /// Height of the open lobby list, which grows with the lobbies up to a
    /// page and gains a page switcher past that.
    pub(super) fn lobby_list_height(&self, s: f32) -> f32 {
        lobby_list_height(self.lobby_count(), s)
    }

    /// The main server card: population and connection at a glance; a click
    /// opens the lobby list below it.
    pub(super) fn central_card(&mut self, card: Rect, drop: f32, s: f32) -> Option<Action> {
        use crate::server_status::{State, quality};
        let now = get_time();
        // Keep the card current; refresh faster while the list is open.
        self.servers.watch_every(
            crate::settings::main_server(),
            now,
            if self.lobbies_open { 4. } else { 15. },
        );
        let hovered = contains(card);
        panel(card);
        if hovered {
            draw_rectangle(card.x, card.y, card.w, card.h, alpha(hover_color(), 0.55));
        }
        draw_rectangle(card.x, card.y, 4.0 * s, card.h, accent());
        let state = self.servers.state(crate::settings::main_server()).cloned();
        let connection = self.servers.connection(crate::settings::main_server());
        // Chevron, then the signal, right-aligned.
        let chevron_x = card.right() - 22.0 * s;
        let chevron_y = card.y + card.h * 0.5;
        let turn = self.lobbies_open_amount * std::f32::consts::PI;
        let (sin, cos) = turn.sin_cos();
        let point = |dx: f32, dy: f32| {
            vec2(
                chevron_x + dx * cos - dy * sin,
                chevron_y + dx * sin + dy * cos,
            )
        };
        draw_line(
            point(-5.0 * s, -2.5 * s).x,
            point(-5.0 * s, -2.5 * s).y,
            point(0., 2.5 * s).x,
            point(0., 2.5 * s).y,
            2.0 * s,
            muted_color(),
        );
        draw_line(
            point(0., 2.5 * s).x,
            point(0., 2.5 * s).y,
            point(5.0 * s, -2.5 * s).x,
            point(5.0 * s, -2.5 * s).y,
            2.0 * s,
            muted_color(),
        );
        let (bars, rtt, population) = match &state {
            Some(State::Online(probe)) => (
                Some(connection.map_or(0, |(rtt, loss)| quality(rtt, loss))),
                connection.map(|c| c.0),
                Some(format!("{} online", probe.status.players)),
            ),
            Some(State::Offline(_)) => (Some(0), None, None),
            _ => (None, None, None),
        };
        let signal_x = chevron_x - 88.0 * s;
        signal(signal_x, card.y + 31.0 * s, bars, rtt, s);
        let mut name_right = signal_x - 12.0 * s;
        if let Some(population) = &population {
            right_label(
                population,
                name_right,
                card.y + 30.0 * s,
                13.0 * s,
                muted_color(),
            );
            name_right -= label_width(population, 13.0 * s) + 12.0 * s;
        }
        label(
            &fitted_label(
                crate::settings::main_server(),
                20.0 * s,
                name_right - card.x - 17.0 * s,
            ),
            card.x + 17.0 * s,
            card.y + 31.0 * s,
            20.0 * s,
            text_color(),
        );
        if hovered && is_mouse_button_pressed(MouseButton::Left) {
            self.lobbies_open = !self.lobbies_open;
            if self.lobbies_open {
                self.lobby_page = 0;
                self.servers.refresh(crate::settings::main_server());
            }
        }
        ease_toward(
            &mut self.lobbies_open_amount,
            if self.lobbies_open { 1.0 } else { 0.0 },
            14.0,
        );
        if drop < 1.0 {
            return None;
        }
        let area = Rect::new(card.x, card.bottom() + 4.0 * s, card.w, drop - 4.0 * s);
        let _clip = crate::ui_layout::Scissor::new(area);
        panel(Rect::new(
            area.x,
            area.y,
            area.w,
            self.lobby_list_height(s) - 4.0 * s,
        ));
        let row_h = LOBBY_ROW * s;
        let inner = Rect::new(area.x, area.y + 4.0 * s, area.w, area.h - 8.0 * s);
        let lobbies = match &state {
            Some(State::Online(probe)) if !probe.status.lobbies.is_empty() => &probe.status.lobbies,
            Some(State::Online(_)) => {
                label(
                    "No lobbies are open yet. Play starts one.",
                    inner.x + 17.0 * s,
                    inner.y + 20.0 * s,
                    14.0 * s,
                    muted_color(),
                );
                return None;
            }
            Some(State::Offline(error)) => {
                label(
                    &fitted_label(error, 13.0 * s, inner.w - 34.0 * s),
                    inner.x + 17.0 * s,
                    inner.y + 20.0 * s,
                    13.0 * s,
                    RED,
                );
                return None;
            }
            _ => {
                label(
                    "Checking lobbies…",
                    inner.x + 17.0 * s,
                    inner.y + 20.0 * s,
                    14.0 * s,
                    muted_color(),
                );
                return None;
            }
        };
        let pages = lobby_pages(lobbies.len());
        self.lobby_page = self.lobby_page.min(pages - 1);
        if pages > 1 {
            crate::ui_layout::claim_wheel(area);
        }
        if pages > 1 && contains(area) {
            let steps = crate::ui_layout::scroll_steps(
                &mut self.lobby_scroll_fraction,
                crate::ui_layout::wheel_lines().y,
            );
            if steps < 0 {
                self.lobby_page = (self.lobby_page + steps.unsigned_abs() as usize).min(pages - 1);
            } else if steps > 0 {
                self.lobby_page = self.lobby_page.saturating_sub(steps as usize);
            }
        }
        let first = self.lobby_page * LOBBIES_PER_PAGE;
        let mut chosen = None;
        for (slot, (index, lobby)) in lobbies
            .iter()
            .enumerate()
            .skip(first)
            .take(LOBBIES_PER_PAGE)
            .enumerate()
        {
            let ry = inner.y + slot as f32 * row_h;
            let row = Rect::new(inner.x + 6.0 * s, ry, inner.w - 12.0 * s, row_h - 3.0 * s);
            // Servers without lobby sizes report a capacity of one or less;
            // their single lobby takes anyone up to the server's limit.
            let limited = lobby.capacity > 1;
            let full = limited && lobby.humans >= lobby.capacity;
            let over = contains(row) && contains(inner) && !full;
            if over {
                draw_rectangle(row.x, row.y, row.w, row.h, alpha(hover_color(), 0.8));
            }
            label(
                &format!("Lobby {}", index + 1),
                row.x + 11.0 * s,
                row.y + 19.0 * s,
                15.0 * s,
                text_color(),
            );
            let meter_w = 64.0 * s;
            let meter_x = row.right() - meter_w - 54.0 * s;
            let detail = format!("{} · {}", lobby.mode, lobby.map);
            label(
                &fitted_label(&detail, 12.0 * s, meter_x - row.x - 94.0 * s),
                row.x + 84.0 * s,
                row.y + 18.0 * s,
                12.0 * s,
                muted_color(),
            );
            if limited {
                progress(
                    Rect::new(meter_x, row.y + 10.0 * s, meter_w, 6.0 * s),
                    f32::from(lobby.humans) / f32::from(lobby.capacity),
                    if full { RED } else { accent() },
                );
            }
            right_label(
                &if full {
                    "Full".to_owned()
                } else if limited {
                    format!("{}/{}", lobby.humans, lobby.capacity)
                } else {
                    format!("{} playing", lobby.humans)
                },
                row.right() - 10.0 * s,
                row.y + 18.0 * s,
                13.0 * s,
                if full { RED } else { text_color() },
            );
            if over && is_mouse_button_pressed(MouseButton::Left) {
                chosen = Some(index as u8);
            }
        }
        if pages > 1 {
            let bar_y = inner.y + LOBBIES_PER_PAGE as f32 * row_h + 2.0 * s;
            draw_rectangle(
                inner.x + 6.0 * s,
                bar_y - 1.0 * s,
                inner.w - 12.0 * s,
                1.0,
                line_color(),
            );
            let center = inner.center().x;
            center_label(
                &format!("Page {} of {}", self.lobby_page + 1, pages),
                center,
                bar_y + 18.0 * s,
                12.0 * s,
                muted_color(),
            );
            let size = 26.0 * s;
            let previous = Rect::new(center - 70.0 * s - size, bar_y + 3.0 * s, size, size);
            let next = Rect::new(center + 70.0 * s, bar_y + 3.0 * s, size, size);
            if small_button(previous, "‹", s * 0.8) {
                self.lobby_page = self.lobby_page.saturating_sub(1);
            }
            if small_button(next, "›", s * 0.8) {
                self.lobby_page = (self.lobby_page + 1).min(pages - 1);
            }
        }
        chosen.map(Action::JoinLobby)
    }
}
