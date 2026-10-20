use crate::ui::*;
use macroquad::prelude::*;
use tankyard_core::{Mode, Snapshot};
use tankyard_protocol::{
    MatchSettings, ServerCommand, ServerKind, ServerPermission as Permission, ServerRole as Role,
};
use tankyard_server::NetworkClient;

pub struct CommunityUi {
    pub typing: bool,
    pub admin_open: bool,
    draft: String,
    room: String,
    selected: Option<u64>,
    round: Option<u32>,
    page: usize,
    tab: usize,
    reason: String,
    reason_focus: bool,
    room_name: String,
    room_focus: bool,
    announcement: String,
    announcement_focus: bool,
    minutes: f32,
    minimum_role: Role,
    nominated_role: Role,
    permissions: Vec<Permission>,
    bots: usize,
    configuration: Option<MatchSettings>,
    match_mode: Mode,
    match_map: String,
    difficulty: usize,
    notice: String,
    chat_revision: u64,
    chat_scale: u32,
    cached_room: String,
    text_lines: Vec<String>,
    scroll: usize,
    docked: bool,
}
impl Default for CommunityUi {
    fn default() -> Self {
        Self {
            typing: false,
            admin_open: false,
            draft: String::new(),
            room: "lobby".into(),
            selected: None,
            round: None,
            page: 0,
            tab: 0,
            reason: String::new(),
            reason_focus: false,
            room_name: String::new(),
            room_focus: false,
            announcement: String::new(),
            announcement_focus: false,
            minutes: 10.,
            minimum_role: Role::Player,
            nominated_role: Role::Moderator,
            permissions: tankyard_server::access::Grant::for_role(Role::Moderator).permissions,
            bots: 12,
            configuration: None,
            match_mode: Mode::Ffa,
            match_map: "open".into(),
            difficulty: 1,
            notice: String::new(),
            chat_revision: u64::MAX,
            chat_scale: 0,
            cached_room: String::new(),
            text_lines: vec![],
            scroll: 0,
            docked: false,
        }
    }
}
impl CommunityUi {
    pub fn message(&mut self, message: &str) {
        if self.admin_open || self.typing {
            self.notice = message.to_owned();
        }
    }
    pub fn focused(&self) -> bool {
        self.typing || self.admin_open
    }
    /// Called before gameplay hotkeys, so Enter and Escape have a single owner.
    pub fn keys(&mut self, client: &mut NetworkClient, alive: bool) -> bool {
        if client.permissions.is_empty() {
            self.admin_open = false;
        }
        if !client.chat_enabled {
            self.typing = false;
        }
        if self.focused() && is_key_pressed(KeyCode::Escape) {
            self.typing = false;
            self.admin_open = false;
            self.reason_focus = false;
            self.room_focus = false;
            self.announcement_focus = false;
            while get_char_pressed().is_some() {}
            return true;
        }
        if self.typing && is_key_pressed(KeyCode::Enter) {
            let text = self.draft.trim().replace('\u{2014}', "-");
            if !text.is_empty() {
                if client.send_chat(&self.room, &text) {
                    self.draft.clear();
                    self.typing = false;
                } else {
                    self.notice = "Message could not be sent.".into();
                }
            } else {
                self.typing = false;
            }
            return true;
        }
        if client.chat_enabled
            && !self.admin_open
            && ((alive && is_key_pressed(KeyCode::Enter)) || is_key_pressed(KeyCode::Slash))
        {
            self.typing = true;
            self.scroll = 0;
            while get_char_pressed().is_some() {}
            return true;
        }
        false
    }
    pub fn captures_pointer(&self, client: &NetworkClient, s: f32) -> bool {
        (!client.permissions.is_empty() && contains(Self::server_button(s)))
            || (self.typing && contains(self.chat_rect(s)))
            || self.admin_open
    }
    fn server_button(s: f32) -> Rect {
        Rect::new(254.0 * s, 54.0 * s, 110.0 * s, 24.0 * s)
    }
    fn chat_rect(&self, s: f32) -> Rect {
        if self.docked {
            // Keep chat readable beside upgrades and the Sandbox panel.
            Rect::new(screen_width() - 380. * s, 270. * s, 358. * s, 224. * s)
        } else {
            Rect::new(22.0 * s, 87.0 * s, 358.0 * s, 224.0 * s)
        }
    }
    fn submit(&mut self, client: &mut NetworkClient, command: ServerCommand) {
        self.notice = if client.admin(command) {
            "Request sent."
        } else {
            "Request could not be sent."
        }
        .into();
    }
    pub fn draw(
        &mut self,
        client: &mut NetworkClient,
        snapshot: &Snapshot,
        s: f32,
        dock_chat: bool,
    ) {
        self.docked = dock_chat;
        if self.round != Some(snapshot.match_state.round) {
            self.round = Some(snapshot.match_state.round);
            self.selected = None;
            self.page = 0;
            self.reason.clear();
            self.reason_focus = false;
            self.configuration = None;
        }
        if self.configuration != client.match_settings {
            if let Some(settings) = &client.match_settings {
                self.bots = settings.bots;
                self.match_mode = settings.mode;
                self.match_map.clone_from(&settings.map);
                self.difficulty = DIFFICULTIES
                    .iter()
                    .position(|name| settings.difficulty.eq_ignore_ascii_case(name))
                    .unwrap_or(1);
            }
            self.configuration.clone_from(&client.match_settings);
        }
        if !client.permissions.is_empty() && small_button(Self::server_button(s), "Server", s * 0.8)
        {
            self.admin_open = !self.admin_open;
            self.typing = false;
            self.notice.clear();
        }
        if client.chat_enabled && !self.admin_open {
            self.chat(client, s);
        }
        if self.admin_open {
            self.admin(client, snapshot, s);
        }
        if self.admin_open && !self.reason_focus && !self.room_focus && !self.announcement_focus {
            while get_char_pressed().is_some() {}
        }
    }
    fn chat(&mut self, client: &mut NetworkClient, s: f32) {
        if !client.rooms.iter().any(|r| r.id == self.room) {
            self.room = "lobby".into();
        }
        let r = self.chat_rect(s);
        if self.typing {
            panel(r);
            if !client.rooms.is_empty() {
                let index = client
                    .rooms
                    .iter()
                    .position(|room| room.id == self.room)
                    .unwrap_or(0);
                let display: String = client.rooms[index].name.chars().take(22).collect();
                center_label(
                    &display,
                    r.x + r.w * 0.5,
                    r.y + 26.0 * s,
                    13.0 * s,
                    text_color(),
                );
                let previous = small_button(
                    Rect::new(r.x + 8.0 * s, r.y + 8.0 * s, 28.0 * s, 24.0 * s),
                    "<",
                    s * 0.7,
                );
                let next = small_button(
                    Rect::new(r.x + r.w - 36.0 * s, r.y + 8.0 * s, 28.0 * s, 24.0 * s),
                    ">",
                    s * 0.7,
                );
                if previous || next {
                    let selected = if next {
                        (index + 1) % client.rooms.len()
                    } else {
                        (index + client.rooms.len() - 1) % client.rooms.len()
                    };
                    let id = client.rooms[selected].id.clone();
                    if client.join_room(&id) {
                        self.room = id;
                    }
                }
            }
            text_field(
                Rect::new(
                    r.x + 8.0 * s,
                    r.y + r.h - 44.0 * s,
                    r.w - 16.0 * s,
                    34.0 * s,
                ),
                &mut self.draft,
                &mut true,
                160,
                s * 0.78,
                false,
            );
        }
        if self.chat_revision != client.chat_revision
            || self.chat_scale != s.to_bits()
            || self.cached_room != self.room
        {
            let old_len = self.text_lines.len();
            let changed_room = self.cached_room != self.room;
            self.text_lines.clear();
            for message in client.chat_messages.iter().filter(|m| m.room == self.room) {
                for line in chat_lines(
                    &format!(
                        "{}: {}",
                        tankyard_core::display_player_name(
                            &message.name,
                            message.game_creator,
                            message.server_badge
                        ),
                        message.text
                    ),
                    r.w - 16.0 * s,
                    13.0 * s,
                ) {
                    self.text_lines.push(line);
                }
            }
            self.scroll = if changed_room {
                0
            } else if self.scroll > 0 {
                self.scroll + self.text_lines.len().saturating_sub(old_len)
            } else {
                0
            };
            self.cached_room.clone_from(&self.room);
            self.chat_revision = client.chat_revision;
            self.chat_scale = s.to_bits();
        }
        let count = if self.typing { 8 } else { 5 };
        let maximum = self.text_lines.len().saturating_sub(count);
        if self.typing && contains(r) {
            let wheel = mouse_wheel().1;
            let amount = (wheel.abs().ceil() as usize).min(16) * 3;
            if wheel > 0.0 {
                self.scroll = self.scroll.saturating_add(amount);
            } else if wheel < 0.0 {
                self.scroll = self.scroll.saturating_sub(amount);
            }
        }
        self.scroll = self.scroll.min(maximum);
        let end = self
            .text_lines
            .len()
            .saturating_sub(if self.typing { self.scroll } else { 0 });
        if !self.typing && end > 0 {
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                (49. + end.min(count) as f32 * 16.) * s,
                alpha(surface_color(), 0.88),
            );
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                (49. + end.min(count) as f32 * 16.) * s,
                s,
                alpha(muted_color(), 0.28),
            );
        }
        for (i, line) in self.text_lines[end.saturating_sub(count)..end]
            .iter()
            .enumerate()
        {
            label(
                line,
                r.x + 8.0 * s,
                r.y + (if self.typing { 53.0 } else { 38.0 } + i as f32 * 16.0) * s,
                13.0 * s,
                text_color(),
            );
        }
        if !self.typing {
            label(
                "Enter / Slash to chat",
                r.x + 8.0 * s,
                r.y + 15.0 * s,
                12.0 * s,
                muted_color(),
            );
        }
    }
    fn admin(&mut self, client: &mut NetworkClient, snapshot: &Snapshot, s: f32) {
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            alpha(bg_color(), 0.72),
        );
        let (r, s) = admin_layout(screen_width(), screen_height(), s);
        panel(r);
        label(
            if client.kind == ServerKind::Official {
                "Central server"
            } else if client.role == Role::Owner {
                "Server host"
            } else {
                "Server moderation"
            },
            r.x + 20.0 * s,
            r.y + 31.0 * s,
            24.0 * s,
            text_color(),
        );
        if small_button(
            Rect::new(r.x + r.w - 94.0 * s, r.y + 12.0 * s, 76.0 * s, 28.0 * s),
            "Close",
            s * 0.8,
        ) {
            self.admin_open = false;
        }
        for (i, title) in ["Players", "Rooms", "Settings", "Audit", "Announce"]
            .iter()
            .enumerate()
        {
            if button(
                Rect::new(
                    r.x + 20.0 * s + i as f32 * 128.0 * s,
                    r.y + 50.0 * s,
                    120.0 * s,
                    29.0 * s,
                ),
                title,
                self.tab == i,
                true,
                s * 0.8,
            ) {
                self.tab = i;
                self.reason_focus = false;
                self.room_focus = false;
            }
        }
        match self.tab {
            0 => self.players(client, snapshot, r, s),
            1 => self.rooms(client, r, s),
            2 => self.settings(client, r, s),
            3 => self.audit(client, r, s),
            _ => self.announcements(client, r, s),
        }
        let notice = fitted_label(&self.notice, 13. * s, r.w - 40. * s);
        label(
            &notice,
            r.x + 20.0 * s,
            r.y + r.h - 20.0 * s,
            13.0 * s,
            muted_color(),
        );
    }
    fn players(&mut self, client: &mut NetworkClient, snapshot: &Snapshot, r: Rect, s: f32) {
        let players: Vec<_> = snapshot.tanks.iter().filter(|t| !t.bot).collect();
        let pages = players.len().div_ceil(10).max(1);
        self.page = self.page.min(pages - 1);
        for (i, tank) in players.iter().skip(self.page * 10).take(10).enumerate() {
            let name = fitted_player_name(tank, 13. * s, 185. * s);
            if button(
                Rect::new(
                    r.x + 20.0 * s,
                    r.y + (96.0 + i as f32 * 32.0) * s,
                    238.0 * s,
                    28.0 * s,
                ),
                &format!("{}  #{}", name, tank.id),
                self.selected == Some(tank.id),
                true,
                s * 0.72,
            ) {
                self.selected = Some(tank.id);
            }
        }
        if small_button(
            Rect::new(r.x + 20.0 * s, r.y + 425.0 * s, 75.0 * s, 28.0 * s),
            "Prev",
            s * 0.7,
        ) {
            self.page = self.page.saturating_sub(1);
        }
        label(
            &format!("{} / {}", self.page + 1, pages),
            r.x + 112.0 * s,
            r.y + 446.0 * s,
            14.0 * s,
            text_color(),
        );
        if small_button(
            Rect::new(r.x + 182.0 * s, r.y + 425.0 * s, 75.0 * s, 28.0 * s),
            "Next",
            s * 0.7,
        ) {
            self.page = (self.page + 1).min(pages - 1);
        }
        let x = r.x + 278.0 * s;
        label(
            "Select a player",
            x,
            r.y + 110.0 * s,
            17.0 * s,
            text_color(),
        );
        if self
            .selected
            .is_some_and(|id| !players.iter().any(|t| t.id == id))
        {
            self.selected = None;
        }
        label("Reason", x, r.y + 143.0 * s, 13.0 * s, muted_color());
        text_field(
            Rect::new(x, r.y + 151.0 * s, 407.0 * s, 32.0 * s),
            &mut self.reason,
            &mut self.reason_focus,
            160,
            s * 0.8,
            false,
        );
        if let Some(player) = self.selected {
            for (i, (title, permission)) in [
                ("Warn", Permission::Warn),
                ("Kick", Permission::Kick),
                ("Mute", Permission::Mute),
                ("Ban", Permission::Ban),
            ]
            .into_iter()
            .enumerate()
            {
                if button(
                    Rect::new(
                        x + i as f32 * 104.0 * s,
                        r.y + 195.0 * s,
                        95.0 * s,
                        31.0 * s,
                    ),
                    title,
                    false,
                    client.permissions.contains(&permission),
                    s * 0.75,
                ) {
                    let reason = self.reason.trim().replace('\u{2014}', "-");
                    if permission == Permission::Warn && reason.trim().is_empty() {
                        self.notice = "Enter a reason for the warning.".into();
                        continue;
                    }
                    let command = match permission {
                        Permission::Warn => ServerCommand::Warn { player, reason },
                        Permission::Kick => ServerCommand::Kick { player, reason },
                        Permission::Ban => ServerCommand::Ban {
                            player,
                            minutes: self.minutes as u32,
                            reason,
                        },
                        _ => ServerCommand::Mute {
                            player,
                            minutes: self.minutes as u32,
                        },
                    };
                    self.submit(client, command);
                }
            }
            if button(
                Rect::new(x, r.y + 234.0 * s, 129.0 * s, 26.0 * s),
                "Unmute",
                false,
                client.permissions.contains(&Permission::Mute),
                s * 0.7,
            ) {
                self.submit(client, ServerCommand::Mute { player, minutes: 0 });
            }
        }
        slider_value(
            Rect::new(x + 145. * s, r.y + 226. * s, 262. * s, 48. * s),
            "Duration (minutes)",
            &mut self.minutes,
            [1., 1440.],
            1.,
            s * 0.8,
            |value| format!("{value:.0}"),
        );
        if !client.permissions.contains(&Permission::ManageRoles) {
            return;
        }
        label(
            "Role and permissions",
            x,
            r.y + 290.0 * s,
            16.0 * s,
            text_color(),
        );
        for (i, role) in [Role::Player, Role::Moderator, Role::Admin]
            .into_iter()
            .enumerate()
        {
            if button(
                Rect::new(
                    x + i as f32 * 137.0 * s,
                    r.y + 301.0 * s,
                    129.0 * s,
                    27.0 * s,
                ),
                &format!("{role:?}"),
                self.nominated_role == role,
                role < client.role,
                s * 0.7,
            ) {
                self.nominated_role = role;
                self.permissions = tankyard_server::access::Grant::for_role(role).permissions;
            }
        }
        for (i, permission) in Permission::ALL.into_iter().enumerate() {
            let title = permission_name(permission);
            let selected = self.permissions.contains(&permission);
            if button(
                Rect::new(
                    x + (i % 2) as f32 * 206.0 * s,
                    r.y + (340.0 + (i / 2) as f32 * 31.0) * s,
                    199.0 * s,
                    26.0 * s,
                ),
                title,
                selected,
                self.nominated_role != Role::Player && client.permissions.contains(&permission),
                s * 0.7,
            ) {
                if selected {
                    self.permissions.retain(|p| *p != permission);
                } else {
                    self.permissions.push(permission);
                }
            }
        }
        if let Some(player) = self.selected
            && button(
                Rect::new(x, r.y + 507.0 * s, 407.0 * s, 32.0 * s),
                "Apply role",
                true,
                true,
                s * 0.8,
            )
        {
            self.submit(
                client,
                ServerCommand::Nominate {
                    player,
                    role: self.nominated_role,
                    permissions: self.permissions.clone(),
                },
            );
        }
    }
    fn announcements(&mut self, client: &mut NetworkClient, r: Rect, s: f32) {
        label(
            "Announcement",
            r.x + 20. * s,
            r.y + 116. * s,
            20. * s,
            text_color(),
        );
        wrap(
            "Shown to everyone on this server for five seconds. Moderation and bans apply only to this server.",
            r.x + 20. * s,
            r.y + 149. * s,
            r.w - 40. * s,
            15. * s,
            muted_color(),
        );
        text_field(
            Rect::new(r.x + 20. * s, r.y + 214. * s, r.w - 40. * s, 42. * s),
            &mut self.announcement,
            &mut self.announcement_focus,
            160,
            s,
            false,
        );
        if button(
            Rect::new(r.x + 20. * s, r.y + 274. * s, 240. * s, 36. * s),
            "Send announcement",
            true,
            client.permissions.contains(&Permission::Announce)
                && !self.announcement.trim().is_empty(),
            s * 0.9,
        ) {
            self.submit(
                client,
                ServerCommand::Announce {
                    text: self.announcement.trim().to_owned(),
                },
            );
        }
    }
    fn rooms(&mut self, client: &mut NetworkClient, r: Rect, s: f32) {
        let can_manage = client.permissions.contains(&Permission::ManageRooms);
        for (i, room) in client.rooms.clone().iter().enumerate() {
            label(
                &room.name,
                r.x + 20.0 * s,
                r.y + (116.0 + i as f32 * 33.0) * s,
                15.0 * s,
                text_color(),
            );
            if button(
                Rect::new(
                    r.x + 195.0 * s,
                    r.y + (95.0 + i as f32 * 33.0) * s,
                    77.0 * s,
                    26.0 * s,
                ),
                "Delete",
                false,
                can_manage && room.id != "lobby",
                s * 0.7,
            ) {
                self.submit(
                    client,
                    ServerCommand::DeleteRoom {
                        id: room.id.clone(),
                    },
                );
            }
        }
        let x = r.x + 305.0 * s;
        label("New room", x, r.y + 117.0 * s, 17.0 * s, text_color());
        text_field(
            Rect::new(x, r.y + 137.0 * s, 380.0 * s, 34.0 * s),
            &mut self.room_name,
            &mut self.room_focus,
            40,
            s * 0.8,
            false,
        );
        label("Minimum role", x, r.y + 205.0 * s, 14.0 * s, muted_color());
        for (i, role) in [Role::Player, Role::Moderator, Role::Admin]
            .into_iter()
            .enumerate()
        {
            if button(
                Rect::new(
                    x + i as f32 * 129.0 * s,
                    r.y + 219.0 * s,
                    122.0 * s,
                    28.0 * s,
                ),
                &format!("{role:?}"),
                self.minimum_role == role,
                role <= client.role,
                s * 0.7,
            ) {
                self.minimum_role = role;
            }
        }
        if button(
            Rect::new(x, r.y + 266.0 * s, 380.0 * s, 33.0 * s),
            "Create room",
            true,
            can_manage && client.chat_enabled,
            s * 0.8,
        ) {
            let name = self.room_name.trim().replace('\u{2014}', "-");
            let id: String = name
                .to_ascii_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .take(32)
                .collect();
            self.submit(
                client,
                ServerCommand::CreateRoom {
                    id,
                    name,
                    minimum_role: self.minimum_role,
                },
            );
        }
    }
    fn settings(&mut self, client: &mut NetworkClient, r: Rect, s: f32) {
        let x = r.x + 20.0 * s;
        let enabled =
            client.permissions.contains(&Permission::Configure) && client.match_settings.is_some();
        label("Bot limit", x, r.y + 121.0 * s, 17.0 * s, text_color());
        if button(
            Rect::new(x, r.y + 139.0 * s, 45.0 * s, 30.0 * s),
            "-",
            false,
            enabled,
            s,
        ) {
            self.bots = self.bots.saturating_sub(1);
        }
        center_label(
            &self.bots.to_string(),
            x + 94.0 * s,
            r.y + 162.0 * s,
            20.0 * s,
            text_color(),
        );
        if button(
            Rect::new(x + 142.0 * s, r.y + 139.0 * s, 45.0 * s, 30.0 * s),
            "+",
            false,
            enabled,
            s,
        ) {
            self.bots = (self.bots + 1).min(128);
        }
        if button(
            Rect::new(x + 202.0 * s, r.y + 139.0 * s, 108.0 * s, 30.0 * s),
            "Apply",
            true,
            enabled,
            s * 0.8,
        ) {
            self.submit(client, ServerCommand::SetBots { count: self.bots });
        }
        if button(
            Rect::new(x, r.y + 203.0 * s, 310.0 * s, 34.0 * s),
            "Restart round",
            false,
            enabled,
            s * 0.8,
        ) {
            self.submit(client, ServerCommand::RestartRound);
        }
        if button(
            Rect::new(x, r.y + 258.0 * s, 310.0 * s, 34.0 * s),
            "Reload mods",
            false,
            client.permissions.contains(&Permission::ReloadMods),
            s * 0.8,
        ) {
            self.submit(client, ServerCommand::ReloadMods);
        }
        label(
            "Reloading mods starts a new round.",
            x,
            r.y + 321.0 * s,
            13.0 * s,
            muted_color(),
        );
        label(
            "Bot changes apply immediately.",
            x,
            r.y + 345.0 * s,
            13.0 * s,
            muted_color(),
        );

        let x = r.x + 380.0 * s;
        label("Mode", x, r.y + 121.0 * s, 17.0 * s, text_color());
        let step = setting_choice(
            Rect::new(x, r.y + 139.0 * s, 310.0 * s, 32.0 * s),
            self.match_mode.name(),
            enabled,
            s,
        );
        if step != 0 {
            let index = Mode::ALL
                .iter()
                .position(|mode| *mode == self.match_mode)
                .unwrap_or(0);
            self.match_mode =
                Mode::ALL[(index as i32 + step).rem_euclid(Mode::ALL.len() as i32) as usize];
        }
        label("Map", x, r.y + 211.0 * s, 17.0 * s, text_color());
        let index = client
            .bundle
            .maps
            .iter()
            .position(|map| map.id == self.match_map)
            .unwrap_or(0);
        if let Some(map) = client.bundle.maps.get(index) {
            let step = setting_choice(
                Rect::new(x, r.y + 229.0 * s, 310.0 * s, 32.0 * s),
                &map.name,
                enabled,
                s,
            );
            if step != 0 {
                let index =
                    (index as i32 + step).rem_euclid(client.bundle.maps.len() as i32) as usize;
                self.match_map.clone_from(&client.bundle.maps[index].id);
            }
        }
        label("Bot difficulty", x, r.y + 301.0 * s, 17.0 * s, text_color());
        let step = setting_choice(
            Rect::new(x, r.y + 319.0 * s, 310.0 * s, 32.0 * s),
            DIFFICULTIES[self.difficulty],
            enabled,
            s,
        );
        self.difficulty =
            (self.difficulty as i32 + step).rem_euclid(DIFFICULTIES.len() as i32) as usize;
        if button(
            Rect::new(x, r.y + 386.0 * s, 310.0 * s, 36.0 * s),
            "Apply match and restart",
            true,
            enabled,
            s * 0.8,
        ) {
            self.submit(
                client,
                ServerCommand::ConfigureMatch {
                    mode: self.match_mode,
                    map: self.match_map.clone(),
                    difficulty: DIFFICULTIES[self.difficulty].into(),
                },
            );
        }
        label(
            "Players stay connected during restart.",
            x,
            r.y + 449.0 * s,
            13.0 * s,
            muted_color(),
        );
    }
    fn audit(&mut self, client: &mut NetworkClient, r: Rect, s: f32) {
        if button(
            Rect::new(r.x + 20.0 * s, r.y + 94.0 * s, 175.0 * s, 30.0 * s),
            "Refresh",
            true,
            client.permissions.contains(&Permission::ViewAudit),
            s * 0.8,
        ) {
            self.submit(client, ServerCommand::ReadAudit);
        }
        for (i, line) in client.audit_lines.iter().rev().take(12).enumerate() {
            let display: String = line.chars().take(89).collect();
            label(
                &display,
                r.x + 20.0 * s,
                r.y + (153.0 + i as f32 * 29.0) * s,
                11.0 * s,
                text_color(),
            );
        }
    }
}
const DIFFICULTIES: [&str; 4] = ["Beginner", "Normal", "Advanced", "Expert"];

fn setting_choice(r: Rect, text: &str, enabled: bool, s: f32) -> i32 {
    let previous = button(
        Rect::new(r.x, r.y, 32.0 * s, r.h),
        "<",
        false,
        enabled,
        s * 0.8,
    );
    let next = button(
        Rect::new(r.x + r.w - 32.0 * s, r.y, 32.0 * s, r.h),
        ">",
        false,
        enabled,
        s * 0.8,
    );
    let mut size = 17.0 * s;
    while size > 10.0 * s && label_width(text, size) > r.w - 82.0 * s {
        size -= s;
    }
    center_label(
        text,
        r.x + r.w * 0.5,
        r.y + r.h * 0.5 + size * 0.31,
        size,
        text_color(),
    );
    i32::from(next) - i32::from(previous)
}

fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::Warn => "Warn",
        Permission::Announce => "Announce",
        Permission::Kick => "Kick",
        Permission::Ban => "Ban",
        Permission::Mute => "Mute",
        Permission::ManageRooms => "Manage rooms",
        Permission::ManageRoles => "Manage roles",
        Permission::Configure => "Settings",
        Permission::ReloadMods => "Reload mods",
        Permission::ViewAudit => "View audit",
    }
}
fn chat_lines(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for c in text.chars() {
        line.push(c);
        if label_width(&line, size) > width {
            if let Some(space) = line.rfind(char::is_whitespace)
                && space > 0
            {
                let rest = line[space..].trim_start().to_owned();
                lines.push(line[..space].to_owned());
                line = rest;
            } else {
                line.pop();
                lines.push(std::mem::take(&mut line));
                line.push(c);
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn admin_layout(width: f32, height: f32, requested: f32) -> (Rect, f32) {
    let s = requested
        .min((width - 32.) / 710.)
        .min((height - 32.) / 600.);
    (
        Rect::new(
            (width - 710. * s) * 0.5,
            (height - 600. * s) * 0.5,
            710. * s,
            600. * s,
        ),
        s,
    )
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn staff_panels_are_centered_and_fit_at_every_interface_scale() {
        for (width, height) in [
            (800., 600.),
            (1280., 800.),
            (1920., 1080.),
            (3840., 2160.),
            (800., 1200.),
        ] {
            for interface in [0.8, 1., 1.4] {
                let (rect, scale) = admin_layout(
                    width,
                    height,
                    crate::ui_layout::scale(width, height, interface),
                );
                assert!((rect.center().x - width * 0.5).abs() < 0.01);
                assert!((rect.center().y - height * 0.5).abs() < 0.01);
                assert!(
                    rect.x >= 15.9
                        && rect.y >= 15.9
                        && rect.x + rect.w <= width - 15.9
                        && rect.y + rect.h <= height - 15.9
                );
                assert!(scale > 0.);
            }
        }
    }
}
