mod about;
mod community;
mod mods_page;
mod play;
mod settings_page;

use crate::ui::RED;
use crate::ui::{
    ui_circle as draw_circle, ui_line as draw_line, ui_rectangle as draw_rectangle,
    ui_rectangle_lines as draw_rectangle_lines,
};
use crate::ui_layout::height as screen_height;
use crate::{
    render,
    settings::{Settings, bindable_pressed},
    ui::*,
};
use macroquad::prelude::*;
use tankyard_client::achievements::{ACHIEVEMENTS, AchievementMode};
use tankyard_content::Content;
use tankyard_core::{Mode, WorldConfig};

// Baselines and button rows in the Display tab. Keep the damn labels out
// of the button hitboxes, even when the whole menu gets scaled down.
const DISPLAY_RESOLUTION_LABEL: f32 = 19.0;
const DISPLAY_RESOLUTION_ROW: f32 = 30.0;
const DISPLAY_RESOLUTION_STEP: f32 = 33.0;
const DISPLAY_FRAME_LABEL: f32 = 215.0;
const DISPLAY_FRAME_ROW: f32 = 226.0;
const DISPLAY_BATTERY_ROW: f32 = 272.0;
const DISPLAY_BATTERY_NOTE: f32 = 318.0;
// The Graphics tab's right column.
const GRAPHICS_AA_LABEL: f32 = 19.0;
const GRAPHICS_AA_ROW: f32 = 30.0;
const GRAPHICS_SHAPE_LABEL: f32 = 86.0;
const GRAPHICS_SHAPE_ROW: f32 = 99.0;
const GRAPHICS_TEXTURE_LABEL: f32 = 152.0;
const GRAPHICS_TEXTURE_ROW: f32 = 163.0;
const GRAPHICS_TEXTURE_NOTE: f32 = 213.0;
const GRAPHICS_RENDERER_ROW: f32 = 240.0;
const GRAPHICS_RENDERER_NOTE: f32 = 292.0;
const GRAPHICS_DEVICE_LABEL: f32 = 253.0;
const GRAPHICS_DEVICE_TEXT: f32 = 275.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Modes,
    Join,
    Host,
    Encyclopedia,
    Settings,
    Achievements,
    Mods,
    Credits,
    Practice,
    Controls,
    Party,
}
pub enum Action {
    None,
    OpenEditor,
    Page(Page),
    Back,
    QuickPlay,
    Offline,
    Host,
    CreateParty,
    ApplyParty,
    JoinParty,
    CopyParty,
    ResumeParty,
    LeaveParty,
    OpenParty,
    PartyReady(bool),
    Join,
    Quit,
    Save,
    ResetSettings,
    PreviewAlarm,
    Practice(usize),
    /// Join the main server, in this lobby.
    JoinLobby(u8),
    StartCommunity,
    StopCommunity,
    JoinCommunity,
}
pub struct Menu {
    pub page: Page,
    pub party_setup: bool,
    pub party_invite: Option<tankyard_server::party::Invite>,
    pub party_address: String,
    pub party_input: String,
    party_focus: bool,
    pub party_roster: Vec<String>,
    pub party_active: bool,
    pub party_leader: bool,
    pub party_configuring: bool,
    pub party_network: String,
    pub mode: Mode,
    pub map: usize,
    pub bots: usize,
    pub difficulty: usize,
    pub width: f32,
    pub height: f32,
    pub seed: u64,
    pub password: String,
    pub name_active: bool,
    pub address_active: bool,
    pub password_active: bool,
    /// The Host page shows the dedicated community server.
    host_community: bool,
    community_fields: [bool; 3],
    community_port: String,
    pub dedicated: crate::hosting::Dedicated,
    /// Why world post-processing fell back to bilinear, if it did.
    pub post_error: Option<String>,
    pub class_index: usize,
    pub tree: bool,
    tree_view: crate::tank_tree::Tree,
    pub settings_tab: usize,
    pub binding: Option<usize>,
    pub error: String,
    pub info: String,
    motion_time: f32,
    pub(crate) ambient: crate::ambient::Ambient,
    scroll_panel: crate::ui_layout::ScrollPanel,
    scroll_page: Option<(Page, usize)>,
    scale_edit: crate::ui_layout::ScaleEdit,
    scale_anchor: Option<f32>,
    pub maps: Vec<tankyard_content::MapDef>,
    mod_entries: Vec<crate::mod_library::Entry>,
    pub mods_scanned: bool,
    mod_path: String,
    mod_path_active: bool,
    mod_review: Option<usize>,
    mod_viewer: Option<crate::mod_viewer::Viewer>,
    mod_trust: bool,
    mod_scroll: usize,
    mod_scroll_fraction: f32,
    /// The pack shown in the Mods page's detail pane.
    mod_selected: usize,
    mod_icons: std::collections::HashMap<String, Texture2D>,
    /// The system file chooser, while it is open.
    mod_chooser: Option<crate::file_dialog::Pending>,
    /// The pack whose Remove button was pressed once and awaits a second press.
    mod_remove: Option<String>,
    /// Ask for staff pack passwords on the next frame.
    pub staff_unlock: bool,
    /// A staff pack was disabled, so tools must reload their keys.
    pub staff_changed: bool,
    /// Enabled pack IDs with no installed copy that loads, and the message
    /// that reported them; it offers to disable them while it is shown.
    mod_missing: Option<(String, Vec<String>)>,
    /// How far the message panel at the bottom raises the page's bottom edge.
    message_lift: f32,
    /// Set when packs changed on disk, so the game reloads their content.
    pub catalog_stale: bool,
    /// Select this pack ID after the next rescan.
    mod_focus: Option<String>,
    /// The main menu as the enabled packs style it.
    style: tankyard_mods::presentation::MenuStyle,
    splash: Option<String>,
    /// Open the first flagged pack's review (screenshot checks only).
    pub preview_review: bool,
    /// Open the file browser on the first script pack (screenshot checks).
    pub preview_files: bool,
    map_page: usize,
    history: Vec<Page>,
    practice_content: Content,
    practice_page: usize,
    pub offline_level: u8,
    dynamic_arena: bool,
    server_browser: crate::server_browser::ServerBrowser,
    /// Live status of servers shown in menus.
    pub servers: crate::server_status::Monitor,
    lobbies_open: bool,
    lobbies_open_amount: f32,
    /// The page of the lobby list on show.
    lobby_page: usize,
    lobby_scroll_fraction: f32,
}
impl Menu {
    pub fn new() -> Self {
        Self {
            page: Page::Main,
            party_setup: false,
            party_invite: None,
            party_address: String::new(),
            party_input: String::new(),
            party_focus: false,
            party_roster: Vec::new(),
            party_active: false,
            party_leader: false,
            party_configuring: false,
            party_network: String::new(),
            mode: Mode::Ffa,
            map: 0,
            bots: 12,
            difficulty: 1,
            width: 3600.0,
            height: 2600.0,
            seed: 42,
            password: String::new(),
            name_active: false,
            address_active: false,
            password_active: false,
            host_community: false,
            community_fields: [false; 3],
            community_port: String::new(),
            dedicated: Default::default(),
            post_error: None,
            class_index: 0,
            tree: true,
            tree_view: crate::tank_tree::Tree::default(),
            settings_tab: 0,
            binding: None,
            error: String::new(),
            info: String::new(),
            motion_time: 0.0,
            ambient: crate::ambient::Ambient::new(),
            scroll_panel: Default::default(),
            scroll_page: None,
            scale_edit: Default::default(),
            scale_anchor: None,
            maps: tankyard_content::builtin_maps(),
            mod_entries: vec![],
            mods_scanned: false,
            mod_path: String::new(),
            mod_path_active: false,
            mod_review: None,
            mod_viewer: None,
            mod_trust: false,
            mod_scroll: 0,
            mod_scroll_fraction: 0.,
            mod_selected: 0,
            mod_icons: Default::default(),
            mod_chooser: None,
            mod_remove: None,
            staff_unlock: false,
            staff_changed: false,
            mod_missing: None,
            message_lift: 0.0,
            catalog_stale: false,
            mod_focus: None,
            style: Default::default(),
            splash: None,
            preview_review: false,
            preview_files: false,
            map_page: 0,
            history: Vec::new(),
            practice_content: Content::builtin(),
            practice_page: 0,
            offline_level: 1,
            dynamic_arena: true,
            server_browser: crate::server_browser::ServerBrowser::default(),
            servers: Default::default(),
            lobbies_open: false,
            lobbies_open_amount: 0.0,
            lobby_page: 0,
            lobby_scroll_fraction: 0.,
        }
    }
    pub fn clear_focus(&mut self) {
        cancel_pointer_capture();
        self.scale_edit.cancel();
        self.scale_anchor = None;
        self.name_active = false;
        self.address_active = false;
        self.password_active = false;
        self.binding = None;
        self.party_focus = false;
    }
    pub fn go(&mut self, page: Page) {
        self.info.clear();
        if self.page != page {
            if self.history.len() == 16 {
                self.history.remove(0);
            }
            self.history.push(self.page);
            self.page = page;
        }
        self.clear_focus();
        self.error.clear();
    }
    pub fn reset(&mut self, page: Page) {
        self.history.clear();
        self.page = page;
        self.clear_focus();
        self.error.clear();
    }
    pub fn back(&mut self) -> bool {
        self.info.clear();
        self.clear_focus();
        self.error.clear();
        if let Some(page) = self.history.pop() {
            self.page = page;
            false
        } else {
            self.page = Page::Main;
            true
        }
    }
    pub fn focus_next(&mut self, reverse: bool) {
        if self.page == Page::Party {
            let address = self.party_invite.is_some();
            let was_address = self.address_active;
            let was_invite = self.party_focus;
            self.clear_focus();
            if address && (was_invite || (!was_address && !reverse)) {
                self.address_active = true;
            } else {
                self.party_focus = true;
            }
            return;
        }
        let fields: &[usize] = match self.page {
            Page::Main => &[0],
            Page::Join => &[0, 1, 2],
            Page::Host => &[0, 2],
            Page::Settings if matches!(self.settings_tab, 4 | 6) => &[0, 1],
            _ => return,
        };
        let active = [self.name_active, self.address_active, self.password_active];
        let current = fields.iter().position(|&index| active[index]);
        let next = current.map_or(if reverse { fields.len() - 1 } else { 0 }, |current| {
            if reverse {
                (current + fields.len() - 1) % fields.len()
            } else {
                (current + 1) % fields.len()
            }
        });
        self.clear_focus();
        match fields[next] {
            0 => self.name_active = true,
            1 => self.address_active = true,
            _ => self.password_active = true,
        }
    }
    pub fn difficulty_name(&self) -> &'static str {
        ["Beginner", "Normal", "Advanced", "Expert"][self.difficulty.min(3)]
    }
    pub fn select_mode(&mut self, mode: Mode) {
        if self.mode != mode
            && matches!(mode, Mode::CoopPve | Mode::BossRush)
            && !self.party_setup
            && !self.party_active
        {
            self.bots = 3;
        }
        self.mode = mode;
        let map = match mode {
            Mode::Maze => "maze",
            Mode::Domination => "domination",
            Mode::Survival => "survival",
            Mode::BossRush | Mode::CoopPve => "boss",
            Mode::TwoTeams
            | Mode::FourTeams
            | Mode::Mothership
            | Mode::CaptureCore
            | Mode::Tag
            | Mode::Infection
            | Mode::KingOfTheHill => "fortress",
            _ => "open",
        };
        self.map = self
            .maps
            .iter()
            .position(|candidate| candidate.id == map)
            .or_else(|| {
                self.maps
                    .iter()
                    .position(|candidate| candidate.id == "open")
            })
            .unwrap_or(0);
        self.map_page = self.map / 8;
    }
    pub fn load_party_settings(&mut self, settings: &tankyard_protocol::MatchSettings) {
        self.mode = settings.mode;
        self.map = self
            .maps
            .iter()
            .position(|map| map.id == settings.map)
            .unwrap_or(0);
        self.map_page = self.map / 8;
        self.bots = settings.bots.clamp(2, 8);
        self.difficulty = ["Beginner", "Normal", "Advanced", "Expert"]
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&settings.difficulty))
            .unwrap_or(1);
        if let Some(party) = &settings.party {
            self.width = f32::from(party.width);
            self.height = f32::from(party.height);
            self.dynamic_arena = party.dynamic_arena;
            self.offline_level = party.starting_level;
        }
    }
    pub fn party_match_settings(&self) -> tankyard_protocol::MatchSettings {
        tankyard_protocol::MatchSettings {
            mode: self.mode,
            map: self.config().map,
            difficulty: self.difficulty_name().into(),
            bots: self.bots.clamp(2, 8),
            party: Some(tankyard_protocol::PartyMatchOptions {
                width: self.width as u16,
                height: self.height as u16,
                dynamic_arena: self.dynamic_arena,
                starting_level: self.offline_level,
            }),
        }
    }
    pub fn config(&self) -> WorldConfig {
        WorldConfig {
            mode: self.mode,
            dynamic_arena: self.dynamic_arena,
            map: self
                .maps
                .get(self.map)
                .map_or("open", |m| m.id.as_str())
                .into(),
            seed: self.seed,
            width: self.width,
            height: self.height,
            ..Default::default()
        }
    }
    /// Restyle the main menu for the enabled packs. A new splash is picked
    /// only when the list of splashes changes.
    pub fn set_presentation(&mut self, presentation: &tankyard_mods::Presentation) {
        let menu = &presentation.menu;
        if menu.splashes != self.style.splashes {
            self.splash = (!menu.splashes.is_empty()).then(|| {
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_nanos() as usize);
                menu.splashes[seed % menu.splashes.len()].clone()
            });
        }
        self.style = menu.clone();
    }
    pub fn draw(
        &mut self,
        content: &Content,
        settings: &mut Settings,
        match_claims: &[String],
        s: f32,
    ) -> Action {
        pointer_scope(1 + self.page as u64);
        // While a pack's review sheet is open, nothing beneath it responds.
        let reviewing =
            self.page == Page::Mods && (self.mod_review.is_some() || self.mod_viewer.is_some());
        let interactive = set_interactive(false);
        set_interactive(interactive && !reviewing);
        if is_key_pressed(KeyCode::Tab) && self.binding.is_none() {
            self.focus_next(is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift));
        }
        self.servers.poll(get_time());
        self.motion_time = if settings.reduced_motion {
            0.0
        } else {
            get_time() as f32
        };
        if self.page == Page::Main {
            let backdrop = crate::graphics::menu_background(128.0 * s, self.motion_time);
            if self.style.ambient != Some(false) {
                self.ambient.draw(settings);
            } else if !backdrop {
                render::menu_grid();
            }
        } else {
            render::menu_grid();
        }
        let w = screen_width();
        let h = screen_height();
        let margin = 40.0 * s;
        let title = self.style.title.as_deref().unwrap_or("Tank Yard");
        if self.page == Page::Main {
            let mut size = 53.0 * s;
            while size > 24.0 * s && label_width(title, size) > w - 260.0 * s {
                size -= 2.0 * s;
            }
            center_label(title, w * 0.5, 78.0 * s, size, text_color());
            if let Some(subtitle) = &self.style.subtitle {
                center_label(
                    &fitted_label(subtitle, 15.0 * s, w - 120.0 * s),
                    w * 0.5,
                    102.0 * s,
                    15.0 * s,
                    muted_color(),
                );
            }
            if let Some(splash) = &self.splash {
                splash_text(
                    splash,
                    w * 0.5 + label_width(title, size) * 0.5 + 10.0 * s,
                    80.0 * s,
                    s,
                    self.motion_time,
                );
            }
        } else {
            let header = self
                .style
                .title
                .as_deref()
                .map_or_else(|| "TANK YARD".to_owned(), str::to_uppercase);
            label(
                &fitted_label(&header, 48.0 * s, w - margin * 2.0 - 180.0 * s),
                margin,
                80.0 * s,
                48.0 * s,
                text_color(),
            );
            draw_line(margin, 126.0 * s, w - margin, 126.0 * s, 1.0, line_color());
        }
        if self.page != Page::Main
            && small_button(
                Rect::new(margin, h - 62.0 * s, 110.0 * s, 34.0 * s),
                "‹  Back",
                s,
            )
        {
            return Action::Back;
        }
        if self.page == Page::Main
            && small_button(
                Rect::new(w - 120.0 * s, 35.0 * s, 80.0 * s, 26.0 * s),
                "Credits",
                s * 0.7,
            )
        {
            return Action::Page(Page::Credits);
        }
        if settings.show_fps {
            right_label(
                &format!("{} FPS", get_fps()),
                w - 18. * s,
                20. * s,
                13. * s,
                muted_color(),
            );
        }
        if self.page == Page::Settings {
            self.settings_tabs(s);
        }
        if self.page == Page::Encyclopedia {
            return self.encyclopedia(content, s);
        }
        let key = (
            self.page,
            if self.page == Page::Settings {
                self.settings_tab
            } else {
                0
            },
        );
        if self.scroll_page != Some(key) {
            self.scroll_panel.reset();
            self.scroll_page = Some(key);
        }
        let top = if self.page == Page::Settings {
            210.
        } else if self.page == Page::Main {
            118.
        } else {
            144.
        } * s;
        let footer_space = if self.page == Page::Main { 12. } else { 78. };
        let message = if self.error.is_empty() {
            &self.info
        } else {
            &self.error
        };
        let message_room = self.message_room(w, margin, s);
        let message_height = if message.is_empty() {
            0.
        } else {
            16. + wrapped_lines(message, message_room, 13. * s).len().min(4) as f32 * 16.
        };
        self.message_lift = if message_height == 0. {
            0.
        } else {
            message_height * s + 6. * s
        };
        let bottom = h - footer_space * s - self.message_lift;
        let anchor = self.scale_anchor.take();
        let canvas = self.scroll_panel.begin(s, top, bottom, anchor);
        let mut action = match self.page {
            Page::Main => self.main(content, settings, s),
            Page::Modes => self.modes(settings, s),
            Page::Join => self.connection(settings, false, s),
            Page::Host => self.connection(settings, true, s),
            Page::Encyclopedia => self.encyclopedia(content, s),
            Page::Settings => self.settings(settings, s),
            Page::Achievements => self.achievements(settings, match_claims, s),
            Page::Mods => self.mods(settings, s),
            Page::Credits => self.credits(s),
            Page::Practice => self.practice(settings, s),
            Page::Controls => self.controls(settings, s),
            Page::Party => self.party(settings, s),
        };
        drop(canvas);
        if reviewing {
            // The file browser can open from the review sheet and returns to it.
            set_interactive(interactive && self.mod_viewer.is_none());
            self.mod_review_overlay(settings, s);
            set_interactive(interactive);
            if let Some(viewer) = &mut self.mod_viewer
                && !viewer.draw(s)
            {
                self.mod_viewer = None;
            }
            return action;
        }
        set_interactive(interactive);
        let footer = match self.page {
            Page::Modes => self.modes_footer(settings, s),
            Page::Controls => self.controls_footer(s),
            Page::Settings => {
                if button(
                    Rect::new(
                        screen_width() - 472.0 * s,
                        screen_height() - 62.0 * s,
                        200.0 * s,
                        35.0 * s,
                    ),
                    "Reset to defaults",
                    false,
                    true,
                    s * 0.85,
                ) {
                    return Action::ResetSettings;
                }
                if button(
                    Rect::new(
                        screen_width() - 260.0 * s,
                        screen_height() - 62.0 * s,
                        220.0 * s,
                        35.0 * s,
                    ),
                    "Save settings",
                    true,
                    true,
                    s * 0.85,
                ) {
                    return Action::Save;
                }
                Action::None
            }
            _ => Action::None,
        };
        if !matches!(footer, Action::None) {
            action = footer;
        }
        if !self.error.is_empty() || !self.info.is_empty() {
            let (message, color) = if self.error.is_empty() {
                (&self.info, accent())
            } else {
                (&self.error, RED)
            };
            let lines = wrapped_lines(message, self.message_room(w, margin, s), 13. * s);
            let height = (16. + lines.len().min(4) as f32 * 16.) * s;
            let y = h - (footer_space + 4.) * s - height;
            panel(Rect::new(margin, y, w - margin * 2., height));
            for (index, line) in lines.into_iter().take(4).enumerate() {
                label(
                    &line,
                    margin + 12. * s,
                    y + (22. + index as f32 * 16.) * s,
                    13. * s,
                    color,
                );
            }
            if let Some(missing) = self.missing_offer().map(<[String]>::to_vec) {
                let switch = Rect::new(
                    w - margin - 56. * s,
                    y + (height - 24. * s) * 0.5,
                    44. * s,
                    24. * s,
                );
                right_label(
                    "Disable?",
                    switch.x - 10. * s,
                    switch.y + 17. * s,
                    13. * s,
                    text_color(),
                );
                let mut on = false;
                if toggle_switch(switch, &mut on, s) {
                    settings.enabled_mods.retain(|id| !missing.contains(id));
                    let _ = settings.save();
                    let ready = crate::mod_library::ready_paths(settings);
                    self.held_mods(&ready.held, ready.missing);
                    self.info = format!("Disabled {}.", missing.join(", "));
                }
            }
        }
        action
    }
    fn main(&mut self, content: &Content, settings: &mut Settings, s: f32) -> Action {
        // Recalculate the centered layout after window and fullscreen changes.
        let w = screen_width();
        let h = screen_height();
        let center = w * 0.5;
        let pane_w = (510.0 * s).min(w - 48.0 * s);
        let x = center - pane_w * 0.5;
        // The lobby list opens inline, so the page shifts up by part of its
        // height instead of the list covering the buttons below it.
        let drop = self.lobbies_open_amount * self.lobby_list_height(s);
        let y = ((h * 0.23).max(139.0 * s) - drop * 0.45).max(160.0 * s);
        let row_h = 44.0 * s;
        // A small preview gives the menu life without overlaying its controls.
        let showcase = self.style.showcase.as_deref().unwrap_or("vector");
        if let Some(basic) = content.class(showcase).or_else(|| content.class("vector")) {
            render::class_model(
                basic,
                crate::ui_layout::position(vec2(center, y - 25.0 * s)),
                25.0 * s,
                -0.4 + (self.motion_time * 0.26).sin() * 0.16,
                0.0,
                0.0,
                player_color(),
                self.motion_time,
            );
        }
        label("NAME", x, y + 17.0 * s, 13.0 * s, muted_color());
        text_field(
            Rect::new(x, y + 27.0 * s, pane_w, row_h),
            &mut settings.player_name,
            &mut self.name_active,
            24,
            s,
            false,
        );
        label("MAIN SERVER", x, y + 102.0 * s, 13.0 * s, muted_color());
        let card = Rect::new(x, y + 113.0 * s, pane_w, 48.0 * s);
        if let Some(action) = self.central_card(card, drop, s) {
            let _ = settings.save();
            return action;
        }
        let play_y = y + 190.0 * s + drop;
        if button(
            Rect::new(x, play_y, pane_w - 162.0 * s, 55.0 * s),
            if self.party_active {
                "Resume party [Enter]"
            } else {
                "Play [Enter]"
            },
            true,
            true,
            s * 1.08,
        ) || is_key_pressed(KeyCode::Enter)
        {
            let _ = settings.save();
            return if self.party_active {
                Action::ResumeParty
            } else {
                Action::QuickPlay
            };
        }
        if button(
            Rect::new(x + pane_w - 152.0 * s, play_y, 152.0 * s, 55.0 * s),
            if self.party_active {
                "Party roster"
            } else {
                "Play offline"
            },
            false,
            true,
            s * 0.9,
        ) {
            return if self.party_active {
                Action::OpenParty
            } else {
                Action::Offline
            };
        }
        let third = (pane_w - 27.0 * s) / 4.0;
        for (index, (title, action)) in [
            ("Practice [F6]", Action::Page(Page::Practice)),
            ("Host server", Action::Page(Page::Host)),
            ("Join server", Action::Page(Page::Join)),
            ("Map editor", Action::OpenEditor),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(
                    x + index as f32 * (third + 9.0 * s),
                    play_y + 65.0 * s,
                    third,
                    41.0 * s,
                ),
                title,
                false,
                true,
                s * 0.85,
            ) {
                let _ = settings.save();
                return action;
            }
        }
        let links_y = play_y + 122.0 * s;
        for (index, (title, page)) in [
            (
                if self.party_active {
                    "Party modes"
                } else {
                    "Offline modes"
                },
                Page::Modes,
            ),
            ("Tank tree", Page::Encyclopedia),
            ("Mods", Page::Mods),
            ("Settings", Page::Settings),
            ("Parties", Page::Party),
        ]
        .into_iter()
        .enumerate()
        {
            // Wider cells for the longer labels keep every title on one line.
            const WEIGHTS: [f32; 5] = [1.3, 1.05, 0.8, 0.95, 0.9];
            let total: f32 = WEIGHTS.iter().sum();
            let before: f32 = WEIGHTS[..index].iter().sum();
            if small_button(
                Rect::new(
                    x + pane_w * before / total,
                    links_y,
                    pane_w * WEIGHTS[index] / total - 6.0 * s,
                    33.0 * s,
                ),
                title,
                s * 0.84,
            ) {
                let _ = settings.save();
                return Action::Page(page);
            }
        }
        let t_y = (links_y + 51.0 * s).min(h - 94.0 * s);
        if small_button(
            Rect::new(
                x + (pane_w - 142.0 * s) * 0.5,
                t_y + 39.0 * s,
                142.0 * s,
                26.0 * s,
            ),
            "Achievements",
            s * 0.75,
        ) {
            return Action::Page(Page::Achievements);
        }
        if small_button(
            Rect::new(x, t_y + 39.0 * s, 131.0 * s, 26.0 * s),
            "Help [F1]",
            s * 0.75,
        ) {
            return Action::Page(Page::Controls);
        }
        if small_button(
            Rect::new(x + pane_w - 86.0 * s, t_y + 39.0 * s, 86.0 * s, 26.0 * s),
            "Quit",
            s * 0.75,
        ) {
            return Action::Quit;
        }
        if small_button(
            Rect::new(x, t_y, 131.0 * s, 30.0 * s),
            if settings.dark_arena {
                "Dark  [F9]"
            } else {
                "Light  [F9]"
            },
            s * 0.78,
        ) {
            settings.dark_arena = !settings.dark_arena;
            let _ = settings.save();
        }
        right_label(
            &format!(
                "{} tanks  ·  {} bots in offline mode",
                content.classes.len(),
                self.bots
            ),
            x + pane_w,
            t_y + 20.0 * s,
            13.0 * s,
            muted_color(),
        );
        let pane = Rect::new(x, y - 60.0 * s, pane_w, t_y + 65.0 * s - (y - 60.0 * s));
        bastion_badge(pane, self.message_lift, self.motion_time, s);
        Action::None
    }
    /// The community server: its settings while stopped, then its live
    /// status, address and console while it runs.
    fn encyclopedia(&mut self, content: &Content, s: f32) -> Action {
        self.tree_view.draw(content, &mut self.class_index, s);
        Action::None
    }
}
/// The Bastion mark and motto from Credits, in the main menu's lower right,
/// above any message. It steps aside on windows too small to show it beside
/// the menu.
fn bastion_badge(pane: Rect, lift: f32, time: f32, s: f32) {
    let w = screen_width();
    let h = screen_height();
    let name = tankyard_server::bastion::NAME;
    let motto = tankyard_server::bastion::TAGLINE;
    let text_width = label_width(name, 14.0 * s).max(label_width(motto, 11.0 * s));
    let badge = Rect::new(
        w - 22.0 * s - (74.0 * s + text_width + 18.0 * s),
        h - 22.0 * s - 66.0 * s - lift,
        74.0 * s + text_width + 18.0 * s,
        66.0 * s,
    );
    let clear_of_menu =
        badge.x >= pane.x + pane.w + 12.0 * s || badge.y >= pane.y + pane.h + 8.0 * s;
    if badge.x < 0.0 || !clear_of_menu {
        return;
    }
    panel(badge);
    tankyard_client::boss_model::boss_model_geometry(
        tankyard_core::BossKind::Bastion,
        crate::ui_layout::position(vec2(badge.x + 36.0 * s, badge.center().y)),
        17.0 * s,
        -0.2,
        crate::accessibility::motion_time(time),
        0,
        None,
        &mut render::draw_model_primitive,
    );
    label(
        name,
        badge.x + 74.0 * s,
        badge.y + 29.0 * s,
        14.0 * s,
        text_color(),
    );
    label(
        motto,
        badge.x + 74.0 * s,
        badge.y + 47.0 * s,
        11.0 * s,
        muted_color(),
    );
}

pub fn mode_blurb(mode: Mode) -> &'static str {
    match mode {
        Mode::Ffa => "Earn score and fight every other tank.",
        Mode::TwoTeams => "Fight with one of two teams.",
        Mode::FourTeams => "Fight with one of four teams.",
        Mode::Maze => "Fight through a connected labyrinth.",
        Mode::Domination => "Capture control points to score.",
        Mode::Survival => "Survive as the arena shrinks.",
        Mode::Tag => "Eliminations convert enemy tanks.",
        Mode::Infection => "Survive the infected or spread the outbreak.",
        Mode::KingOfTheHill => "Capture and defend the moving hill.",
        Mode::Mothership => "Protect your armed command ship.",
        Mode::CaptureCore => "Steal, carry, and return the core.",
        Mode::BossRush => "Cooperate through six boss waves.",
        Mode::Deathmatch => "Get the most kills before time runs out.",
        Mode::Sandbox => "Choose classes and test your build.",
        Mode::RandomEvents => "Fight through rotating arena events.",
        Mode::CoopPve => "Clear ten enemy waves with your team.",
    }
}

/// Height of one lobby row, before scaling.
const LOBBY_ROW: f32 = 31.0;
const LOBBIES_PER_PAGE: usize = 3;
/// Height of the page switcher under a long lobby list.
const LOBBY_PAGER: f32 = 32.0;

fn lobby_pages(lobbies: usize) -> usize {
    lobbies.div_ceil(LOBBIES_PER_PAGE).max(1)
}

/// The open lobby list: a row per lobby up to a page (or one row for a
/// message), plus the page switcher when there's more than one page. Every
/// page is as tall as a full one, so the buttons below stay put.
fn lobby_list_height(lobbies: usize, s: f32) -> f32 {
    let rows = lobbies.clamp(1, LOBBIES_PER_PAGE) as f32;
    let pager = if lobby_pages(lobbies) > 1 {
        LOBBY_PAGER
    } else {
        0.0
    };
    (rows * LOBBY_ROW + pager + 12.0) * s
}

/// A pack's state for its list row and badge.
fn mod_status(entry: &crate::mod_library::Entry, settings: &Settings) -> (&'static str, Color) {
    use tankyard_mods::scan::Verdict;
    let enabled = entry.enabled(settings);
    match entry.verdict() {
        _ if entry.error.is_some() => ("Cannot load", RED),
        _ if entry
            .staff
            .as_ref()
            .is_some_and(|staff| crate::credentials::unlocked(staff.identity)) =>
        {
            ("Unlocked", accent())
        }
        _ if entry.staff.is_some() && enabled => ("Locked", warm_text_color()),
        Verdict::Clean if enabled => ("Enabled", accent()),
        Verdict::Clean => ("Reviewed", Color::new(0.30, 0.64, 0.30, 1.0)),
        Verdict::Review if entry.approved(settings) => ("Accepted", warm_text_color()),
        Verdict::Review => ("Needs review", warm_text_color()),
        Verdict::Dangerous if entry.approved(settings) => ("Accepted risk", RED),
        Verdict::Dangerous => ("Dangerous", RED),
    }
}

/// A pack's splash beside the main menu title: tilted and gently pulsing.
fn splash_text(text: &str, x: f32, y: f32, s: f32, time: f32) {
    let size = 17.0 * s * (1.0 + (time * 5.0).sin().abs() * 0.07);
    let text = fitted_label(text, size, 320.0 * s);
    rotated_label(&text, x, y, size, -0.28, warm_text_color());
}

/// A compact on/off switch; returns true when clicked.
fn toggle_switch(rect: Rect, on: &mut bool, s: f32) -> bool {
    let clicked = contains(rect) && is_mouse_button_pressed(MouseButton::Left);
    if clicked {
        *on = !*on;
    }
    capsule(rect, if *on { accent() } else { line_color() });
    let knob = rect.h * 0.5 - 3.0 * s;
    let cx = if *on {
        rect.right() - rect.h * 0.5
    } else {
        rect.x + rect.h * 0.5
    };
    draw_circle(cx, rect.center().y, knob, WHITE);
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_sections_never_overlap_at_common_screen_sizes() {
        // Label top estimates include the bold font ascenders. A button must
        // not intrude into the label's vertical ink or another hitbox.
        let groups = [
            (
                DISPLAY_RESOLUTION_LABEL - 13.0,
                DISPLAY_RESOLUTION_ROW + 4.0 * DISPLAY_RESOLUTION_STEP + 28.0,
            ),
            (DISPLAY_FRAME_LABEL - 13.0, DISPLAY_FRAME_ROW + 34.0),
            (DISPLAY_BATTERY_ROW, DISPLAY_BATTERY_NOTE + 28.0),
        ];
        assert!(groups.windows(2).all(|pair| pair[0].1 + 6.0 <= pair[1].0));
        let graphics = [
            (GRAPHICS_AA_LABEL - 13.0, GRAPHICS_AA_ROW + 31.0),
            (GRAPHICS_SHAPE_LABEL - 13.0, GRAPHICS_SHAPE_ROW + 31.0),
            (GRAPHICS_TEXTURE_LABEL - 13.0, GRAPHICS_TEXTURE_ROW + 31.0),
            (GRAPHICS_TEXTURE_NOTE - 11.0, GRAPHICS_TEXTURE_NOTE),
        ];
        assert!(graphics.windows(2).all(|pair| pair[0].1 + 6.0 <= pair[1].0));
        for (width, height) in [
            (800.0, 600.0),
            (1024.0, 768.0),
            (1280.0, 800.0),
            (1600.0, 900.0),
            (1920.0, 1080.0),
        ] {
            for setting_scale in [0.8_f32, 1.0, 1.4] {
                let s = crate::ui_layout::scale(width, height, setting_scale);
                let footer_top = height.max(800. * s) - 79. * s;
                let last_button_bottom = (158.0 + 64.0 + groups.last().unwrap().1) * s;
                assert!(
                    last_button_bottom + 16.0 * s < footer_top,
                    "{width}x{height} at {setting_scale}"
                );
                let col = (width - 110.0 * s) * 0.5;
                let right = 40.0 * s + col + 30.0 * s;
                assert!(right + col <= width - 24.0 * s);
                let reset = Rect::new(width - 472. * s, height - 62. * s, 200. * s, 35. * s);
                assert!(reset.x > 150. * s);
                assert!(reset.right() < width - 260. * s);
            }
        }
    }

    #[test]
    fn back_restores_the_previous_page_and_navigation_cancels_field_capture() {
        let mut menu = Menu::new();
        menu.go(Page::Host);
        menu.password_active = true;
        menu.go(Page::Modes);
        assert!(!menu.password_active);
        assert!(!menu.back());
        assert!(menu.page == Page::Host);
        assert!(!menu.back());
        assert!(menu.page == Page::Main);
        menu.reset(Page::Controls);
        menu.binding = Some(2);
        assert!(menu.back());
        assert!(menu.binding.is_none());
        for _ in 0..100 {
            menu.go(Page::Host);
            menu.go(Page::Modes);
        }
        assert!(menu.history.len() <= 16);
    }

    #[test]
    fn tab_navigation_only_visits_visible_fields_and_wraps_both_directions() {
        let mut menu = Menu::new();
        menu.reset(Page::Join);
        menu.focus_next(false);
        assert!(menu.name_active);
        menu.focus_next(false);
        assert!(menu.address_active && !menu.name_active);
        menu.focus_next(false);
        assert!(menu.password_active);
        menu.focus_next(false);
        assert!(menu.name_active);
        menu.focus_next(true);
        assert!(menu.password_active);
        menu.reset(Page::Host);
        menu.focus_next(true);
        assert!(menu.password_active && !menu.address_active);
        menu.focus_next(true);
        assert!(menu.name_active);
        menu.reset(Page::Settings);
        menu.settings_tab = 4;
        menu.focus_next(true);
        assert!(menu.address_active && !menu.password_active);
        menu.settings_tab = 6;
        menu.clear_focus();
        menu.focus_next(false);
        assert!(menu.name_active);
        menu.focus_next(false);
        assert!(menu.address_active);
    }
}
