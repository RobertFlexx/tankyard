use crate::{
    menu::Action,
    render,
    ui::{ui_circle, ui_circle_lines, ui_line as draw_line, ui_rectangle, ui_rectangle_lines, *},
    voice::VoiceView,
};
use macroquad::prelude::*;
use std::collections::{HashSet, VecDeque};
use tankyard_content::Content;
use tankyard_core::{EntityId, Snapshot};
use tankyard_protocol::PartyMember;
use tankyard_server::NetworkClient;

/// How long a "joined" or "left" line stays up, in seconds.
const NOTICE_SECONDS: f64 = 5.;
/// Below this share of its health, a member's bar turns red.
const LOW_HEALTH: f32 = 0.3;
/// The rail's row for the microphone, in its units.
const RAIL_MIC: f32 = 25.;
/// The ring around someone who is talking.
const TALKING_RING: Color = Color::new(0.3, 0.9, 0.46, 1.);

/// A member's tank as the shared match sees it.
#[derive(Clone, Copy)]
struct Vitals {
    level: u8,
    health: f32,
    alive: bool,
}

fn vitals(snapshot: Option<&Snapshot>, player: EntityId) -> Option<Vitals> {
    let tank = snapshot?.tanks.iter().find(|tank| tank.id == player)?;
    Some(Vitals {
        level: tank.level,
        health: (tank.health / tank.max_health.max(1.)).clamp(0., 1.),
        alive: tank.alive,
    })
}

/// The icon, tint and words for this player's own microphone.
fn mic_face<'a>(voice: &VoiceView<'a>) -> (&'a Texture2D, Color, &'static str) {
    if voice.open {
        (
            voice.microphone,
            if voice.talking {
                TALKING_RING
            } else {
                accent_color()
            },
            "Mic on",
        )
    } else if voice.available {
        (voice.microphone_off, text_color(), "Muted")
    } else {
        (voice.microphone_off, muted_color(), "No mic")
    }
}

fn draw_icon(icon: &Texture2D, at: Vec2, size: f32, tint: Color) {
    let p = crate::ui_layout::position(at);
    draw_texture_ex(
        icon,
        p.x,
        p.y,
        tint,
        DrawTextureParams {
            dest_size: Some(Vec2::splat(size)),
            ..Default::default()
        },
    );
}

/// A button with a microphone icon and a word beside it.
fn voice_button(
    rect: Rect,
    (icon, tint, text): (&Texture2D, Color, &str),
    selected: bool,
    enabled: bool,
    s: f32,
) -> bool {
    let pressed = button(rect, "", selected, enabled, s * 0.58);
    let size = rect.h - 7. * s;
    draw_icon(icon, vec2(rect.x + 4. * s, rect.y + 3.5 * s), size, tint);
    let x = rect.x + size + 7. * s;
    label(
        &fitted_label(text, 11. * s, rect.right() - x - 3. * s),
        x,
        rect.center().y + 4. * s,
        11. * s,
        if enabled { text_color() } else { muted_color() },
    );
    pressed
}

fn health_color(health: f32) -> Color {
    if health < LOW_HEALTH {
        crate::ui::RED
    } else {
        accent_color()
    }
}

#[derive(Default)]
pub struct PartyUi {
    pub open: bool,
    muted: HashSet<u64>,
    draft: String,
    typing: bool,
    just_opened: bool,
    last_seen: u64,
    revision: u64,
    scale: u32,
    lines: Vec<(String, String)>,
    scroll: usize,
    scroll_fraction: f32,
    confirming_leave: bool,
    confirming_kick: Option<u64>,
    /// The roster last seen, to tell who came and went. Empty until the
    /// first one arrives, so joining a party announces nobody.
    known: Vec<(u64, String)>,
    arrivals: VecDeque<(String, f64)>,
    pub notice: String,
}
impl PartyUi {
    pub fn sync(&mut self, client: &NetworkClient) {
        self.muted.retain(|id| {
            client
                .party_members
                .iter()
                .any(|member| member.client == *id)
        });
        self.note_roster(&client.party_members, client.client_id(), get_time());
    }
    fn note_roster(&mut self, members: &[PartyMember], me: u64, now: f64) {
        if self
            .confirming_kick
            .is_some_and(|id| !members.iter().any(|m| m.client == id))
        {
            self.confirming_kick = None;
        }
        if !self.known.is_empty() {
            for member in members.iter().filter(|m| m.client != me) {
                if !self.known.iter().any(|(id, _)| *id == member.client) {
                    self.arrivals
                        .push_back((format!("{} joined the party", member.name), now));
                }
            }
            for (id, name) in &self.known {
                if *id != me && !members.iter().any(|m| m.client == *id) {
                    self.arrivals
                        .push_back((format!("{name} left the party"), now));
                }
            }
        }
        if self.known.len() != members.len()
            || self
                .known
                .iter()
                .zip(members)
                .any(|((id, name), member)| *id != member.client || *name != member.name)
        {
            self.known = members
                .iter()
                .map(|member| (member.client, member.name.clone()))
                .collect();
        }
        while self.arrivals.len() > 3
            || self
                .arrivals
                .front()
                .is_some_and(|(_, at)| now - at > NOTICE_SECONDS)
        {
            self.arrivals.pop_front();
        }
    }
    pub fn focus_chat(&mut self) {
        self.open = true;
        self.typing = true;
        self.just_opened = true;
        while get_char_pressed().is_some() {}
        self.notice.clear();
    }
    pub fn close(&mut self) {
        self.open = false;
        self.typing = false;
        self.confirming_leave = false;
        self.confirming_kick = None;
    }
    /// Whether the chat box has the keyboard.
    pub fn typing(&self) -> bool {
        self.typing
    }
    pub fn keys(&mut self) -> bool {
        if !self.open {
            return false;
        }
        if is_key_pressed(KeyCode::Escape) {
            self.close();
        }
        true
    }
    pub fn muted_players(&self, client: &NetworkClient) -> HashSet<u64> {
        client
            .party_members
            .iter()
            .filter(|member| self.muted.contains(&member.client))
            .map(|member| member.player)
            .collect()
    }
    pub fn captures_pointer(&self, members: usize, s: f32, menu: bool, dock_right: bool) -> bool {
        self.open
            || contains(rail_layout(
                screen_width(),
                screen_height(),
                members,
                s,
                menu,
                dock_right,
            ))
    }
    /// Arrows at the screen's edge toward allies out of view, so a party can
    /// find each other. Opponents stay on the minimap only.
    pub fn markers(
        members: &[PartyMember],
        local: EntityId,
        snapshot: &Snapshot,
        view: &render::View,
        s: f32,
    ) {
        let Some(team) = snapshot
            .tanks
            .iter()
            .find(|tank| tank.id == local)
            .map(|tank| tank.team)
            .filter(|team| *team != 0)
        else {
            return;
        };
        let (w, h) = (screen_width(), screen_height());
        let center = vec2(w, h) * 0.5;
        for member in members.iter().filter(|member| member.player != local) {
            let Some(tank) = snapshot
                .tanks
                .iter()
                .find(|tank| tank.id == member.player && tank.tangible() && tank.team == team)
            else {
                continue;
            };
            if view.visible(tank.pos, tank.radius) {
                continue;
            }
            let Some(d) = (view.screen(tank.pos) - center).try_normalize() else {
                continue;
            };
            let tip = crate::hud::edge_point(d, s);
            let n = vec2(-d.y, d.x);
            let size = 10. * s;
            let health = (tank.health / tank.max_health.max(1.)).clamp(0., 1.);
            let wing = |grow: f32, side: f32| {
                tip - d * (size * 0.7 + grow) + n * side * (size * 0.8 + grow)
            };
            draw_triangle(
                tip + d * (size + 2.5 * s),
                wing(2. * s, 1.),
                wing(2. * s, -1.),
                alpha(tankyard_client::tank_model::OUTLINE, 0.85),
            );
            draw_triangle(
                tip + d * size,
                wing(0., 1.),
                wing(0., -1.),
                health_color(health),
            );
            let name = fitted_label(&member.name, 11. * s, 92. * s);
            let at = tip - d * 24. * s;
            let half = label_width(&name, 11. * s) * 0.5;
            center_label(
                &name,
                at.x.clamp(half + 6. * s, w - half - 6. * s),
                at.y.clamp(14. * s, h - 6. * s),
                11. * s,
                text_color(),
            );
        }
    }
    /// Draw the party rail. True when its microphone button was pressed.
    pub fn rail(
        &mut self,
        client: &NetworkClient,
        content: &Content,
        voice: VoiceView<'_>,
        requested: f32,
        menu: bool,
        dock_right: bool,
    ) -> bool {
        let snapshot = client.snapshot.as_ref();
        let (rect, s) = rail_geometry(
            screen_width(),
            screen_height(),
            client.party_members.len(),
            requested,
            menu,
            dock_right,
        );
        let unread = client
            .chat_messages
            .iter()
            .filter(|message| message.id > self.last_seen)
            .count();
        let columns = rail_columns(client.party_members.len(), dock_right);
        crate::ui_layout::cover(rect);
        // Asked before the rail's own button, which would otherwise take the
        // click and open the panel instead of muting.
        let mic = rail_mic(rect, s);
        let mic_hovered = voice.available && crate::ui::contains(mic);
        let toggled = mic_hovered && crate::ui::take_click();
        // Over the microphone, only the microphone lights up.
        let pointer = mic_hovered.then(|| crate::ui::set_interactive(false));
        let opened = button(rect, "", false, true, s);
        if let Some(was) = pointer {
            crate::ui::set_interactive(was);
        }
        if opened {
            self.open = true;
            self.typing = false;
            self.notice.clear();
        }
        center_label(
            &format!("PARTY · {}", client.party_members.len()),
            rect.center().x,
            rect.y + 19. * s,
            11. * s,
            accent(),
        );
        for (index, member) in client.party_members.iter().enumerate() {
            let center = vec2(
                rect.x + (22. + (index % columns) as f32 * 36.) * s,
                rect.y + (43. + (index / columns) as f32 * 36.) * s,
            );
            let state = vitals(snapshot, member.player);
            let down = state.is_some_and(|state| !state.alive);
            let talking = if member.client == client.client_id() {
                voice.talking
            } else {
                voice.talking(member.player) && !self.muted.contains(&member.client)
            };
            if talking {
                ui_circle_lines(center.x, center.y, 14.5 * s, 2. * s, TALKING_RING);
            }
            if let Some(class) = content.class(&member.class) {
                render::class_model(
                    class,
                    center,
                    9. * s,
                    -0.4,
                    0.,
                    0.,
                    if down || self.muted.contains(&member.client) {
                        muted_color()
                    } else {
                        player_color()
                    },
                    0.,
                );
            }
            if let Some(state) = state.filter(|state| state.alive) {
                let bar = Rect::new(center.x - 12. * s, center.y + 13. * s, 24. * s, 3. * s);
                ui_rectangle(bar.x, bar.y, bar.w, bar.h, alpha(line_color(), 0.6));
                ui_rectangle(
                    bar.x,
                    bar.y,
                    bar.w * state.health,
                    bar.h,
                    health_color(state.health),
                );
            }
            if member.leader {
                // A small amber pip marks the host.
                ui_rectangle(
                    center.x + 7. * s,
                    center.y - 13. * s,
                    5. * s,
                    5. * s,
                    warm_text_color(),
                );
            }
            if member.ready {
                ui_circle(
                    center.x + 10. * s,
                    center.y + 8. * s,
                    3. * s,
                    accent_color(),
                );
            }
        }
        let (icon, tint, word) = mic_face(&voice);
        ui_rectangle(
            mic.x,
            mic.y,
            mic.w,
            mic.h,
            alpha(
                if voice.open { accent() } else { line_color() },
                if mic_hovered { 0.5 } else { 0.22 },
            ),
        );
        draw_icon(icon, vec2(mic.x + 3. * s, mic.y + 1.5 * s), 17. * s, tint);
        label(
            &fitted_label(word, 10. * s, mic.w - 25. * s),
            mic.x + 23. * s,
            mic.y + 14. * s,
            10. * s,
            if voice.available {
                text_color()
            } else {
                muted_color()
            },
        );
        // Who just came or went, beside the rail.
        for (index, (text, _)) in self.arrivals.iter().enumerate() {
            let size = 11. * s;
            let width = label_width(text, size) + 16. * s;
            let x = if dock_right || menu {
                rect.x - width - 6. * s
            } else {
                rect.right() + 6. * s
            };
            let y = rect.y + index as f32 * 24. * s;
            ui_rectangle(x, y, width, 20. * s, alpha(line_color(), 0.92));
            label(text, x + 8. * s, y + 14. * s, size, text_color());
        }
        if unread > 0 {
            let text = format!("{} new", unread.min(64));
            let pill_w = label_width(&text, 10. * s) + 12. * s;
            let pill = Rect::new(
                rect.center().x - pill_w * 0.5,
                rect.bottom() - 19. * s,
                pill_w,
                15. * s,
            );
            ui_rectangle(pill.x, pill.y, pill.w, pill.h, accent());
            center_label(
                &text,
                pill.center().x,
                pill.bottom() - 4. * s,
                10. * s,
                WHITE,
            );
        } else {
            center_label(
                "Open · chat",
                rect.center().x,
                rect.bottom() - 10. * s,
                10. * s,
                muted_color(),
            );
        }
        toggled
    }
    pub fn panel(
        &mut self,
        client: &mut NetworkClient,
        content: &Content,
        voice: VoiceView<'_>,
        network: &str,
        has_public_invite: bool,
        in_menu: bool,
        requested: f32,
    ) -> Action {
        if let Some(reason) = client.last_action.take() {
            self.notice = reason;
        }
        let can_kick = client
            .permissions
            .contains(&tankyard_protocol::ServerPermission::Kick);
        let view = PartyView {
            members: &client.party_members,
            me: client.client_id(),
            messages: &client.chat_messages,
            revision: client.chat_revision,
            chat_enabled: client.chat_enabled,
            snapshot: client.snapshot.as_ref(),
            can_kick,
            voice,
        };
        let (action, message) = self.panel_view(
            &view,
            content,
            network,
            has_public_invite,
            in_menu,
            requested,
        );
        if let Some(text) = message {
            if client.send_chat("lobby", &text) {
                self.draft.clear();
                self.scroll = 0;
                self.typing = true;
            } else {
                self.notice = "Message could not be sent. Check the connection.".into();
            }
        }
        action
    }
    /// Draw the party overlay from plain data. Returns the chosen action and
    /// a chat message to send, if any.
    pub fn panel_view(
        &mut self,
        view: &PartyView<'_>,
        content: &Content,
        network: &str,
        has_public_invite: bool,
        in_menu: bool,
        requested: f32,
    ) -> (Action, Option<String>) {
        let (r, s) = panel_layout(screen_width(), screen_height(), requested);
        pointer_scope(840);
        panel(r);
        let x = r.x + 22. * s;
        let y = r.y;
        let mut action = Action::None;
        let mut outgoing = None;
        // Header: who hosts, how full the party is, and how it is reached.
        label("YOUR PARTY", x, y + 35. * s, 22. * s, text_color());
        let leader = view
            .members
            .iter()
            .find(|member| member.leader)
            .map_or_else(|| "the leader".to_owned(), |member| member.name.clone());
        label(
            &fitted_label(
                &format!(
                    "{} of 8 players · {} ready · {leader} hosts",
                    view.members.len(),
                    view.members.iter().filter(|m| m.ready).count()
                ),
                12. * s,
                r.w - 230. * s,
            ),
            x,
            y + 58. * s,
            12. * s,
            muted_color(),
        );
        if small_button(
            Rect::new(r.right() - 57. * s, y + 17. * s, 36. * s, 32. * s),
            "×",
            s,
        ) {
            self.close();
        }
        let link = if network.starts_with("Internet relay ready") {
            ("Relay", accent())
        } else if network.starts_with("Direct") {
            ("Direct", accent())
        } else if network.is_empty() {
            ("Local", muted_color())
        } else {
            ("Network", warm_text_color())
        };
        let badge_w = label_width(link.0, 11. * s) + 30. * s;
        let badge = Rect::new(r.right() - 67. * s - badge_w, y + 22. * s, badge_w, 22. * s);
        ui_rectangle(
            badge.x,
            badge.y,
            badge.w,
            badge.h,
            alpha(line_color(), 0.22),
        );
        ui_circle(badge.x + 11. * s, badge.center().y, 4. * s, link.1);
        label(
            link.0,
            badge.x + 21. * s,
            badge.bottom() - 6. * s,
            11. * s,
            text_color(),
        );
        draw_line(
            r.x + 1.,
            y + 74. * s,
            r.right() - 1.,
            y + 74. * s,
            1.,
            alpha(line_color(), 0.8),
        );
        // Members, with every open seat visible so inviting is obvious.
        let left = Rect::new(x, y + 88. * s, 262. * s, 336. * s);
        let chat = Rect::new(
            left.right() + 18. * s,
            left.y,
            r.right() - left.right() - 40. * s,
            336. * s,
        );
        label("MEMBERS", left.x, left.y + 12. * s, 11. * s, muted_color());
        let ready = view.members.iter().any(|m| m.client == view.me && m.ready);
        if button(
            Rect::new(left.right() - 112. * s, left.y - 5. * s, 112. * s, 24. * s),
            if ready { "Ready ✓" } else { "I'm ready" },
            ready,
            true,
            s * 0.6,
        ) {
            action = Action::PartyReady(!ready);
        }
        for slot in 0..8 {
            let row = Rect::new(
                left.x,
                left.y + (22. + slot as f32 * 38.) * s,
                left.w,
                34. * s,
            );
            let Some(member) = view.members.get(slot) else {
                ui_rectangle_lines(row.x, row.y, row.w, row.h, 1., alpha(line_color(), 0.45));
                label(
                    "Open seat",
                    row.x + 12. * s,
                    row.y + 22. * s,
                    12. * s,
                    alpha(
                        muted_color(),
                        if slot == view.members.len() { 1. } else { 0.55 },
                    ),
                );
                if slot == view.members.len()
                    && button(
                        Rect::new(row.right() - 88. * s, row.y + 4. * s, 84. * s, 26. * s),
                        "Invite",
                        false,
                        has_public_invite,
                        s * 0.62,
                    )
                {
                    action = Action::CopyParty;
                }
                continue;
            };
            let me = member.client == view.me;
            ui_rectangle(
                row.x,
                row.y,
                row.w,
                row.h,
                alpha(
                    if me { accent() } else { line_color() },
                    if me { 0.10 } else { 0.16 },
                ),
            );
            let talking = if me {
                view.voice.talking
            } else {
                view.voice.talking(member.player) && !self.muted.contains(&member.client)
            };
            if talking {
                ui_circle_lines(
                    row.x + 18. * s,
                    row.center().y,
                    13. * s,
                    2. * s,
                    TALKING_RING,
                );
            }
            if let Some(class) = content.class(&member.class) {
                render::class_model(
                    class,
                    vec2(row.x + 18. * s, row.center().y),
                    8. * s,
                    -0.4,
                    0.,
                    0.,
                    if self.muted.contains(&member.client) {
                        muted_color()
                    } else {
                        player_color()
                    },
                    0.,
                );
            }
            let name = format!("{}{}", member.name, if me { " (you)" } else { "" });
            let badge_width = if member.leader {
                label_width("LEADER", 9. * s) + 8. * s
            } else {
                0.
            };
            let host_tools = !me && view.can_kick && !member.leader;
            let name = fitted_label(
                &name,
                12. * s,
                row.w
                    - (if me {
                        132.
                    } else if host_tools {
                        186.
                    } else {
                        132.
                    }) * s
                    - badge_width,
            );
            label(
                &name,
                row.x + 46. * s,
                row.y + 15. * s,
                12. * s,
                text_color(),
            );
            if member.leader {
                label(
                    "LEADER",
                    row.x + 52. * s + label_width(&name, 12. * s),
                    row.y + 15. * s,
                    9. * s,
                    warm_text_color(),
                );
            }
            let class_name = content
                .class(&member.class)
                .map_or(member.class.as_str(), |class| class.name.as_str());
            let health = vitals(view.snapshot, member.player);
            let state = if member.ready {
                "Ready"
            } else if member.in_menu {
                "In menus"
            } else if health.is_some_and(|health| !health.alive) {
                "Down"
            } else {
                "In match"
            };
            let line = match health {
                Some(health) => format!("Lv {} · {state} · {class_name}", health.level),
                None => format!("{state} · {class_name}"),
            };
            label(
                &fitted_label(
                    &line,
                    10. * s,
                    row.w
                        - (if me {
                            128.
                        } else if host_tools {
                            182.
                        } else {
                            128.
                        }) * s,
                ),
                row.x + 46. * s,
                row.y + 28. * s,
                10. * s,
                muted_color(),
            );
            if let Some(health) = health.filter(|health| health.alive) {
                let bar = Rect::new(row.x, row.bottom() - 2. * s, row.w, 2. * s);
                ui_rectangle(
                    bar.x,
                    bar.y,
                    bar.w * health.health,
                    bar.h,
                    health_color(health.health),
                );
            }
            if host_tools {
                let confirming = self.confirming_kick == Some(member.client);
                let kick = Rect::new(row.right() - 130. * s, row.y + 6. * s, 50. * s, 22. * s);
                if button(
                    kick,
                    if confirming { "Sure?" } else { "Kick" },
                    confirming,
                    true,
                    s * 0.58,
                ) {
                    if confirming {
                        self.confirming_kick = None;
                        action = Action::KickFromParty(member.player);
                    } else {
                        self.confirming_kick = Some(member.client);
                    }
                }
            }
            let toggle = Rect::new(row.right() - 76. * s, row.y + 6. * s, 70. * s, 22. * s);
            if me {
                let face = mic_face(&view.voice);
                if voice_button(toggle, face, view.voice.open, view.voice.available, s) {
                    action = Action::ToggleMic;
                }
            } else {
                let muted = self.muted.contains(&member.client);
                let face = if muted {
                    (view.voice.microphone_off, muted_color(), "Muted")
                } else if talking {
                    (view.voice.microphone, TALKING_RING, "Mute")
                } else {
                    (view.voice.microphone, text_color(), "Mute")
                };
                if voice_button(toggle, face, muted, true, s) && !self.muted.remove(&member.client)
                {
                    self.muted.insert(member.client);
                }
            }
        }
        // Chat: names stand apart from text, newest at the bottom.
        label(
            "PARTY CHAT",
            chat.x,
            chat.y + 12. * s,
            11. * s,
            muted_color(),
        );
        let transcript = Rect::new(chat.x, chat.y + 22. * s, chat.w, 252. * s);
        ui_rectangle(
            transcript.x,
            transcript.y,
            transcript.w,
            transcript.h,
            alpha(line_color(), 0.16),
        );
        if self.revision != view.revision || self.scale != s.to_bits() {
            let old = self.lines.len();
            self.lines.clear();
            for message in view
                .messages
                .iter()
                .filter(|message| message.room == "lobby")
            {
                let mut first = true;
                for line in wrapped_lines(
                    &format!("{}: {}", message.name, message.text),
                    transcript.w - 20. * s,
                    12. * s,
                ) {
                    // The sender's name is drawn separately on a message's first line.
                    let body = if first {
                        line.strip_prefix(&format!("{}:", message.name))
                            .map(|rest| (message.name.clone(), rest.to_owned()))
                    } else {
                        None
                    };
                    first = false;
                    self.lines.push(body.unwrap_or((String::new(), line)));
                }
            }
            if self.scroll > 0 {
                self.scroll = self
                    .scroll
                    .saturating_add(self.lines.len().saturating_sub(old));
            }
            self.revision = view.revision;
            self.scale = s.to_bits();
        }
        self.last_seen = view.messages.back().map_or(0, |message| message.id);
        let count = 14;
        if self.lines.len() > count {
            crate::ui_layout::claim_wheel(transcript);
        }
        if contains(transcript) {
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
        self.scroll = self.scroll.min(self.lines.len().saturating_sub(count));
        let end = self.lines.len().saturating_sub(self.scroll);
        if self.lines.is_empty() {
            label(
                "Say hello. Their drones are friendly, mostly.",
                transcript.x + 10. * s,
                transcript.y + 24. * s,
                12. * s,
                muted_color(),
            );
        }
        for (index, (name, line)) in self.lines[end.saturating_sub(count)..end]
            .iter()
            .enumerate()
        {
            let line_y = transcript.y + (20. + index as f32 * 17.) * s;
            let mut text_x = transcript.x + 10. * s;
            if !name.is_empty() {
                let prefix = format!("{name}:");
                label(&prefix, text_x, line_y, 12. * s, accent_color());
                text_x += label_width(&prefix, 12. * s);
            }
            label(line, text_x, line_y, 12. * s, text_color());
        }
        if self.scroll > 0 {
            let more = Rect::new(
                transcript.right() - 108. * s,
                transcript.bottom() - 24. * s,
                100. * s,
                18. * s,
            );
            if button(more, "Newest ↓", false, true, s * 0.5) {
                self.scroll = 0;
            }
        }
        let input = Rect::new(chat.x, chat.y + 282. * s, chat.w - 68. * s, 34. * s);
        text_field(
            input,
            &mut self.draft,
            &mut self.typing,
            160,
            s * 0.68,
            false,
        );
        let send = button(
            Rect::new(input.right() + 6. * s, input.y, 62. * s, input.h),
            "Send",
            true,
            view.chat_enabled && !self.draft.trim().is_empty(),
            s * 0.72,
        );
        if (send || (self.typing && !self.just_opened && is_key_pressed(KeyCode::Enter)))
            && !self.draft.trim().is_empty()
        {
            outgoing = Some(self.draft.trim().to_owned());
        }
        label(
            "Enter sends · Esc closes · mutes only affect what you hear",
            chat.x,
            chat.y + 332. * s,
            10. * s,
            muted_color(),
        );
        self.just_opened = false;
        // Status and actions.
        // A microphone that failed says why here, beside its button.
        let voice_problem = view.voice.problem.filter(|_| self.notice.is_empty());
        let status = if let Some(problem) = voice_problem {
            problem
        } else if self.notice.is_empty() {
            if network.is_empty() {
                "Invite-only party. The match keeps running while you browse menus."
            } else {
                network
            }
        } else {
            &self.notice
        };
        for (index, line) in wrapped_lines(status, r.w - 44. * s, 12. * s)
            .into_iter()
            .take(2)
            .enumerate()
        {
            label(
                &line,
                x,
                y + (448. + index as f32 * 16.) * s,
                12. * s,
                if self.notice.is_empty() && voice_problem.is_none() {
                    muted_color()
                } else {
                    text_color()
                },
            );
        }
        let by = r.bottom() - 55. * s;
        if self.confirming_leave {
            label(
                if view.members.iter().any(|m| m.client == view.me && m.leader) {
                    "Leaving ends this party for everyone."
                } else {
                    "Leave this party?"
                },
                x,
                by - 12. * s,
                12. * s,
                crate::ui::RED,
            );
            if button(
                Rect::new(x, by, 144. * s, 34. * s),
                "Leave party",
                false,
                true,
                s * 0.8,
            ) {
                action = Action::LeaveParty;
            }
            if small_button(
                Rect::new(x + 154. * s, by, 90. * s, 34. * s),
                "Cancel",
                s * 0.8,
            ) {
                self.confirming_leave = false;
            }
        } else {
            if small_button(Rect::new(x, by, 106. * s, 34. * s), "Leave party", s * 0.75) {
                self.confirming_leave = true;
            }
            if button(
                Rect::new(x + 116. * s, by, 132. * s, 34. * s),
                "Copy invite",
                false,
                has_public_invite,
                s * 0.8,
            ) {
                action = Action::CopyParty;
            }
            if button(
                Rect::new(r.right() - 294. * s, by, 124. * s, 34. * s),
                "Invites / IP",
                false,
                true,
                s * 0.8,
            ) {
                self.close();
                action = Action::Page(crate::menu::Page::Party);
            }
            if button(
                Rect::new(r.right() - 160. * s, by, 138. * s, 34. * s),
                if in_menu {
                    "Enter match"
                } else {
                    "Back to match"
                },
                true,
                true,
                s * 0.8,
            ) {
                self.close();
                action = Action::ResumeParty;
            }
        }
        (action, outgoing)
    }
}

/// Everything the party overlay shows, independent of the network client.
pub struct PartyView<'a> {
    pub members: &'a [PartyMember],
    pub me: u64,
    pub messages: &'a std::collections::VecDeque<tankyard_protocol::ChatMessage>,
    pub revision: u64,
    pub chat_enabled: bool,
    /// The match the party shares, for each member's level and health.
    pub snapshot: Option<&'a Snapshot>,
    /// Whether this player may remove others, normally the host.
    pub can_kick: bool,
    /// Who's talking, and this player's microphone.
    pub voice: VoiceView<'a>,
}

fn panel_layout(width: f32, height: f32, requested: f32) -> (Rect, f32) {
    let s = requested
        .min((width - 24.).max(1.) / 720.)
        .min((height - 24.).max(1.) / 560.);
    (
        Rect::new(
            (width - 720. * s) * 0.5,
            (height - 560. * s) * 0.5,
            720. * s,
            560. * s,
        ),
        s,
    )
}
fn rail_columns(members: usize, dock_right: bool) -> usize {
    if dock_right { members.clamp(2, 4) } else { 2 }
}
fn rail_geometry(
    width: f32,
    height: f32,
    members: usize,
    requested: f32,
    menu: bool,
    dock_right: bool,
) -> (Rect, f32) {
    let columns = rail_columns(members, dock_right);
    let rows = members.min(8).div_ceil(columns).max(1);
    let width_units = 8. + 36. * columns as f32;
    let units = 43. + rows as f32 * 36. + RAIL_MIC;
    let s = if menu {
        requested
    } else if dock_right {
        requested.min(
            (crate::ui_layout::staff_button(width, height, requested).y - 278. * requested)
                .max(24.)
                / units,
        )
    } else {
        requested.min((height - 87. * requested - 265. * requested).max(48.) / units)
    };
    let h = units * s;
    (
        Rect::new(
            if menu || dock_right {
                width - 22. * requested - width_units * s
            } else {
                22. * requested
            },
            if menu {
                (height - h) * 0.5
            } else if dock_right {
                270. * requested
            } else {
                91. * requested
            },
            width_units * s,
            h,
        ),
        s,
    )
}
/// The microphone button in a rail drawn at `rect`, just above its hint.
fn rail_mic(rect: Rect, s: f32) -> Rect {
    Rect::new(
        rect.x + 6. * s,
        rect.bottom() - (RAIL_MIC + 19.) * s,
        rect.w - 12. * s,
        20. * s,
    )
}
fn rail_layout(
    width: f32,
    height: f32,
    members: usize,
    s: f32,
    menu: bool,
    dock_right: bool,
) -> Rect {
    rail_geometry(width, height, members, s, menu, dock_right).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(client: u64, name: &str) -> PartyMember {
        PartyMember {
            client,
            player: client + 100,
            name: name.into(),
            class: "vector".into(),
            leader: client == 1,
            in_menu: false,
            ready: false,
        }
    }

    #[test]
    fn the_roster_announces_who_came_and_went_but_not_the_first_one() {
        let mut party = PartyUi::default();
        let first = [member(1, "Host"), member(2, "Guest")];
        party.note_roster(&first, 2, 0.);
        assert!(party.arrivals.is_empty(), "joining announces nobody");
        let more = [member(1, "Host"), member(2, "Guest"), member(3, "Friend")];
        party.note_roster(&more, 2, 1.);
        assert_eq!(party.arrivals.back().unwrap().0, "Friend joined the party");
        party.note_roster(&first, 2, 2.);
        assert_eq!(party.arrivals.back().unwrap().0, "Friend left the party");
        party.note_roster(&first, 2, 2. + NOTICE_SECONDS + 1.);
        assert!(party.arrivals.is_empty(), "notices fade");
    }

    #[test]
    fn a_kick_awaiting_confirmation_is_dropped_when_that_member_leaves() {
        let mut party = PartyUi::default();
        let all = [member(1, "Host"), member(2, "Guest")];
        party.note_roster(&all, 1, 0.);
        party.confirming_kick = Some(2);
        party.note_roster(&all[..1], 1, 1.);
        assert_eq!(party.confirming_kick, None);
    }

    #[test]
    fn panels_fit_small_windows_and_rail_stays_between_stats_and_header() {
        for (w, h) in [(640., 480.), (800., 600.), (1280., 800.), (2560., 1080.)] {
            for preference in [0.8, 1., 1.4] {
                let s = crate::ui_layout::scale(w, h, preference);
                let (panel, _) = panel_layout(w, h, s);
                assert!(
                    panel.x >= 0. && panel.y >= 0. && panel.right() <= w && panel.bottom() <= h
                );
                for members in 1..=8 {
                    let rail = rail_layout(w, h, members, s, false, false);
                    assert!(
                        rail.y >= 78. * s && rail.bottom() <= h - 247. * s,
                        "{w} {h} {s}"
                    );
                    assert!(rail.x >= 0. && rail.right() <= w);
                    let rail = rail_layout(w, h, members, s, false, true);
                    assert!(
                        rail.bottom() <= crate::ui_layout::staff_button(w, h, s).y - 8. * s + 0.1
                    );
                    // Its microphone sits inside it, clear of the last row.
                    let rows = members.min(8).div_ceil(rail_columns(members, true)).max(1);
                    let scale = rail.h / (43. + rows as f32 * 36. + RAIL_MIC);
                    let mic = rail_mic(rail, scale);
                    assert!(mic.x >= rail.x && mic.right() <= rail.right());
                    assert!(mic.y >= rail.y + (43. + (rows - 1) as f32 * 36. + 16.) * scale - 0.01);
                    assert!(mic.bottom() <= rail.bottom());
                }
            }
        }
    }
}
