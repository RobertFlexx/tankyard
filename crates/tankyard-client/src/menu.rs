use crate::ui::RED;
use crate::ui::{
    ui_line as draw_line, ui_rectangle as draw_rectangle,
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
    ambient: crate::ambient::Ambient,
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
    mod_trust: bool,
    mod_scroll: usize,
    /// The pack shown in the Mods page's detail pane.
    mod_selected: usize,
    mod_icons: std::collections::HashMap<String, Texture2D>,
    /// The system file chooser, while it is open.
    mod_chooser: Option<crate::file_dialog::Pending>,
    /// The pack whose Remove button was pressed once and awaits a second press.
    mod_remove: Option<String>,
    /// Set when packs changed on disk, so the game reloads their content.
    pub catalog_stale: bool,
    /// Select this pack ID after the next rescan.
    mod_focus: Option<String>,
    /// The main menu as the enabled packs style it.
    style: tankyard_mods::presentation::MenuStyle,
    splash: Option<String>,
    /// Open the first flagged pack's review (screenshot checks only).
    pub preview_review: bool,
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
    lobby_scroll: f32,
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
            mod_trust: false,
            mod_scroll: 0,
            mod_selected: 0,
            mod_icons: Default::default(),
            mod_chooser: None,
            mod_remove: None,
            catalog_stale: false,
            mod_focus: None,
            style: Default::default(),
            splash: None,
            preview_review: false,
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
            lobby_scroll: 0.0,
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
        let reviewing = self.page == Page::Mods && self.mod_review.is_some();
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
        let message_height = if message.is_empty() {
            0.
        } else {
            16. + wrapped_lines(message, w - margin * 2. - 24. * s, 13. * s)
                .len()
                .min(4) as f32
                * 16.
        };
        let bottom = h
            - footer_space * s
            - if message_height == 0. {
                0.
            } else {
                message_height * s + 6. * s
            };
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
            set_interactive(interactive);
            self.mod_review_overlay(settings, s);
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
            let lines = wrapped_lines(message, (w - margin * 2. - 24. * s).max(1.), 13. * s);
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
        let lobbies = self.lobby_rows();
        let drop = self.lobbies_open_amount * lobby_list_height(lobbies, s);
        let y = ((h * 0.23).max(139.0 * s) - drop * 0.45).max(160.0 * s);
        let row_h = 44.0 * s;
        // A small preview gives the menu life without overlaying its controls.
        let showcase = self.style.showcase.as_deref().unwrap_or("vector");
        if let Some(basic) = content.class(showcase).or_else(|| content.class("vector")) {
            render::class_model(
                basic,
                vec2(center, y - 25.0 * s),
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
        let third = (pane_w - 18.0 * s) / 3.0;
        for (index, (title, page)) in [
            ("Practice [F6]", Page::Practice),
            ("Host server", Page::Host),
            ("Join server", Page::Join),
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
                return Action::Page(page);
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
        bastion_badge(pane, self.motion_time, s);
        Action::None
    }
    /// The community server: its settings while stopped, then its live
    /// status, address and console while it runs.
    fn community_panel(&mut self, settings: &mut Settings, rect: Rect, s: f32) -> Option<Action> {
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
                let saved =
                    crate::hosting::settings(server, self.config(), vec![], self.difficulty_name())
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

    /// Rows the lobby list needs: one per lobby, or one status message.
    fn lobby_rows(&self) -> usize {
        match self.servers.state(crate::settings::main_server()) {
            Some(crate::server_status::State::Online(probe)) => probe.status.lobbies.len().max(1),
            _ => 1,
        }
    }

    /// The main server card: population and connection at a glance; a click
    /// opens the lobby list below it.
    fn central_card(&mut self, card: Rect, drop: f32, s: f32) -> Option<Action> {
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
                self.lobby_scroll = 0.0;
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
            lobby_list_height(self.lobby_rows(), s) - 4.0 * s,
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
        let content = lobbies.len() as f32 * row_h;
        let max_scroll = (content - inner.h).max(0.0);
        if contains(area) {
            self.lobby_scroll -= mouse_wheel().1.clamp(-1.0, 1.0) * row_h;
        }
        self.lobby_scroll = self.lobby_scroll.clamp(0.0, max_scroll);
        let mut chosen = None;
        for (index, lobby) in lobbies.iter().enumerate() {
            let ry = inner.y + index as f32 * row_h - self.lobby_scroll;
            if ry + row_h < inner.y || ry > inner.bottom() {
                continue;
            }
            let row = Rect::new(
                inner.x + 6.0 * s,
                ry,
                inner.w - 12.0 * s - if max_scroll > 0.0 { 8.0 * s } else { 0.0 },
                row_h - 3.0 * s,
            );
            let full = lobby.capacity > 0 && lobby.humans >= lobby.capacity;
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
            let ratio = if lobby.capacity == 0 {
                0.0
            } else {
                f32::from(lobby.humans) / f32::from(lobby.capacity)
            };
            progress(
                Rect::new(meter_x, row.y + 10.0 * s, meter_w, 6.0 * s),
                ratio,
                if full { RED } else { accent() },
            );
            right_label(
                &if full {
                    "Full".to_owned()
                } else {
                    format!("{}/{}", lobby.humans, lobby.capacity)
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
        if max_scroll > 0.0 {
            let track = Rect::new(inner.right() - 7.0 * s, inner.y, 3.0 * s, inner.h);
            draw_rectangle(track.x, track.y, track.w, track.h, alpha(line_color(), 0.6));
            let thumb = (inner.h / content * inner.h).max(16.0 * s);
            let at = self.lobby_scroll / max_scroll * (inner.h - thumb);
            draw_rectangle(track.x, track.y + at, track.w, thumb, muted_color());
        }
        chosen.map(Action::JoinLobby)
    }

    fn modes(&mut self, _settings: &mut Settings, s: f32) -> Action {
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
    fn modes_footer(&mut self, settings: &mut Settings, s: f32) -> Action {
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
    fn party(&mut self, settings: &mut Settings, s: f32) -> Action {
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
    fn practice(&mut self, settings: &Settings, s: f32) -> Action {
        use tankyard_client::practice::{CHALLENGES, Goal, clock};
        for (index, key) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4]
            .into_iter()
            .enumerate()
        {
            if is_key_pressed(key) {
                return Action::Practice(self.practice_page * 4 + index);
            }
        }
        let margin = 40.0 * s;
        label("Practice", margin, 164.0 * s, 26.0 * s, text_color());
        label(
            "Eight challenges. Personal bests are saved on this device.",
            margin,
            190.0 * s,
            15.0 * s,
            muted_color(),
        );
        for page in 0..2 {
            if button(
                Rect::new(
                    screen_width() - (330. - page as f32 * 145.) * s,
                    153. * s,
                    135. * s,
                    32. * s,
                ),
                if page == 0 {
                    "Foundations"
                } else {
                    "Combat trials"
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
                    vec2(r.x + r.w - 66.0 * s, r.y + 68.0 * s),
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
    fn controls(&mut self, settings: &Settings, s: f32) -> Action {
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
    fn controls_footer(&mut self, s: f32) -> Action {
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
    fn connection(&mut self, settings: &mut Settings, host: bool, s: f32) -> Action {
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
                self.community_panel(settings, Rect::new(rx, y - 16.0 * s, rw, 438.0 * s), s)
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
    fn encyclopedia(&mut self, content: &Content, s: f32) -> Action {
        self.tree_view.draw(content, &mut self.class_index, s);
        Action::None
    }
    fn accessibility_preview(&self, r: Rect, settings: &Settings, s: f32) {
        panel(r);
        label(
            "LIVE PREVIEW",
            r.x + 14. * s,
            r.y + 22. * s,
            12. * s,
            muted_color(),
        );
        let time = crate::accessibility::motion_time(get_time() as f32);
        for team in 1..=4 {
            let x = r.x + r.w * (team as f32 - 0.5) / 4.;
            let color = render::team_color(team, false, settings.colorblind);
            if let Some(class) = self.practice_content.class("vector") {
                render::class_model(
                    class,
                    vec2(x, r.y + 65. * s),
                    15. * s,
                    -0.4 + (time * 0.7).sin() * 0.2,
                    0.,
                    0.,
                    color,
                    time,
                );
            }
            if settings.colorblind {
                render::team_marker(team, false, vec2(x, r.y + 70. * s), 3. * s);
            }
            center_label(&format!("Team {team}"), x, r.y + 104. * s, 11. * s, color);
        }
        label(
            "Your tank",
            r.x + 14. * s,
            r.y + 140. * s,
            13. * s,
            render::team_color(0, true, settings.colorblind),
        );
        right_label(
            "Opponent",
            r.x + r.w - 14. * s,
            r.y + 140. * s,
            13. * s,
            render::team_color(0, false, settings.colorblind),
        );
        center_label(
            if settings.reduced_motion {
                "Motion paused"
            } else {
                "Motion preview"
            },
            r.center().x,
            r.y + 167. * s,
            11. * s,
            muted_color(),
        );
    }
    fn settings_tabs(&mut self, s: f32) {
        let x = 40. * s;
        let y = 158. * s;
        let tab_w = (screen_width() - 2. * x - 7. * 8. * s) / 8.;
        // Indices are stable for saved state; Graphics was added last but
        // sits beside Display.
        for (slot, (index, name)) in [
            (0, "Display"),
            (7, "Graphics"),
            (1, "Style"),
            (2, "Controls"),
            (3, "Accessibility"),
            (4, "Profile"),
            (5, "Audio"),
            (6, "Servers"),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(x + slot as f32 * (tab_w + 8.0 * s), y, tab_w, 38.0 * s),
                name,
                self.settings_tab == index,
                true,
                s * 0.85,
            ) {
                self.settings_tab = index;
                self.clear_focus();
            }
        }
    }
    /// Presets, render scale, upscaling and post-processing.
    fn graphics(
        &mut self,
        settings: &mut Settings,
        x: f32,
        by: f32,
        col: f32,
        s: f32,
    ) -> Option<Action> {
        label("QUALITY PRESET", x, by + 19.0 * s, 13.0 * s, muted_color());
        // (name, render scale, upscaler, sharpness, FXAA, shape quality)
        // Macs with MetalFX upscale with it; it needs less sharpening.
        let metalfx = crate::graphics::metalfx_available();
        let (upscaler, performance, balanced) = if metalfx {
            (crate::upscale::METALFX, 0.3, 0.15)
        } else {
            (1, 0.45, 0.3)
        };
        let presets = [
            ("Performance", 0.65, upscaler, performance, false, 0),
            ("Balanced", 0.85, upscaler, balanced, false, 1),
            ("Quality", 1.0, upscaler, 0.0, true, 2),
            ("Ultra", 1.5, 0, 0.15, false, 2),
        ];
        let width = col / presets.len() as f32;
        for (i, (name, scale, upscaler, sharpness, fxaa, shape)) in presets.into_iter().enumerate()
        {
            let current = (settings.render_scale - scale).abs() < 0.01
                && settings.upscaler == upscaler
                && (settings.sharpness - sharpness).abs() < 0.01
                && settings.fxaa == fxaa
                && settings.shape_quality == shape;
            if button(
                Rect::new(
                    x + i as f32 * width,
                    by + 30.0 * s,
                    width - 6.0 * s,
                    33.0 * s,
                ),
                name,
                current,
                true,
                s * 0.72,
            ) {
                settings.render_scale = scale;
                settings.upscaler = upscaler;
                settings.sharpness = sharpness;
                settings.fxaa = fxaa;
                settings.shape_quality = shape;
                return Some(Action::Save);
            }
        }
        let (w, h) = (screen_width(), screen_height());
        let physical = miniquad::window::screen_size();
        let ratio = (physical.0 / w.max(1.0)).max(1.0);
        slider_value(
            Rect::new(x, by + 82.0 * s, col, 48.0 * s),
            "Render scale",
            &mut settings.render_scale,
            [0.5, 2.0],
            0.05,
            s,
            |value| {
                format!(
                    "{:.0}%  ·  {} × {}",
                    value * 100.0,
                    (w * ratio * value).round(),
                    (h * ratio * value).round()
                )
            },
        );
        label(
            if settings.render_scale > 1.0 {
                "Above 100% supersamples: smoother edges, more GPU work."
            } else if settings.render_scale < 1.0 {
                "Below 100% renders fewer pixels, then upscales. Menus stay sharp."
            } else {
                "The arena renders at full resolution."
            },
            x,
            by + 143.0 * s,
            11.0 * s,
            muted_color(),
        );
        label("UPSCALER", x, by + 172.0 * s, 13.0 * s, muted_color());
        let names = &crate::upscale::UPSCALERS[..if metalfx { 3 } else { 2 }];
        let width = col / names.len() as f32;
        for (i, &name) in names.iter().enumerate() {
            if button(
                Rect::new(
                    x + i as f32 * width,
                    by + 183.0 * s,
                    width - 6.0 * s,
                    33.0 * s,
                ),
                if i == 1 { "AMD FSR 1" } else { name },
                // A MetalFX setting from another Mac runs as FSR 1 here.
                usize::from(match settings.upscaler {
                    crate::upscale::METALFX if !metalfx => 1,
                    upscaler => upscaler,
                }) == i,
                true,
                s * 0.72,
            ) {
                settings.upscaler = i as u8;
            }
        }
        slider(
            Rect::new(x, by + 228.0 * s, col, 48.0 * s),
            "Sharpening",
            &mut settings.sharpness,
            0.0,
            1.0,
            s,
        );
        toggle(
            Rect::new(x, by + 290.0 * s, col, 33.0 * s),
            "FXAA edge smoothing",
            &mut settings.fxaa,
            s,
        );
        if let Some(error) = &self.post_error {
            wrap(
                &format!(
                    "Post-processing is unavailable on this device, so the arena uses bilinear filtering. {error}"
                ),
                x,
                by + 348.0 * s,
                col,
                14.0 * s,
                RED,
            );
        }
        None
    }
    fn interface_scale(&mut self, rect: Rect, settings: &mut Settings, s: f32, anchor: f32) {
        let mut preview = self.scale_edit.preview(settings.ui_scale);
        if slider_value(
            rect,
            "Interface scale",
            &mut preview,
            [0.8, 1.4],
            0.01,
            s,
            |value| format!("{:.0}%", value * 100.),
        ) {
            self.scale_edit.stage(preview);
        }
        if self
            .scale_edit
            .commit(&mut settings.ui_scale, active_slider("Interface scale"))
        {
            self.scale_anchor = Some(anchor);
        }
        label(
            "Release to apply",
            rect.x,
            rect.y + 67. * s,
            11. * s,
            muted_color(),
        );
    }
    fn settings(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 158.0 * s;
        let col = (screen_width() - 110.0 * s) * 0.5;
        let rx = x + col + 30.0 * s;
        let by = y + 64.0 * s;
        // The animated background looks nice until a shape wanders under
        // a damn caption. Quiet cards keep controls readable on every tab.
        let card_top = by - 11.0 * s;
        let card_height = (screen_height() - 79.0 * s - card_top).max(0.0);
        for px in [x - 12.0 * s, rx - 12.0 * s] {
            let card = Rect::new(px, card_top, col + 24.0 * s, card_height);
            draw_rectangle(card.x, card.y, card.w, card.h, alpha(surface_color(), 0.55));
            draw_rectangle_lines(card.x, card.y, card.w, card.h, s, alpha(line_color(), 0.55));
        }
        match self.settings_tab {
            0 => {
                if toggle(
                    Rect::new(x, by, col, 33.0 * s),
                    "Fullscreen [F11]",
                    &mut settings.fullscreen,
                    s,
                ) {
                    return Action::Save;
                }
                toggle(
                    Rect::new(x, by + 47.0 * s, col, 33.0 * s),
                    "VSync  (restart required)",
                    &mut settings.vsync,
                    s,
                );
                toggle(
                    Rect::new(x, by + 94.0 * s, col, 33.0 * s),
                    "FPS and network diagnostics",
                    &mut settings.show_fps,
                    s,
                );
                self.interface_scale(
                    Rect::new(x, by + 154.0 * s, col, 48.0 * s),
                    settings,
                    s,
                    408.,
                );
                wrap(
                    "Rendering quality, upscaling and antialiasing are on the Graphics tab.",
                    x,
                    by + 252.0 * s,
                    col,
                    15.0 * s,
                    muted_color(),
                );
                label(
                    "RESOLUTION · render size in fullscreen",
                    rx,
                    by + DISPLAY_RESOLUTION_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                let native_title = crate::native_resolution::current().map_or_else(
                    || "Native (monitor resolution)".to_owned(),
                    |(width, height)| format!("Native · {width} × {height}"),
                );
                if button(
                    Rect::new(rx, by + DISPLAY_RESOLUTION_ROW * s, col, 28. * s),
                    &native_title,
                    settings.native_resolution,
                    true,
                    s * 0.85,
                ) {
                    settings.native_resolution = true;
                    return Action::Save;
                }
                for (index, (width, height)) in
                    [(1024, 768), (1280, 800), (1600, 900), (1920, 1080)]
                        .into_iter()
                        .enumerate()
                {
                    if button(
                        Rect::new(
                            rx,
                            by + (DISPLAY_RESOLUTION_ROW
                                + (index + 1) as f32 * DISPLAY_RESOLUTION_STEP)
                                * s,
                            col,
                            28.0 * s,
                        ),
                        &format!("{width} × {height}"),
                        !settings.native_resolution
                            && settings.width == width
                            && settings.height == height,
                        true,
                        s * 0.85,
                    ) {
                        settings.native_resolution = false;
                        settings.width = width;
                        settings.height = height;
                        return Action::Save;
                    }
                }
                label(
                    "FRAME LIMIT",
                    rx,
                    by + DISPLAY_FRAME_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (index, cap) in [0, 60, 120, 144, 240].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + index as f32 * (col / 5.0),
                            by + DISPLAY_FRAME_ROW * s,
                            col / 5.0 - 7.0 * s,
                            34.0 * s,
                        ),
                        &if cap == 0 {
                            "∞".into()
                        } else {
                            cap.to_string()
                        },
                        settings.frame_cap == cap,
                        true,
                        s * 0.73,
                    ) {
                        settings.frame_cap = cap;
                    }
                }
            }
            7 => {
                if let Some(action) = self.graphics(settings, x, by, col, s) {
                    return action;
                }
                label(
                    "MSAA  (restart to apply)",
                    rx,
                    by + GRAPHICS_AA_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (index, (samples, title)) in [(1, "Off"), (4, "4x MSAA"), (8, "8x MSAA")]
                    .into_iter()
                    .enumerate()
                {
                    if button(
                        Rect::new(
                            rx + index as f32 * (col / 3.0),
                            by + GRAPHICS_AA_ROW * s,
                            col / 3.0 - 6.0 * s,
                            31.0 * s,
                        ),
                        title,
                        crate::graphics::samples(settings.anti_aliasing) == samples,
                        crate::graphics::supports_samples(samples),
                        s * 0.73,
                    ) {
                        settings.anti_aliasing = samples;
                        let _ = settings.save();
                    }
                }
                label(
                    "SHAPE QUALITY",
                    rx,
                    by + GRAPHICS_SHAPE_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (i, title) in ["Low", "Normal", "High"].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + i as f32 * col / 3.0,
                            by + GRAPHICS_SHAPE_ROW * s,
                            col / 3.0 - 6.0 * s,
                            31.0 * s,
                        ),
                        title,
                        usize::from(settings.shape_quality) == i,
                        true,
                        s * 0.72,
                    ) {
                        settings.shape_quality = i as u8;
                    }
                }
                label(
                    "TEXTURE FILTERING",
                    rx,
                    by + GRAPHICS_TEXTURE_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (i, level) in [1, 2, 4, 8, 16].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + i as f32 * col / 5.0,
                            by + GRAPHICS_TEXTURE_ROW * s,
                            col / 5.0 - 6.0 * s,
                            31.0 * s,
                        ),
                        &format!("{level}x"),
                        settings.anisotropic_filtering == level,
                        level <= crate::graphics::max_anisotropy(),
                        s * 0.72,
                    ) {
                        settings.anisotropic_filtering = level;
                    }
                }
                label(
                    if crate::graphics::texture_count() == 0 {
                        "Sharpens textured tanks from mods."
                    } else if crate::graphics::max_anisotropy() <= 1 {
                        "This graphics device supports standard filtering."
                    } else {
                        "Filtering sharpens textured tanks viewed at an angle."
                    },
                    rx,
                    by + GRAPHICS_TEXTURE_NOTE * s,
                    11. * s,
                    muted_color(),
                );
                if cfg!(target_os = "macos") {
                    if toggle(
                        Rect::new(rx, by + GRAPHICS_RENDERER_ROW * s, col, 33.0 * s),
                        "Metal renderer  (restart to apply)",
                        &mut settings.metal,
                        s,
                    ) {
                        let _ = settings.save();
                    }
                    wrap(
                        match (crate::graphics::metal(), settings.metal) {
                            (true, true) => "Rendering with Metal, native to Apple GPUs.",
                            (false, false) => {
                                "Rendering with OpenGL, translated to Metal by macOS. Metal is faster."
                            }
                            (_, true) => "Metal starts after a restart.",
                            (_, false) => "OpenGL starts after a restart.",
                        },
                        rx,
                        by + GRAPHICS_RENDERER_NOTE * s,
                        col,
                        11.0 * s,
                        muted_color(),
                    );
                }
            }
            1 => {
                label("Appearance", x, by + 20.0 * s, 25.0 * s, text_color());
                wrap(
                    "Adjust motion, effects, and colors.",
                    x,
                    by + 57.0 * s,
                    col,
                    20.0 * s,
                    muted_color(),
                );
                if toggle(
                    Rect::new(x, by + 165.0 * s, col, 33.0 * s),
                    "Dark theme [F9]",
                    &mut settings.dark_arena,
                    s,
                ) {
                    let _ = settings.save();
                }
                slider(
                    Rect::new(x, by + 225.0 * s, col, 48.0 * s),
                    "Visual effects",
                    &mut settings.effects,
                    0.0,
                    1.0,
                    s,
                );
                slider(
                    Rect::new(x, by + 305.0 * s, col, 48.0 * s),
                    "Camera shake",
                    &mut settings.shake,
                    0.0,
                    1.0,
                    s,
                );
                slider(
                    Rect::new(rx, by + 50.0 * s, col, 48.0 * s),
                    "Camera smoothing",
                    &mut settings.camera_smoothing,
                    0.0,
                    1.0,
                    s,
                );
            }
            2 => {
                toggle(
                    Rect::new(rx, by, col, 33.0 * s),
                    "Auto fire",
                    &mut settings.auto_fire,
                    s,
                );
                let names = [
                    "Move forward",
                    "Move backward",
                    "Move left",
                    "Move right",
                    "Active ability",
                    "Toggle auto fire",
                ];
                for (index, name) in names.iter().enumerate() {
                    let r = Rect::new(x, by + index as f32 * 53.0 * s, col, 42.0 * s);
                    if button(r, name, self.binding == Some(index), true, s * 0.9) {
                        self.binding = Some(index);
                    }
                    right_label(
                        if self.binding == Some(index) {
                            "PRESS A KEY"
                        } else {
                            &settings.bindings[index]
                        },
                        x + col - 16.0 * s,
                        r.y + 28.0 * s,
                        16.0 * s,
                        accent(),
                    );
                }
                if let Some(index) = self.binding
                    && let Some(binding) = bindable_pressed()
                {
                    settings.bindings[index] = binding;
                    self.binding = None;
                }
                wrap(
                    "Aim with the mouse. Left click fires and directs drones to the cursor. Right click recalls drones. Keys 1–8 buy stats; hold Shift to fill a stat. Tab opens tank upgrades. Escape pauses.",
                    rx,
                    by + 86.0 * s,
                    col,
                    21.0 * s,
                    text_color(),
                );
                wrap(
                    "Click a control, then press a letter, arrow key, Space, LeftShift, or LeftControl.",
                    rx,
                    by + 256.0 * s,
                    col,
                    18.0 * s,
                    muted_color(),
                );
                if small_button(
                    Rect::new(rx, by + 350.0 * s, col, 36.0 * s),
                    "Restore default bindings",
                    s * 0.9,
                ) {
                    settings.bindings = Settings::default().bindings;
                    self.binding = None;
                }
            }
            3 => {
                toggle(
                    Rect::new(x, by, col, 33.0 * s),
                    "Colorblind palette",
                    &mut settings.colorblind,
                    s,
                );
                toggle(
                    Rect::new(x, by + 58.0 * s, col, 33.0 * s),
                    "Reduced motion",
                    &mut settings.reduced_motion,
                    s,
                );
                toggle(
                    Rect::new(x, by + 116.0 * s, col, 33.0 * s),
                    "High-contrast labels",
                    &mut settings.high_contrast,
                    s,
                );
                self.interface_scale(
                    Rect::new(x, by + 188.0 * s, col, 48.0 * s),
                    settings,
                    s,
                    442.,
                );
                wrap(
                    "Reduced motion stops decorative animation, particles, camera easing and shake. Combat movement and attack warnings stay visible.",
                    rx,
                    by + 20.0 * s,
                    col,
                    17.0 * s,
                    text_color(),
                );
                crate::accessibility::configure(
                    settings.colorblind,
                    settings.high_contrast,
                    settings.reduced_motion,
                );
                self.accessibility_preview(
                    Rect::new(rx, by + 112. * s, col, 180. * s),
                    settings,
                    s,
                );
                label("LOCAL PROFILE", rx, by + 310.0 * s, 13.0 * s, accent());
                label(
                    &format!(
                        "{} matches  ·  {} kills  ·  Best {}",
                        settings.games_played, settings.total_kills, settings.highest_score
                    ),
                    rx,
                    by + 343.0 * s,
                    18.0 * s,
                    text_color(),
                );
            }
            5 => {
                label("Proximity voice", x, by + 20. * s, 25. * s, text_color());
                wrap(
                    "Nearby players become quieter as they move away. Your microphone starts muted. Use M or the microphone button to talk, and headphones to avoid feedback.",
                    x,
                    by + 60. * s,
                    col,
                    18. * s,
                    muted_color(),
                );
                slider(
                    Rect::new(rx, by + 30. * s, col, 48. * s),
                    "Master volume",
                    &mut settings.master_volume,
                    0.,
                    1.,
                    s,
                );
                slider(
                    Rect::new(rx, by + 120. * s, col, 48. * s),
                    "Nearby voices",
                    &mut settings.voice_volume,
                    0.,
                    1.,
                    s,
                );
                slider(
                    Rect::new(rx, by + 210. * s, col, 48. * s),
                    "Boss alarm",
                    &mut settings.effects_volume,
                    0.,
                    1.,
                    s,
                );
                if button(
                    Rect::new(rx, by + 294. * s, col, 34. * s),
                    "Preview boss alarm",
                    false,
                    true,
                    s * 0.8,
                ) {
                    return Action::PreviewAlarm;
                }
                wrap(
                    "Voice is available online while alive. Microphone and speaker devices follow your operating system's defaults. Server mutes also apply to voice.",
                    rx,
                    by + 352. * s,
                    col,
                    18. * s,
                    muted_color(),
                );
            }
            _ => {
                label("PLAYER NAME", x, by + 15.0 * s, 13.0 * s, muted_color());
                text_field(
                    Rect::new(x, by + 29.0 * s, col, 44.0 * s),
                    &mut settings.player_name,
                    &mut self.name_active,
                    24,
                    s,
                    false,
                );
                label(
                    "DEFAULT SERVER ADDRESS",
                    x,
                    by + 116.0 * s,
                    13.0 * s,
                    muted_color(),
                );
                text_field(
                    Rect::new(x, by + 130.0 * s, col, 44.0 * s),
                    &mut settings.server_address,
                    &mut self.address_active,
                    128,
                    s,
                    false,
                );
                wrap(
                    "Choose Favorites or Recent. Sort by address, move favorites with the arrows, or remove individual entries with ×.",
                    x,
                    by + 220. * s,
                    col,
                    18. * s,
                    muted_color(),
                );
                if let Err(error) = self.server_browser.draw(
                    settings,
                    &mut self.servers,
                    Rect::new(rx, by, col, (card_height - 22. * s).max(300. * s)),
                    s,
                ) {
                    self.error = error;
                }
                wrap(
                    "Settings file",
                    x,
                    by + 304.0 * s,
                    col,
                    19.0 * s,
                    muted_color(),
                );
                wrap(
                    &Settings::path().display().to_string(),
                    x,
                    by + 337.0 * s,
                    col,
                    13.0 * s,
                    muted_color(),
                );
            }
        }
        Action::None
    }
    fn credits(&self, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 177.0 * s;
        let width = screen_width() - 80.0 * s;
        label("Credits", x, y, 32.0 * s, text_color());
        label(
            "Made by RobertFlexx.",
            x,
            y + 52.0 * s,
            26.0 * s,
            text_color(),
        );
        wrap(
            "Inspired by Diep.io and Arras.io.",
            x,
            y + 94.0 * s,
            width.min(860.0 * s),
            20.0 * s,
            muted_color(),
        );
        label("BUILT WITH", x, y + 168.0 * s, 13.0 * s, accent());
        label(
            "Rust  /  Macroquad  /  Renet + Netcode  /  CPAL audio",
            x,
            y + 204.0 * s,
            23.0 * s,
            text_color(),
        );
        label("LICENSE", x, y + 264.0 * s, 13.0 * s, accent());
        label(
            "MIT or Apache-2.0",
            x,
            y + 300.0 * s,
            23.0 * s,
            text_color(),
        );
        wrap(
            "DejaVu Sans Bold font; Lucide microphone icons (ISC). Third-party licenses are in NOTICE.md.",
            x,
            y + 355.0 * s,
            width.min(900.0 * s),
            18.0 * s,
            muted_color(),
        );
        wrap(
            "Tank Yard is not affiliated with Diep.io or Arras.io.",
            x,
            y + 411.0 * s,
            width.min(900.0 * s),
            18.0 * s,
            muted_color(),
        );
        tankyard_client::boss_model::boss_model_geometry(
            tankyard_core::BossKind::Bastion,
            vec2(x + 28. * s, y + 488. * s),
            22. * s,
            -0.2,
            crate::accessibility::motion_time(get_time() as f32),
            0,
            None,
            &mut render::draw_model_primitive,
        );
        label(
            &fitted_label(
                &format!(
                    "Preventing cheaters thanks to {}",
                    tankyard_server::bastion::NAME
                ),
                13. * s,
                (width - 80. * s).max(1.),
            ),
            x + 68. * s,
            y + 480. * s,
            13. * s,
            text_color(),
        );
        label(
            tankyard_server::bastion::TAGLINE,
            x + 68. * s,
            y + 505. * s,
            11. * s,
            muted_color(),
        );
        Action::None
    }
    fn achievements(&self, settings: &mut Settings, match_claims: &[String], s: f32) -> Action {
        let x = 40.0 * s;
        let y = 169.0 * s;
        label("Achievements", x, y, 30.0 * s, text_color());
        for (i, (mode, name)) in [
            (AchievementMode::PerMatch, "Per match"),
            (AchievementMode::Global, "Global"),
            (AchievementMode::Off, "Off"),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(x + i as f32 * 137.0 * s, y + 20.0 * s, 125.0 * s, 32.0 * s),
                name,
                settings.achievement_mode == mode,
                true,
                s,
            ) && settings.achievement_mode != mode
            {
                settings.achievement_mode = mode;
                return Action::Save;
            }
        }
        let mode = settings.achievement_mode;
        let unlocked = if mode == AchievementMode::Global {
            settings.unlocked_achievements.as_slice()
        } else {
            match_claims
        };
        right_label(
            &format!(
                "{} / {} unlocked",
                if mode == AchievementMode::Off {
                    0
                } else {
                    unlocked.len()
                },
                ACHIEVEMENTS.iter().filter(|a| a.available(mode)).count()
            ),
            screen_width() - x,
            y,
            18.0 * s,
            muted_color(),
        );
        label(
            match mode {
                AchievementMode::PerMatch => {
                    "Reset each match. Sandbox and modded games do not count."
                }
                AchievementMode::Global => {
                    "Saved on this device across matches. Sandbox and modded games do not count."
                }
                AchievementMode::Off => "Achievements are off. Your saved global records are kept.",
            },
            x,
            y + 77.0 * s,
            15.0 * s,
            muted_color(),
        );
        let gap = 12.0 * s;
        let width = (screen_width() - x * 2.0 - gap * 2.0) / 3.0;
        let height = ((screen_height() - y - 183.0 * s) / 5.0 - gap).min(85.0 * s);
        for (i, a) in ACHIEVEMENTS
            .iter()
            .filter(|a| a.available(mode))
            .enumerate()
        {
            let earned = unlocked.iter().any(|id| id == a.id);
            let r = Rect::new(
                x + (i % 3) as f32 * (width + gap),
                y + 96.0 * s + (i / 3) as f32 * (height + gap),
                width,
                height,
            );
            let color = if earned { card_color(i) } else { line_color() };
            draw_rectangle(
                r.x,
                r.y,
                r.w,
                r.h,
                mix(surface_color(), color, if earned { 0.22 } else { 0.12 }),
            );
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                2.0 * s,
                if earned {
                    mix(color, text_color(), 0.25)
                } else {
                    line_color()
                },
            );
            achievement_badge(
                vec2(r.x + 29.0 * s, r.y + r.h * 0.5),
                19.0 * s,
                color,
                earned,
            );
            label(
                a.title,
                r.x + 58.0 * s,
                r.y + 27.0 * s,
                18.0 * s,
                if earned { text_color() } else { muted_color() },
            );
            wrap(
                a.description,
                r.x + 58.0 * s,
                r.y + 49.0 * s,
                r.w - 71.0 * s,
                12.0 * s,
                muted_color(),
            );
        }
        Action::None
    }
    fn mods(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 169.0 * s;
        let w = screen_width() - x * 2.0;
        label("Mods", x, y, 30.0 * s, text_color());
        label(
            "Lua script packs and data packs for offline games and servers you host.",
            x,
            y + 31.0 * s,
            16.0 * s,
            muted_color(),
        );
        if small_button(
            Rect::new(
                screen_width() - 165.0 * s,
                // Below the page's scroll clip, level with the title.
                y - 20.0 * s,
                125.0 * s,
                30.0 * s,
            ),
            "Refresh",
            s,
        ) {
            self.mods_scanned = false;
            self.catalog_stale = true;
        }
        if small_button(
            Rect::new(
                screen_width() - 375.0 * s,
                y - 20.0 * s,
                200.0 * s,
                30.0 * s,
            ),
            "Open mods folder",
            s,
        ) {
            let folder = crate::mod_library::library_dir();
            match crate::file_dialog::reveal(&folder) {
                Ok(()) => {
                    self.info = format!(
                        "Opened {}. Drop pack folders or .zip files there, then press Refresh.",
                        folder.display()
                    )
                }
                Err(error) => self.error = error,
            }
        }
        if let Some(result) = self.mod_chooser.as_ref().and_then(|chooser| chooser.poll()) {
            self.mod_chooser = None;
            match result {
                Ok(Some(path)) => self.import_result(settings, crate::mod_library::import(&path)),
                Ok(None) => {}
                Err(error) => self.error = error,
            }
        }
        if !self.mods_scanned {
            self.mods_scanned = true;
            self.mod_review = None;
            self.mod_entries = crate::mod_library::discover();
            self.mod_icons.clear();
            self.mod_selected = self
                .mod_selected
                .min(self.mod_entries.len().saturating_sub(1));
            if let Some(focus) = self.mod_focus.take()
                && let Some(index) = self
                    .mod_entries
                    .iter()
                    .position(|entry| entry.id() == Some(focus.as_str()))
            {
                self.mod_selected = index;
                let visible_from = index.saturating_sub(3);
                self.mod_scroll = self.mod_scroll.min(index).max(visible_from);
            }
            if self.preview_review {
                self.mod_review = self
                    .mod_entries
                    .iter()
                    .position(|entry| entry.verdict() != tankyard_mods::scan::Verdict::Clean);
            }
        }
        // Import row.
        let import_y = y + 52.0 * s;
        label(
            "ADD A MOD PACK",
            x,
            import_y + 12.0 * s,
            11.0 * s,
            muted_color(),
        );
        let choosing = self.mod_chooser.is_some();
        if button(
            Rect::new(x, import_y + 20.0 * s, 190.0 * s, 38.0 * s),
            if choosing {
                "Choosing…"
            } else {
                "Browse for a pack…"
            },
            true,
            !choosing,
            s * 0.82,
        ) {
            self.mod_chooser = Some(crate::file_dialog::choose_mod_pack());
            self.error.clear();
            self.info = "Pick a .zip pack, or the mod.json inside a pack folder.".into();
        }
        let field_x = x + 200.0 * s;
        let field_w = (w - 336.0 * s).min(560.0 * s);
        text_field(
            Rect::new(field_x, import_y + 20.0 * s, field_w, 38.0 * s),
            &mut self.mod_path,
            &mut self.mod_path_active,
            512,
            s * 0.8,
            false,
        );
        if button(
            Rect::new(
                field_x + field_w + 10.0 * s,
                import_y + 20.0 * s,
                120.0 * s,
                38.0 * s,
            ),
            "Import",
            !self.mod_path.trim().is_empty(),
            !self.mod_path.trim().is_empty(),
            s * 0.85,
        ) {
            let path = self.mod_path.trim().trim_matches('"').to_owned();
            self.import_result(
                settings,
                crate::mod_library::import(std::path::Path::new(&path)),
            );
            self.mod_path.clear();
        }
        label(
            "Or paste a .zip or pack folder path, or drop a .zip onto this window. Every pack is reviewed before it can run.",
            x,
            import_y + 76.0 * s,
            12.0 * s,
            muted_color(),
        );
        // Library list on the left, the selected pack's details on the right.
        let area_y = import_y + 92.0 * s;
        let area_h = (screen_height() - area_y - 112.0 * s).max(120.0 * s);
        let list_w = (w * 0.42).clamp(260.0 * s, 470.0 * s);
        let list = Rect::new(x, area_y, list_w, area_h);
        let detail = Rect::new(x + list_w + 14.0 * s, area_y, w - list_w - 14.0 * s, area_h);
        let row_h = 56.0 * s;
        let visible = ((list.h + 6.0 * s) / (row_h + 6.0 * s)).floor().max(1.0) as usize;
        if contains(list) && self.mod_review.is_none() {
            let wheel = mouse_wheel().1;
            if wheel < 0. {
                self.mod_scroll += 1;
            } else if wheel > 0. {
                self.mod_scroll = self.mod_scroll.saturating_sub(1);
            }
        }
        self.mod_scroll = self
            .mod_scroll
            .min(self.mod_entries.len().saturating_sub(visible));
        if self.mod_entries.is_empty() {
            panel(list);
            wrap(
                "No packs yet. Browse for a pack, drop a .zip onto this window, or put pack folders in the mods folder.",
                list.x + 16.0 * s,
                list.y + 30.0 * s,
                list.w - 32.0 * s,
                15.0 * s,
                muted_color(),
            );
            panel(detail);
            label(
                "Make your own",
                detail.x + 22.0 * s,
                detail.y + 36.0 * s,
                20.0 * s,
                text_color(),
            );
            wrap(
                "A pack is a folder with mod.json, optional Lua scripts and art, and a tankyard.toml with its name, creator, version, icon and links. See docs/modding.md.",
                detail.x + 22.0 * s,
                detail.y + 66.0 * s,
                detail.w - 44.0 * s,
                15.0 * s,
                muted_color(),
            );
        }
        let mut toggle = None;
        for (slot, index) in (self.mod_scroll..self.mod_entries.len())
            .take(visible)
            .enumerate()
        {
            let entry = &self.mod_entries[index];
            let r = Rect::new(
                list.x,
                list.y + slot as f32 * (row_h + 6.0 * s),
                list.w,
                row_h,
            );
            let enabled = entry.enabled(settings);
            let selected = self.mod_selected == index;
            // An outline marks the selection, keeping status colors readable.
            if button(r, "", false, true, s) {
                self.mod_selected = index;
            }
            if selected {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.5 * s, accent());
            }
            if enabled {
                draw_rectangle(r.x, r.y, 4.0 * s, r.h, accent());
            }
            let icon = Rect::new(r.x + 12.0 * s, r.y + 8.0 * s, 40.0 * s, 40.0 * s);
            self.mod_icon(index, icon, s);
            let entry = &self.mod_entries[index];
            let text_x = icon.right() + 12.0 * s;
            let switch = Rect::new(r.right() - 58.0 * s, r.y + 15.0 * s, 46.0 * s, 26.0 * s);
            let text_w = switch.x - text_x - 10.0 * s;
            let version = entry
                .version()
                .map(|v| format!("  v{v}"))
                .unwrap_or_default();
            label(
                &fitted_label(&format!("{}{version}", entry.name()), 16.0 * s, text_w),
                text_x,
                r.y + 24.0 * s,
                16.0 * s,
                text_color(),
            );
            let (status, color) = mod_status(entry, settings);
            let byline = match (&entry.error, entry.creator()) {
                (Some(_), _) => "Cannot load".to_owned(),
                (None, Some(creator)) => format!("{status} · by {creator}"),
                (None, None) => status.to_owned(),
            };
            label(
                &fitted_label(&byline, 12.0 * s, text_w),
                text_x,
                r.y + 43.0 * s,
                12.0 * s,
                if entry.error.is_some() { RED } else { color },
            );
            let mut on = enabled;
            if entry.error.is_none() && toggle_switch(switch, &mut on, s) {
                self.mod_selected = index;
                if !enabled && !entry.approved(settings) {
                    self.mod_review = Some(index);
                    self.mod_trust = false;
                } else {
                    toggle = Some(index);
                }
            }
        }
        if let Some(index) = toggle {
            self.toggle_mod(settings, index);
        }
        if !self.mod_entries.is_empty() {
            self.mod_details(settings, detail, s);
        }
        label(
            &fitted_label(
                &format!(
                    "Library: {}  ·  Scripts run in a LuaJIT sandbox with no file, network or system access.",
                    crate::mod_library::library_dir().display()
                ),
                13.0 * s,
                w,
            ),
            x,
            screen_height() - 93.0 * s,
            13.0 * s,
            muted_color(),
        );
        Action::None
    }
    /// A pack's icon, or a tile with its initial when it has none.
    fn mod_icon(&mut self, index: usize, rect: Rect, s: f32) {
        let entry = &self.mod_entries[index];
        let key = format!("{}#{}", entry.path, entry.digest);
        if let Some(data) = &entry.icon
            && !self.mod_icons.contains_key(&key)
        {
            let texture = Texture2D::from_rgba8(data.width, data.height, &data.rgba);
            texture.set_filter(FilterMode::Linear);
            self.mod_icons.insert(key.clone(), texture);
        }
        if let Some(texture) = self.mod_icons.get(&key) {
            // Fit inside the tile, keeping the icon's shape.
            let (tw, th) = (texture.width().max(1.0), texture.height().max(1.0));
            let scale = (rect.w / tw).min(rect.h / th);
            let size = vec2(tw * scale, th * scale);
            draw_texture_ex(
                texture,
                rect.center().x - size.x * 0.5,
                rect.center().y - size.y * 0.5,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
            return;
        }
        let name = entry.name();
        // A stable color per pack from its name.
        let hue = name
            .bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
        let palette = [
            CYAN,
            RED,
            crate::ui::VIOLET,
            AMBER,
            Color::new(0.33, 0.78, 0.45, 1.0),
        ];
        let color = palette[hue as usize % palette.len()];
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0 * s, alpha(BLACK, 0.25));
        let initial: String = name
            .chars()
            .next()
            .map_or('?', |c| c.to_ascii_uppercase())
            .to_string();
        center_label(
            &initial,
            rect.center().x,
            rect.center().y + rect.h * 0.2,
            rect.h * 0.55,
            WHITE,
        );
    }

    /// The selected pack: who made it, what it contains, where it lives, and
    /// whether it is safe to run.
    fn mod_details(&mut self, settings: &mut Settings, r: Rect, s: f32) {
        let index = self.mod_selected.min(self.mod_entries.len() - 1);
        panel(r);
        let px = r.x + 22.0 * s;
        let pw = r.w - 44.0 * s;
        let icon = Rect::new(px, r.y + 20.0 * s, 84.0 * s, 84.0 * s);
        self.mod_icon(index, icon, s);
        let entry = &self.mod_entries[index];
        let tx = icon.right() + 18.0 * s;
        let tw = r.right() - 22.0 * s - tx;
        label(
            &fitted_label(entry.name(), 24.0 * s, tw),
            tx,
            r.y + 44.0 * s,
            24.0 * s,
            text_color(),
        );
        let mut facts = Vec::new();
        if let Some(version) = entry.version() {
            facts.push(format!("Version {version}"));
        }
        if let Some(creator) = entry.creator() {
            facts.push(format!("by {creator}"));
        }
        if let Some(license) = entry
            .project
            .as_ref()
            .and_then(|p| p.details.license.as_deref())
        {
            facts.push(format!("{license} license"));
        }
        label(
            &fitted_label(&facts.join("  ·  "), 13.0 * s, tw),
            tx,
            r.y + 66.0 * s,
            13.0 * s,
            muted_color(),
        );
        let (status, color) = mod_status(entry, settings);
        let badge_w = label_width(status, 11.0 * s) + 20.0 * s;
        let badge = Rect::new(tx, r.y + 78.0 * s, badge_w, 22.0 * s);
        capsule(badge, mix(surface_color(), color, 0.22));
        center_label(
            status,
            badge.center().x,
            badge.bottom() - 6.5 * s,
            11.0 * s,
            color,
        );
        let mut tag_x = badge.right() + 8.0 * s;
        let tags = entry
            .project
            .as_ref()
            .map(|p| p.details.tags.clone())
            .unwrap_or_default();
        for tag in tags {
            let tag_w = label_width(&tag, 11.0 * s) + 18.0 * s;
            if tag_x + tag_w > r.right() - 22.0 * s {
                break;
            }
            // Capsules overlap their parts, so they need an opaque color.
            capsule(
                Rect::new(tag_x, badge.y, tag_w, badge.h),
                mix(surface_color(), line_color(), 0.5),
            );
            center_label(
                &tag,
                tag_x + tag_w * 0.5,
                badge.bottom() - 6.5 * s,
                11.0 * s,
                text_color(),
            );
            tag_x += tag_w + 6.0 * s;
        }
        let mut y = r.y + 136.0 * s;
        let description = match (&entry.error, entry.description()) {
            (Some(error), _) => format!("This pack cannot load: {error}"),
            (None, Some(text)) => text.to_owned(),
            (None, None) => "No description.".to_owned(),
        };
        let lines = wrapped_lines(&description, pw, 14.0 * s).len().min(6);
        wrap(
            &description,
            px,
            y,
            pw,
            14.0 * s,
            if entry.error.is_some() {
                RED
            } else {
                text_color()
            },
        );
        y += lines as f32 * 19.0 * s + 14.0 * s;
        if let Some(manifest) = &entry.manifest {
            let count = |n: usize, one: &str, many: &str| match n {
                0 => None,
                1 => Some(format!("1 {one}")),
                n => Some(format!("{n} {many}")),
            };
            let contents: Vec<String> = [
                count(manifest.classes.len(), "tank", "tanks"),
                count(manifest.maps.len(), "map", "maps"),
                count(manifest.scripts.len(), "script", "scripts"),
                count(manifest.textures.len(), "texture", "textures"),
                count(manifest.hooks.len(), "hook", "hooks"),
                manifest.rules.as_ref().map(|_| "custom rules".to_owned()),
                (!manifest.dependencies.is_empty())
                    .then(|| format!("needs {}", manifest.dependencies.join(", "))),
            ]
            .into_iter()
            .flatten()
            .collect();
            label("CONTENTS", px, y, 11.0 * s, muted_color());
            y += 18.0 * s;
            let summary = if contents.is_empty() {
                "Settings only".to_owned()
            } else {
                contents.join("  ·  ")
            };
            label(
                &fitted_label(&summary, 13.0 * s, pw),
                px,
                y,
                13.0 * s,
                text_color(),
            );
            y += 18.0 * s;
            let kb = entry.size as f32 / 1024.0;
            label(
                &fitted_label(
                    &format!(
                        "{} · {} · {} files · {} · id {}",
                        if entry.scripted() {
                            "Lua script pack"
                        } else {
                            "Data pack"
                        },
                        if entry.archive { "zip" } else { "folder" },
                        entry.files,
                        if kb >= 1024.0 {
                            format!("{:.1} MB", kb / 1024.0)
                        } else if entry.size < 1024 {
                            format!("{} bytes", entry.size)
                        } else {
                            format!("{kb:.0} KB")
                        },
                        manifest.id
                    ),
                    12.0 * s,
                    pw,
                ),
                px,
                y,
                12.0 * s,
                muted_color(),
            );
            y += 22.0 * s;
        }
        if let Some(error) = &entry.project_error {
            label(&fitted_label(error, 12.0 * s, pw), px, y, 12.0 * s, RED);
            y += 18.0 * s;
        }
        let links: Vec<(&str, String)> = entry
            .project
            .as_ref()
            .map(|p| {
                p.links
                    .all()
                    .into_iter()
                    .map(|(l, u)| (l, u.to_owned()))
                    .collect()
            })
            .unwrap_or_default();
        let creator_url = entry.project.as_ref().and_then(|p| p.creator.url.clone());
        let mut link_x = px;
        let mut opened = None;
        for (title, url) in links
            .iter()
            .map(|(t, u)| (*t, u.as_str()))
            .chain(creator_url.as_deref().map(|u| ("Creator", u)))
        {
            let bw = label_width(title, 12.0 * s) + 36.0 * s;
            if link_x + bw > r.right() - 22.0 * s {
                break;
            }
            if small_button(
                Rect::new(link_x, y + 4.0 * s, bw, 30.0 * s),
                &format!("{title} ↗"),
                s * 0.72,
            ) {
                opened = Some(url.to_owned());
            }
            link_x += bw + 8.0 * s;
        }
        if let Some(url) = opened {
            match crate::links::open(&url) {
                Ok(()) => self.info = format!("Opened {url}"),
                Err(error) => self.error = error,
            }
        }
        let enabled = entry.enabled(settings);
        let footer = r.bottom() - 52.0 * s;
        let removable = entry.in_library();
        let path_room = pw - if removable { 410.0 } else { 300.0 } * s;
        label(
            &fitted_label(&entry.path, 11.0 * s, path_room),
            px,
            footer + 22.0 * s,
            11.0 * s,
            muted_color(),
        );
        if removable {
            let confirming = self.mod_remove.as_deref() == Some(entry.path.as_str());
            if small_button(
                Rect::new(r.right() - 400.0 * s, footer, 102.0 * s, 34.0 * s),
                if confirming { "Confirm" } else { "Remove" },
                s * 0.78,
            ) {
                if confirming {
                    self.mod_remove = None;
                    let entry = entry.clone();
                    match crate::mod_library::remove(&entry) {
                        Ok(()) => {
                            if let Some(id) = entry.id() {
                                settings.enabled_mods.retain(|e| e != id);
                                let _ = settings.save();
                            }
                            self.info = format!("Removed {}.", entry.name());
                            self.error.clear();
                            self.mods_scanned = false;
                            self.catalog_stale = true;
                        }
                        Err(error) => self.error = error,
                    }
                    return;
                }
                self.mod_remove = Some(entry.path.clone());
                self.info = format!(
                    "Press Confirm to delete {} from the mods folder.",
                    entry.name()
                );
            }
        }
        if small_button(
            Rect::new(r.right() - 290.0 * s, footer, 120.0 * s, 34.0 * s),
            "Safety review",
            s * 0.78,
        ) {
            self.mod_review = Some(index);
            self.mod_trust = false;
        }
        // Packs that still need confirmation say so instead of looking safe.
        let needs_review = !enabled && !entry.approved(settings);
        if button(
            Rect::new(r.right() - 162.0 * s, footer, 140.0 * s, 34.0 * s),
            if enabled {
                "Disable"
            } else if needs_review {
                "Review & enable"
            } else {
                "Enable"
            },
            !enabled && !needs_review,
            entry.error.is_none(),
            s * 0.78,
        ) {
            if !enabled && !entry.approved(settings) {
                self.mod_review = Some(index);
                self.mod_trust = false;
            } else {
                self.toggle_mod(settings, index);
            }
        }
    }

    /// The review sheet for one pack, drawn above the page and its canvas.
    fn mod_review_overlay(&mut self, settings: &mut Settings, s: f32) {
        use tankyard_mods::scan::{Severity, Verdict};
        if let Some(index) = self.mod_review
            && let Some(entry) = self.mod_entries.get(index)
        {
            let panel_w = (720.0 * s).min(screen_width() - 48.0 * s);
            let panel_h = (470.0 * s).min(screen_height() - 160.0 * s);
            let r = Rect::new(
                (screen_width() - panel_w) * 0.5,
                (screen_height() - panel_h) * 0.5,
                panel_w,
                panel_h,
            );
            draw_rectangle(
                0.,
                0.,
                screen_width(),
                screen_height(),
                alpha(bg_color(), 0.55),
            );
            panel(r);
            let verdict = entry.verdict();
            let (title, color) = match verdict {
                Verdict::Clean => ("No problems found", Color::new(0.30, 0.64, 0.30, 1.0)),
                Verdict::Review => ("Review before enabling", warm_text_color()),
                Verdict::Dangerous => ("This pack tries to reach your computer", RED),
            };
            draw_rectangle(r.x, r.y, r.w, 5.0 * s, color);
            label(
                &fitted_label(entry.name(), 22.0 * s, r.w - 40.0 * s),
                r.x + 20.0 * s,
                r.y + 38.0 * s,
                22.0 * s,
                text_color(),
            );
            label(title, r.x + 20.0 * s, r.y + 62.0 * s, 14.0 * s, color);
            let summary = match verdict {
                Verdict::Clean => {
                    "Nothing in this pack tries to use system commands, files, native code or hidden data."
                }
                Verdict::Review => {
                    "Some contents are unusual for a game mod. The sandbox still blocks system access, but check that you expected them."
                }
                Verdict::Dangerous => {
                    "The sandbox blocks these attempts, but a pack that makes them was probably written to cause harm. Only continue if you fully trust its author."
                }
            };
            let mut ly = wrap(
                summary,
                r.x + 20.0 * s,
                r.y + 88.0 * s,
                r.w - 40.0 * s,
                13.0 * s,
                muted_color(),
            ) + 8.0 * s;
            let footer = r.bottom() - 62.0 * s;
            let findings = &entry.report.findings;
            if findings.is_empty() {
                label(
                    "No findings.",
                    r.x + 20.0 * s,
                    ly + 12.0 * s,
                    13.0 * s,
                    muted_color(),
                );
            }
            for finding in findings {
                if ly + 20.0 * s
                    > footer
                        - (if verdict == Verdict::Dangerous {
                            30.0
                        } else {
                            0.0
                        }) * s
                {
                    label("…", r.x + 20.0 * s, ly + 10.0 * s, 13.0 * s, muted_color());
                    break;
                }
                let (tag, tint) = match finding.severity {
                    Severity::Danger => ("DANGER", RED),
                    Severity::Warning => ("REVIEW", warm_text_color()),
                    Severity::Info => ("NOTE", muted_color()),
                };
                label(tag, r.x + 20.0 * s, ly + 12.0 * s, 10.0 * s, tint);
                let place = if finding.line > 0 {
                    format!("{}:{}", finding.file, finding.line)
                } else {
                    finding.file.clone()
                };
                label(
                    &fitted_label(
                        &format!("{place} — {}", finding.message),
                        12.0 * s,
                        r.w - 110.0 * s,
                    ),
                    r.x + 84.0 * s,
                    ly + 12.0 * s,
                    12.0 * s,
                    text_color(),
                );
                ly += 20.0 * s;
            }
            let enabled = entry.enabled(settings);
            let needs_trust = verdict == Verdict::Dangerous && !entry.approved(settings);
            if needs_trust {
                crate::ui::toggle(
                    Rect::new(r.x + 20.0 * s, footer - 34.0 * s, r.w - 40.0 * s, 30.0 * s),
                    "I trust this pack's author and accept the risk",
                    &mut self.mod_trust,
                    s * 0.8,
                );
            }
            let by = r.bottom() - 52.0 * s;
            if small_button(
                Rect::new(r.x + 20.0 * s, by, 110.0 * s, 36.0 * s),
                "Close",
                s * 0.85,
            ) {
                self.mod_review = None;
            }
            if !enabled
                && entry.error.is_none()
                && button(
                    Rect::new(r.right() - 220.0 * s, by, 200.0 * s, 36.0 * s),
                    if verdict == Verdict::Clean {
                        "Enable"
                    } else {
                        "Enable anyway"
                    },
                    verdict == Verdict::Clean,
                    !needs_trust || self.mod_trust,
                    s * 0.85,
                )
            {
                if verdict != Verdict::Clean && !settings.approved_mods.contains(&entry.digest) {
                    settings.approved_mods.push(entry.digest.clone());
                }
                self.mod_review = None;
                self.toggle_mod(settings, index);
            }
            if is_key_pressed(KeyCode::Escape) {
                self.mod_review = None;
            }
        }
    }
    fn toggle_mod(&mut self, settings: &mut Settings, index: usize) {
        let Some(entry) = self.mod_entries.get(index) else {
            return;
        };
        let Some(id) = entry.id().map(str::to_owned) else {
            return;
        };
        let previous = settings.enabled_mods.clone();
        if settings.enabled_mods.contains(&id) {
            settings.enabled_mods.retain(|e| e != &id);
        } else if settings.enabled_mods.len() >= tankyard_mods::MAX_MODS {
            self.error = "At most 16 packs can be enabled at once.".into();
            return;
        } else {
            settings.enabled_mods.push(id);
        }
        let (ready, held) = crate::mod_library::ready_paths(settings);
        match crate::mod_library::load(&ready) {
            Ok(loaded) => {
                self.maps = loaded.bundle.maps;
                self.map = self.map.min(self.maps.len() - 1);
                self.error = held.join(" ");
                self.info.clear();
                let _ = settings.save();
            }
            Err(error) => {
                settings.enabled_mods = previous;
                self.error = error;
            }
        }
    }
    /// Report an import. A pack that passed review is enabled right away;
    /// a flagged one waits for the player's review.
    pub fn import_result(
        &mut self,
        settings: &mut Settings,
        result: Result<crate::mod_library::Entry, String>,
    ) {
        match result {
            Ok(entry) => {
                self.error.clear();
                let clean = entry.verdict() == tankyard_mods::scan::Verdict::Clean;
                let id = entry.id().unwrap_or_default().to_owned();
                let room = settings.enabled_mods.len() < tankyard_mods::MAX_MODS;
                self.info = if clean && (entry.enabled(settings) || room) {
                    if !entry.enabled(settings) {
                        settings.enabled_mods.push(id.clone());
                        let _ = settings.save();
                    }
                    format!("Added and enabled {}. It passed review.", entry.name())
                } else if clean {
                    format!(
                        "Added {}. Disable another pack to enable it (16 at most).",
                        entry.name()
                    )
                } else {
                    format!(
                        "Added {}. Review its findings before enabling it.",
                        entry.name()
                    )
                };
                self.mod_focus = Some(id);
                self.mods_scanned = false;
                self.catalog_stale = true;
            }
            Err(error) => {
                self.info.clear();
                self.error = format!("Import failed: {error}");
            }
        }
    }
}
/// The Bastion mark and motto from Credits, in the main menu's lower right.
/// It steps aside on windows too small to show it beside the menu.
fn bastion_badge(pane: Rect, time: f32, s: f32) {
    let w = screen_width();
    let h = screen_height();
    let name = tankyard_server::bastion::NAME;
    let motto = tankyard_server::bastion::TAGLINE;
    let text_width = label_width(name, 14.0 * s).max(label_width(motto, 11.0 * s));
    let badge = Rect::new(
        w - 22.0 * s - (74.0 * s + text_width + 18.0 * s),
        h - 22.0 * s - 66.0 * s,
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
        vec2(badge.x + 36.0 * s, badge.center().y),
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
/// The open lobby list: up to three rows, scrolling beyond that.
fn lobby_list_height(rows: usize, s: f32) -> f32 {
    (rows.clamp(1, 3) as f32 * LOBBY_ROW + 12.0) * s
}

/// A pack's state for its list row and badge.
fn mod_status(entry: &crate::mod_library::Entry, settings: &Settings) -> (&'static str, Color) {
    use tankyard_mods::scan::Verdict;
    let enabled = entry.enabled(settings);
    match entry.verdict() {
        _ if entry.error.is_some() => ("Cannot load", RED),
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
