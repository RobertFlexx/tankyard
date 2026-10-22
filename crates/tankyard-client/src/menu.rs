use crate::ui::RED;
use crate::ui::{
    ui_line as draw_line, ui_rectangle as draw_rectangle,
    ui_rectangle_lines as draw_rectangle_lines,
};
use crate::ui_layout::height as screen_height;
use crate::{
    render,
    settings::{CENTRAL_SERVER, Settings, bindable_pressed},
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
const DISPLAY_AA_LABEL: f32 = 287.0;
const DISPLAY_AA_ROW: f32 = 298.0;
const DISPLAY_SHAPE_LABEL: f32 = 356.0;
const DISPLAY_SHAPE_ROW: f32 = 367.0;
const DISPLAY_TEXTURE_LABEL: f32 = 425.0;
const DISPLAY_TEXTURE_ROW: f32 = 436.0;

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
    mod_catalog: Vec<(String, tankyard_mods::ModInfo)>,
    mods_scanned: bool,
    map_page: usize,
    history: Vec<Page>,
    practice_content: Content,
    practice_page: usize,
    pub offline_level: u8,
    dynamic_arena: bool,
    server_browser: crate::server_browser::ServerBrowser,
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
            mod_catalog: vec![],
            mods_scanned: false,
            map_page: 0,
            history: Vec::new(),
            practice_content: Content::builtin(),
            practice_page: 0,
            offline_level: 1,
            dynamic_arena: true,
            server_browser: crate::server_browser::ServerBrowser::default(),
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
    pub fn draw(
        &mut self,
        content: &Content,
        settings: &mut Settings,
        match_claims: &[String],
        s: f32,
    ) -> Action {
        pointer_scope(1 + self.page as u64);
        if is_key_pressed(KeyCode::Tab) && self.binding.is_none() {
            self.focus_next(is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift));
        }
        self.motion_time = if settings.reduced_motion {
            0.0
        } else {
            get_time() as f32
        };
        if self.page == Page::Main {
            self.ambient.draw(settings);
        } else {
            render::menu_grid();
        }
        let w = screen_width();
        let h = screen_height();
        let margin = 40.0 * s;
        if self.page == Page::Main {
            center_label("Tank Yard", w * 0.5, 78.0 * s, 53.0 * s, text_color());
        } else {
            label("TANK YARD", margin, 80.0 * s, 48.0 * s, text_color());
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
                (&self.info, CYAN)
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
        let y = (h * 0.23).max(139.0 * s);
        let row_h = 44.0 * s;
        // A small preview gives the menu life without overlaying its controls.
        if let Some(basic) = content.class("vector") {
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
        panel(Rect::new(x, y + 113.0 * s, pane_w, 48.0 * s));
        draw_rectangle(x, y + 113.0 * s, 4.0 * s, 48.0 * s, CYAN);
        label(
            CENTRAL_SERVER,
            x + 17.0 * s,
            y + 144.0 * s,
            20.0 * s,
            text_color(),
        );
        let play_y = y + 190.0 * s;
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
            ("Settings", Page::Settings),
            ("Parties", Page::Party),
        ]
        .into_iter()
        .enumerate()
        {
            if small_button(
                Rect::new(
                    x + index as f32 * (pane_w / 4.0),
                    links_y,
                    pane_w / 4.0 - 6.0 * s,
                    33.0 * s,
                ),
                title,
                s * 0.88,
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
        Action::None
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
            label("YOUR PARTY", x + pad, 276. * s, 17. * s, CYAN);
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
            label("HOST A PARTY", x + pad, 276. * s, 17. * s, CYAN);
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
            label("STAY CONNECTED", right + pad, 276. * s, 17. * s, CYAN);
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
                        CYAN
                    } else {
                        mix(CYAN, text_color(), 0.45)
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
            if host {
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
                &mut self.password,
                &mut self.password_active,
                64,
                s,
                true,
            );
            label(self.mode.name(), x, y + 219.0 * s, 24.0 * s, CYAN);
            label(
                &format!(
                    "{} bot limit  ·  {}  ·  UDP port 5000",
                    self.bots,
                    self.difficulty_name()
                ),
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
                "For LAN games, share your local IP. For internet games, share your public IP and forward UDP port 5000.",
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
        if host {
            panel(Rect::new(rx, y - 16.0 * s, rw, 438.0 * s));
            label(
                "SERVER DETAILS",
                rx + 22.0 * s,
                y + 20.0 * s,
                15.0 * s,
                CYAN,
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
        } else if let Err(error) =
            self.server_browser
                .draw(settings, Rect::new(rx, y - 16.0 * s, rw, 438.0 * s), s)
        {
            self.error = error;
        }
        if button(
            Rect::new(x, screen_height() - 140.0 * s, width, 46.0 * s),
            if host {
                "Start server & play [Enter]"
            } else {
                "Connect  →"
            },
            true,
            true,
            s,
        ) {
            return if host { Action::Host } else { Action::Join };
        }
        if is_key_pressed(KeyCode::Enter) {
            return if host { Action::Host } else { Action::Join };
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
        let tab_w = (screen_width() - 2. * x - 6. * 8. * s) / 7.;
        for (index, name) in [
            "Display",
            "Style",
            "Controls",
            "Accessibility",
            "Profile",
            "Audio",
            "Servers",
        ]
        .iter()
        .enumerate()
        {
            if button(
                Rect::new(x + index as f32 * (tab_w + 8.0 * s), y, tab_w, 38.0 * s),
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
                slider(
                    Rect::new(x, by + 154.0 * s, col, 48.0 * s),
                    "Particle intensity",
                    &mut settings.effects,
                    0.0,
                    1.0,
                    s,
                );
                slider(
                    Rect::new(x, by + 229.0 * s, col, 48.0 * s),
                    "Screen shake",
                    &mut settings.shake,
                    0.0,
                    1.0,
                    s,
                );
                self.interface_scale(
                    Rect::new(x, by + 304.0 * s, col, 48.0 * s),
                    settings,
                    s,
                    558.,
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
                label(
                    "ANTIALIASING  (restart to apply)",
                    rx,
                    by + DISPLAY_AA_LABEL * s,
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
                            by + DISPLAY_AA_ROW * s,
                            col / 3.0 - 6.0 * s,
                            33.0 * s,
                        ),
                        title,
                        settings.anti_aliasing == samples,
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
                    by + DISPLAY_SHAPE_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (i, title) in ["Low", "Normal", "High"].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + i as f32 * col / 3.0,
                            by + DISPLAY_SHAPE_ROW * s,
                            col / 3.0 - 6.0 * s,
                            28.0 * s,
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
                    by + DISPLAY_TEXTURE_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (i, level) in [1, 2, 4, 8, 16].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + i as f32 * col / 5.0,
                            by + DISPLAY_TEXTURE_ROW * s,
                            col / 5.0 - 6.0 * s,
                            27.0 * s,
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
                        "Saved for textured tanks; built-in tanks use geometry."
                    } else if crate::graphics::max_anisotropy() <= 1 {
                        "This graphics device supports standard filtering."
                    } else {
                        "Filtering sharpens textured tanks viewed at an angle."
                    },
                    rx,
                    by + 480. * s,
                    11. * s,
                    muted_color(),
                );
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
                        CYAN,
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
                label("LOCAL PROFILE", rx, by + 310.0 * s, 13.0 * s, CYAN);
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
        label("BUILT WITH", x, y + 168.0 * s, 13.0 * s, CYAN);
        label(
            "Rust  /  Macroquad  /  Renet + Netcode  /  CPAL audio",
            x,
            y + 204.0 * s,
            23.0 * s,
            text_color(),
        );
        label("LICENSE", x, y + 264.0 * s, 13.0 * s, CYAN);
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
        label("Mods", x, y, 30.0 * s, text_color());
        label(
            "Choose mods for your next offline game or hosted server.",
            x,
            y + 31.0 * s,
            16.0 * s,
            muted_color(),
        );
        if small_button(
            Rect::new(
                screen_width() - 165.0 * s,
                y - 29.0 * s,
                125.0 * s,
                32.0 * s,
            ),
            "Refresh",
            s,
        ) {
            self.mods_scanned = false;
        }
        if !self.mods_scanned {
            self.mods_scanned = true;
            self.mod_catalog.clear();
            if let Ok(entries) = std::fs::read_dir("mods") {
                let mut paths: Vec<_> = entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .take(64)
                    .collect();
                paths.sort();
                for path in paths {
                    let parsed =
                        tankyard_mods::read_bounded(&path.join("mod.json")).and_then(|bytes| {
                            serde_json::from_slice::<tankyard_mods::Manifest>(&bytes)
                                .map_err(|e| e.to_string())
                        });
                    if let Ok(m) = parsed
                        && m.api_version == tankyard_mods::API_VERSION
                        && tankyard_mods::identifier(&m.id)
                        && !m.name.is_empty()
                        && m.name.len() <= 64
                        && !m.name.chars().any(char::is_control)
                        && !m.version.is_empty()
                        && m.version.len() <= 32
                        && !m.version.chars().any(char::is_control)
                    {
                        self.mod_catalog.push((
                            path.to_string_lossy().into_owned(),
                            tankyard_mods::ModInfo {
                                id: m.id,
                                name: m.name,
                                version: m.version,
                                behavior_digest: 0,
                            },
                        ));
                    }
                }
            }
        }
        if self.mod_catalog.is_empty() {
            label(
                "Put a mod folder containing mod.json in the mods directory.",
                x,
                y + 88.0 * s,
                18.0 * s,
                muted_color(),
            );
        }
        let visible = ((screen_height() - y - 160.0 * s) / (58.0 * s))
            .floor()
            .max(1.0) as usize;
        for (i, (path, info)) in self.mod_catalog.iter().take(visible).enumerate() {
            let r = Rect::new(
                x,
                y + 60.0 * s + i as f32 * 58.0 * s,
                screen_width() - x * 2.0,
                49.0 * s,
            );
            let enabled = settings.enabled_mods.contains(path);
            panel(r);
            label(
                &info.name,
                r.x + 15.0 * s,
                r.y + 23.0 * s,
                18.0 * s,
                text_color(),
            );
            label(
                &format!("{}  ·  {}", info.id, info.version),
                r.x + 15.0 * s,
                r.y + 41.0 * s,
                11.0 * s,
                muted_color(),
            );
            if button(
                Rect::new(r.x + r.w - 139.0 * s, r.y + 8.0 * s, 124.0 * s, 33.0 * s),
                if enabled { "Enabled" } else { "Enable" },
                enabled,
                true,
                s * 0.8,
            ) {
                let previous = settings.enabled_mods.clone();
                if enabled {
                    settings.enabled_mods.retain(|p| p != path);
                } else {
                    settings.enabled_mods.push(path.clone());
                }
                let paths: Vec<_> = settings
                    .enabled_mods
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect();
                match tankyard_mods::load(&paths) {
                    Ok(loaded) => {
                        self.maps = loaded.bundle.maps;
                        self.map = self.map.min(self.maps.len() - 1);
                        self.error.clear();
                        let _ = settings.save();
                    }
                    Err(error) => {
                        settings.enabled_mods = previous;
                        self.error = error;
                    }
                }
            }
        }
        label(
            "Modded servers provide tank and map data when you join.",
            x,
            screen_height() - 93.0 * s,
            14.0 * s,
            muted_color(),
        );
        Action::None
    }
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
            (DISPLAY_AA_LABEL - 13.0, DISPLAY_AA_ROW + 33.0),
            (DISPLAY_SHAPE_LABEL - 13.0, DISPLAY_SHAPE_ROW + 28.0),
            (DISPLAY_TEXTURE_LABEL - 13.0, DISPLAY_TEXTURE_ROW + 27.0),
        ];
        assert!(groups.windows(2).all(|pair| pair[0].1 + 6.0 <= pair[1].0));
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
