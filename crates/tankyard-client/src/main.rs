use crate::ui::RED;
#[cfg(all(target_os = "linux", target_env = "gnu", target_feature = "crt-static"))]
compile_error!(
    "The Linux graphical client cannot use static glibc with runtime-loaded GPU drivers. Use scripts/build_linux_compatible.sh; headless binaries can still be built statically."
);

mod community_ui;
mod effects;
mod graphics;
mod hud;
mod menu;
mod notifications;
mod practice_ui;
mod render;
mod session;
mod settings;
mod startup;
use tankyard_client::achievements;
use tankyard_client::tank_model;
mod ui;

use clap::Parser;
use hud::{Hud, HudAction};
use macroquad::prelude::*;
use menu::{Action, Menu, Page};
use session::Session;
use settings::Settings;
use tankyard_content::Content;
use tankyard_core::{DT, Mode, PlayerInput, V2};
use ui::*;

#[derive(Parser, Debug)]
#[command(name = "tankyard", version, about = "Tank Yard - tank arena")]
struct Cli {
    /// Start windowed with multisampling and anisotropic filtering disabled.
    #[arg(long)]
    safe_graphics: bool,
    #[arg(long)]
    offline: bool,
    /// Open a practice challenge directly.
    #[arg(long, value_parser = ["shape_sprint", "drone_drill", "skirmish", "last_stand"], conflicts_with_all = ["offline", "host", "join", "connect_token", "class", "level"])]
    practice: Option<String>,
    #[arg(long)]
    host: bool,
    #[arg(long)]
    join: Option<String>,
    #[arg(long, default_value = "ffa")]
    mode: String,
    #[arg(long, default_value_t = 12)]
    bots: usize,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, default_value_t = 3600.0)]
    width: f32,
    #[arg(long, default_value_t = 2600.0)]
    height: f32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long)]
    map: Option<String>,
    #[arg(long)]
    class: Option<String>,
    #[arg(long)]
    level: Option<u8>,
    #[arg(long, default_value = "")]
    password: String,
    #[arg(long)]
    no_audio: bool,
    #[arg(long)]
    smoke_frames: Option<u64>,
    #[arg(long)]
    screenshot: Option<String>,
    #[arg(long = "mod")]
    mods: Vec<std::path::PathBuf>,
    #[arg(long)]
    connect_token: Option<std::path::PathBuf>,
    #[arg(long)]
    account_token_file: Option<std::path::PathBuf>,
}
fn window_conf(cli: &Cli) -> Conf {
    let mut settings = Settings::load();
    if cli.safe_graphics {
        settings.use_safe_graphics();
    }
    Conf {
        window_title: "Tank Yard".into(),
        window_width: settings.width as i32,
        window_height: settings.height as i32,
        high_dpi: true,
        // Native framebuffer MSAA, with a selectable quality level.
        sample_count: match settings.anti_aliasing {
            8 => 8,
            4 => 4,
            _ => 1,
        },
        fullscreen: settings.fullscreen,
        platform: macroquad::miniquad::conf::Platform {
            swap_interval: Some(if settings.vsync { 1 } else { 0 }),
            ..Default::default()
        },
        ..Default::default()
    }
}
struct App {
    settings: Settings,
    content: Content,
    menu: Menu,
    hud: Hud,
    session: Option<Session>,
    view: render::View,
    effects: effects::Effects,
    paused: bool,
    menu_over_game: bool,
    menu_return_paused: bool,
    accumulator: f32,
    sequence: u32,
    frames: u64,
    smoke: Option<u64>,
    screenshot: Option<String>,
    quit: bool,
    respawn_requested: bool,
    ability_queued: bool,
    fire_queued: bool,
    achievement_feed: achievements::Feed,
    bundle_revision: u64,
    community: community_ui::CommunityUi,
    catalog_paths: Vec<String>,
}
impl App {
    async fn new(cli: Cli) -> Self {
        ui::init_font();
        // Older launch scripts still pass this flag.
        let _ = cli.no_audio;
        let mut settings = Settings::load();
        if cli.safe_graphics {
            settings.use_safe_graphics();
        }
        if !cli.mods.is_empty() {
            settings.enabled_mods = cli
                .mods
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
        }
        if let Some(name) = cli.name {
            settings.player_name = name;
            settings.validate();
        }
        let mut content = Content::builtin();
        if let Err(error) = content.validate() {
            log::error!("Invalid built-in content: {error}");
        }
        let mut menu = Menu::new();
        menu.difficulty = settings.bot_difficulty;
        let paths: Vec<_> = settings
            .enabled_mods
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        match tankyard_mods::load(&paths) {
            Ok(loaded) => {
                graphics::load(&loaded.bundle.textures, settings.anisotropic_filtering);
                content = loaded.bundle.content;
                menu.maps = loaded.bundle.maps;
            }
            Err(error) => menu.error = error,
        }
        menu.select_mode(parse_mode(&cli.mode));
        menu.bots = cli.bots.min(64);
        menu.password = cli.password;
        menu.width = cli.width;
        menu.height = cli.height;
        menu.seed = cli.seed;
        if let Some(map) = cli.map {
            menu.map = menu
                .maps
                .iter()
                .position(|m| m.id == map)
                .unwrap_or(menu.map);
        }
        let catalog_paths = settings.enabled_mods.clone();
        let mut app = Self {
            settings,
            content,
            menu,
            hud: Hud::new(),
            session: None,
            view: render::View::new(),
            effects: effects::Effects::new(),
            paused: false,
            menu_over_game: false,
            menu_return_paused: false,
            accumulator: 0.0,
            sequence: 0,
            frames: 0,
            smoke: cli.smoke_frames,
            screenshot: cli.screenshot,
            quit: false,
            respawn_requested: false,
            ability_queued: false,
            fire_queued: false,
            achievement_feed: achievements::Feed::default(),
            bundle_revision: 0,
            community: community_ui::CommunityUi::default(),
            catalog_paths,
        };
        if let Some(id) = cli.practice {
            if let Some(index) = tankyard_client::practice::CHALLENGES
                .iter()
                .position(|challenge| challenge.id == id)
            {
                app.handle(Action::Practice(index));
            }
        } else if let Some(path) = cli.connect_token {
            let result = (|| -> Result<Session, String> {
                let credential = if let Some(path) = cli.account_token_file {
                    use std::io::Read;
                    let mut text = String::new();
                    std::fs::File::open(path)
                        .map_err(|e| e.to_string())?
                        .take(1025)
                        .read_to_string(&mut text)
                        .map_err(|e| e.to_string())?;
                    if text.len() > 1024 {
                        return Err("Account credential exceeds 1024 bytes.".into());
                    }
                    Some(text.trim().to_owned())
                } else {
                    None
                };
                Session::join_token(
                    &path,
                    &app.settings.player_name,
                    &app.menu.password,
                    credential,
                )
            })();
            match result {
                Ok(session) => app.launch(session),
                Err(error) => app.menu.error = error,
            }
        } else if let Some(address) = cli.join {
            app.settings.server_address = address;
            app.handle(Action::Join);
        } else if cli.host {
            app.handle(Action::Host);
        } else if cli.offline || app.smoke.is_some() {
            app.handle(Action::Offline);
        }
        if let Some(session) = &mut app.session {
            if let Some(level) = cli.level {
                session.sandbox_level(level);
            }
            if let Some(class) = cli.class {
                session.sandbox_class(&class);
            }
        }
        app
    }
    fn scale(&self) -> f32 {
        // Keep the HUD inside the window after applying the user's scale setting.
        let fit = (screen_width() / 1280.0).min(screen_height() / 800.0);
        (fit * self.settings.ui_scale)
            .min((screen_width() - 24.0).max(320.0) / 1280.0)
            .min((screen_height() - 24.0).max(240.0) / 800.0)
            .clamp(0.35, 2.75)
    }
    fn launch(&mut self, session: Session) {
        if self.session.is_some() {
            self.save_progress();
        }
        self.content.clone_from(&session.bundle.content);
        self.bundle_revision = session.bundle_revision;
        graphics::load(
            &session.bundle.textures,
            self.settings.anisotropic_filtering,
        );
        self.session = Some(session);
        self.menu_over_game = false;
        self.menu_return_paused = false;
        self.paused = false;
        self.accumulator = 0.0;
        self.sequence = 0;
        self.respawn_requested = false;
        self.ability_queued = false;
        self.fire_queued = false;
        self.menu.clear_focus();
        self.effects = effects::Effects::new();
        self.hud = Hud::new();
        self.hud.practice = self
            .session
            .as_ref()
            .is_some_and(|session| session.practice.is_some());
        self.community = community_ui::CommunityUi::default();
        self.menu.error.clear();
        if let Some(tank) = self
            .session
            .as_ref()
            .and_then(|s| s.display.as_ref()?.tanks.iter().find(|t| t.id == s.player))
        {
            self.view.center = tank.pos;
        }
    }
    fn handle(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Page(page) => {
                self.save_menu_settings();
                self.menu.go(page);
            }
            Action::Back => {
                self.save_menu_settings();
                if self.menu.back() && self.menu_over_game {
                    self.menu_over_game = false;
                    self.paused = self.menu_return_paused;
                }
            }
            Action::Offline => {
                let paths: Vec<_> = self
                    .settings
                    .enabled_mods
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect();
                let attempt = if paths.is_empty() {
                    Ok(Session::offline(
                        self.menu.config(),
                        self.menu.bots,
                        &self.settings.player_name,
                    ))
                } else {
                    Session::offline_modded(
                        self.menu.config(),
                        self.menu.bots,
                        &self.settings.player_name,
                        &paths,
                    )
                };
                match attempt {
                    Ok(mut session) => {
                        if let session::Engine::Offline(local) = &mut session.engine {
                            local.bots.difficulty =
                                tankyard_server::difficulty_from_name(self.menu.difficulty_name())
                                    .unwrap_or_default();
                        }
                        self.launch(session);
                    }
                    Err(error) => self.menu.error = error,
                }
            }
            Action::Host => match Session::host(
                self.menu.config(),
                self.menu.bots,
                &self.settings.player_name,
                &self.menu.password,
                self.settings
                    .enabled_mods
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect(),
                self.menu.difficulty_name(),
            ) {
                Ok(session) => self.launch(session),
                Err(error) => {
                    self.menu.error = error;
                    self.menu.page = Page::Host;
                }
            },
            Action::Join => match Session::join(
                &self.settings.server_address,
                &self.settings.player_name,
                &self.menu.password,
            ) {
                Ok(session) => {
                    let address = self.settings.server_address.clone();
                    self.settings.remember_server(&address);
                    let _ = self.settings.save();
                    self.launch(session);
                }
                Err(error) => {
                    self.menu.error = error;
                    self.menu.page = Page::Join;
                }
            },
            Action::Practice(index) => {
                match Session::practice(index, &self.settings.player_name, self.menu.seed) {
                    Ok(session) => self.launch(session),
                    Err(error) => {
                        self.menu.error = error;
                        self.menu.page = Page::Practice;
                    }
                }
            }
            Action::Quit => {
                self.quit = true;
            }
            Action::Save => match self.settings.save() {
                Ok(()) => self.menu.error = "Settings saved.".into(),
                Err(error) => self.menu.error = format!("Could not save: {error}"),
            },
        }
    }
    fn finish_session(&mut self) {
        self.save_progress();
        self.community = community_ui::CommunityUi::default();
        self.restore_menu();
    }
    fn save_progress(&mut self) {
        if let Some(mut session) = self.session.take() {
            if let Some(run) = &mut session.practice {
                tankyard_client::practice::record(&mut self.settings.practice_records, run);
            }
            self.settings.record_session(&session.achievement_tracker);
        }
        if let Err(error) = self.settings.save() {
            log::warn!("Cannot save local settings: {error}");
        }
    }
    fn restore_menu(&mut self) {
        let paths: Vec<_> = self
            .settings
            .enabled_mods
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        match tankyard_mods::load(&paths) {
            Ok(loaded) => {
                self.content = loaded.bundle.content;
                self.menu.maps = loaded.bundle.maps;
                self.menu.map = self.menu.map.min(self.menu.maps.len().saturating_sub(1));
                graphics::load(&loaded.bundle.textures, self.settings.anisotropic_filtering);
            }
            Err(error) => {
                self.content = Content::builtin();
                self.menu.maps = tankyard_content::builtin_maps();
                log::warn!("Cannot restore local mods: {error}");
            }
        }
        self.paused = false;
        self.menu_over_game = false;
        self.menu.reset(Page::Main);
        self.respawn_requested = false;
        while get_char_pressed().is_some() {}
        self.menu.error.clear();
    }
    fn open_menu(&mut self, page: Page) {
        self.save_menu_settings();
        if self.menu_over_game {
            self.menu.go(page);
        } else {
            self.menu_return_paused = self.paused;
            self.menu.reset(page);
        }
        self.hud.evolution = false;
        self.hud.sandbox = false;
        self.menu_over_game = true;
        self.paused = true;
    }
    fn save_menu_settings(&mut self) {
        if matches!(self.menu.page, Page::Settings | Page::Mods)
            && let Err(error) = self.settings.save()
        {
            log::warn!("Cannot save menu settings: {error}");
        }
    }
    fn shortcut_page(&mut self, page: Page) {
        if self.menu.page == page && (self.menu_over_game || self.session.is_none()) {
            self.handle(Action::Back);
        } else if self.session.is_some() {
            self.open_menu(page);
        } else {
            self.handle(Action::Page(page));
        }
    }
    fn restart_offline(&mut self) {
        if let Some(index) = self
            .session
            .as_ref()
            .and_then(|session| session.practice.as_ref().map(|run| run.index))
        {
            self.handle(Action::Practice(index));
        } else if self.session.as_ref().is_some_and(Session::offline_mode) {
            self.handle(Action::Offline);
        }
    }
    fn shortcuts(&mut self) -> bool {
        if self.menu.binding.is_some() {
            if is_key_pressed(KeyCode::Escape) {
                self.menu.binding = None;
                return true;
            }
            return false;
        }
        if self.community.focused() || self.hud.sandbox {
            return false;
        }
        if is_key_pressed(KeyCode::F1) {
            self.shortcut_page(Page::Controls);
        } else if is_key_pressed(KeyCode::F6) {
            self.shortcut_page(Page::Practice);
        } else if is_key_pressed(KeyCode::F10) {
            self.shortcut_page(Page::Settings);
        } else if is_key_pressed(KeyCode::F5)
            && !self.menu_over_game
            && self.session.as_ref().is_some_and(|session| {
                session.offline_mode()
                    && (self.paused || session.practice.as_ref().is_some_and(|run| run.done()))
            })
        {
            self.restart_offline();
            return true;
        }
        if is_key_pressed(KeyCode::F3) {
            self.settings.show_fps = !self.settings.show_fps;
            if let Err(error) = self.settings.save() {
                log::warn!("Cannot save diagnostics preference: {error}");
            }
        }
        if self.session.is_none() && is_key_pressed(KeyCode::Escape) {
            if self.menu.page == Page::Main {
                self.menu.clear_focus();
            } else {
                self.handle(Action::Back);
            }
            return true;
        }
        false
    }
    fn input(&self, s: f32) -> PlayerInput {
        let blocked =
            self.paused || self.menu_over_game || self.hud.sandbox || self.community.focused();
        if blocked {
            return self.session.as_ref().map_or(
                PlayerInput {
                    sequence: self.sequence,
                    ..Default::default()
                },
                |session| session.neutral_input(self.sequence),
            );
        }
        // The ability input requests a respawn while the player is dead.
        let awaiting_respawn = self
            .session
            .as_ref()
            .and_then(|session| {
                session
                    .display
                    .as_ref()?
                    .tanks
                    .iter()
                    .find(|tank| tank.id == session.player)
            })
            .is_some_and(|tank| !tank.alive);
        if awaiting_respawn {
            return PlayerInput {
                sequence: self.sequence,
                ability: self.respawn_requested,
                ..Default::default()
            };
        }
        let mut movement = V2::ZERO;
        if is_key_down(self.settings.key(0)) {
            movement.y -= 1.0;
        }
        if is_key_down(self.settings.key(1)) {
            movement.y += 1.0;
        }
        if is_key_down(self.settings.key(2)) {
            movement.x -= 1.0;
        }
        if is_key_down(self.settings.key(3)) {
            movement.x += 1.0;
        }
        let position = self
            .session
            .as_ref()
            .and_then(|session| {
                session
                    .display
                    .as_ref()?
                    .tanks
                    .iter()
                    .find(|t| t.id == session.player)
            })
            .map(|t| t.pos)
            .unwrap_or(self.view.center);
        let (mx, my) = mouse_position();
        let cursor = self.view.world(vec2(mx, my)) - position;
        let aim = cursor.angle();
        let over_ui = self.pointer_over_ui(s);
        let fire = self.settings.auto_fire
            || ((is_mouse_button_down(MouseButton::Left) || self.fire_queued) && !over_ui);
        PlayerInput {
            sequence: self.sequence,
            movement: movement.normalized(),
            aim,
            aim_distance: cursor.length().min(5000.),
            fire,
            alt_fire: is_mouse_button_down(MouseButton::Right),
            ability: is_key_down(self.settings.key(4)) || self.ability_queued,
        }
    }
    fn pointer_over_ui(&self, s: f32) -> bool {
        self.session.as_ref().is_some_and(|session| {
            let community = match &session.engine {
                session::Engine::Online { client, .. } => {
                    self.community.captures_pointer(client, s)
                }
                _ => false,
            };
            community
                || session.display.as_ref().is_some_and(|snapshot| {
                    self.hud
                        .captures_pointer(snapshot, &self.content, session.player, s)
                })
        })
    }
    fn update(&mut self, dt: f32, s: f32) {
        if self.session.is_none() && self.catalog_paths != self.settings.enabled_mods {
            self.catalog_paths.clone_from(&self.settings.enabled_mods);
            let paths: Vec<_> = self
                .catalog_paths
                .iter()
                .map(std::path::PathBuf::from)
                .collect();
            match tankyard_mods::load(&paths) {
                Ok(loaded) => {
                    self.content = loaded.bundle.content;
                    self.menu.maps = loaded.bundle.maps;
                    graphics::load(&loaded.bundle.textures, self.settings.anisotropic_filtering);
                }
                Err(error) => self.menu.error = error,
            }
        }
        self.effects.update(dt);
        self.achievement_feed.update(dt);
        let now = get_time();
        if let Some(session) = &mut self.session {
            session.update_network(dt, now, &self.content);
            if self.bundle_revision != session.bundle_revision {
                self.content.clone_from(&session.bundle.content);
                self.bundle_revision = session.bundle_revision;
                graphics::load(
                    &session.bundle.textures,
                    self.settings.anisotropic_filtering,
                );
                self.hud = Hud::new();
            }
            if let Some(message) = session.notice.take() {
                self.community.message(&message);
                self.effects.notice(&message);
            }
        }
        if self.shortcuts() {
            return;
        }
        if self.session.is_none() {
            return;
        }
        if let Some(run) = self
            .session
            .as_ref()
            .and_then(|session| session.practice.as_ref())
            && run.done()
            && !self.menu_over_game
            && !self.paused
        {
            if is_key_pressed(KeyCode::R) {
                let index = run.index;
                self.save_progress();
                self.handle(Action::Practice(index));
            } else if is_key_pressed(KeyCode::Escape) {
                self.finish_session();
                self.menu.page = Page::Practice;
            }
            return;
        }
        let mut community_key = false;
        if !self.menu_over_game
            && !self.paused
            && let Some(session) = &mut self.session
            && let session::Engine::Online { client, .. } = &mut session.engine
        {
            let alive = session.display.as_ref().is_some_and(|snapshot| {
                snapshot
                    .tanks
                    .iter()
                    .any(|t| t.id == session.player && t.alive)
            });
            community_key = self.community.keys(client, alive);
        }
        if !self.menu_over_game {
            if !community_key && !self.community.focused() && is_key_pressed(KeyCode::Escape) {
                self.paused = !self.paused;
                self.hud.evolution = false;
                self.hud.sandbox = false;
            }
            if !self.paused && !community_key && !self.community.focused() {
                if is_key_pressed(self.settings.key(5)) {
                    self.settings.auto_fire = !self.settings.auto_fire;
                }
                if is_key_pressed(KeyCode::Tab) {
                    self.hud.evolution = !self.hud.evolution;
                }
                if (is_key_pressed(KeyCode::PageUp) || is_key_pressed(KeyCode::PageDown))
                    && let Some(session) = &mut self.session
                {
                    let step =
                        if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                            15
                        } else {
                            1
                        };
                    session.sandbox_adjust_level(if is_key_pressed(KeyCode::PageUp) {
                        step
                    } else {
                        -step
                    });
                }
                if is_key_pressed(KeyCode::T)
                    && self
                        .session
                        .as_ref()
                        .is_some_and(|session| session.practice.is_none())
                    && self
                        .session
                        .as_ref()
                        .and_then(|session| session.display.as_ref())
                        .is_some_and(|snapshot| snapshot.config.mode == Mode::Sandbox)
                {
                    self.hud.sandbox = !self.hud.sandbox;
                }
                for (index, key) in [
                    KeyCode::Key1,
                    KeyCode::Key2,
                    KeyCode::Key3,
                    KeyCode::Key4,
                    KeyCode::Key5,
                    KeyCode::Key6,
                    KeyCode::Key7,
                    KeyCode::Key8,
                ]
                .into_iter()
                .enumerate()
                {
                    if is_key_pressed(key)
                        && let Some(session) = &mut self.session
                    {
                        session.upgrade_stat(
                            STAT_ORDER[index],
                            is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
                        );
                    }
                }
            }
        } else if is_key_pressed(KeyCode::Escape) {
            self.handle(Action::Back);
        }
        if !community_key
            && !self.community.focused()
            && !self.paused
            && !self.menu_over_game
            && is_key_pressed(KeyCode::Enter)
            && self
                .session
                .as_ref()
                .and_then(|session| {
                    session
                        .display
                        .as_ref()?
                        .tanks
                        .iter()
                        .find(|t| t.id == session.player)
                })
                .is_some_and(|t| !t.alive)
        {
            self.respawn_requested = true;
        }
        let offline = self.session.as_ref().is_some_and(|s| s.offline_mode());
        // Keep short taps until a simulation tick consumes them, including at
        // render rates above 60 Hz and down/up events within one frame.
        let can_attack = !self.paused
            && !self.menu_over_game
            && !self.hud.sandbox
            && !self.community.focused()
            && self.session.as_ref().is_some_and(|session| {
                session.display.as_ref().is_some_and(|snapshot| {
                    !snapshot.match_state.finished
                        && snapshot
                            .tanks
                            .iter()
                            .any(|tank| tank.id == session.player && tank.alive)
                })
            });
        if can_attack {
            self.ability_queued |= is_key_pressed(self.settings.key(4));
            self.fire_queued |=
                is_mouse_button_pressed(MouseButton::Left) && !self.pointer_over_ui(s);
        } else {
            self.ability_queued = false;
            self.fire_queued = false;
        }
        let speed = if offline { self.hud.speed } else { 1.0 };
        if !(offline && (self.paused || self.menu_over_game)) {
            self.accumulator += (dt * speed).min(DT * 8.0);
        }
        let mut steps = 0;
        while self.accumulator >= DT && steps < 8 {
            self.sequence = self.sequence.wrapping_add(1);
            let mut input = self.input(s);
            if self.smoke.is_some() && !self.paused {
                input.movement = V2::new((now as f32 * 0.4).cos(), (now as f32 * 0.4).sin()) * 0.4;
                input.aim = now as f32 * 0.25;
                input.fire = true;
            }
            if let Some(session) = &mut self.session {
                session.tick(input, &self.content);
            }
            self.ability_queued = false;
            self.fire_queued = false;
            self.accumulator -= DT;
            steps += 1;
        }
        if self.accumulator >= DT {
            if let Some(session) = &mut self.session {
                session.dropped_ticks += (self.accumulator / DT) as u64;
            }
            self.accumulator = 0.0;
        }
        if let Some(run) = self
            .session
            .as_mut()
            .and_then(|session| session.practice.as_mut())
            && run.done()
            && !run.recorded
        {
            tankyard_client::practice::record(&mut self.settings.practice_records, run);
            if let Err(error) = self.settings.save() {
                log::warn!("Cannot save practice records: {error}");
            }
        }
        if let Some(session) = &mut self.session {
            session.present_prediction(self.accumulator / DT);
        }
        if let Some(session) = &self.session
            && let Some(snapshot) = &session.display
            && let Some(tank) = snapshot.tanks.iter().find(|t| t.id == session.player)
        {
            let shake = if self.settings.reduced_motion {
                0.0
            } else {
                self.effects.shake * self.settings.shake
            };
            let followed = if !tank.alive && snapshot.config.mode == Mode::CoopPve {
                snapshot
                    .tanks
                    .iter()
                    .filter(|ally| ally.alive && ally.team == tank.team)
                    .min_by(|a, b| {
                        a.pos
                            .distance(tank.pos)
                            .total_cmp(&b.pos.distance(tank.pos))
                    })
                    .unwrap_or(tank)
            } else {
                tank
            };
            let position = session.position(followed.id, followed.pos, self.accumulator / DT);
            self.view.follow(
                position,
                dt,
                shake,
                if self.settings.reduced_motion {
                    0.0
                } else {
                    self.settings.camera_smoothing
                },
            );
            // Ease remote aim along the shortest arc across +/-PI.
            let easing = if self.settings.reduced_motion {
                1.0
            } else {
                1.0 - (-dt * 22.0).exp()
            };
            self.effects.sync_round(snapshot.match_state.round);
            self.effects
                .sync_barrels(&snapshot.tanks, dt, self.settings.reduced_motion);
            for other in snapshot.tanks.iter().filter(|t| t.alive) {
                let angle = self
                    .effects
                    .display_aim
                    .entry(other.id)
                    .or_insert(other.aim);
                let delta = (other.aim - *angle + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                // Local aim follows the cursor immediately.
                *angle += delta
                    * if other.id == session.player {
                        1.0
                    } else {
                        easing
                    };
            }
            // Sample local aim between simulation ticks, too.
            if tank.alive
                && !self.paused
                && !self.menu_over_game
                && !self.hud.sandbox
                && !self.community.focused()
                && self.smoke.is_none()
            {
                let (mx, my) = mouse_position();
                self.effects
                    .display_aim
                    .insert(tank.id, (self.view.world(vec2(mx, my)) - position).angle());
            }
            self.effects.sync_projectiles(
                snapshot.tick,
                &snapshot.projectiles,
                session.render_fraction(self.accumulator / DT),
                dt,
                self.settings.reduced_motion,
            );
            let _ = self
                .effects
                .ingest(&session.events, session.player, self.settings.effects);
        }
        if let Some(session) = &mut self.session {
            session.events.clear();
            if session
                .display
                .as_ref()
                .and_then(|snapshot| snapshot.tanks.iter().find(|tank| tank.id == session.player))
                .is_some_and(|tank| tank.alive)
            {
                self.respawn_requested = false;
            }
        }
        if let Some(session) = &self.session
            && let Some(progress) = session
                .achievement_tracker
                .progress(self.settings.games_played, self.settings.total_kills)
            && achievements::unlock(
                progress,
                &mut self.settings.unlocked_achievements,
                &mut self.achievement_feed,
            )
            && let Err(error) = self.settings.save()
        {
            log::warn!("Cannot save achievements: {error}");
        }
    }
    fn draw(&mut self, s: f32) {
        if self.session.is_none() || self.menu_over_game {
            let menu_scale = s.min(screen_height() / 800.0).min(screen_width() / 1180.0);
            let action = self
                .menu
                .draw(&self.content, &mut self.settings, menu_scale);
            self.handle(action);
            notifications::draw(&self.achievement_feed, s, self.settings.reduced_motion);
            return;
        }
        let mut actions = Vec::new();
        if let Some(session) = &self.session {
            set_interactive(
                !self.paused
                    && !self.community.focused()
                    && !session.practice.as_ref().is_some_and(|run| run.done()),
            );
            if let Some(snapshot) = &session.display {
                render::world(
                    snapshot,
                    &self.content,
                    session.player,
                    &self.view,
                    &self.effects,
                    &self.settings,
                    session.render_fraction(self.accumulator / DT),
                    session.animation_time(self.accumulator / DT),
                    session.local_animation_time(self.accumulator / DT),
                    &session.previous_positions,
                );
                actions = self.hud.draw(
                    snapshot,
                    &self.content,
                    session.player,
                    &self.settings,
                    session.diagnostics(),
                    s,
                    &self.view,
                );
                if self.effects.notice_life > 0.0 {
                    let opacity = (self.effects.notice_life / 0.35).min(1.0);
                    center_label(
                        &self.effects.notice,
                        screen_width() * 0.5,
                        screen_height() * 0.25,
                        25.0 * s,
                        alpha(AMBER, opacity),
                    );
                }
            } else {
                render::menu_background(get_time() as f32);
                center_label(
                    "Connecting…",
                    screen_width() * 0.5,
                    screen_height() * 0.45,
                    32.0 * s,
                    CYAN,
                );
                center_label(
                    &self.settings.server_address,
                    screen_width() * 0.5,
                    screen_height() * 0.45 + 38.0 * s,
                    18.0 * s,
                    muted_color(),
                );
                if small_button(
                    Rect::new(
                        screen_width() * 0.5 - 100.0 * s,
                        screen_height() * 0.45 + 75.0 * s,
                        200.0 * s,
                        38.0 * s,
                    ),
                    "Cancel",
                    s,
                ) {
                    actions.push(HudAction::Menu);
                }
            }
            if let Some(error) = &session.failure {
                draw_rectangle(
                    0.0,
                    0.0,
                    screen_width(),
                    screen_height(),
                    alpha(bg_color(), 0.92),
                );
                center_label(
                    "Disconnected",
                    screen_width() * 0.5,
                    screen_height() * 0.4,
                    34.0 * s,
                    RED,
                );
                wrap(
                    error,
                    screen_width() * 0.5 - 320.0 * s,
                    screen_height() * 0.4 + 50.0 * s,
                    640.0 * s,
                    21.0 * s,
                    text_color(),
                );
                if button(
                    Rect::new(
                        screen_width() * 0.5 - 125.0 * s,
                        screen_height() * 0.6,
                        250.0 * s,
                        42.0 * s,
                    ),
                    "Return to menu",
                    true,
                    true,
                    s,
                ) {
                    actions.push(HudAction::Menu);
                }
            }
        }
        set_interactive(true);
        if !self.paused
            && let Some(session) = &mut self.session
            && session.failure.is_none()
            && let session::Engine::Online { client, .. } = &mut session.engine
            && let Some(snapshot) = &session.display
        {
            self.community
                .draw(client, snapshot, s, self.hud.evolution || self.hud.sandbox);
        }
        for action in actions {
            self.hud_action(action);
        }
        if self.paused && !self.menu_over_game && self.session.is_some() {
            self.pause(s);
        }
        if !self.paused
            && let Some(run) = self
                .session
                .as_ref()
                .and_then(|session| session.practice.as_ref())
        {
            match practice_ui::draw(run, &self.settings.practice_records, s) {
                practice_ui::Action::None => {}
                practice_ui::Action::Retry(index) => {
                    self.save_progress();
                    self.handle(Action::Practice(index));
                }
                practice_ui::Action::Browse => {
                    self.finish_session();
                    self.menu.page = Page::Practice;
                }
            }
        }
        notifications::draw(&self.achievement_feed, s, self.settings.reduced_motion);
    }
    fn hud_action(&mut self, action: HudAction) {
        match action {
            HudAction::Respawn => {
                self.respawn_requested = true;
            }
            HudAction::Stat(index) => {
                if let Some(session) = &mut self.session {
                    session.upgrade_stat(
                        index,
                        is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
                    );
                }
            }
            HudAction::ToggleTheme => {
                self.settings.dark_arena = !self.settings.dark_arena;
                let _ = self.settings.save();
            }
            HudAction::Evolve(class) => {
                if let Some(session) = &mut self.session {
                    session.evolve(&class);
                }
            }
            HudAction::Encyclopedia => {
                if let Some(tank) = self
                    .session
                    .as_ref()
                    .and_then(|s| s.display.as_ref()?.tanks.iter().find(|t| t.id == s.player))
                {
                    self.menu.class_index = self
                        .content
                        .classes
                        .iter()
                        .position(|c| c.id == tank.class)
                        .unwrap_or(0);
                    self.menu.tree = true;
                }
                self.hud.evolution = false;
                self.open_menu(Page::Encyclopedia);
            }
            HudAction::Pause => self.paused = true,
            HudAction::Controls => self.open_menu(Page::Controls),
            HudAction::Menu => self.finish_session(),
            HudAction::SandboxLevel(level) => {
                if let Some(session) = &mut self.session {
                    session.sandbox_level(level);
                }
            }
            HudAction::SandboxClass(class) => {
                if let Some(session) = &mut self.session {
                    session.sandbox_class(&class);
                }
            }
            HudAction::Bot => {
                if let Some(session) = &mut self.session {
                    session.add_bot();
                }
            }
            HudAction::Invulnerable(enabled) => {
                if let Some(session) = &mut self.session {
                    session.sandbox_invulnerability(enabled);
                }
            }
            HudAction::Boss(kind) => {
                if let Some(session) = &mut self.session {
                    session.sandbox_boss(kind);
                }
            }
            HudAction::Speed(value) => {
                if self.session.as_ref().is_some_and(|s| s.offline_mode()) {
                    self.hud.speed = value;
                } else {
                    self.effects
                        .notice("Game speed is controlled by the server");
                    self.hud.speed = 1.0;
                }
            }
        }
    }
    fn pause(&mut self, s: f32) {
        draw_rectangle(
            0.0,
            0.0,
            screen_width(),
            screen_height(),
            alpha(bg_color(), 0.88),
        );
        let x = screen_width() * 0.5 - 145.0 * s;
        let y = screen_height() * 0.5 - 225.0 * s;
        center_label("PAUSED", screen_width() * 0.5, y, 38.0 * s, text_color());
        let offline = self.session.as_ref().is_some_and(|s| s.offline_mode());
        if !offline {
            center_label(
                "Online game is still running",
                screen_width() * 0.5,
                y + 30.0 * s,
                16.0 * s,
                muted_color(),
            );
        }
        let mut items = vec![
            "Resume",
            "Settings [F10]",
            "Controls [F1]",
            "Practice [F6]",
            "Tank tree",
        ];
        if offline {
            items.push("Restart [F5]");
        }
        items.extend(["Return to menu", "Quit"]);
        for (index, name) in items.iter().enumerate() {
            if button(
                Rect::new(
                    x,
                    y + 58.0 * s + index as f32 * 44.0 * s,
                    290.0 * s,
                    39.0 * s,
                ),
                name,
                index == 0,
                true,
                s * 0.9,
            ) {
                match *name {
                    "Resume" => self.paused = false,
                    "Settings [F10]" => self.open_menu(Page::Settings),
                    "Controls [F1]" => self.open_menu(Page::Controls),
                    "Practice [F6]" => self.open_menu(Page::Practice),
                    "Tank tree" => self.open_menu(Page::Encyclopedia),
                    "Restart [F5]" => self.restart_offline(),
                    "Return to menu" => self.finish_session(),
                    _ => self.quit = true,
                }
            }
        }
    }
}
fn main() {
    let cli = Cli::parse();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let conf = window_conf(&cli);
    startup::install(&conf);
    macroquad::Window::from_config(conf, run(cli));
}

async fn run(cli: Cli) {
    prevent_quit();
    // Drop GPU resources before the window and its OpenGL library are closed.
    let _resources = graphics::Resources;
    let mut app = App::new(cli).await;
    // Render directly at the window's drawable resolution.
    macroquad::texture::set_default_filter_mode(FilterMode::Linear);
    loop {
        if is_quit_requested() {
            break;
        }
        let start = get_time();
        let dt = get_frame_time().clamp(0.0, 0.15);
        // Recalculate camera zoom after resizing and as the tank levels up.
        let level = app
            .session
            .as_ref()
            .and_then(|session| {
                session
                    .display
                    .as_ref()?
                    .tanks
                    .iter()
                    .find(|t| t.id == session.player)
            })
            .map_or(1, |tank| tank.level);
        let target_zoom = (screen_height() / 900.0).clamp(0.7, 2.75)
            / (1.0 + (f32::from(level.saturating_sub(1)) * 0.006));
        let ease = 1.0 - (-dt * 9.0).exp();
        app.view.zoom += (target_zoom - app.view.zoom) * ease;
        if is_key_pressed(KeyCode::F9) {
            app.settings.dark_arena = !app.settings.dark_arena;
            let _ = app.settings.save();
        }
        if is_key_pressed(KeyCode::F11) {
            app.settings.fullscreen = !app.settings.fullscreen;
            set_fullscreen(app.settings.fullscreen);
            let _ = app.settings.save();
        }
        set_theme(app.settings.dark_arena);
        graphics::set_quality(app.settings.shape_quality);
        graphics::set_filter(app.settings.anisotropic_filtering);
        if app.frames == 1 {
            graphics::refresh_capabilities(app.settings.anisotropic_filtering);
        }
        set_reduced_motion(app.settings.reduced_motion);
        let s = app.scale();
        app.update(dt, s);
        app.draw(s);
        // Discard characters that no field consumed this frame.
        while get_char_pressed().is_some() {}
        if let Some(session) = &mut app.session {
            session.flush_network();
        }
        app.frames += 1;
        if app.smoke.is_some_and(|limit| app.frames >= limit) {
            if let Some(path) = &app.screenshot {
                get_screen_data().export_png(path);
            }
            let snapshot = app.session.as_ref().and_then(|s| s.display.as_ref());
            println!(
                "Tank Yard smoke: frames={} tick={} tanks={} projectiles={} targets={} mode={}",
                app.frames,
                snapshot.map(|s| s.tick).unwrap_or(0),
                snapshot.map(|s| s.tanks.len()).unwrap_or(0),
                snapshot.map(|s| s.projectiles.len()).unwrap_or(0),
                snapshot.map(|s| s.targets.len()).unwrap_or(0),
                app.menu.mode.name()
            );
            break;
        }
        if app.quit {
            break;
        }
        if app.settings.frame_cap > 0 {
            let target = 1.0 / app.settings.frame_cap as f64;
            let remaining = target - (get_time() - start);
            if remaining > 0.001 {
                std::thread::sleep(std::time::Duration::from_secs_f64(remaining - 0.0005));
            }
        }
        next_frame().await;
    }
    app.save_progress();
}
fn parse_mode(value: &str) -> Mode {
    match value
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "")
        .as_str()
    {
        "2teams" | "twoteams" => Mode::TwoTeams,
        "4teams" | "fourteams" => Mode::FourTeams,
        "maze" => Mode::Maze,
        "domination" => Mode::Domination,
        "survival" => Mode::Survival,
        "tag" => Mode::Tag,
        "mothership" => Mode::Mothership,
        "capturecore" | "capturethecore" | "ctf" => Mode::CaptureCore,
        "bossrush" | "boss" => Mode::BossRush,
        "deathmatch" | "teamdeathmatch" | "tdm" => Mode::Deathmatch,
        "sandbox" => Mode::Sandbox,
        "randomevents" | "events" => Mode::RandomEvents,
        "cooppve" | "pve" => Mode::CoopPve,
        _ => Mode::Ffa,
    }
}
