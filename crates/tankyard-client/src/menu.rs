use crate::ui::RED;
use crate::{
    render,
    settings::{Settings, bindable_pressed},
    ui::*,
};
use macroquad::prelude::*;
use tankyard_client::achievements::ACHIEVEMENTS;
use tankyard_content::{Ability, ClassDef, Content};
use tankyard_core::{Mode, WorldConfig};

// Baselines and button rows in the Display tab. Keep the damn labels out
// of the button hitboxes, even when the whole menu gets scaled down.
const DISPLAY_RESOLUTION_LABEL: f32 = 19.0;
const DISPLAY_RESOLUTION_ROW: f32 = 30.0;
const DISPLAY_RESOLUTION_STEP: f32 = 39.0;
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
}
pub enum Action {
    None,
    Page(Page),
    Back,
    Offline,
    Host,
    Join,
    Quit,
    Save,
    Practice(usize),
}
pub struct Menu {
    pub page: Page,
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
    pub class_page: usize,
    pub tree: bool,
    pub settings_tab: usize,
    pub binding: Option<usize>,
    pub error: String,
    motion_time: f32,
    pub maps: Vec<tankyard_content::MapDef>,
    mod_catalog: Vec<(String, tankyard_mods::ModInfo)>,
    mods_scanned: bool,
    map_page: usize,
    history: Vec<Page>,
    practice_content: Content,
}
impl Menu {
    pub fn new() -> Self {
        Self {
            page: Page::Main,
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
            class_page: 0,
            tree: false,
            settings_tab: 0,
            binding: None,
            error: String::new(),
            motion_time: 0.0,
            maps: tankyard_content::builtin_maps(),
            mod_catalog: vec![],
            mods_scanned: false,
            map_page: 0,
            history: Vec::new(),
            practice_content: Content::builtin(),
        }
    }
    pub fn clear_focus(&mut self) {
        self.name_active = false;
        self.address_active = false;
        self.password_active = false;
        self.binding = None;
    }
    pub fn go(&mut self, page: Page) {
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
        let fields: &[usize] = match self.page {
            Page::Main => &[0],
            Page::Join => &[0, 1, 2],
            Page::Host => &[0, 2],
            Page::Settings if self.settings_tab == 4 => &[0, 1],
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
        self.mode = mode;
        let map = match mode {
            Mode::Maze => "maze",
            Mode::Domination => "domination",
            Mode::Survival => "survival",
            Mode::BossRush => "boss",
            Mode::TwoTeams | Mode::FourTeams | Mode::Mothership | Mode::CaptureCore | Mode::Tag => {
                "fortress"
            }
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
    pub fn config(&self) -> WorldConfig {
        WorldConfig {
            mode: self.mode,
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
    pub fn draw(&mut self, content: &Content, settings: &mut Settings, s: f32) -> Action {
        if is_key_pressed(KeyCode::Tab) && self.binding.is_none() {
            self.focus_next(is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift));
        }
        self.motion_time = if settings.reduced_motion {
            0.0
        } else {
            get_time() as f32
        };
        render::menu_background(self.motion_time);
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
        let action = match self.page {
            Page::Main => self.main(content, settings, s),
            Page::Modes => self.modes(settings, s),
            Page::Join => self.connection(settings, false, s),
            Page::Host => self.connection(settings, true, s),
            Page::Encyclopedia => self.encyclopedia(content, s),
            Page::Settings => self.settings(settings, s),
            Page::Achievements => self.achievements(settings, s),
            Page::Mods => self.mods(settings, s),
            Page::Credits => self.credits(s),
            Page::Practice => self.practice(settings, s),
            Page::Controls => self.controls(settings, s),
        };
        if !self.error.is_empty() {
            label(&self.error, margin + 140.0 * s, h - 40.0 * s, 17.0 * s, RED);
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
                CYAN,
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
        label("GAME MODE", x, y + 102.0 * s, 13.0 * s, muted_color());
        let choices = [Mode::Ffa, Mode::TwoTeams, Mode::FourTeams, Mode::Maze];
        for (index, mode) in choices.into_iter().enumerate() {
            let rw = (pane_w - 9.0 * s) * 0.5;
            if button(
                Rect::new(
                    x + (index % 2) as f32 * (rw + 9.0 * s),
                    y + 113.0 * s + (index / 2) as f32 * 49.0 * s,
                    rw,
                    40.0 * s,
                ),
                mode.name(),
                self.mode == mode,
                true,
                s * 0.88,
            ) {
                self.select_mode(mode);
            }
        }
        let play_y = y + 229.0 * s;
        if button(
            Rect::new(x, play_y, pane_w - 190.0 * s, 51.0 * s),
            "Play offline [Enter]",
            true,
            true,
            s * 1.08,
        ) || is_key_pressed(KeyCode::Enter)
        {
            let _ = settings.save();
            return Action::Offline;
        }
        if button(
            Rect::new(x + pane_w - 180.0 * s, play_y, 180.0 * s, 51.0 * s),
            "Practice [F6]",
            false,
            true,
            s * 0.9,
        ) {
            return Action::Page(Page::Practice);
        }
        let half = (pane_w - 9.0 * s) * 0.5;
        if button(
            Rect::new(x, play_y + 61.0 * s, half, 41.0 * s),
            "Host server",
            false,
            true,
            s * 0.88,
        ) {
            let _ = settings.save();
            return Action::Page(Page::Host);
        }
        if button(
            Rect::new(x + half + 9.0 * s, play_y + 61.0 * s, half, 41.0 * s),
            "Join server",
            false,
            true,
            s * 0.88,
        ) {
            let _ = settings.save();
            return Action::Page(Page::Join);
        }
        let links_y = play_y + 122.0 * s;
        for (index, (title, page)) in [
            ("Modes", Page::Modes),
            ("Tank tree", Page::Encyclopedia),
            ("Settings", Page::Settings),
            ("Mods", Page::Mods),
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
            Rect::new(w - 120.0 * s, 35.0 * s, 80.0 * s, 26.0 * s),
            "Credits",
            s * 0.7,
        ) {
            return Action::Page(Page::Credits);
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
    fn modes(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 152.0 * s;
        label("Game modes", x, y + 12.0 * s, 25.0 * s, text_color());
        let gap = 12.0 * s;
        let width = (screen_width() - 80.0 * s - gap * 2.0) / 3.0;
        let height = 87.0 * s;
        for (index, mode) in Mode::ALL.into_iter().enumerate() {
            let bx = x + (index % 3) as f32 * (width + gap);
            let by = y + 35.0 * s + (index / 3) as f32 * (height + gap);
            let r = Rect::new(bx, by, width, height);
            let selected = self.mode == mode;
            if button(r, mode.name(), selected, true, s) {
                self.select_mode(mode);
            }
            label(
                mode_blurb(mode),
                bx + 16.0 * s,
                by + 70.0 * s,
                13.0 * s,
                muted_color(),
            );
        }
        let by = y + 35.0 * s + 4.0 * (height + gap) + 14.0 * s;
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
        label(
            &format!("AI TANKS  {}", self.bots),
            x,
            by + 99.0 * s,
            18.0 * s,
            text_color(),
        );
        if small_button(
            Rect::new(x + 170.0 * s, by + 77.0 * s, 36.0 * s, 32.0 * s),
            "−",
            s,
        ) {
            self.bots = self.bots.saturating_sub(2);
        }
        if small_button(
            Rect::new(x + 215.0 * s, by + 77.0 * s, 36.0 * s, 32.0 * s),
            "+",
            s,
        ) {
            self.bots = (self.bots + 2).min(64);
        }
        label(
            &format!("WORLD  {:.0} × {:.0}", self.width, self.height),
            x + 300.0 * s,
            by + 99.0 * s,
            16.0 * s,
            muted_color(),
        );
        for (index, (name, width, height)) in [
            ("Compact", 2400.0, 1800.0),
            ("Standard", 3600.0, 2600.0),
            ("Large", 6000.0, 4000.0),
        ]
        .into_iter()
        .enumerate()
        {
            if small_button(
                Rect::new(
                    x + (520.0 + index as f32 * 110.0) * s,
                    by + 77.0 * s,
                    100.0 * s,
                    32.0 * s,
                ),
                name,
                s * 0.85,
            ) {
                self.width = width;
                self.height = height;
            }
        }
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
            "Launch offline  →",
            true,
            true,
            s * 0.85,
        ) {
            return Action::Offline;
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
                return Action::Practice(index);
            }
        }
        let margin = 40.0 * s;
        label("Practice", margin, 164.0 * s, 26.0 * s, text_color());
        label(
            "Four challenges. Personal bests are saved on this device.",
            margin,
            190.0 * s,
            15.0 * s,
            muted_color(),
        );
        let gap = 18.0 * s;
        let width = (screen_width() - margin * 2.0 - gap) * 0.5;
        let height = 213.0 * s;
        for (index, challenge) in CHALLENGES.iter().enumerate() {
            let r = Rect::new(
                margin + (index % 2) as f32 * (width + gap),
                216.0 * s + (index / 2) as f32 * (height + gap),
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
                    CYAN,
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
                &format!("Start [{}]", index + 1),
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
        ];
        for (column, rows) in [movement, shortcuts].iter().enumerate() {
            let x = margin + column as f32 * (width + gap);
            panel(Rect::new(x, 222.0 * s, width, 397.0 * s));
            for (index, (action, key)) in rows.iter().enumerate() {
                let y = (252.0 + index as f32 * 42.0) * s;
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
            660.0 * s,
            14.0 * s,
            muted_color(),
        );
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
                    "{} bots  ·  {}  ·  UDP port 5000",
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
            ) && !settings.favorites.contains(&settings.server_address)
                && settings.favorites.len() < 32
            {
                settings.favorites.push(settings.server_address.clone());
                let _ = settings.save();
            }
            wrap(
                "Enter the server's IP address and port, then connect.",
                x,
                y + 370.0 * s,
                width,
                18.0 * s,
                muted_color(),
            );
        }
        let rx = x + width + 48.0 * s;
        let rw = screen_width() - rx - 40.0 * s;
        panel(Rect::new(rx, y - 16.0 * s, rw, 438.0 * s));
        label(
            if host {
                "SERVER DETAILS"
            } else {
                "YOUR SERVERS"
            },
            rx + 22.0 * s,
            y + 20.0 * s,
            15.0 * s,
            CYAN,
        );
        if host {
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
        } else {
            let servers: Vec<String> = settings
                .favorites
                .iter()
                .chain(settings.recent_servers.iter())
                .take(8)
                .cloned()
                .collect();
            if servers.is_empty() {
                wrap(
                    "Favorites and recent servers appear here.",
                    rx + 22.0 * s,
                    y + 77.0 * s,
                    rw - 44.0 * s,
                    20.0 * s,
                    muted_color(),
                );
            }
            for (index, address) in servers.iter().enumerate() {
                if button(
                    Rect::new(
                        rx + 16.0 * s,
                        y + 48.0 * s + index as f32 * 43.0 * s,
                        rw - 32.0 * s,
                        36.0 * s,
                    ),
                    address,
                    settings.server_address == *address,
                    true,
                    s * 0.78,
                ) {
                    settings.server_address = address.clone();
                }
            }
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
        let x = 40.0 * s;
        let y = 159.0 * s;
        let left = (screen_width() * 0.55 - 45.0 * s).max(430.0 * s);
        label("Tank tree", x, y, 26.0 * s, text_color());
        right_label(
            &format!("{} tanks", content.classes.len()),
            x + left,
            y,
            15.0 * s,
            muted_color(),
        );
        if small_button(
            Rect::new(x, y + 19.0 * s, 110.0 * s, 30.0 * s),
            if self.tree { "Catalogue" } else { "Branches" },
            s * 0.9,
        ) {
            self.tree = !self.tree;
        }
        let item_h = 100.0 * s;
        let count = (((screen_height() - 290.0 * s) / item_h).floor() as usize).max(1) * 4;
        let max_page = content.classes.len().saturating_sub(1) / count.max(1);
        self.class_page = self.class_page.min(max_page);
        let scroll = mouse_wheel().1;
        if scroll < 0.0 {
            self.class_page = (self.class_page + 1).min(max_page);
        } else if scroll > 0.0 {
            self.class_page = self.class_page.saturating_sub(1);
        }
        if !self.tree {
            for (row, (index, class)) in content
                .classes
                .iter()
                .enumerate()
                .skip(self.class_page * count)
                .take(count)
                .enumerate()
            {
                let r = Rect::new(
                    x + (row % 4) as f32 * (left / 4.0),
                    y + 64.0 * s + (row / 4) as f32 * item_h,
                    left / 4.0 - 7.0 * s,
                    item_h - 5.0 * s,
                );
                if class_card(class, r, self.class_index == index, index, s) {
                    self.class_index = index;
                }
            }
            if small_button(
                Rect::new(
                    x + left - 125.0 * s,
                    screen_height() - 100.0 * s,
                    52.0 * s,
                    30.0 * s,
                ),
                "‹",
                s,
            ) {
                self.class_page = self.class_page.saturating_sub(1);
            }
            if small_button(
                Rect::new(
                    x + left - 59.0 * s,
                    screen_height() - 100.0 * s,
                    52.0 * s,
                    30.0 * s,
                ),
                "›",
                s,
            ) {
                self.class_page = (self.class_page + 1).min(max_page);
            }
            label(
                &format!("PAGE {} / {}", self.class_page + 1, max_page + 1),
                x,
                screen_height() - 79.0 * s,
                13.0 * s,
                muted_color(),
            );
        } else if let Some(class) = content.classes.get(self.class_index) {
            let center = Rect::new(x + left * 0.28, y + 152.0 * s, left * 0.44, 88.0 * s);
            let parent_indices: Vec<usize> = class
                .parents
                .iter()
                .filter_map(|id| content.classes.iter().position(|c| &c.id == id))
                .collect();
            for (index, &ci) in parent_indices.iter().enumerate() {
                let r = Rect::new(
                    x + index as f32 * (left / parent_indices.len().max(1) as f32),
                    y + 66.0 * s,
                    left / parent_indices.len().max(1) as f32 - 8.0 * s,
                    76.0 * s,
                );
                draw_line(
                    r.x + r.w * 0.5,
                    r.y + r.h,
                    center.x + center.w * 0.5,
                    center.y,
                    1.0,
                    alpha(CYAN, 0.4),
                );
                if class_card(&content.classes[ci], r, false, ci, s) {
                    self.class_index = ci;
                }
            }
            class_card(class, center, true, self.class_index, s);
            let child_indices: Vec<usize> = class
                .upgrades
                .iter()
                .filter_map(|id| content.classes.iter().position(|c| &c.id == id))
                .collect();
            if child_indices.is_empty() {
                center_label(
                    "Last upgrade",
                    x + left * 0.5,
                    y + 303.0 * s,
                    16.0 * s,
                    AMBER,
                );
            }
            for (index, &ci) in child_indices.iter().enumerate() {
                let col = index % 3;
                let row = index / 3;
                let r = Rect::new(
                    x + col as f32 * left / 3.0,
                    y + 274.0 * s + row as f32 * 94.0 * s,
                    left / 3.0 - 8.0 * s,
                    84.0 * s,
                );
                draw_line(
                    center.x + center.w * 0.5,
                    center.y + center.h,
                    r.x + r.w * 0.5,
                    r.y,
                    1.0,
                    alpha(CYAN, 0.4),
                );
                if class_card(&content.classes[ci], r, false, ci, s) {
                    self.class_index = ci;
                }
            }
        }
        let rx = x + left + 30.0 * s;
        let rw = screen_width() - rx - 40.0 * s;
        if let Some(class) = content.classes.get(self.class_index) {
            panel(Rect::new(
                rx,
                y - 15.0 * s,
                rw,
                screen_height() - y - 91.0 * s,
            ));
            label(
                &class.name,
                rx + 22.0 * s,
                y + 22.0 * s,
                30.0 * s,
                text_color(),
            );
            label(
                &format!("TIER {}  ·  LEVEL {}", class.tier + 1, class.min_level),
                rx + 22.0 * s,
                y + 48.0 * s,
                13.0 * s,
                AMBER,
            );
            render::class_preview(
                class,
                Rect::new(rx + 25.0 * s, y + 67.0 * s, rw - 50.0 * s, 139.0 * s),
                42.0 * s,
                -0.35,
                CYAN,
            );
            let mut by = wrap(
                &class.description,
                rx + 22.0 * s,
                y + 231.0 * s,
                rw - 44.0 * s,
                19.0 * s,
                text_color(),
            ) + 10.0 * s;
            label("STRENGTHS", rx + 22.0 * s, by, 12.0 * s, CYAN);
            by = wrap(
                &class.strengths,
                rx + 22.0 * s,
                by + 23.0 * s,
                rw - 44.0 * s,
                17.0 * s,
                muted_color(),
            ) + 8.0 * s;
            label("TRADEOFFS", rx + 22.0 * s, by, 12.0 * s, AMBER);
            by = wrap(
                &class.weaknesses,
                rx + 22.0 * s,
                by + 23.0 * s,
                rw - 44.0 * s,
                17.0 * s,
                muted_color(),
            ) + 8.0 * s;
            if class.ability != Ability::None {
                label(
                    &format!(
                        "ABILITY  {:?}  /  {:.0}s cooldown",
                        class.ability, class.ability_cooldown
                    ),
                    rx + 22.0 * s,
                    by,
                    14.0 * s,
                    CYAN,
                );
            }
            label(
                &format!("Level {} upgrade", class.min_level),
                rx + 22.0 * s,
                screen_height() - 114.0 * s,
                13.0 * s,
                muted_color(),
            );
        }
        Action::None
    }
    fn settings(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 158.0 * s;
        let tab_w = 155.0 * s;
        for (index, name) in ["Display", "Style", "Controls", "Accessibility", "Profile"]
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
                    set_fullscreen(settings.fullscreen);
                    let _ = settings.save();
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
                slider(
                    Rect::new(x, by + 304.0 * s, col, 48.0 * s),
                    "Interface scale",
                    &mut settings.ui_scale,
                    0.8,
                    1.4,
                    s,
                );
                label(
                    "RESOLUTION",
                    rx,
                    by + DISPLAY_RESOLUTION_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (index, (width, height)) in
                    [(1024, 768), (1280, 800), (1600, 900), (1920, 1080)]
                        .into_iter()
                        .enumerate()
                {
                    if button(
                        Rect::new(
                            rx,
                            by + (DISPLAY_RESOLUTION_ROW + index as f32 * DISPLAY_RESOLUTION_STEP)
                                * s,
                            col,
                            34.0 * s,
                        ),
                        &format!("{width} × {height}"),
                        settings.width == width && settings.height == height,
                        true,
                        s * 0.85,
                    ) {
                        settings.width = width;
                        settings.height = height;
                        if !settings.fullscreen {
                            request_new_screen_size(width as f32, height as f32);
                        }
                        let _ = settings.save();
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
                        true,
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
                slider(
                    Rect::new(x, by + 188.0 * s, col, 48.0 * s),
                    "Interface scale",
                    &mut settings.ui_scale,
                    0.8,
                    1.4,
                    s,
                );
                wrap(
                    "Reduced motion turns off camera easing, shake, and menu animation.",
                    rx,
                    by + 20.0 * s,
                    col,
                    21.0 * s,
                    text_color(),
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
                if small_button(
                    Rect::new(x, by + 206.0 * s, col, 35.0 * s),
                    "Clear recent server history",
                    s,
                ) {
                    settings.recent_servers.clear();
                }
                label("FAVORITE SERVERS", rx, by + 15.0 * s, 13.0 * s, CYAN);
                let favorites = settings.favorites.clone();
                for (index, address) in favorites.iter().take(7).enumerate() {
                    let fy = by + 32.0 * s + index as f32 * 43.0 * s;
                    if button(
                        Rect::new(rx, fy, col - 50.0 * s, 35.0 * s),
                        address,
                        settings.server_address == *address,
                        true,
                        s * 0.8,
                    ) {
                        settings.server_address = address.clone();
                    }
                    if small_button(
                        Rect::new(rx + col - 41.0 * s, fy, 41.0 * s, 35.0 * s),
                        "×",
                        s,
                    ) {
                        settings.favorites.retain(|saved| saved != address);
                    }
                }
                if favorites.is_empty() {
                    wrap(
                        "No favorites yet. Add one from Join server.",
                        rx,
                        by + 65.0 * s,
                        col,
                        20.0 * s,
                        muted_color(),
                    );
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
            "Rust  /  Macroquad  /  Renet + Netcode",
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
            "DejaVu Sans Bold font. Third-party licenses are in NOTICE.md.",
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
        Action::None
    }
    fn achievements(&self, settings: &Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 169.0 * s;
        label("Achievements", x, y, 30.0 * s, text_color());
        right_label(
            &format!(
                "{} / {} unlocked",
                settings.unlocked_achievements.len(),
                ACHIEVEMENTS.len()
            ),
            screen_width() - x,
            y,
            18.0 * s,
            muted_color(),
        );
        label(
            "Saved on this device. Sandbox and modded games do not count.",
            x,
            y + 30.0 * s,
            15.0 * s,
            muted_color(),
        );
        let gap = 12.0 * s;
        let width = (screen_width() - x * 2.0 - gap * 2.0) / 3.0;
        let height = ((screen_height() - y - 135.0 * s) / 5.0 - gap).min(85.0 * s);
        for (i, a) in ACHIEVEMENTS.iter().enumerate() {
            let earned = settings.unlocked_achievements.iter().any(|id| id == a.id);
            let r = Rect::new(
                x + (i % 3) as f32 * (width + gap),
                y + 54.0 * s + (i / 3) as f32 * (height + gap),
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
fn class_card(class: &ClassDef, r: Rect, selected: bool, index: usize, s: f32) -> bool {
    let hovered = contains(r);
    let color = card_color(index);
    draw_rectangle(
        r.x,
        r.y,
        r.w,
        r.h,
        mix(color, WHITE, if hovered { 0.38 } else { 0.2 }),
    );
    draw_rectangle(
        r.x,
        r.y + r.h * 0.60,
        r.w,
        r.h * 0.40,
        mix(color, BLACK, 0.10),
    );
    draw_rectangle_lines(
        r.x,
        r.y,
        r.w,
        r.h,
        if selected { 3.5 * s } else { 2.0 * s },
        Color::new(0.333, 0.333, 0.333, 1.0),
    );
    render::class_preview(
        class,
        Rect::new(r.x + 7.0 * s, r.y + 5.0 * s, r.w - 14.0 * s, r.h - 38.0 * s),
        19.0 * s,
        -0.45,
        CYAN,
    );
    let size = (14.0 * s).min((r.w - 10.0 * s) / (class.name.chars().count().max(1) as f32 * 0.65));
    arcade_center(&class.name, r.x + r.w * 0.5, r.y + r.h - 19.0 * s, size);
    arcade_center(
        &format!("Level {}", class.min_level),
        r.x + r.w * 0.5,
        r.y + r.h - 5.0 * s,
        10.0 * s,
    );
    hovered && is_mouse_button_pressed(MouseButton::Left)
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
        Mode::Mothership => "Protect your armed command ship.",
        Mode::CaptureCore => "Steal, carry, and return the core.",
        Mode::BossRush => "Cooperate through six boss waves.",
        Mode::Deathmatch => "Get the most kills before time runs out.",
        Mode::Sandbox => "Choose classes and test your build.",
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
                DISPLAY_RESOLUTION_ROW + 3.0 * DISPLAY_RESOLUTION_STEP + 34.0,
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
            let fit = (width / 1280.0_f32).min(height / 800.0);
            for setting_scale in [0.8_f32, 1.0, 1.4] {
                let s = (fit * setting_scale)
                    .min(((width - 24.0) / 1280.0).max(0.35))
                    .min(((height - 24.0) / 800.0).max(0.35))
                    .clamp(0.35, 2.75)
                    .min(height / 800.0)
                    .min(width / 1180.0);
                let footer_top = height - 62.0 * s;
                let last_button_bottom = (158.0 + 64.0 + groups.last().unwrap().1) * s;
                assert!(
                    last_button_bottom + 16.0 * s < footer_top,
                    "{width}x{height} at {setting_scale}"
                );
                let col = (width - 110.0 * s) * 0.5;
                let right = 40.0 * s + col + 30.0 * s;
                assert!(right + col <= width - 24.0 * s);
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
    }
}
