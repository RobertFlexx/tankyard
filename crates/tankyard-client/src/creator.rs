//! Creator interface. Server signatures and host policy authorize every online action.
use crate::{
    session::{Engine, Session},
    ui::*,
};
use macroquad::prelude::*;
use ring::signature::Ed25519KeyPair;
use tankyard_core::{BossKind, V2};
use tankyard_server::creator::{Command, apply};

pub struct CreatorUi {
    pub open: bool,
    key: Option<Ed25519KeyPair>,
    tab: usize,
    page: usize,
    boss: usize,
    speed: f32,
    damage: f32,
    reload: f32,
    level: f32,
    bots: f32,
    god: bool,
    ability: bool,
    freeze: bool,
    pending: std::collections::VecDeque<(Command, f64)>,
    last_sent: f64,
    round: Option<u32>,
    authorization: Option<[u8; 32]>,
    pub notice: String,
    reason: String,
    reason_focus: bool,
    announcement: String,
    announcement_focus: bool,
    minutes: f32,
    map: String,
}
impl CreatorUi {
    pub fn new() -> Self {
        let key = crate::credentials::load("creator.key", &tankyard_protocol::CREATOR_PUBLIC_KEY);
        Self {
            open: false,
            key,
            tab: 0,
            page: 0,
            boss: 0,
            speed: 1.,
            damage: 1.,
            reload: 1.,
            level: 55.,
            bots: 12.,
            god: false,
            ability: false,
            freeze: false,
            pending: std::collections::VecDeque::new(),
            last_sent: -1.,
            round: None,
            authorization: None,
            notice: "F8 opens or collapses creator tools.".into(),
            reason: String::new(),
            reason_focus: false,
            announcement: String::new(),
            announcement_focus: false,
            minutes: 10.,
            map: "open".into(),
        }
    }
    pub fn enabled(&self) -> bool {
        self.key.is_some()
    }
    pub fn authenticate(&mut self, session: &mut Session) {
        if let Some(round) = session
            .display
            .as_ref()
            .map(|snapshot| snapshot.match_state.round)
            && self.round != Some(round)
        {
            self.reset_controls();
            if let Some(snapshot) = &session.display {
                self.map.clone_from(&snapshot.config.map);
            }
            self.round = Some(round);
        }
        let Some(key) = &self.key else {
            return;
        };
        match &mut session.engine {
            Engine::Offline(local) => {
                if let Some(t) = local
                    .world
                    .tanks
                    .iter_mut()
                    .find(|t| t.id == session.player && !t.bot)
                {
                    t.game_creator = true;
                }
                if let Some(snapshot) = &mut session.display
                    && let Some(t) = snapshot
                        .tanks
                        .iter_mut()
                        .find(|t| t.id == session.player && !t.bot)
                {
                    t.game_creator = true;
                }
            }
            Engine::Online { client, .. } => {
                if let Some(nonce) = client.creator_challenge.take() {
                    if self.authorization != Some(nonce) {
                        self.authorization = Some(nonce);
                        self.open = false;
                        self.pending.clear();
                        self.god = false;
                        self.ability = false;
                        self.freeze = false;
                        self.speed = 1.;
                        self.damage = 1.;
                        self.reload = 1.;
                    }
                    let message = tankyard_protocol::creator_challenge(
                        client.client_id(),
                        &nonce,
                        client.creator_server_id.as_deref().unwrap_or(""),
                    );
                    if !client.submit_creator_proof(key.sign(&message).as_ref().to_vec()) {
                        self.notice = "Creator verification could not be queued. Reconnect.".into();
                    }
                }
            }
        }
    }
    fn button_rect(s: f32) -> Rect {
        crate::ui_layout::staff_button(screen_width(), screen_height(), s)
    }
    pub fn captures_pointer(&self, s: f32) -> bool {
        self.enabled() && (self.open || contains(Self::button_rect(s)))
    }
    fn send(&mut self, session: &mut Session, command: Command) {
        if !self.enabled() {
            return;
        }
        match &mut session.engine {
            Engine::Offline(local) => {
                match apply(local, session.player, command) {
                    Ok(text) => self.notice = text,
                    Err(text) => self.notice = text,
                }
                session.events.append(&mut local.world.events);
                session.display = Some(local.world.snapshot());
            }
            Engine::Online { client, .. } => {
                if !client.owner_extension_available {
                    self.notice = "This host has disabled creator administration. Your badge remains available.".into();
                    return;
                }
                let Ok(payload) = serde_json::to_vec(&command) else {
                    return;
                };
                let Some(message) = client.owner_command_to_sign(&payload) else {
                    self.notice = "Reconnect before using owner tools.".into();
                    return;
                };
                let Some(key) = &self.key else {
                    return;
                };
                let signature = key.sign(&message).as_ref().to_vec();
                self.notice = if client.owner_extension(payload, signature) {
                    "Command queued for the server."
                } else {
                    "Command could not be queued."
                }
                .into();
            }
        }
    }
    pub fn reset_controls(&mut self) {
        self.reason_focus = false;
        self.announcement_focus = false;
        self.authorization = None;
        self.open = false;
        self.pending.clear();
        self.god = false;
        self.ability = false;
        self.freeze = false;
        self.speed = 1.;
        self.damage = 1.;
        self.reload = 1.;
    }
    fn defer(&mut self, command: Command) {
        let kind = std::mem::discriminant(&command);
        self.pending
            .retain(|(c, _)| std::mem::discriminant(c) != kind);
        if self.pending.len() < 8 {
            self.pending.push_back((command, get_time() + 0.2));
        }
    }
    pub fn draw(&mut self, session: &mut Session, s: f32) {
        pointer_scope(300);
        if !self.enabled() || session.failure.is_some() {
            self.open = false;
            self.pending.clear();
            return;
        }
        self.authenticate(session);
        if get_time() - self.last_sent >= 0.55
            && self
                .pending
                .front()
                .is_some_and(|(_, time)| get_time() >= *time)
            && let Some((command, _)) = self.pending.pop_front()
        {
            self.last_sent = get_time();
            self.send(session, command);
        }
        set_interactive(true);
        if !self.open {
            if button(
                Self::button_rect(s),
                "Creator tools [F8]",
                false,
                true,
                s * 0.72,
            ) {
                self.open = true;
            }
            return;
        }
        let s = s.min(screen_height() / 850.).min(screen_width() / 600.);
        draw_rectangle(
            0.,
            0.,
            screen_width(),
            screen_height(),
            alpha(bg_color(), 0.92),
        );
        let r = Rect::new(
            screen_width() * 0.5 - 250. * s,
            screen_height() * 0.5 - 363. * s,
            500. * s,
            726. * s,
        );
        panel(r);
        draw_rectangle(r.x, r.y, r.w, 4. * s, AMBER);
        center_label(
            "GAME CREATOR",
            r.center().x,
            r.y + 38. * s,
            24. * s,
            text_color(),
        );
        if small_button(
            Rect::new(r.x + r.w - 44. * s, r.y + 12. * s, 30. * s, 28. * s),
            "−",
            s,
        ) {
            self.open = false;
        }
        let available = match &session.engine {
            Engine::Offline(_) => true,
            Engine::Online { client, .. } => {
                client.owner_extension_available
                    && session.display.as_ref().is_some_and(|snapshot| {
                        snapshot
                            .tanks
                            .iter()
                            .any(|t| t.id == session.player && t.game_creator)
                    })
            }
        };
        center_label(
            if available {
                "Creator tools · enabled by this host"
            } else {
                "Creator tools disabled by this host"
            },
            r.center().x,
            r.y + 64. * s,
            12. * s,
            muted_color(),
        );
        let tab_width = (r.w - 74. * s) / 6.;
        for (index, title) in [
            "Player",
            "World",
            "Tank class",
            "Moderation",
            "Announce",
            "Maps",
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(
                    r.x + 22. * s + index as f32 * (tab_width + 6. * s),
                    r.y + 82. * s,
                    tab_width,
                    32. * s,
                ),
                title,
                self.tab == index,
                true,
                s * 0.7,
            ) {
                self.tab = index;
                self.reason_focus = false;
                self.announcement_focus = false;
            }
        }
        set_interactive(available);
        let x = r.x + 24. * s;
        let y = r.y + 136. * s;
        let width = r.w - 48. * s;
        match self.tab {
            5 => {
                label("ARENA STYLE", x, y + 18. * s, 15. * s, muted_color());
                let maps = &session.bundle.maps;
                let pages = maps.len().div_ceil(8);
                self.page = self.page.min(pages.saturating_sub(1));
                for (slot, map) in maps.iter().skip(self.page * 8).take(8).enumerate() {
                    if button(
                        Rect::new(
                            x + (slot % 2) as f32 * (width + 10. * s) / 2.,
                            y + (40. + (slot / 2) as f32 * 60.) * s,
                            (width - 10. * s) / 2.,
                            48. * s,
                        ),
                        &map.name,
                        self.map == map.id,
                        available,
                        s * 0.8,
                    ) {
                        self.map.clone_from(&map.id);
                    }
                }
                if pages > 1 {
                    if small_button(
                        Rect::new(x, y + 292. * s, 100. * s, 32. * s),
                        "Previous",
                        s * 0.75,
                    ) {
                        self.page = self.page.saturating_sub(1);
                    }
                    center_label(
                        &format!("{} / {pages}", self.page + 1),
                        r.center().x,
                        y + 315. * s,
                        14. * s,
                        muted_color(),
                    );
                    if small_button(
                        Rect::new(x + width - 100. * s, y + 292. * s, 100. * s, 32. * s),
                        "Next",
                        s * 0.75,
                    ) {
                        self.page = (self.page + 1).min(pages - 1);
                    }
                }
                if let Some(map) = maps.iter().find(|map| map.id == self.map) {
                    for (row, line) in wrapped_lines(&map.description, width, 17. * s)
                        .iter()
                        .take(2)
                        .enumerate()
                    {
                        label(
                            line,
                            x,
                            y + (355. + row as f32 * 22.) * s,
                            17. * s,
                            text_color(),
                        );
                    }
                }
                wrap(
                    "Keeps tank upgrades and scores. Refreshes cover, shapes and shots for everyone on this server.",
                    x,
                    y + 416. * s,
                    width,
                    15. * s,
                    muted_color(),
                );
                if button(
                    Rect::new(x, y + 490. * s, width, 36. * s),
                    "Apply map style",
                    true,
                    available,
                    s * 0.85,
                ) {
                    self.send(
                        session,
                        Command::Map {
                            id: self.map.clone(),
                        },
                    );
                }
            }
            0 => {
                if slider_value(
                    Rect::new(x, y, width, 48. * s),
                    "Movement speed",
                    &mut self.speed,
                    [0.25, 6.],
                    0.25,
                    s,
                    |v| format!("{v:.2}×"),
                ) {
                    self.defer(Command::Speed { value: self.speed });
                }
                if slider_value(
                    Rect::new(x, y + 65. * s, width, 48. * s),
                    "Shot damage",
                    &mut self.damage,
                    [0.25, 8.],
                    0.25,
                    s,
                    |v| format!("{v:.2}×"),
                ) {
                    self.defer(Command::Damage { value: self.damage });
                }
                if slider_value(
                    Rect::new(x, y + 130. * s, width, 48. * s),
                    "Reload speed",
                    &mut self.reload,
                    [0.25, 6.],
                    0.25,
                    s,
                    |v| format!("{v:.2}×"),
                ) {
                    self.defer(Command::Reload { value: self.reload });
                }
                for (i, (name, selected)) in [
                    ("Invulnerable", self.god),
                    ("No ability cooldown", self.ability),
                ]
                .into_iter()
                .enumerate()
                {
                    if button(
                        Rect::new(x, y + (202. + i as f32 * 44.) * s, width, 36. * s),
                        &format!("{name}: {}", if selected { "On" } else { "Off" }),
                        selected,
                        available,
                        s * 0.8,
                    ) {
                        if i == 0 {
                            self.god = !self.god;
                            self.send(session, Command::God { enabled: self.god });
                        } else {
                            self.ability = !self.ability;
                            self.send(
                                session,
                                Command::Ability {
                                    unlimited: self.ability,
                                },
                            );
                        }
                    }
                }
                if slider_value(
                    Rect::new(x, y + 300. * s, width, 48. * s),
                    "Level",
                    &mut self.level,
                    [1., 55.],
                    1.,
                    s,
                    |v| format!("{v:.0}"),
                ) {
                    self.defer(Command::Level {
                        value: self.level as u8,
                    });
                }
                for (i, name) in [
                    "Restore health",
                    "Max level + full build",
                    "Teleport to center",
                    "Reset power modifiers",
                ]
                .into_iter()
                .enumerate()
                {
                    if button(
                        Rect::new(x, y + (372. + i as f32 * 42.) * s, width, 34. * s),
                        name,
                        false,
                        available,
                        s * 0.8,
                    ) {
                        let command = match i {
                            0 => Command::Heal,
                            1 => Command::FullBuild,
                            2 => {
                                let center = session
                                    .display
                                    .as_ref()
                                    .map_or(V2::ZERO, |s| s.config.bounds().center());
                                Command::Teleport {
                                    x: center.x,
                                    y: center.y,
                                }
                            }
                            _ => {
                                self.speed = 1.;
                                self.damage = 1.;
                                self.reload = 1.;
                                self.god = false;
                                self.ability = false;
                                self.freeze = false;
                                self.pending.clear();
                                Command::Reset
                            }
                        };
                        self.send(session, command);
                    }
                }
            }
            1 => {
                label("BOSS", x, y + 15. * s, 13. * s, muted_color());
                for (i, kind) in BossKind::ALL.into_iter().enumerate() {
                    if button(
                        Rect::new(
                            x + (i % 3) as f32 * (width + 9. * s) / 3.,
                            y + (30. + (i / 3) as f32 * 40.) * s,
                            (width - 18. * s) / 3.,
                            32. * s,
                        ),
                        kind.name(),
                        self.boss == i,
                        available,
                        s * 0.6,
                    ) {
                        self.boss = i;
                    }
                }
                if button(
                    Rect::new(x, y + 198. * s, width, 36. * s),
                    "Spawn selected boss",
                    true,
                    available,
                    s * 0.85,
                ) {
                    self.send(
                        session,
                        Command::Boss {
                            kind: BossKind::ALL[self.boss],
                        },
                    );
                }
                if slider_value(
                    Rect::new(x, y + 254. * s, width, 48. * s),
                    "Bot population",
                    &mut self.bots,
                    [0., 64.],
                    1.,
                    s,
                    |v| format!("{v:.0}"),
                ) {
                    self.defer(Command::Bots {
                        count: self.bots as usize,
                    });
                }
                for (i, name) in [
                    "Freeze bot movement",
                    "Clear projectiles",
                    "Remove bosses + their shots",
                    "Eliminate opposing tanks",
                ]
                .into_iter()
                .enumerate()
                {
                    if button(
                        Rect::new(x, y + (330. + i as f32 * 44.) * s, width, 36. * s),
                        name,
                        i == 0 && self.freeze,
                        available,
                        s * 0.78,
                    ) {
                        let command = match i {
                            0 => {
                                self.freeze = !self.freeze;
                                Command::FreezeBots {
                                    enabled: self.freeze,
                                }
                            }
                            1 => Command::ClearProjectiles,
                            2 => Command::ClearBosses,
                            _ => Command::EliminateEnemies,
                        };
                        self.send(session, command);
                    }
                }
            }
            2 => {
                let classes = &session.bundle.content.classes;
                self.page = self.page.min(classes.len().saturating_sub(1) / 18);
                let mut class = None;
                for (slot, tank) in classes.iter().skip(self.page * 18).take(18).enumerate() {
                    if button(
                        Rect::new(
                            x + (slot % 2) as f32 * (width + 10. * s) / 2.,
                            y + (slot / 2) as f32 * 47. * s,
                            (width - 10. * s) / 2.,
                            38. * s,
                        ),
                        &tank.name,
                        false,
                        available,
                        s * 0.72,
                    ) {
                        class = Some(tank.id.clone());
                    }
                }
                if let Some(id) = class {
                    self.send(session, Command::Class { id });
                }
                set_interactive(true);
                if small_button(
                    Rect::new(x, y + 455. * s, 100. * s, 32. * s),
                    "Previous",
                    s * 0.75,
                ) {
                    self.page = self.page.saturating_sub(1);
                }
                center_label(
                    &format!("{} / {}", self.page + 1, classes_len(session).div_ceil(18)),
                    r.center().x,
                    y + 477. * s,
                    14. * s,
                    muted_color(),
                );
                if small_button(
                    Rect::new(x + width - 100. * s, y + 455. * s, 100. * s, 32. * s),
                    "Next",
                    s * 0.75,
                ) {
                    self.page = (self.page + 1).min(classes_len(session).saturating_sub(1) / 18);
                }
            }
            3 => {
                wrap(
                    "Moderation applies to this server only. Bans never exclude anyone from other dedicated servers.",
                    x,
                    y + 8. * s,
                    width,
                    13. * s,
                    muted_color(),
                );
                label("Reason", x, y + 43. * s, 10. * s, muted_color());
                text_field(
                    Rect::new(x, y + 47. * s, width, 30. * s),
                    &mut self.reason,
                    &mut self.reason_focus,
                    160,
                    s * 0.8,
                    false,
                );
                slider_value(
                    Rect::new(x, y + 91. * s, width, 48. * s),
                    "Duration (minutes)",
                    &mut self.minutes,
                    [1., 1440.],
                    1.,
                    s * 0.8,
                    |v| format!("{v:.0}"),
                );
                let players: Vec<_> = session
                    .display
                    .as_ref()
                    .map(|snapshot| {
                        snapshot
                            .tanks
                            .iter()
                            .filter(|t| !t.bot && t.id != session.player)
                            .map(|t| (t.id, t.display_name()))
                            .collect()
                    })
                    .unwrap_or_default();
                let pages = players.len().div_ceil(4).max(1);
                self.page = self.page.min(pages - 1);
                for (slot, (id, name)) in players.iter().skip(self.page * 4).take(4).enumerate() {
                    let row = y + (154. + slot as f32 * 70.) * s;
                    let name = fitted_label(name, 14. * s, width);
                    label(&name, x, row + 15. * s, 14. * s, text_color());
                    for (i, caption) in ["Warn", "Kick", "Mute", "Ban"].into_iter().enumerate() {
                        if button(
                            Rect::new(
                                x + i as f32 * (width + 6. * s) / 4.,
                                row + 24. * s,
                                (width - 18. * s) / 4.,
                                30. * s,
                            ),
                            caption,
                            false,
                            available,
                            s * 0.75,
                        ) {
                            let reason = self.reason.trim().to_owned();
                            if i == 0 && reason.is_empty() {
                                self.notice = "Enter a reason for the warning.".into();
                                continue;
                            }
                            let command = match i {
                                0 => tankyard_protocol::ServerCommand::Warn {
                                    player: *id,
                                    reason,
                                },
                                1 => tankyard_protocol::ServerCommand::Kick {
                                    player: *id,
                                    reason,
                                },
                                2 => tankyard_protocol::ServerCommand::Mute {
                                    player: *id,
                                    minutes: self.minutes as u32,
                                },
                                _ => tankyard_protocol::ServerCommand::Ban {
                                    player: *id,
                                    minutes: self.minutes as u32,
                                    reason,
                                },
                            };
                            self.send(session, Command::Moderate { command });
                        }
                    }
                }
                if players.is_empty() {
                    label(
                        "No other human players connected.",
                        x,
                        y + 174. * s,
                        16. * s,
                        muted_color(),
                    );
                }
                set_interactive(true);
                if button(
                    Rect::new(x, y + 455. * s, 100. * s, 32. * s),
                    "Previous",
                    false,
                    self.page > 0,
                    s * 0.75,
                ) {
                    self.page = self.page.saturating_sub(1);
                }
                center_label(
                    &format!("{} / {}", self.page + 1, pages),
                    r.center().x,
                    y + 477. * s,
                    14. * s,
                    muted_color(),
                );
                if button(
                    Rect::new(x + width - 100. * s, y + 455. * s, 100. * s, 32. * s),
                    "Next",
                    false,
                    self.page + 1 < pages,
                    s * 0.75,
                ) {
                    self.page += 1;
                }
            }
            _ => {
                wrap(
                    "A centered banner is shown to everyone on this server for five seconds.",
                    x,
                    y + 14. * s,
                    width,
                    16. * s,
                    muted_color(),
                );
                text_field(
                    Rect::new(x, y + 90. * s, width, 40. * s),
                    &mut self.announcement,
                    &mut self.announcement_focus,
                    160,
                    s,
                    false,
                );
                if button(
                    Rect::new(x, y + 154. * s, width, 36. * s),
                    "Send announcement",
                    true,
                    available && !self.announcement.trim().is_empty(),
                    s * 0.9,
                ) {
                    self.send(
                        session,
                        Command::Moderate {
                            command: tankyard_protocol::ServerCommand::Announce {
                                text: self.announcement.trim().to_owned(),
                            },
                        },
                    );
                }
            }
        }
        set_interactive(true);
        let size = 12. * s;
        let note = fitted_label(&self.notice, size, width);
        center_label(
            &note,
            r.center().x,
            r.y + r.h - 18. * s,
            size,
            muted_color(),
        );
    }
}
fn classes_len(session: &Session) -> usize {
    session.bundle.content.classes.len()
}
