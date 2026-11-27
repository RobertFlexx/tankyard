use crate::ui::*;
use macroquad::prelude::*;
use tankyard_core::{BossKind, Mode, ServerBadge, Snapshot, administration::WeaponTuning};
use tankyard_protocol::{
    MatchSettings, OwnerTank, ServerCommand, ServerHealth, ServerKind,
    ServerPermission as Permission, ServerRole as Role,
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
    search: String,
    search_focus: bool,
    owner_draft: Option<OwnerTank>,
    class_filter: String,
    class_page: usize,
    ban_page: usize,
    audit_page: usize,
    /// The ban a first click on Lift picked; the second click lifts it.
    lifting: Option<u64>,
    /// When this panel last asked for the live health numbers.
    health_asked: f64,
    /// Whether the open Bans tab has asked for the list yet.
    bans_asked: bool,
    summon: usize,
    minutes: f32,
    minimum_role: Role,
    nominated_role: Role,
    permissions: Vec<Permission>,
    bots: usize,
    configuration: Option<MatchSettings>,
    rules: tankyard_mods::GameRules,
    rules_loaded: bool,
    match_mode: Mode,
    match_map: String,
    difficulty: usize,
    notice: String,
    chat_revision: u64,
    chat_scale: u32,
    cached_room: String,
    text_lines: Vec<String>,
    scroll: usize,
    scroll_fraction: f32,
    docked: bool,
    compact_chat: bool,
    recall: crate::chat_recall::ChatRecall,
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
            search: String::new(),
            search_focus: false,
            owner_draft: None,
            class_filter: String::new(),
            class_page: 0,
            ban_page: 0,
            audit_page: 0,
            lifting: None,
            health_asked: f64::NEG_INFINITY,
            bans_asked: false,
            summon: 0,
            minutes: 10.,
            minimum_role: Role::Player,
            nominated_role: Role::Moderator,
            permissions: tankyard_server::access::Grant::for_role(Role::Moderator).permissions,
            bots: 12,
            configuration: None,
            rules: Default::default(),
            rules_loaded: false,
            match_mode: Mode::Ffa,
            match_map: "open".into(),
            difficulty: 1,
            notice: String::new(),
            chat_revision: u64::MAX,
            chat_scale: 0,
            cached_room: String::new(),
            text_lines: vec![],
            scroll: 0,
            scroll_fraction: 0.,
            docked: false,
            compact_chat: false,
            recall: crate::chat_recall::ChatRecall::default(),
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
            self.search_focus = false;
            while get_char_pressed().is_some() {}
            return true;
        }
        if self.typing {
            self.recall.keys(&mut self.draft);
        }
        if self.typing && is_key_pressed(KeyCode::Enter) {
            let text = self.draft.trim().replace('\u{2014}', "-");
            if !text.is_empty() {
                if client.send_chat(&self.room, &text) {
                    self.recall.note(&text);
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
            self.recall.stop();
            while get_char_pressed().is_some() {}
            return true;
        }
        false
    }
    pub fn captures_pointer(
        &self,
        client: &NetworkClient,
        s: f32,
        creator: bool,
        owner: bool,
    ) -> bool {
        ((!client.permissions.is_empty() || owner) && contains(Self::server_button(s, creator)))
            || (self.typing && contains(self.chat_rect(s)))
            || self.admin_open
    }
    fn server_button(s: f32, creator: bool) -> Rect {
        if creator {
            Rect::new(254.0 * s, 54.0 * s, 110.0 * s, 24.0 * s)
        } else {
            crate::ui_layout::staff_button(screen_width(), screen_height(), s)
        }
    }
    fn chat_rect(&self, s: f32) -> Rect {
        let width = if self.compact_chat { 286. } else { 358. } * s;
        if self.docked {
            Rect::new(screen_width() - width - 22. * s, 270. * s, width, 224. * s)
        } else {
            Rect::new(22. * s, 87. * s, width, 224. * s)
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
        creator: bool,
        owner: bool,
    ) {
        self.docked = dock_chat;
        self.compact_chat = snapshot.tanks.iter().any(|t| t.creator_boss && t.alive);
        if self.round != Some(snapshot.match_state.round) {
            self.round = Some(snapshot.match_state.round);
            self.selected = None;
            self.page = 0;
            self.reason.clear();
            self.reason_focus = false;
            self.configuration = None;
            self.owner_draft = None;
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
        let title = if creator {
            "Server"
        } else if owner {
            "Owner tools [F8]"
        } else {
            "Server tools [F8]"
        };
        if (!client.permissions.is_empty() || owner)
            && small_button(Self::server_button(s, creator), title, s * 0.72)
        {
            if client.permissions.is_empty() && owner && client.central_owner_challenge.is_some() {
                // Asking for owner tools here is the owner's consent to
                // prove who they are to this server.
                client.staff_consent = true;
                client.last_action = Some(
                    "Verifying your owner identity with this server. Verify only where you trust the host.".into(),
                );
            } else if client.permissions.is_empty() {
                client.last_action = Some(crate::central_owner::CentralOwner::unavailable(client));
            } else {
                self.admin_open = !self.admin_open;
                self.typing = false;
                self.notice.clear();
            }
        }
        if client.chat_enabled && client.party_members.is_empty() && !self.admin_open {
            self.chat(client, s);
        }
        if self.admin_open {
            self.admin(client, snapshot, s);
        }
        if self.admin_open
            && !self.reason_focus
            && !self.room_focus
            && !self.announcement_focus
            && !self.search_focus
        {
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
            || self.chat_scale != (s.to_bits() ^ u32::from(self.compact_chat))
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
            self.chat_scale = s.to_bits() ^ u32::from(self.compact_chat);
        }
        let count = if self.typing { 8 } else { 5 };
        let maximum = self.text_lines.len().saturating_sub(count);
        if self.typing && contains(r) {
            let steps = crate::ui_layout::scroll_steps(
                &mut self.scroll_fraction,
                crate::ui_layout::wheel_lines().y * 3.,
            );
            let amount = steps.unsigned_abs() as usize;
            if steps > 0 {
                self.scroll = self.scroll.saturating_add(amount);
            } else if steps < 0 {
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
        let central_owner = client.kind == ServerKind::Official
            && client.role == Role::Owner
            && snapshot.tanks.iter().any(|tank| {
                Some(tank.id) == client.player_id && tank.server_badge == ServerBadge::CentralOwner
            });
        let tabs = if central_owner {
            &ADMIN_TABS[..]
        } else {
            &ADMIN_TABS[..ADMIN_TABS.len() - 1]
        };
        self.tab = self.tab.min(tabs.len() - 1);
        let tab_width = (r.w - 40. * s - 5. * s * (tabs.len() - 1) as f32) / tabs.len() as f32;
        for (i, &tab) in tabs.iter().enumerate() {
            if button(
                Rect::new(
                    r.x + 20.0 * s + i as f32 * (tab_width + 5. * s),
                    r.y + 50.0 * s,
                    tab_width,
                    29.0 * s,
                ),
                tab.title(),
                self.tab == i,
                true,
                s * 0.7,
            ) {
                if tab == AdminTab::MyTank && self.tab != i {
                    self.owner_draft = None;
                    client.admin(ServerCommand::ReadOwnTank);
                }
                self.tab = i;
                self.reason_focus = false;
                self.room_focus = false;
                self.search_focus = false;
                self.announcement_focus = false;
                self.lifting = None;
                self.bans_asked = false;
                self.health_asked = f64::NEG_INFINITY;
                if tab == AdminTab::Rules {
                    self.rules_loaded = false;
                }
            }
        }
        match tabs[self.tab] {
            AdminTab::Players => self.players(client, snapshot, r, s),
            AdminTab::Rooms => self.rooms(client, r, s),
            AdminTab::Settings => self.settings(client, r, s),
            AdminTab::Bans => self.bans(client, r, s),
            AdminTab::Live => self.live(client, r, s),
            AdminTab::Audit => self.audit(client, r, s),
            AdminTab::Rules => self.rules(client, snapshot, r, s),
            AdminTab::Announce => self.announcements(client, r, s),
            AdminTab::MyTank => self.my_tank(client, r, s),
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
    fn my_tank(&mut self, client: &mut NetworkClient, r: Rect, s: f32) {
        let x = r.x + 20. * s;
        let y = r.y;
        let Some(current) = client.own_tank.as_ref() else {
            let waiting = !client.own_tank_unavailable();
            if waiting {
                client.admin(ServerCommand::ReadOwnTank);
            } else if small_button(
                Rect::new(x, y + 146. * s, 100. * s, 32. * s),
                "Retry",
                s * 0.8,
            ) {
                client.retry_own_tank();
            }
            label(
                if waiting {
                    "Reading your tank…"
                } else {
                    "No reply. These controls require central server 2.1.5 or newer."
                },
                x,
                y + 125. * s,
                15. * s,
                muted_color(),
            );
            return;
        };
        let draft = self.owner_draft.get_or_insert_with(|| current.clone());
        let list_width = 270. * s;
        label("Choose a hull", x, y + 112. * s, 18. * s, text_color());
        text_field(
            Rect::new(x, y + 128. * s, list_width, 32. * s),
            &mut self.class_filter,
            &mut self.search_focus,
            48,
            0.8 * s,
            false,
        );
        if self.class_filter.is_empty() && !self.search_focus {
            label(
                "Search by name or ID",
                x + 9. * s,
                y + 150. * s,
                14. * s,
                muted_color(),
            );
        }
        let query = self.class_filter.trim().to_lowercase();
        let classes = client.bundle.content.classes.iter().filter(|c| {
            query.is_empty() || c.id.contains(&query) || c.name.to_lowercase().contains(&query)
        });
        const ROWS: usize = 9;
        let pages = classes.clone().count().div_ceil(ROWS).max(1);
        self.class_page = self.class_page.min(pages - 1);
        let mut visible = classes.skip(self.class_page * ROWS).take(ROWS).peekable();
        if visible.peek().is_none() {
            label(
                "No matching hulls.",
                x,
                y + 195. * s,
                14. * s,
                muted_color(),
            );
        }
        for (row, class) in visible.enumerate() {
            let name = fitted_label(&class.name, 15. * s, list_width - 18. * s);
            if button(
                Rect::new(x, y + (174. + 34. * row as f32) * s, list_width, 30. * s),
                &name,
                class.id == draft.class,
                true,
                s * 0.8,
            ) {
                draft.class.clone_from(&class.id);
            }
        }
        self.class_page = pager(
            Rect::new(x, y + 490. * s, list_width, 30. * s),
            self.class_page,
            pages,
            s,
        );
        let x = x + 302. * s;
        let width = r.w - 342. * s;
        if let Some(class) = client.bundle.content.class(&draft.class) {
            let center = x + width * 0.5;
            crate::render::class_model(
                class,
                vec2(center, y + 182. * s),
                32. * s,
                -0.38,
                0.,
                0.,
                player_color(),
                0.,
            );
            center_label(
                &fitted_label(&class.name, 20. * s, width),
                center,
                y + 250. * s,
                20. * s,
                text_color(),
            );
        }
        slider_value(
            Rect::new(x, y + 287. * s, width, 36. * s),
            "Weapon damage",
            &mut draft.weapons.damage,
            WeaponTuning::DAMAGE,
            0.05,
            s,
            |v| format!("{v:.2}×"),
        );
        slider_value(
            Rect::new(x, y + 354. * s, width, 36. * s),
            "Shot speed",
            &mut draft.weapons.shot_speed,
            WeaponTuning::SHOT_SPEED,
            0.05,
            s,
            |v| format!("{v:.2}×"),
        );
        toggle(
            Rect::new(x, y + 405. * s, width, 31. * s),
            "Invulnerability",
            &mut draft.invulnerable,
            s,
        );
        if small_button(
            Rect::new(x, y + 449. * s, width * 0.48, 34. * s),
            "Reset to 1×",
            s * 0.8,
        ) {
            draft.weapons = WeaponTuning::default();
        }
        let apply = button(
            Rect::new(x + width * 0.52, y + 449. * s, width * 0.48, 34. * s),
            "Apply",
            true,
            draft != current,
            s * 0.8,
        );
        wrap(
            "Changes apply to your tank for this round and mark the arena as modded.",
            x,
            y + 502. * s,
            width,
            14. * s,
            muted_color(),
        );
        if apply {
            let tank = draft.clone();
            self.submit(client, ServerCommand::SetOwnTank { tank });
        }
    }

    fn players(&mut self, client: &mut NetworkClient, snapshot: &Snapshot, r: Rect, s: f32) {
        let search = self.search.trim().to_lowercase();
        let players: Vec<_> = snapshot
            .tanks
            .iter()
            .filter(|t| !t.bot && matches_search(t, &search))
            .collect();
        text_field(
            Rect::new(r.x + 20.0 * s, r.y + 90.0 * s, 238.0 * s, 28.0 * s),
            &mut self.search,
            &mut self.search_focus,
            24,
            s * 0.72,
            false,
        );
        if self.search.is_empty() && !self.search_focus {
            label(
                "Search name or #id",
                r.x + 30.0 * s,
                r.y + 109.0 * s,
                13.0 * s,
                muted_color(),
            );
        }
        let pages = players.len().div_ceil(10).max(1);
        self.page = self.page.min(pages - 1);
        for (i, tank) in players.iter().skip(self.page * 10).take(10).enumerate() {
            let name = fitted_player_name(tank, 13. * s, 185. * s);
            if button(
                Rect::new(
                    r.x + 20.0 * s,
                    r.y + (124.0 + i as f32 * 32.0) * s,
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
        if players.is_empty() {
            label(
                "Nobody by that name.",
                r.x + 28.0 * s,
                r.y + 146.0 * s,
                13.0 * s,
                muted_color(),
            );
        }
        if small_button(
            Rect::new(r.x + 20.0 * s, r.y + 452.0 * s, 75.0 * s, 28.0 * s),
            "Prev",
            s * 0.7,
        ) {
            self.page = self.page.saturating_sub(1);
        }
        center_label(
            &format!("{} / {}", self.page + 1, pages),
            r.x + 139.0 * s,
            r.y + 473.0 * s,
            14.0 * s,
            text_color(),
        );
        if small_button(
            Rect::new(r.x + 183.0 * s, r.y + 452.0 * s, 75.0 * s, 28.0 * s),
            "Next",
            s * 0.7,
        ) {
            self.page = (self.page + 1).min(pages - 1);
        }
        let x = r.x + 278.0 * s;
        // A search that hides the selected player doesn't unselect them.
        if self
            .selected
            .is_some_and(|id| !snapshot.tanks.iter().any(|t| t.id == id && !t.bot))
        {
            self.selected = None;
        }
        match self
            .selected
            .and_then(|id| snapshot.tanks.iter().find(|t| t.id == id))
        {
            Some(tank) => {
                let name = fitted_player_name(tank, 17. * s, 200. * s);
                label(&name, x, r.y + 110.0 * s, 17.0 * s, text_color());
                let class = client
                    .bundle
                    .content
                    .class(&tank.class)
                    .map_or(tank.class.as_str(), |class| class.name.as_str());
                let detail = format!(
                    "Level {} {class} · {} points{}",
                    tank.level,
                    tank.score,
                    if tank.alive { "" } else { " · down" }
                );
                let detail = fitted_label(&detail, 12. * s, 200. * s);
                right_label(
                    &detail,
                    x + 407. * s,
                    r.y + 110.0 * s,
                    12. * s,
                    muted_color(),
                );
            }
            None => label(
                "Select a player",
                x,
                r.y + 110.0 * s,
                17.0 * s,
                text_color(),
            ),
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
        crate::staff_ui::reason_chips(
            Rect::new(x, r.y + 187.0 * s, 407.0 * s, 24.0 * s),
            &mut self.reason,
            s,
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
                        r.y + 217.0 * s,
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
                            minutes: crate::staff_ui::mute_minutes(self.minutes),
                        },
                    };
                    self.submit(client, command);
                }
            }
            if button(
                Rect::new(x, r.y + 262.0 * s, 129.0 * s, 26.0 * s),
                "Unmute",
                false,
                client.permissions.contains(&Permission::Mute),
                s * 0.7,
            ) {
                self.submit(client, ServerCommand::Mute { player, minutes: 0 });
            }
        }
        slider_integer_value(
            Rect::new(x + 145. * s, r.y + 254. * s, 262. * s, 48. * s),
            "Duration",
            &mut self.minutes,
            [1., crate::staff_ui::MOST_MINUTES],
            1.,
            s * 0.8,
            |value| crate::staff_ui::span(value as u64),
        );
        crate::staff_ui::duration_chips(
            Rect::new(x + 145. * s, r.y + 304. * s, 262. * s, 23. * s),
            &mut self.minutes,
            s,
        );
        if !client.permissions.contains(&Permission::ManageRoles) {
            return;
        }
        label(
            "Role and permissions",
            x,
            r.y + 346.0 * s,
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
                    r.y + 355.0 * s,
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
                    r.y + (392.0 + (i / 2) as f32 * 30.0) * s,
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
                Rect::new(x, r.y + 546.0 * s, 407.0 * s, 32.0 * s),
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
    fn rules(&mut self, client: &mut NetworkClient, snapshot: &Snapshot, r: Rect, s: f32) {
        if !self.rules_loaded {
            self.rules = snapshot.config.rules;
            self.rules_loaded = true;
        }
        let enabled = client.permissions.contains(&Permission::Configure);
        let body = Rect::new(r.x + 22. * s, r.y + 104. * s, r.w - 44. * s, 290. * s);
        set_interactive(enabled);
        crate::rules_ui::controls(&mut self.rules, body, s);
        crate::rules_ui::presets(
            &mut self.rules,
            Rect::new(body.x, r.y + 389. * s, body.w, 30. * s),
            s,
        );
        set_interactive(true);
        if button(
            Rect::new(body.x, r.y + 438. * s, 260. * s, 35. * s),
            "Apply live rules",
            true,
            enabled,
            s * 0.9,
        ) {
            self.submit(client, ServerCommand::ConfigureRules { rules: self.rules });
        }
        if small_button(
            Rect::new(body.x + 282. * s, r.y + 438. * s, 160. * s, 35. * s),
            "Read current",
            s * 0.8,
        ) {
            self.rules = snapshot.config.rules;
        }
        label(
            "Applies to every arena. Health keeps its current percentage.",
            body.x,
            r.y + 501. * s,
            13. * s,
            muted_color(),
        );
        label(
            "No restart; customized rounds do not earn standard records.",
            body.x,
            r.y + 525. * s,
            13. * s,
            muted_color(),
        );
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
        let now = crate::staff_ui::unix_seconds();
        let rows = crate::staff_ui::audit_rows(&client.audit_lines, now);
        let pages = rows.len().div_ceil(AUDIT_PER_PAGE).max(1);
        self.audit_page = self.audit_page.min(pages - 1);
        if rows.is_empty() {
            label(
                "Nothing here yet. Press Refresh to read the log.",
                r.x + 20.0 * s,
                r.y + 157.0 * s,
                14.0 * s,
                muted_color(),
            );
        }
        for (i, row) in rows
            .iter()
            .rev()
            .skip(self.audit_page * AUDIT_PER_PAGE)
            .take(AUDIT_PER_PAGE)
            .enumerate()
        {
            let row = fitted_label(row, 12.0 * s, r.w - 40.0 * s);
            label(
                &row,
                r.x + 20.0 * s,
                r.y + (155.0 + i as f32 * 29.0) * s,
                12.0 * s,
                text_color(),
            );
        }
        self.audit_page = pager(
            Rect::new(r.x + r.w - 220. * s, r.y + 94. * s, 200. * s, 30. * s),
            self.audit_page,
            pages,
            s,
        );
    }
    fn bans(&mut self, client: &mut NetworkClient, r: Rect, s: f32) {
        let allowed = client.permissions.contains(&Permission::Ban);
        if allowed && !self.bans_asked {
            self.bans_asked = true;
            client.ban_rows_fresh = false;
            client.admin(ServerCommand::ReadBans);
        }
        if button(
            Rect::new(r.x + 20.0 * s, r.y + 94.0 * s, 175.0 * s, 30.0 * s),
            "Refresh",
            true,
            allowed,
            s * 0.8,
        ) {
            self.lifting = None;
            self.submit(client, ServerCommand::ReadBans);
        }
        if !allowed {
            label(
                "Your role doesn't include the Ban permission.",
                r.x + 20.0 * s,
                r.y + 157.0 * s,
                14.0 * s,
                muted_color(),
            );
            return;
        }
        let pages = client.ban_rows.len().div_ceil(BANS_PER_PAGE).max(1);
        self.ban_page = self.ban_page.min(pages - 1);
        if client.ban_rows.is_empty() {
            label(
                if client.ban_rows_fresh {
                    "No bans in force. A peaceful server, or a very forgiving one."
                } else {
                    "Asking the server…"
                },
                r.x + 20.0 * s,
                r.y + 157.0 * s,
                14.0 * s,
                muted_color(),
            );
        }
        let rows: Vec<_> = client
            .ban_rows
            .iter()
            .skip(self.ban_page * BANS_PER_PAGE)
            .take(BANS_PER_PAGE)
            .cloned()
            .collect();
        for (i, ban) in rows.iter().enumerate() {
            let y = r.y + (136.0 + i as f32 * 49.0) * s;
            draw_rectangle(
                r.x + 20. * s,
                y,
                r.w - 40. * s,
                44. * s,
                alpha(muted_color(), 0.08),
            );
            let who = fitted_label(&ban.who, 14. * s, r.w - 290. * s);
            label(&who, r.x + 30. * s, y + 19. * s, 14. * s, text_color());
            let reason = if ban.reason.is_empty() {
                "No reason given".to_owned()
            } else {
                fitted_label(&ban.reason, 11. * s, r.w - 290. * s).into_owned()
            };
            label(&reason, r.x + 30. * s, y + 36. * s, 11. * s, muted_color());
            right_label(
                &format!(
                    "{} left",
                    crate::staff_ui::span(u64::from(ban.minutes_left))
                ),
                r.x + r.w - 128. * s,
                y + 27. * s,
                13. * s,
                muted_color(),
            );
            let asking = self.lifting == Some(ban.id);
            if button(
                Rect::new(r.x + r.w - 118. * s, y + 7. * s, 90. * s, 30. * s),
                if asking { "Really?" } else { "Lift" },
                asking,
                true,
                s * 0.75,
            ) {
                if asking {
                    self.lifting = None;
                    self.submit(client, ServerCommand::LiftBan { ban: ban.id });
                } else {
                    self.lifting = Some(ban.id);
                }
            }
        }
        self.ban_page = pager(
            Rect::new(r.x + r.w - 220. * s, r.y + 94. * s, 200. * s, 30. * s),
            self.ban_page,
            pages,
            s,
        );
    }
    fn live(&mut self, client: &mut NetworkClient, r: Rect, s: f32) {
        let can_read = client.permissions.contains(&Permission::ViewAudit);
        if can_read
            && get_time() - self.health_asked >= HEALTH_SECONDS
            && client.admin(ServerCommand::ReadHealth)
        {
            self.health_asked = get_time();
        }
        let x = r.x + 20. * s;
        label("Server health", x, r.y + 116. * s, 18. * s, text_color());
        if !can_read {
            label(
                "Your role doesn't include View audit.",
                x,
                r.y + 146. * s,
                13. * s,
                muted_color(),
            );
        } else if let Some(health) = client.server_health {
            for (i, (name, value, color)) in health_rows(&health).into_iter().enumerate() {
                let y = r.y + (148. + i as f32 * 27.) * s;
                label(name, x, y, 13. * s, muted_color());
                label(&value, x + 92. * s, y, 13. * s, color);
            }
        } else {
            label(
                if client.health_unavailable() {
                    "Update the server to use health monitoring."
                } else {
                    "Asking the server…"
                },
                x,
                r.y + 148. * s,
                13. * s,
                muted_color(),
            );
        }

        let x = r.x + 356. * s;
        let width = r.w - 376. * s;
        let can_summon = client.permissions.contains(&Permission::Configure);
        label("Boss events", x, r.y + 116. * s, 18. * s, text_color());
        let cell = (width - 12. * s) / 3.;
        for (i, kind) in BossKind::ALL.into_iter().enumerate() {
            if button(
                Rect::new(
                    x + (i % 3) as f32 * (cell + 6. * s),
                    r.y + (130. + (i / 3) as f32 * 33.) * s,
                    cell,
                    28. * s,
                ),
                kind.name(),
                self.summon == i,
                can_summon,
                s * 0.52,
            ) {
                self.summon = i;
            }
        }
        let y = r.y + (137. + 33. * BossKind::ALL.len().div_ceil(3) as f32) * s;
        if button(
            Rect::new(x, y, width, 34. * s),
            &format!("Summon the {}", BossKind::ALL[self.summon].name()),
            true,
            can_summon,
            s * 0.75,
        ) {
            self.submit(
                client,
                ServerCommand::SummonBoss {
                    boss: BossKind::ALL[self.summon].id().to_owned(),
                },
            );
        }
        if button(
            Rect::new(x, y + 42. * s, width, 32. * s),
            "Dismiss every boss",
            false,
            can_summon,
            s * 0.72,
        ) {
            self.submit(client, ServerCommand::DismissBosses);
        }
        wrap(
            "Appears in your arena for everyone in it. A summoned boss marks the round as customized, so it doesn't count toward records.",
            x,
            y + 94. * s,
            width,
            13. * s,
            muted_color(),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AdminTab {
    Players,
    Rooms,
    Settings,
    Bans,
    Live,
    Audit,
    Announce,
    Rules,
    MyTank,
}
const ADMIN_TABS: [AdminTab; 9] = [
    AdminTab::Players,
    AdminTab::Rooms,
    AdminTab::Settings,
    AdminTab::Bans,
    AdminTab::Live,
    AdminTab::Audit,
    AdminTab::Announce,
    AdminTab::Rules,
    AdminTab::MyTank,
];
impl AdminTab {
    fn title(self) -> &'static str {
        match self {
            Self::Players => "Players",
            Self::Rooms => "Rooms",
            Self::Settings => "Settings",
            Self::Bans => "Bans",
            Self::Live => "Live",
            Self::Audit => "Audit",
            Self::Announce => "Announce",
            Self::Rules => "Rules",
            Self::MyTank => "My tank",
        }
    }
}
const AUDIT_PER_PAGE: usize = 14;
const BANS_PER_PAGE: usize = 9;
/// How often the Live tab refreshes the server's numbers.
const HEALTH_SECONDS: f64 = 3.;

fn matches_search(tank: &tankyard_core::Tank, search: &str) -> bool {
    search.is_empty()
        || tank.name.to_lowercase().contains(search)
        || search
            .strip_prefix('#')
            .unwrap_or(search)
            .parse::<u64>()
            .is_ok_and(|id| id == tank.id)
}

/// "< 2 / 5 >". Returns the page to show next.
fn pager(r: Rect, page: usize, pages: usize, s: f32) -> usize {
    let mut page = page;
    if button(
        Rect::new(r.x, r.y, 48. * s, r.h),
        "<",
        false,
        page > 0,
        s * 0.8,
    ) {
        page -= 1;
    }
    center_label(
        &format!("{} / {pages}", page + 1),
        r.x + r.w * 0.5,
        r.y + r.h * 0.68,
        14. * s,
        text_color(),
    );
    if button(
        Rect::new(r.x + r.w - 48. * s, r.y, 48. * s, r.h),
        ">",
        false,
        page + 1 < pages,
        s * 0.8,
    ) {
        page += 1;
    }
    page
}

/// Name, value and color for each line of the health panel. Tick time goes
/// amber past half the budget and red past ninety percent.
fn health_rows(health: &ServerHealth) -> Vec<(&'static str, String, Color)> {
    let plain = text_color();
    let tick = health.tick_p95_micros as f32 / health.tick_budget_micros.max(1) as f32;
    let tick_color = if tick >= 0.9 {
        Color::new(0.95, 0.36, 0.33, 1.)
    } else if tick >= 0.5 {
        AMBER
    } else {
        Color::new(0.45, 0.86, 0.55, 1.)
    };
    let ms = |micros: u32| format!("{:.1} ms", micros as f32 / 1000.);
    let traffic = if health.snapshot_kib >= 1024 * 1024 {
        format!("{:.1} GiB sent", health.snapshot_kib as f32 / 1048576.)
    } else {
        format!("{:.1} MiB sent", health.snapshot_kib as f32 / 1024.)
    };
    vec![
        (
            "Uptime",
            crate::staff_ui::span(health.uptime_seconds / 60),
            plain,
        ),
        (
            "Players",
            format!("{} + {} bots", health.humans, health.bots),
            plain,
        ),
        ("Arenas", health.arenas.to_string(), plain),
        ("Bosses", health.bosses.to_string(), plain),
        (
            "Tick",
            format!(
                "{} avg, {} p95 of {}",
                ms(health.tick_average_micros),
                ms(health.tick_p95_micros),
                ms(health.tick_budget_micros)
            ),
            tick_color,
        ),
        ("Traffic", traffic, plain),
        (
            "Rejected",
            format!("{} packets", health.rejected_packets),
            plain,
        ),
        ("Bans", health.bans.to_string(), plain),
    ]
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
        .min((height - 32.) / 640.);
    (
        Rect::new(
            (width - 710. * s) * 0.5,
            (height - 640. * s) * 0.5,
            710. * s,
            640. * s,
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
