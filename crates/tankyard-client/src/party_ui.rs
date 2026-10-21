use crate::{menu::Action, render, ui::*};
use macroquad::prelude::*;
use std::collections::HashSet;
use tankyard_content::Content;
use tankyard_server::NetworkClient;

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
    lines: Vec<String>,
    scroll: usize,
    confirming_leave: bool,
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
    pub fn rail(
        &mut self,
        client: &NetworkClient,
        content: &Content,
        requested: f32,
        menu: bool,
        dock_right: bool,
    ) {
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
        if button(rect, "", false, true, s) {
            self.open = true;
            self.typing = false;
            self.notice.clear();
        }
        center_label("PARTY", rect.center().x, rect.y + 19. * s, 11. * s, CYAN);
        for (index, member) in client.party_members.iter().enumerate() {
            let center = vec2(
                rect.x + (22. + (index % columns) as f32 * 36.) * s,
                rect.y + (43. + (index / columns) as f32 * 36.) * s,
            );
            if let Some(class) = content.class(&member.class) {
                render::class_model(
                    class,
                    center,
                    9. * s,
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
        }
        center_label(
            &if unread > 0 {
                format!("{} new", unread.min(64))
            } else {
                format!("{}/8 · open", client.party_members.len())
            },
            rect.center().x,
            rect.bottom() - 10. * s,
            10. * s,
            if unread > 0 { CYAN } else { muted_color() },
        );
    }
    pub fn panel(
        &mut self,
        client: &mut NetworkClient,
        content: &Content,
        network: &str,
        has_public_invite: bool,
        in_menu: bool,
        requested: f32,
    ) -> Action {
        let (r, s) = panel_layout(screen_width(), screen_height(), requested);
        pointer_scope(840);
        panel(r);
        let x = r.x + 22. * s;
        let y = r.y;
        label("YOUR PARTY", x, y + 35. * s, 22. * s, text_color());
        label(
            &format!(
                "{} / 8 friends · hosted by the leader",
                client.party_members.len()
            ),
            x,
            y + 60. * s,
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
        let left = Rect::new(x, y + 86. * s, 250. * s, 340. * s);
        let chat = Rect::new(
            left.right() + 18. * s,
            left.y,
            r.right() - left.right() - 40. * s,
            340. * s,
        );
        label("MEMBERS", left.x, left.y + 12. * s, 11. * s, muted_color());
        for (index, member) in client.party_members.iter().enumerate() {
            let row = Rect::new(
                left.x,
                left.y + (24. + index as f32 * 38.) * s,
                left.w,
                34. * s,
            );
            draw_rectangle(row.x, row.y, row.w, row.h, alpha(line_color(), 0.16));
            if let Some(class) = content.class(&member.class) {
                render::class_model(
                    class,
                    vec2(row.x + 19. * s, row.center().y),
                    9. * s,
                    -0.4,
                    0.,
                    0.,
                    player_color(),
                    0.,
                );
            }
            let me = member.client == client.client_id();
            let name = format!("{}{}", member.name, if me { " · you" } else { "" });
            label(
                &fitted_label(&name, 12. * s, 128. * s),
                row.x + 42. * s,
                row.y + 14. * s,
                12. * s,
                text_color(),
            );
            label(
                if member.leader {
                    "Leader"
                } else if member.in_menu {
                    "In menus"
                } else {
                    "In match"
                },
                row.x + 42. * s,
                row.y + 28. * s,
                10. * s,
                muted_color(),
            );
            if !me
                && small_button(
                    Rect::new(row.right() - 68. * s, row.y + 4. * s, 64. * s, 26. * s),
                    if self.muted.contains(&member.client) {
                        "Unmute"
                    } else {
                        "Mute"
                    },
                    s * 0.65,
                )
                && !self.muted.remove(&member.client)
            {
                self.muted.insert(member.client);
            }
        }
        label(
            "PARTY CHAT",
            chat.x,
            chat.y + 12. * s,
            11. * s,
            muted_color(),
        );
        let transcript = Rect::new(chat.x, chat.y + 24. * s, chat.w, 258. * s);
        draw_rectangle(
            transcript.x,
            transcript.y,
            transcript.w,
            transcript.h,
            alpha(line_color(), 0.16),
        );
        if self.revision != client.chat_revision || self.scale != s.to_bits() {
            let old = self.lines.len();
            self.lines.clear();
            for message in client
                .chat_messages
                .iter()
                .filter(|message| message.room == "lobby")
            {
                self.lines.extend(wrapped_lines(
                    &format!("{}: {}", message.name, message.text),
                    transcript.w - 20. * s,
                    12. * s,
                ));
            }
            if self.scroll > 0 {
                self.scroll = self
                    .scroll
                    .saturating_add(self.lines.len().saturating_sub(old));
            }
            self.revision = client.chat_revision;
            self.scale = s.to_bits();
        }
        self.last_seen = client.chat_messages.back().map_or(0, |message| message.id);
        let count = 14;
        if contains(transcript) {
            let wheel = mouse_wheel().1;
            let amount = (wheel.abs().ceil() as usize).min(16) * 3;
            if wheel > 0. {
                self.scroll = self.scroll.saturating_add(amount);
            } else if wheel < 0. {
                self.scroll = self.scroll.saturating_sub(amount);
            }
        }
        self.scroll = self.scroll.min(self.lines.len().saturating_sub(count));
        let end = self.lines.len().saturating_sub(self.scroll);
        if self.lines.is_empty() {
            label(
                "Say hello to your party.",
                transcript.x + 10. * s,
                transcript.y + 24. * s,
                12. * s,
                muted_color(),
            );
        }
        for (index, line) in self.lines[end.saturating_sub(count)..end]
            .iter()
            .enumerate()
        {
            label(
                line,
                transcript.x + 10. * s,
                transcript.y + (20. + index as f32 * 17.) * s,
                12. * s,
                text_color(),
            );
        }
        let input = Rect::new(chat.x, chat.y + 294. * s, chat.w - 68. * s, 36. * s);
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
            client.chat_enabled && !self.draft.trim().is_empty(),
            s * 0.72,
        );
        if (send || (self.typing && !self.just_opened && is_key_pressed(KeyCode::Enter)))
            && !self.draft.trim().is_empty()
        {
            if client.send_chat("lobby", self.draft.trim()) {
                self.draft.clear();
                self.scroll = 0;
                self.typing = true;
            } else {
                self.notice = "Message could not be sent. Check the connection.".into();
            }
        }
        self.just_opened = false;
        if let Some(reason) = client.last_action.take() {
            self.notice = reason;
        }
        let status = if self.notice.is_empty() {
            if network.is_empty() {
                "Invite-only party · mutes affect the voice you hear. The match keeps running while you browse menus."
            } else {
                network
            }
        } else {
            &self.notice
        };
        for (index, line) in wrapped_lines(status, r.w - 44. * s, 12. * s)
            .into_iter()
            .take(3)
            .enumerate()
        {
            label(
                &line,
                x,
                y + (449. + index as f32 * 16.) * s,
                12. * s,
                muted_color(),
            );
        }
        let by = r.bottom() - 55. * s;
        if self.confirming_leave {
            label(
                if client
                    .party_members
                    .iter()
                    .any(|m| m.client == client.client_id() && m.leader)
                {
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
                return Action::LeaveParty;
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
                return Action::CopyParty;
            }
            if button(
                Rect::new(r.right() - 294. * s, by, 124. * s, 34. * s),
                "Invites / IP",
                false,
                true,
                s * 0.8,
            ) {
                self.close();
                return Action::Page(crate::menu::Page::Party);
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
                return Action::ResumeParty;
            }
        }
        Action::None
    }
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
    let units = 43. + rows as f32 * 36.;
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
                }
            }
        }
    }
}
