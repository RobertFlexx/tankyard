use crate::ui::RED;
#[cfg(all(target_os = "linux", target_env = "gnu", target_feature = "crt-static"))]
compile_error!(
    "The Linux graphical client cannot use static glibc with runtime-loaded GPU drivers. Use scripts/build_linux_compatible.sh; headless binaries can still be built statically."
);

mod accessibility;
mod ambient;
mod central_owner;
mod community_ui;
mod creator;
mod credentials;
mod dlss;
mod editor;
mod effects;
mod file_dialog;
mod first_run;
mod graphics;
mod hardware;
mod hosting;
mod hud;
mod links;
mod menu;
mod mod_library;
mod mod_viewer;
mod motion;
mod native_resolution;
mod notifications;
mod party_ui;
mod practice_ui;
mod presentation;
mod render;
mod rules_ui;
mod scenery;
mod server_browser;
mod server_status;
mod session;
mod settings;
mod splash;
mod startup;
mod syntax;
mod tank_tree;
mod target;
mod ui_layout;
mod ui_text;
mod unlock;
mod upscale;
use tankyard_client::achievements;
use tankyard_client::clipboard;
use tankyard_client::tank_model;
mod ui;
mod voice;
mod window_control;

use clap::Parser;
use hud::{Hud, HudAction};
use macroquad::prelude::*;
use menu::{Action, Menu, Page};
use session::Session;
use settings::{CENTRAL_SERVER, Settings};
use tankyard_content::Content;
use tankyard_core::{DT, Mode, PlayerInput, V2};
use ui::*;

fn manual_fire(held: bool, queued_press: bool, over_ui: bool) -> bool {
    queued_press || (held && !over_ui)
}

#[derive(Parser, Debug)]
#[command(name = "tankyard", version, about = "Tank Yard - tank arena")]
struct Cli {
    /// Start windowed with multisampling and anisotropic filtering disabled.
    #[arg(long)]
    safe_graphics: bool,
    #[arg(long)]
    offline: bool,
    /// Open a practice challenge directly.
    #[arg(long, value_parser = ["shape_sprint", "drone_drill", "skirmish", "last_stand", "parry_lab", "stasis_lab", "boss_hunt", "trap_fort", "siege_school", "skirmisher_hunt", "trapper_hunt"], conflicts_with_all = ["offline", "host", "join", "connect_token", "class", "level"])]
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
    /// Disable microphone access and voice playback.
    no_audio: bool,
    #[arg(long)]
    smoke_frames: Option<u64>,
    #[arg(long)]
    screenshot: Option<String>,
    /// Open a menu page instead of a game (for screenshots and checks).
    #[arg(long, hide = true, value_parser = ["main", "mods", "tree", "credits", "settings", "party", "modes", "party-panel", "mods-review", "mods-files", "lobbies", "community", "graphics", "join", "editor"])]
    page: Option<String>,
    #[arg(long = "mod")]
    mods: Vec<std::path::PathBuf>,
    /// Load --mod packs even when review flags them (findings are printed).
    #[arg(long)]
    accept_mod_risk: bool,
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
        window_title: window_control::bootstrap_title(mod_library::staff_title(
            &mod_library::staff_packs(&settings),
        )),
        window_width: settings.width as i32,
        window_height: settings.height as i32,
        high_dpi: true,
        // Native framebuffer MSAA, with a selectable quality level.
        sample_count: match settings.anti_aliasing.min(graphics::MSAA_LIMIT) {
            8 => 8,
            4 => 4,
            _ => 1,
        },
        // Enter fullscreen after the window's own monitor is known.
        fullscreen: false,
        icon: Some(macroquad::miniquad::conf::Icon {
            small: *include_bytes!("../../../assets/icons/tankyard-16.rgba"),
            medium: *include_bytes!("../../../assets/icons/tankyard-32.rgba"),
            big: *include_bytes!("../../../assets/icons/tankyard-64.rgba"),
        }),
        platform: macroquad::miniquad::conf::Platform {
            linux_wm_class: "tankyard",
            // -1 is adaptive sync where the driver has it, plain vsync
            // elsewhere.
            swap_interval: Some(match (settings.vsync, settings.adaptive_vsync) {
                (false, _) => 0,
                (true, true) => -1,
                (true, false) => 1,
            }),
            apple_low_power_gpu: settings.battery_saver,
            apple_gfx_api: if settings.metal && graphics::metal_device_present() {
                macroquad::miniquad::conf::AppleGfxApi::Metal
            } else {
                macroquad::miniquad::conf::AppleGfxApi::OpenGl
            },
            ..Default::default()
        },
        ..Default::default()
    }
}
fn connection_label(address: &str) -> String {
    address
        .strip_prefix("tankyard://party/")
        .and_then(|invite| invite.split_once('#'))
        .map_or_else(
            || address.to_owned(),
            |(endpoint, _)| format!("Party at {endpoint}"),
        )
}
struct PendingJoin {
    result: std::sync::mpsc::Receiver<Result<Session, String>>,
    address: String,
    return_page: Page,
}
struct App {
    editor: Option<editor::Editor>,
    editor_playtest: bool,
    party_copy: Option<clipboard::Request<()>>,
    /// The last death whose killer the camera was pointed at.
    spectate_death: u32,
    party_restart: Option<(u32, std::time::Instant)>,
    party_ui: party_ui::PartyUi,
    /// Draw the party overlay with sample data (screenshot checks only).
    party_preview: bool,
    creator: creator::CreatorUi,
    /// Where things were last frame, for DLSS motion vectors.
    motion: motion::History,
    central_owner: central_owner::CentralOwner,
    settings: Settings,
    display_revision: u64,
    content: Content,
    menu: Menu,
    hud: Hud,
    session: Option<Session>,
    pending_join: Option<PendingJoin>,
    /// The lobby picked on the main server card, for the next join.
    join_lobby: Option<u8>,
    presentation: presentation::Presentation,
    connecting_address: String,
    last_join: Option<(String, String)>,
    view: render::View,
    effects: effects::Effects,
    paused: bool,
    menu_over_game: bool,
    menu_return_paused: bool,
    accumulator: f32,
    sequence: u32,
    frames: u64,
    smoke: Option<u64>,
    /// Frame times in smoke runs, for the timing summary.
    frame_times: Vec<f32>,
    cpu_times: Vec<f32>,
    screenshot: Option<String>,
    quit: bool,
    respawn_requested: bool,
    ability_queued: bool,
    fire_queued: bool,
    achievement_feed: achievements::Feed,
    match_claims: achievements::Claims,
    bundle_revision: u64,
    community: community_ui::CommunityUi,
    voice: voice::VoiceChat,
    catalog_paths: Vec<String>,
}
impl App {
    async fn new(cli: Cli, dark_theme: bool) -> Self {
        let mut settings = Settings::load();
        settings.dark_arena = dark_theme;
        settings.theme_chosen = true;
        if cli.safe_graphics {
            settings.use_safe_graphics();
        }
        mod_library::migrate(&mut settings);
        if !cli.mods.is_empty() {
            // For this run only: the saved list of enabled packs is untouched.
            settings.session_mods = Some(cli.mods.clone());
            settings.session_mods_trusted = cli.accept_mod_risk;
            for path in &cli.mods {
                let entry = mod_library::inspect(path);
                for finding in &entry.report.findings {
                    eprintln!(
                        "{}: {:?} {}:{} {}",
                        entry.name(),
                        finding.severity,
                        finding.file,
                        finding.line,
                        finding.message
                    );
                }
            }
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
        let ready = mod_library::ready_paths(&settings);
        menu.held_mods(&ready.held, ready.missing);
        match mod_library::load(&ready.paths) {
            Ok(loaded) => {
                graphics::load(&loaded.bundle.textures, settings.anisotropic_filtering);
                ui::set_presentation(&loaded.presentation);
                graphics::set_menu_background(loaded.menu_background.as_ref());
                menu.set_presentation(&loaded.presentation);
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
            editor: None,
            editor_playtest: false,
            party_copy: None,
            spectate_death: 0,
            party_restart: None,
            settings,
            display_revision: 0,
            content,
            menu,
            hud: Hud::new(),
            creator: creator::CreatorUi::new(),
            motion: motion::History::default(),
            central_owner: central_owner::CentralOwner::new(),
            session: None,
            pending_join: None,
            join_lobby: None,
            presentation: Default::default(),
            connecting_address: CENTRAL_SERVER.into(),
            last_join: None,
            view: render::View::new(),
            effects: effects::Effects::new(),
            paused: false,
            menu_over_game: false,
            menu_return_paused: false,
            accumulator: 0.0,
            sequence: 0,
            frames: 0,
            smoke: cli.smoke_frames,
            frame_times: Vec::new(),
            cpu_times: Vec::new(),
            screenshot: cli.screenshot,
            quit: false,
            respawn_requested: false,
            ability_queued: false,
            fire_queued: false,
            achievement_feed: achievements::Feed::default(),
            match_claims: achievements::Claims::default(),
            bundle_revision: 0,
            community: community_ui::CommunityUi::default(),
            voice: voice::VoiceChat::new(cli.no_audio),
            party_ui: party_ui::PartyUi::default(),
            party_preview: false,
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
        } else if let Some(page) = cli.page.as_deref() {
            app.party_preview = page == "party-panel";
            app.menu.preview_review = page == "mods-review";
            app.menu.preview_files = page == "mods-files";
            if page == "lobbies" {
                app.menu.open_lobbies();
            }
            if page == "graphics" {
                app.menu.settings_tab = 7;
            }
            if page == "community" {
                app.menu.open_community();
            }
            if page == "editor" {
                app.handle(Action::OpenEditor);
            }
            app.menu.page = match page {
                "mods" | "mods-review" | "mods-files" => Page::Mods,
                "tree" => Page::Encyclopedia,
                "credits" => Page::Credits,
                "settings" | "graphics" => Page::Settings,
                "party" => Page::Party,
                "modes" => Page::Modes,
                "community" => Page::Host,
                "join" => Page::Join,
                _ => Page::Main,
            };
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
        ui_layout::scale(screen_width(), screen_height(), self.settings.ui_scale)
    }
    fn launch(&mut self, mut session: Session) {
        self.party_ui = party_ui::PartyUi::default();
        if let Some(invite) = &session.party_invite {
            self.connecting_address = format!("Party at {}", invite.address);
        }
        self.menu.party_invite = session.party_invite.clone();
        self.menu.party_address = session
            .party_invite
            .as_ref()
            .map_or_else(String::new, |invite| invite.address.to_string());
        if matches!(
            &session.engine,
            session::Engine::Online { host: Some(_), .. }
        ) && session.party_invite.is_some()
            && self.settings.auto_party_network
        {
            self.menu.party_address.clear();
        }
        self.creator.reset_controls();
        self.creator.authenticate(&mut session);
        self.central_owner.authenticate(&mut session);
        if self.session.is_some() {
            self.save_progress();
        }
        self.content.clone_from(&session.bundle.content);
        if !matches!(&session.engine, session::Engine::Online { host: None, .. }) {
            self.last_join = None;
        }
        self.bundle_revision = session.bundle_revision;
        graphics::load(
            &session.bundle.textures,
            self.settings.anisotropic_filtering,
        );
        self.session = Some(session);
        self.voice.reset();
        self.match_claims = achievements::Claims::default();
        self.achievement_feed = achievements::Feed::default();
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
        self.spectate_death = 0;
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
        if self.active_party()
            && matches!(
                action,
                Action::QuickPlay
                    | Action::JoinLobby(_)
                    | Action::Offline
                    | Action::Host
                    | Action::CreateParty
                    | Action::JoinParty
                    | Action::Join
                    | Action::Practice(_)
            )
        {
            self.menu.error = "Your party is still connected. Resume the match or leave the party before starting another session.".into();
            return;
        }
        if !matches!(action, Action::None) {
            self.menu.info.clear();
        }
        match action {
            Action::None => {}
            Action::OpenEditor => {
                if self.session.is_some() {
                    self.menu.error =
                        "Leave the current match before opening the map editor.".into();
                } else {
                    self.editor = Some(editor::Editor::new(None));
                    self.editor_playtest = false;
                    self.menu.clear_focus();
                }
            }
            Action::OpenParty => {
                self.party_ui.open = self.active_party();
                self.menu.clear_focus();
            }
            Action::PartyReady(ready) => {
                if let Some(Session {
                    engine: session::Engine::Online { client, .. },
                    ..
                }) = &mut self.session
                    && !client.party_ready(ready)
                {
                    self.party_ui.notice = "Ready status could not be sent. Try again.".into();
                }
            }
            Action::ResumeParty => self.resume_party(),
            Action::LeaveParty => self.finish_session(),
            Action::ApplyParty => self.apply_party_settings(),
            Action::Page(page) => {
                if page == Page::Modes
                    && self.active_party()
                    && let Some(Session {
                        engine: session::Engine::Online { client, .. },
                        ..
                    }) = &self.session
                    && let Some(settings) = &client.match_settings
                {
                    self.menu.load_party_settings(settings);
                }

                if page == Page::Modes && self.menu.page != Page::Party {
                    self.menu.party_setup = false;
                }
                if page == Page::Party
                    && self
                        .session
                        .as_ref()
                        .is_none_or(|session| session.party_invite.is_none())
                {
                    self.menu.select_mode(Mode::CoopPve);
                    self.menu.bots = 4;
                }
                self.save_menu_settings();
                if self.active_party() && !self.menu_over_game {
                    self.open_menu(page);
                } else {
                    self.menu.go(page);
                }
            }
            Action::Back => {
                self.save_menu_settings();
                if self.menu.back() && self.menu_over_game {
                    if self.active_party() {
                        self.resume_party();
                    } else {
                        self.menu_over_game = false;
                        self.paused = self.menu_return_paused;
                    }
                }
            }
            Action::QuickPlay => {
                let _ = self.settings.save();
                self.begin_join(settings::main_server().into(), String::new(), Page::Main);
            }
            Action::StartCommunity => {
                let _ = self.settings.save();
                let mods = self.ready_mods();
                let result = hosting::settings(
                    &self.settings.community_server,
                    self.menu.config(),
                    mods,
                    self.menu.difficulty_name(),
                )
                .and_then(|settings| self.menu.dedicated.start(&settings));
                match result {
                    Ok(()) => self.menu.info = "Community server starting…".into(),
                    Err(error) => self.menu.error = error,
                }
            }
            Action::StopCommunity => self.menu.dedicated.stop(),
            Action::JoinCommunity => {
                let address = format!("127.0.0.1:{}", self.menu.dedicated.port);
                let password = self.settings.community_server.password.clone();
                self.begin_join(address, password, Page::Host);
            }
            Action::JoinLobby(lobby) => {
                let _ = self.settings.save();
                self.join_lobby = Some(lobby);
                self.begin_join(settings::main_server().into(), String::new(), Page::Main);
            }
            Action::Offline => {
                let paths = self.ready_mods();
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
                            if self.menu.offline_level > 1 {
                                local.world.config.modded = true;
                                let ids: Vec<_> = local.world.tanks.iter().map(|t| t.id).collect();
                                for id in ids {
                                    local.world.set_practice_level(id, self.menu.offline_level);
                                }
                                session.display = Some(local.world.snapshot());
                            }
                            local.bots.difficulty =
                                tankyard_server::difficulty_from_name(self.menu.difficulty_name())
                                    .unwrap_or_default();
                        }
                        self.launch(session);
                    }
                    Err(error) => self.menu.error = error,
                }
            }
            Action::CreateParty => {
                let mods = self.ready_mods();
                match Session::party(
                    self.menu.config(),
                    self.menu.bots.clamp(2, 8),
                    self.menu.offline_level,
                    &self.settings.player_name,
                    self.menu.difficulty_name(),
                    mods,
                    self.settings.auto_party_network,
                ) {
                    Ok(session) => {
                        self.launch(session);
                    }
                    Err(error) => self.menu.error = error,
                }
            }
            Action::JoinParty => {
                let invite = self.menu.party_input.trim().to_owned();
                match tankyard_server::party::Invite::parse(&invite) {
                    Ok(_) => self.begin_join(invite, String::new(), Page::Party),
                    Err(error) => self.menu.error = error.to_string(),
                }
            }
            Action::CopyParty => {
                if let Some(invite) = &mut self.menu.party_invite {
                    match self.menu.party_address.parse::<std::net::SocketAddr>() {
                        Ok(address)
                            if address.port() > 0
                                && !address.ip().is_unspecified()
                                && !address.ip().is_multicast() =>
                        {
                            if invite.address != address {
                                invite.relay = None;
                            }
                            invite.address = address;
                            if let Some(session) = &mut self.session {
                                session.set_party_address(address);
                            }
                            self.party_copy = Some(clipboard::copy(&invite.link()));
                            self.menu.error.clear();
                            self.menu.info = "Copying invite…".into();
                        }
                        _ => {
                            self.menu.error =
                                "Wait for public-IP discovery, or enter your public IP:PORT.".into()
                        }
                    }
                }
            }
            Action::Host => match self.host_session() {
                Ok(session) => {
                    if let session::Engine::Online {
                        host: Some(host), ..
                    } = &session.engine
                    {
                        self.connecting_address = host.address.to_string();
                    }
                    self.launch(session);
                }
                Err(error) => {
                    self.menu.error = error;
                    self.menu.page = Page::Host;
                }
            },
            Action::Join => self.begin_join(
                self.settings.server_address.clone(),
                self.menu.password.clone(),
                Page::Join,
            ),
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
            Action::Save => {
                self.display_revision = self.display_revision.wrapping_add(1);
                self.achievement_feed = achievements::Feed::default();
                match self.settings.save() {
                    Ok(()) => self.menu.error.clear(),
                    Err(error) => self.menu.error = format!("Could not save: {error}"),
                }
            }
            Action::ResetSettings => {
                self.settings.reset_preferences();
                self.menu.clear_focus();
                self.menu.difficulty = self.settings.bot_difficulty;
                self.handle(Action::Save);
            }
            Action::PreviewAlarm => {
                match self
                    .voice
                    .preview_alarm(self.settings.master_volume * self.settings.effects_volume)
                {
                    Ok(()) => self.menu.error.clear(),
                    Err(error) => self.menu.error = error,
                }
            }
        }
    }
    fn begin_join(&mut self, address: String, password: String, return_page: Page) {
        if self.pending_join.is_some() {
            return;
        }
        self.menu.clear_focus();
        self.menu.error.clear();
        let (sender, result) = std::sync::mpsc::channel();
        let name = self.settings.player_name.clone();
        let destination = address.clone();
        let lobby = self.join_lobby.take();
        self.last_join = Some((address.clone(), password.clone()));
        match std::thread::Builder::new()
            .name("server-connect".into())
            .spawn(move || {
                let _ = sender.send(Session::join(&destination, &name, &password, lobby));
            }) {
            Ok(_) => {
                self.connecting_address.clone_from(&address);
                self.pending_join = Some(PendingJoin {
                    result,
                    address,
                    return_page,
                });
            }
            Err(error) => self.menu.error = format!("Could not start connection: {error}"),
        }
    }
    fn poll_join(&mut self) {
        let outcome =
            self.pending_join
                .as_ref()
                .and_then(|pending| match pending.result.try_recv() {
                    Ok(result) => Some(result),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        Some(Err("Connection closed.".into()))
                    }
                });
        if let Some(outcome) = outcome {
            let pending = self.pending_join.take().unwrap();
            match outcome {
                Ok(session) => {
                    if !pending.address.starts_with("tankyard://party/") {
                        self.settings.remember_server(&pending.address);
                    }
                    let _ = self.settings.save();
                    self.launch(session);
                }
                Err(error) => {
                    self.menu.reset(pending.return_page);
                    self.menu.error = format!(
                        "Could not connect to {}: {error}",
                        connection_label(&pending.address)
                    );
                }
            }
        }
    }
    fn finish_session(&mut self) {
        self.party_ui = party_ui::PartyUi::default();
        self.party_copy = None;
        self.party_restart = None;
        self.menu.party_configuring = false;
        self.menu.party_invite = None;
        self.menu.party_roster.clear();
        self.voice.reset();
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
        if let Err(error) = self.reload_catalog() {
            self.content = Content::builtin();
            self.menu.maps = tankyard_content::builtin_maps();
            ui::set_presentation(&Default::default());
            graphics::set_menu_background(None);
            self.menu.set_presentation(&Default::default());
            log::warn!("Cannot restore local mods: {error}");
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
        if let Some(Session {
            engine: session::Engine::Online { client, .. },
            ..
        }) = &mut self.session
        {
            client.party_presence(true);
        }
    }
    fn host_session(&mut self) -> Result<Session, String> {
        let mods = self.ready_mods();
        Session::host(
            self.menu.config(),
            self.menu.bots,
            &self.settings.player_name,
            &self.menu.password,
            mods,
            self.menu.difficulty_name(),
        )
    }
    /// Load the enabled packs' tanks, maps, textures and interface styling
    /// for the menus and offline play.
    fn reload_catalog(&mut self) -> Result<(), String> {
        self.catalog_paths.clone_from(&self.settings.enabled_mods);
        let paths = self.ready_mods();
        let loaded = mod_library::load(&paths)?;
        self.content = loaded.bundle.content;
        self.menu.maps = loaded.bundle.maps;
        self.menu.map = self.menu.map.min(self.menu.maps.len().saturating_sub(1));
        graphics::load(&loaded.bundle.textures, self.settings.anisotropic_filtering);
        ui::set_presentation(&loaded.presentation);
        graphics::set_menu_background(loaded.menu_background.as_ref());
        self.menu.set_presentation(&loaded.presentation);
        Ok(())
    }
    /// Enabled packs that passed review or were confirmed by the player.
    /// Anything held back is reported on the menu instead of loading.
    fn ready_mods(&mut self) -> Vec<std::path::PathBuf> {
        let ready = mod_library::ready_paths(&self.settings);
        if !ready.held.is_empty() {
            self.menu.held_mods(&ready.held, ready.missing);
        }
        ready.paths
    }
    /// Archives dropped onto the window are imported and reviewed.
    fn dropped_files(&mut self) {
        for file in get_dropped_files() {
            let result = match (&file.path, &file.bytes) {
                (Some(path), _) => mod_library::import(path),
                (None, Some(bytes)) => mod_library::import_bytes(bytes),
                _ => continue,
            };
            self.menu.import_result(&mut self.settings, result);
            if self.session.is_none() {
                self.menu.go(Page::Mods);
            }
        }
    }
    fn active_party(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.party_invite.is_some() && session.failure.is_none())
    }
    fn apply_party_settings(&mut self) {
        let settings = self.menu.party_match_settings();
        let Some(Session {
            engine: session::Engine::Online { client, .. },
            action_result,
            ..
        }) = &mut self.session
        else {
            return;
        };
        if !self.menu.party_active || self.party_restart.is_some() {
            return;
        }
        if !self.menu.party_leader {
            self.menu.error = "Only the party leader can start a new round.".into();
            return;
        }
        let Some(round) = client
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.match_state.round)
        else {
            return;
        };
        client.last_action = None;
        client.last_action_accepted = None;
        *action_result = None;
        if client.admin(tankyard_protocol::ServerCommand::ConfigureParty { settings }) {
            self.party_restart = Some((round, std::time::Instant::now()));
            self.menu.party_configuring = true;
            self.menu.error.clear();
            self.menu.info = "Starting a new round with your party…".into();
        } else {
            self.menu.error =
                "Party controls are not ready. Wait for the leader badge, then try again.".into();
        }
    }
    fn resume_party(&mut self) {
        if !self.active_party() {
            return;
        }
        self.party_ui.close();
        self.menu.clear_focus();
        self.menu_over_game = false;
        self.menu_return_paused = false;
        self.paused = false;
        if let Some(Session {
            engine: session::Engine::Online { client, .. },
            ..
        }) = &mut self.session
        {
            client.party_presence(false);
        }
    }
    fn return_to_menu(&mut self) {
        if self.active_party() {
            self.party_ui.close();
            self.open_menu(Page::Main);
            self.menu.reset(Page::Main);
        } else {
            self.finish_session();
        }
    }
    fn draw_party(&mut self, s: f32) {
        let Some(session) = &mut self.session else {
            return;
        };
        let network = session.party_network_status();
        let session::Engine::Online { client, .. } = &mut session.engine else {
            return;
        };
        let action = self.party_ui.panel(
            client,
            &self.content,
            &network,
            !self.menu.party_address.is_empty(),
            self.menu_over_game,
            s,
        );
        self.handle(action);
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
        if self.party_ui.open {
            return false;
        }
        if self.creator.open {
            if is_key_pressed(KeyCode::Escape) {
                self.creator.open = false;
                return true;
            }
            return false;
        }
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
        let blocked = self.paused
            || self.party_ui.open
            || self.menu_over_game
            || self.hud.sandbox
            || self.creator.open
            || self.community.focused();
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
        // A click accepted over the arena stays accepted until the next tick,
        // even if the cursor crosses a HUD button after the press.
        let steer_return = manual_fire(
            is_mouse_button_down(MouseButton::Left),
            self.fire_queued,
            over_ui,
        );
        let fire = !self.creator.open && (self.settings.auto_fire || steer_return);
        PlayerInput {
            sequence: self.sequence,
            movement: if self.creator.open {
                V2::ZERO
            } else {
                movement.normalized()
            },
            aim,
            aim_distance: cursor.length().min(5000.),
            fire,
            steer_return,
            alt_fire: is_mouse_button_down(MouseButton::Right),
            ability: !self.creator.open
                && (is_key_down(self.settings.key(4)) || self.ability_queued),
        }
    }
    fn pointer_over_ui(&self, s: f32) -> bool {
        if self.active_party()
            && let Some(Session {
                engine: session::Engine::Online { client, .. },
                ..
            }) = &self.session
            && self.party_ui.captures_pointer(
                client.party_members.len(),
                s,
                self.menu_over_game,
                self.hud.evolution,
            )
        {
            return true;
        }
        if self.creator.captures_pointer(s) {
            return true;
        }
        self.voice.captures_pointer(s)
            || (self.effects.boss_banner() && contains(hud::boss_banner_layout(screen_width(), s)))
            || self.session.as_ref().is_some_and(|session| {
                let community = match &session.engine {
                    session::Engine::Online { client, .. } => {
                        self.community.captures_pointer(
                            client,
                            s,
                            self.creator.enabled(),
                            self.central_owner.enabled(),
                        ) || client.active_notice().is_some_and(|notice| {
                            contains(notifications::server_notice_rect(
                                notice,
                                s,
                                if self.effects.boss_banner() {
                                    188.
                                } else {
                                    110.
                                } * s,
                            ))
                        })
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
    /// Overlays drawn after the HUD or pause menu test the same press as the
    /// controls underneath them; while one is under the pointer, only it may
    /// respond. This keeps a click on the party rail from also choosing an
    /// upgrade or a menu entry beneath it.
    fn overlay_captures_pointer(&self, s: f32, menu: bool) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let voice = !menu
            && !self.paused
            && !self.creator.open
            && !self.community.admin_open
            && session.failure.is_none()
            && session.display.is_some()
            && self.voice.captures_pointer(s);
        let session::Engine::Online { client, .. } = &session.engine else {
            return voice;
        };
        let rail = self.active_party()
            && if menu {
                self.menu.page == Page::Main
                    && self
                        .party_ui
                        .captures_pointer(client.party_members.len(), s, true, false)
            } else {
                !self.paused
                    && !self.hud.sandbox
                    && !self.creator.open
                    && !self.community.admin_open
                    && self.party_ui.captures_pointer(
                        client.party_members.len(),
                        s,
                        false,
                        self.hud.evolution,
                    )
            };
        let community = !menu
            && !self.paused
            && !self.creator.open
            && self.community.captures_pointer(
                client,
                s,
                self.creator.enabled(),
                self.central_owner.enabled(),
            );
        voice || rail || community
    }
    fn show_achievement_toasts(&self) -> bool {
        if self.creator.open {
            return false;
        }
        !self.paused
            && !self.menu_over_game
            && !self.hud.sandbox
            && !self.hud.evolution
            && !self.community.admin_open
            && self.session.as_ref().is_some_and(|session| {
                session.display.is_some()
                    && session.failure.is_none()
                    && !session.practice.as_ref().is_some_and(|run| run.done())
            })
    }
    fn update(&mut self, dt: f32, s: f32) {
        if self.editor.is_some() {
            if !self.editor_playtest {
                return;
            }
            if is_key_pressed(KeyCode::Escape) {
                self.finish_session();
                self.editor_playtest = false;
                return;
            }
        }
        self.dropped_files();
        let party_key = self.party_ui.keys();
        if let Some(result) = self.party_copy.as_ref().and_then(|request| request.poll()) {
            self.party_copy = None;
            self.party_ui.notice = match &result {
                Ok(()) => "Invite copied. Share it privately with your friends.".into(),
                Err(error) => error.clone(),
            };
            if self.menu.page == Page::Party {
                self.menu.info.clear();
                match result {
                    Ok(()) => {
                        self.menu.info =
                            "Invite copied. Share it privately with your friends.".into()
                    }
                    Err(error) => self.menu.error = error,
                }
            }
        }
        if self.session.is_some()
            && !self.party_ui.open
            && !party_key
            && !self.paused
            && !self.menu_over_game
            && is_key_pressed(KeyCode::F8)
        {
            if self.creator.enabled() {
                self.creator.open = !self.creator.open;
                self.community.admin_open = false;
            } else if self.session.as_ref().is_some_and(|session| matches!(&session.engine, session::Engine::Online { client, .. } if !client.permissions.is_empty())) {
                self.community.admin_open = !self.community.admin_open;
            } else if self.central_owner.enabled()
                && let Some(Session { engine: session::Engine::Online { client, .. }, .. }) = &self.session {
                self.effects.notice(&central_owner::CentralOwner::unavailable(client));
            }
            self.hud.sandbox = false;
            self.hud.evolution = false;
        }
        if let Some(session) = &mut self.session {
            self.creator.authenticate(session);
            self.central_owner.authenticate(session);
        }
        // Packs never change the content of a game in progress.
        if self.session.is_none()
            && (self.catalog_paths != self.settings.enabled_mods
                || std::mem::take(&mut self.menu.catalog_stale))
            && let Err(error) = self.reload_catalog()
        {
            self.menu.error = error;
        }
        self.effects.update(dt, self.settings.reduced_motion);
        self.voice
            .effects_volume(self.settings.master_volume * self.settings.effects_volume);
        if self.show_achievement_toasts() {
            self.achievement_feed.update(dt);
        }
        let now = get_time();
        if let Some(session) = &mut self.session {
            session.update_network(dt, now, &self.content);
            if self.party_restart.is_some()
                && let Some((false, message)) = session.action_result.take()
            {
                self.party_restart = None;
                self.menu.party_configuring = false;
                self.menu.info.clear();
                self.menu.error = message;
            }
            if self.bundle_revision != session.bundle_revision {
                self.content.clone_from(&session.bundle.content);
                self.bundle_revision = session.bundle_revision;
                graphics::load(
                    &session.bundle.textures,
                    self.settings.anisotropic_filtering,
                );
                self.hud = self.hud.rebuilt();
            }
            if let Some(message) = session.notice.take() {
                if self.party_ui.open {
                    self.party_ui.notice.clone_from(&message);
                }
                self.creator.notice = message.clone();
                self.community.message(&message);
                self.effects.notice(&message);
            }
            let playing = session.failure.is_none()
                && !self.paused
                && !self.menu_over_game
                && session.display.as_ref().is_some_and(|snapshot| {
                    !snapshot.match_state.finished
                        && snapshot
                            .tanks
                            .iter()
                            .any(|t| t.id == session.player && t.alive)
                });
            if let session::Engine::Online { client, .. } = &mut session.engine {
                self.party_ui.sync(client);
                let muted = self.party_ui.muted_players(client);
                self.voice.update(
                    client,
                    playing,
                    self.settings.master_volume * self.settings.voice_volume,
                    &muted,
                );
            }
            if session.party_invite.is_some() {
                self.menu.party_network = session.party_network_status();
                if let Some(invite) = &session.party_invite {
                    let previous = self
                        .menu
                        .party_invite
                        .as_ref()
                        .map(|invite| (invite.address, invite.relay));
                    if previous != Some((invite.address, invite.relay)) {
                        if !self.menu.address_active || self.menu.party_address.trim().is_empty() {
                            self.menu.party_address = invite.address.to_string();
                        }
                        self.menu.party_invite = Some(invite.clone());
                    }
                }
            }
        }
        if let Some((round, started)) = self.party_restart {
            let client = self
                .session
                .as_ref()
                .and_then(|session| match &session.engine {
                    session::Engine::Online { client, .. } => Some(client),
                    _ => None,
                });
            let ready = client.is_some_and(|client| {
                client
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.match_state.round > round)
            });
            let error = client.and_then(|client| client.last_action.clone());
            if ready || error.is_some() || started.elapsed().as_secs() >= 15 || !self.active_party()
            {
                self.party_restart = None;
                self.menu.party_configuring = false;
                if ready {
                    self.resume_party();
                } else {
                    self.menu.error = error.unwrap_or_else(|| "The new round was not confirmed. Check the party connection and try again.".into());
                }
            }
        }
        if self.pending_join.is_some() {
            if is_key_pressed(KeyCode::Escape) {
                self.pending_join = None;
            } else {
                self.poll_join();
            }
            return;
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
            && !self.party_ui.open
            && !party_key
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
            if session.party_invite.is_some() {
                if (alive && is_key_pressed(KeyCode::Enter)) || is_key_pressed(KeyCode::Slash) {
                    self.party_ui.focus_chat();
                    community_key = true;
                }
            } else {
                community_key = self.community.keys(client, alive);
            }
        }
        if !self.menu_over_game && !self.party_ui.open && !party_key {
            if !community_key
                && !self.creator.open
                && !self.community.focused()
                && is_key_pressed(KeyCode::Escape)
            {
                self.paused = !self.paused;
                self.hud.evolution = false;
                self.hud.sandbox = false;
            }
            if !self.paused && !community_key && !self.creator.open && !self.community.focused() {
                if is_key_pressed(KeyCode::M) {
                    self.voice.toggle();
                }
                if (is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::Right))
                    && let Some(session) = &mut self.session
                {
                    session.cycle_spectating(if is_key_pressed(KeyCode::Left) { -1 } else { 1 });
                }
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
        } else if self.menu_over_game
            && !self.party_ui.open
            && !party_key
            && is_key_pressed(KeyCode::Escape)
        {
            self.handle(Action::Back);
        }
        if !community_key
            && !self.creator.open
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
            && !self.party_ui.open
            && !self.menu_over_game
            && !self.hud.sandbox
            && !self.creator.open
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
            // After a death, watch whoever did it unless the player picks
            // someone else.
            if self.spectate_death != self.effects.deaths {
                let dead = session
                    .display
                    .as_ref()
                    .and_then(|snapshot| snapshot.tanks.iter().find(|t| t.id == session.player))
                    .is_some_and(|tank| !tank.alive);
                if dead {
                    if let Some(killer) = self.effects.killed_by {
                        session.watch(killer);
                    }
                    self.spectate_death = self.effects.deaths;
                }
            }
            session.sync_spectating();
            self.hud.spectating = session.spectating;
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
            // The camera stays on the wreck while the killer card shows.
            let followed = snapshot
                .tanks
                .iter()
                .find(|t| Some(t.id) == session.spectating && t.alive && !self.hud.holding_camera())
                .unwrap_or(tank);
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
            self.effects.sync_arena(snapshot.config.arena_scale, dt);
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
                && !self.creator.open
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
            self.effects.note_bosses(&snapshot.bosses);
            let sounds = self.effects.ingest(
                &session.events,
                session.player,
                if self.settings.reduced_motion {
                    0.
                } else {
                    self.settings.effects
                },
            );
            if sounds
                .iter()
                .any(|event| matches!(event.kind, tankyard_core::EventKind::BossSpawn))
            {
                self.voice
                    .boss_alarm(self.settings.master_volume * self.settings.effects_volume);
            }
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
            && self.match_claims.update(
                &session.achievement_tracker,
                self.settings.achievement_mode,
                (self.settings.games_played, self.settings.total_kills),
                &mut self.settings.unlocked_achievements,
                &mut self.achievement_feed,
            )
            && let Err(error) = self.settings.save()
        {
            log::warn!("Cannot save achievements: {error}");
        }
    }
    fn draw(&mut self, s: f32) {
        if !self.editor_playtest
            && let Some(editor) = &mut self.editor
        {
            let action = editor.draw(&self.settings, s);
            match action {
                editor::Action::None => {}
                editor::Action::Exit => {
                    self.editor = None;
                    self.menu.clear_focus();
                }
                editor::Action::Play(loaded, config) => {
                    match Session::editor(config, *loaded, &self.settings.player_name) {
                        Ok(session) => {
                            self.launch(session);
                            self.editor_playtest = true;
                            self.hud.editor_playtest = true;
                        }
                        Err(error) => self.menu.error = error,
                    }
                }
            }
            return;
        }
        if self.party_preview {
            render::menu_grid();
            let members: Vec<_> = [
                ("Mira", "hive", true, false),
                ("Pilot", "fork", false, false),
                ("Kestrel", "lance", false, true),
                ("Juno", "bulwark", false, false),
            ]
            .into_iter()
            .enumerate()
            .map(
                |(n, (name, class, leader, in_menu))| tankyard_protocol::PartyMember {
                    client: n as u64 + 1,
                    player: n as u64 + 10,
                    name: name.into(),
                    class: class.into(),
                    leader,
                    in_menu,
                    ready: n < 2,
                },
            )
            .collect();
            let messages: std::collections::VecDeque<_> = [
                ("Mira", "Welcome! Two Teams on Open, bases are guarded now."),
                ("Kestrel", "brb grabbing a drink"),
                (
                    "Pilot",
                    "Let's push the left lane together, then rotate to the hill when it moves.",
                ),
            ]
            .into_iter()
            .enumerate()
            .map(|(n, (name, text))| tankyard_protocol::ChatMessage {
                id: n as u64 + 1,
                room: "lobby".into(),
                player: 0,
                name: name.into(),
                game_creator: false,
                server_badge: Default::default(),
                text: text.into(),
                time: 0,
            })
            .collect();
            let view = party_ui::PartyView {
                members: &members,
                me: 2,
                messages: &messages,
                revision: 1,
                chat_enabled: true,
            };
            let _ = self.party_ui.panel_view(
                &view,
                &self.content,
                "Internet relay ready · no port forwarding needed. The match runs on your computer.",
                true,
                false,
                s,
            );
            return;
        }
        if self.party_ui.open && self.active_party() {
            render::menu_grid();
            self.draw_party(s);
            return;
        }
        if let Some(pending) = &self.pending_join {
            render::menu_background(accessibility::motion_time(get_time() as f32));
            center_label(
                "Connecting…",
                screen_width() * 0.5,
                screen_height() * 0.42,
                32.0 * s,
                CYAN,
            );
            center_label(
                &fitted_label(
                    &connection_label(&pending.address),
                    18. * s,
                    screen_width() - 44. * s,
                ),
                screen_width() * 0.5,
                screen_height() * 0.42 + 40.0 * s,
                18.0 * s,
                muted_color(),
            );
            if button(
                Rect::new(
                    screen_width() * 0.5 - 110.0 * s,
                    screen_height() * 0.42 + 76.0 * s,
                    220.0 * s,
                    40.0 * s,
                ),
                "Cancel [Esc]",
                false,
                true,
                s,
            ) {
                self.pending_join = None;
            }
            return;
        }
        if self.session.is_none() || self.menu_over_game {
            self.menu.party_active = self.active_party();
            self.menu.party_leader =
                self.session
                    .as_ref()
                    .is_some_and(|session| match &session.engine {
                        session::Engine::Online { client, .. } => {
                            client
                                .party_members
                                .iter()
                                .any(|member| member.client == client.client_id() && member.leader)
                                && client
                                    .permissions
                                    .contains(&tankyard_protocol::ServerPermission::Configure)
                        }
                        _ => false,
                    });
            let menu_scale = s;
            set_interactive(true);
            let covered = self.overlay_captures_pointer(s, true);
            set_interactive(!covered);
            self.menu.party_roster =
                self.session
                    .as_ref()
                    .map_or_else(Vec::new, |session| match &session.engine {
                        session::Engine::Online { client, .. } => client
                            .party_members
                            .iter()
                            .map(|member| member.name.clone())
                            .collect(),
                        _ => Vec::new(),
                    });
            let action = self.menu.draw(
                &self.content,
                &mut self.settings,
                &self.match_claims.unlocked,
                menu_scale,
            );
            self.handle(action);
            set_interactive(true);
            if self.menu.page == Page::Main
                && self.active_party()
                && let Some(Session {
                    engine: session::Engine::Online { client, .. },
                    ..
                }) = &self.session
            {
                self.party_ui.rail(client, &self.content, s, true, false);
            }
            return;
        }
        let practice = self
            .session
            .as_ref()
            .and_then(|session| session.practice.as_ref())
            .is_some_and(|run| !run.done());
        let unobscured = !self.hud.sandbox && !self.community.admin_open;
        let server_notice = self
            .session
            .as_ref()
            .and_then(|session| match &session.engine {
                session::Engine::Online { client, .. } if unobscured => client.active_notice(),
                _ => None,
            });
        let server_height = server_notice.map_or(0., |notice| {
            notifications::server_notice_rect(notice, s, 0.).h + 8. * s
        });
        let axiom = self
            .session
            .as_ref()
            .and_then(|session| session.display.as_ref())
            .and_then(|snapshot| snapshot.tanks.iter().find(|t| t.creator_boss && t.alive));
        let axiom_height = if axiom.is_some() { 61. * s } else { 0. };
        let mut overlays = notifications::top_overlays(
            screen_height(),
            s,
            self.effects.boss_banner(),
            server_height,
            practice,
            if unobscured {
                self.effects.kills.len()
            } else {
                0
            },
            unobscured && self.effects.notice_life > 0.,
        );
        overlays.server_y += axiom_height;
        overlays.practice_y += axiom_height;
        overlays.kill_y += axiom_height;
        overlays.notice_y += axiom_height;
        overlays.achievement_y += axiom_height;
        let mut actions = Vec::new();
        set_interactive(true);
        let covered = self.overlay_captures_pointer(s, false);
        if let Some(session) = &self.session {
            set_interactive(
                !covered
                    && !self.paused
                    && !self.creator.open
                    && !self.community.focused()
                    && !server_notice.is_some_and(|notice| {
                        contains(notifications::server_notice_rect(
                            notice,
                            s,
                            overlays.server_y,
                        ))
                    })
                    && !session.practice.as_ref().is_some_and(|run| run.done()),
            );
            if let Some(snapshot) = &session.display {
                self.presentation.begin_world(&self.settings);
                render::world(
                    snapshot,
                    &self.content,
                    session
                        .bundle
                        .maps
                        .iter()
                        .find(|map| map.id == snapshot.config.map),
                    session.player,
                    &self.view,
                    &self.effects,
                    &self.settings,
                    session.render_fraction(self.accumulator / DT),
                    session.animation_time(self.accumulator / DT),
                    session.local_animation_time(self.accumulator / DT),
                    &session.previous_positions,
                );
                if let Some((_, life)) = self.effects.boss_arrival {
                    let age = 5. - life;
                    let flash = if self.settings.reduced_motion {
                        0.
                    } else if age < 1.2 {
                        ((age * std::f32::consts::TAU * 1.5).sin().max(0.) * (1. - age / 1.2))
                            * 0.20
                    } else {
                        0.
                    };
                    draw_rectangle(0., 0., screen_width(), screen_height(), alpha(RED, flash));
                }
                let fraction = session.render_fraction(self.accumulator / DT);
                self.presentation.finish_world(|render| {
                    self.motion.draw(
                        snapshot,
                        &self.content,
                        session.player,
                        &self.view,
                        &session.previous_positions,
                        fraction,
                        render,
                    )
                });
                self.hud.local_simulation = session.offline_mode();
                self.hud.sandbox_allowed = match &session.engine {
                    session::Engine::Offline(_) => true,
                    session::Engine::Online { client, .. } => client.sandbox_allowed,
                };
                if self.community.admin_open {
                    self.hud.sandbox = false;
                    self.hud.evolution = false;
                }
                self.hud.killed_by = self.effects.killed_by;
                self.hud.deaths = self.effects.deaths;
                self.hud.palette = tankyard_client::map_model::palette(
                    session
                        .bundle
                        .maps
                        .iter()
                        .find(|map| map.id == snapshot.config.map)
                        .map_or("slate", |map| map.background.as_str()),
                );
                actions = self.hud.draw(
                    snapshot,
                    &self.content,
                    session.player,
                    &self.settings,
                    session.diagnostics(),
                    session.map_remaining(),
                    s,
                    &self.view,
                );
                hud::mod_widgets(&session.mod_hud(), s);
                if let Some(axiom) = axiom {
                    let r = hud::boss_banner_layout(screen_width(), s);
                    let r = Rect::new(r.x, 106. * s, r.w, 52. * s);
                    panel(r);
                    center_label(
                        &fitted_label(tankyard_core::creator_boss::NAME, 16. * s, r.w - 20. * s),
                        r.center().x,
                        r.y + 20. * s,
                        16. * s,
                        AMBER,
                    );
                    arcade_track(
                        Rect::new(r.x + 14. * s, r.y + 31. * s, r.w - 28. * s, 12. * s),
                        axiom.health / axiom.max_health,
                        CYAN,
                    );
                    center_label(
                        &format!("{:.0} / {:.0}", axiom.health.max(0.), axiom.max_health),
                        r.center().x,
                        r.y + 41. * s,
                        9. * s,
                        text_color(),
                    );
                }
                if let Some((boss, killer, life)) = &self.effects.boss_defeat {
                    let opacity = (life / 0.4).min(1.);
                    let mut rect = hud::boss_banner_layout(screen_width(), s);
                    rect.y += axiom_height;
                    panel(rect);
                    draw_rectangle(rect.x, rect.y, 4. * s, rect.h, alpha(AMBER, opacity));
                    center_label(
                        "BOSS DEFEATED",
                        rect.center().x,
                        rect.y + 20. * s,
                        12. * s,
                        alpha(AMBER, opacity),
                    );
                    let hunter = snapshot
                        .tanks
                        .iter()
                        .find(|t| t.id == *killer)
                        .map_or_else(|| "a player".to_owned(), |t| t.display_name());
                    center_label(
                        &fitted_label(
                            &format!("{boss} was killed by {hunter}"),
                            20. * s,
                            rect.w - 24. * s,
                        ),
                        rect.center().x,
                        rect.y + 46. * s,
                        20. * s,
                        alpha(text_color(), opacity),
                    );
                } else if let Some((kind, life)) = self.effects.boss_arrival {
                    let opacity = (life / 0.4).min(1.);
                    let mut rect = hud::boss_banner_layout(screen_width(), s);
                    rect.y += axiom_height;
                    let width = rect.w;
                    let y = rect.y;
                    panel(rect);
                    draw_rectangle(rect.x, rect.y, 4. * s, rect.h, alpha(RED, opacity));
                    center_label(
                        "BOSS INCOMING",
                        rect.x + rect.w * 0.5,
                        y + 20. * s,
                        12. * s,
                        alpha(RED, opacity),
                    );
                    center_label(
                        &fitted_label(kind.name(), 22. * s, width - 24. * s),
                        rect.x + rect.w * 0.5,
                        y + 47. * s,
                        22. * s,
                        alpha(text_color(), opacity),
                    );
                }
                for (index, kill) in self
                    .effects
                    .kills
                    .iter()
                    .rev()
                    .filter(|_| !self.hud.sandbox && !self.community.admin_open)
                    .enumerate()
                {
                    let opacity = (kill.life / 0.45).min(1.0);
                    let caption = format!("Eliminated {}", kill.name);
                    let size = 19.
                        * s
                        * (hud::boss_banner_layout(screen_width(), s).w
                            / label_width(&caption, 19. * s).max(1.))
                        .min(1.);
                    center_label(
                        &caption,
                        screen_width() * 0.5,
                        overlays.kill_y + index as f32 * 28. * s,
                        size,
                        alpha(text_color(), opacity),
                    );
                }
                if self.effects.notice_life > 0.0 && !self.hud.sandbox && !self.community.admin_open
                {
                    let opacity = (self.effects.notice_life / 0.35).min(1.0);
                    center_label(
                        &self.effects.notice,
                        screen_width() * 0.5,
                        overlays.notice_y,
                        25.0 * s,
                        alpha(AMBER, opacity),
                    );
                }
            } else {
                render::menu_background(accessibility::motion_time(get_time() as f32));
                center_label(
                    "Connecting…",
                    screen_width() * 0.5,
                    screen_height() * 0.45,
                    32.0 * s,
                    CYAN,
                );
                center_label(
                    &fitted_label(
                        &connection_label(&self.connecting_address),
                        18. * s,
                        screen_width() - 44. * s,
                    ),
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
                set_interactive(true);
                draw_rectangle(
                    0.0,
                    0.0,
                    screen_width(),
                    screen_height(),
                    alpha(bg_color(), 0.92),
                );
                let retry = self.last_join.is_some();
                let size = 21. * s;
                let mut lines =
                    wrapped_lines(error, (640. * s).min(screen_width() - 44. * s), size);
                if lines.len() > 6 {
                    lines.truncate(6);
                    if let Some(last) = lines.last_mut() {
                        last.push('…');
                    }
                }
                let line_height = size * 1.35;
                let buttons_height = if retry { 96. * s } else { 42. * s };
                let block_height =
                    34. * s + 24. * s + lines.len() as f32 * line_height + 24. * s + buttons_height;
                let top = (screen_height() - block_height) * 0.5;
                center_label(
                    "Disconnected",
                    screen_width() * 0.5,
                    top + 34. * s,
                    34. * s,
                    RED,
                );
                for (index, line) in lines.iter().enumerate() {
                    center_label(
                        line,
                        screen_width() * 0.5,
                        top + 58. * s + (index + 1) as f32 * line_height,
                        size,
                        text_color(),
                    );
                }
                let buttons_y = top + 58. * s + lines.len() as f32 * line_height + 24. * s;
                if retry
                    && button(
                        Rect::new(
                            screen_width() * 0.5 - 125.0 * s,
                            buttons_y,
                            250.0 * s,
                            42.0 * s,
                        ),
                        "Reconnect",
                        true,
                        true,
                        s,
                    )
                {
                    actions.push(HudAction::Reconnect);
                }
                if button(
                    Rect::new(
                        screen_width() * 0.5 - 125.0 * s,
                        buttons_y + if retry { 54.0 * s } else { 0.0 },
                        250.0 * s,
                        42.0 * s,
                    ),
                    "Return to menu",
                    !retry,
                    true,
                    s,
                ) {
                    actions.push(HudAction::Menu);
                }
            }
        }
        set_interactive(true);
        if !self.paused
            && !self.creator.open
            && let Some(session) = &mut self.session
            && session.failure.is_none()
            && let session::Engine::Online { client, .. } = &mut session.engine
            && let Some(snapshot) = &session.display
        {
            self.community.draw(
                client,
                snapshot,
                s,
                self.hud.evolution || self.hud.sandbox,
                self.creator.enabled(),
                self.central_owner.enabled(),
            );
        }
        if !self.paused
            && !self.hud.sandbox
            && !self.creator.open
            && !self.community.admin_open
            && self.active_party()
            && let Some(Session {
                engine: session::Engine::Online { client, .. },
                ..
            }) = &self.session
        {
            self.party_ui
                .rail(client, &self.content, s, false, self.hud.evolution);
        }
        if !self.paused
            && !self.menu_over_game
            && unobscured
            && let Some(session) = &mut self.session
            && let session::Engine::Online { client, .. } = &mut session.engine
            && let Some(notice) = client.active_notice()
        {
            let rect = notifications::server_notice_rect(notice, s, overlays.server_y);
            if notifications::server_notice(notice, rect, s) {
                client.server_notice = None;
            }
        }
        for action in actions {
            self.hud_action(action);
        }
        if !self.paused
            && !self.menu_over_game
            && !self.creator.open
            && !self.community.admin_open
            && let Some(session) = &self.session
            && session.failure.is_none()
            && session.display.is_some()
        {
            let (online, muted) = match &session.engine {
                session::Engine::Online { client, .. } => {
                    (client.voice_enabled, client.voice_muted)
                }
                _ => (false, false),
            };
            self.voice.draw(s, online, muted);
        }
        if self.paused
            && !self.menu_over_game
            && self
                .session
                .as_ref()
                .is_some_and(|session| session.failure.is_none())
        {
            self.pause(s);
        }
        if !self.paused
            && let Some(run) = self
                .session
                .as_ref()
                .and_then(|session| session.practice.as_ref())
        {
            match practice_ui::draw(run, &self.settings.practice_records, s, overlays.practice_y) {
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
        if self.show_achievement_toasts() {
            notifications::draw(
                &self.achievement_feed,
                s,
                self.settings.reduced_motion,
                overlays.achievement_y,
            );
        }
        if !self.paused
            && !self.menu_over_game
            && let Some(session) = &mut self.session
        {
            self.creator.draw(session, s);
        }
    }
    fn hud_action(&mut self, action: HudAction) {
        match action {
            HudAction::Spectate(direction) => {
                if let Some(session) = &mut self.session {
                    session.cycle_spectating(direction);
                }
            }
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
            HudAction::Pause if self.editor_playtest => {
                self.finish_session();
                self.editor_playtest = false;
            }
            HudAction::Pause => self.paused = true,
            HudAction::Controls => self.open_menu(Page::Controls),
            HudAction::Menu => self.return_to_menu(),
            HudAction::Reconnect => {
                if let Some((address, password)) = self.last_join.clone() {
                    self.finish_session();
                    self.begin_join(address, password, Page::Main);
                }
            }
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
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.party_invite.is_some())
        {
            items.push("Party invites");
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
                    "Party invites" => self.open_menu(Page::Party),
                    "Restart [F5]" => self.restart_offline(),
                    "Return to menu" => self.return_to_menu(),
                    _ => self.quit = true,
                }
            }
        }
    }
}
fn main() {
    let cli = Cli::parse();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    hardware::prepare(&Settings::load(), cli.safe_graphics);
    let conf = window_conf(&cli);
    startup::install(&conf);
    macroquad::Window::from_config(
        macroquad::conf::Conf {
            miniquad_conf: conf,
            update_on: None,
            default_filter_mode: FilterMode::Linear,
            // Every macroquad draw call is its own render pass, and drivers
            // that emulate OpenGL (macOS) reload and store the whole
            // multisampled framebuffer per pass. Batches big enough for a
            // full arena keep a frame to a handful of passes. Vertices are
            // addressed with u16 indices, which caps them at 65535.
            draw_call_vertex_capacity: 65_535,
            draw_call_index_capacity: 196_605,
        },
        run(cli),
    );
}

async fn run(cli: Cli) {
    prevent_quit();
    // Drop GPU resources before the window and its OpenGL library are closed.
    let _resources = graphics::Resources;
    let mut initial_settings = Settings::load();
    if cli.safe_graphics {
        initial_settings.use_safe_graphics();
    }
    let mut native_resolution = native_resolution::NativeResolution::new(&initial_settings);
    ui::init_font();
    // The splash hands over the live arena it revealed, which the password
    // prompt and then the menu carry on drawing.
    let mut arena = None;
    if cli.smoke_frames.is_none() && cli.screenshot.is_none() {
        let Some(revealed) = splash::show(&initial_settings).await else {
            clipboard::shutdown();
            ui::unload_font();
            return;
        };
        arena = Some(revealed);
    }
    if cli.smoke_frames.is_none()
        && let unlock::Outcome::Quit =
            unlock::unlock_packs(&mod_library::staff_packs(&initial_settings), arena.as_mut()).await
    {
        clipboard::shutdown();
        ui::unload_font();
        return;
    }
    let Some(dark_theme) = first_run::choose_theme(cli.smoke_frames.is_some()).await else {
        ui::unload_font();
        return;
    };
    if cli.screenshot.is_some() {
        graphics::prepare_screen_capture();
    }
    let mut app = App::new(cli, dark_theme).await;
    if let Some(arena) = arena {
        app.menu.ambient = arena;
    }
    macroquad::texture::set_default_filter_mode(FilterMode::Linear);
    loop {
        if is_quit_requested() {
            break;
        }
        if std::mem::take(&mut app.menu.staff_unlock) {
            let packs = mod_library::staff_packs(&app.settings);
            if let unlock::Outcome::Quit = unlock::unlock_packs(&packs, None).await {
                break;
            }
            app.menu.staff_changed = true;
        }
        if std::mem::take(&mut app.menu.staff_changed) {
            app.creator.reload_key();
            app.central_owner = central_owner::CentralOwner::new();
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
                    .find(|t| t.id == session.spectating.unwrap_or(session.player))
            })
            .map_or(1, |tank| tank.level);
        let target_zoom = (screen_height() / 900.0).clamp(0.7, 2.75)
            / (1.0 + (f32::from(level.saturating_sub(1)) * 0.006));
        let ease = 1.0 - (-dt * 9.0).exp();
        app.view.zoom = if app.settings.reduced_motion {
            target_zoom
        } else {
            app.view.zoom + (target_zoom - app.view.zoom) * ease
        };
        if is_key_pressed(KeyCode::F9) {
            app.settings.dark_arena = !app.settings.dark_arena;
            let _ = app.settings.save();
        }
        if is_key_pressed(KeyCode::F11) {
            app.settings.fullscreen = !app.settings.fullscreen;
            let _ = app.settings.save();
        }
        native_resolution.update(&app.settings, app.display_revision);
        set_theme(app.settings.dark_arena);
        graphics::set_quality(app.settings.shape_quality);
        graphics::set_filter(app.settings.anisotropic_filtering);
        if app.frames == 1 {
            hardware::probe();
            graphics::refresh_capabilities(app.settings.anisotropic_filtering);
        }
        set_reduced_motion(app.settings.reduced_motion);
        accessibility::configure(
            app.settings.colorblind,
            app.settings.high_contrast,
            app.settings.reduced_motion,
        );
        let s = app.scale();
        app.update(dt, s);
        // Send this frame's input now; drawing takes a few milliseconds
        // more, and anything queued while drawing goes after it.
        if let Some(session) = &mut app.session {
            session.flush_network();
        }
        let post_error = app.presentation.post_processing_error();
        if app.menu.post_error.as_deref() != post_error {
            app.menu.post_error = post_error.map(str::to_owned);
        }
        app.presentation.begin(&app.settings);
        app.draw(s);
        app.presentation.finish();
        // Discard characters that no field consumed this frame.
        while get_char_pressed().is_some() {}
        if let Some(session) = &mut app.session {
            session.flush_network();
        }
        if app.smoke.is_some() {
            // Time the frame's CPU and GPU work, not the wait for vsync:
            // submit the batch and wait for the GPU to finish it.
            let cpu = (get_time() - start) as f32;
            unsafe {
                let mut gl = get_internal_gl();
                gl.flush();
                if gl.quad_context.info().backend == miniquad::Backend::OpenGl {
                    miniquad::gl::glFinish();
                }
            }
            // Skip startup frames, which include loading and shader compilation.
            if app.frames > 60 {
                app.frame_times.push((get_time() - start) as f32);
                app.cpu_times.push(cpu);
            }
        }
        app.frames += 1;
        if app.smoke.is_some_and(|limit| app.frames >= limit) {
            if let Some(path) = &app.screenshot
                && let Some(image) = graphics::screen_image()
            {
                image.export_png(path);
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
            if !app.frame_times.is_empty() {
                let mut times = app.frame_times.clone();
                times.sort_by(f32::total_cmp);
                let mean = times.iter().sum::<f32>() / times.len() as f32;
                let p99 = times[(times.len() * 99 / 100).min(times.len() - 1)];
                let cpu = app.cpu_times.iter().sum::<f32>() / app.cpu_times.len() as f32;
                println!(
                    "Tank Yard frame work: mean={:.2}ms p99={:.2}ms cpu={:.2}ms ({:.0} fps uncapped)",
                    mean * 1000.,
                    p99 * 1000.,
                    cpu * 1000.,
                    1. / mean.max(1e-6)
                );
            }
            break;
        }
        if app.quit {
            break;
        }
        if app.smoke.is_none() {
            let cap = frame_cap(
                app.settings.frame_cap,
                app.session.is_some(),
                app.settings.battery_saver,
            );
            hold_frame_rate(start, cap, app.settings.vsync);
        }
        next_frame().await;
    }
    native_resolution.remember(&mut app.settings);
    app.save_progress();
    clipboard::shutdown();
}
/// Sleep out the rest of a frame that began at `start`, to hold `cap` frames
/// per second (0 for none).
fn hold_frame_rate(start: f64, cap: u32, vsync: bool) {
    if cap == 0 {
        return;
    }
    // With vsync the sleep ends early, so the frame still catches the display
    // refresh it is due for (menus on a 120 Hz display run at 60 rather than
    // 120).
    let margin = if vsync { 0.002 } else { 0.0005 };
    let remaining = 1.0 / f64::from(cap) - (get_time() - start);
    if remaining > margin + 0.0005 {
        std::thread::sleep(std::time::Duration::from_secs_f64(remaining - margin));
    }
}
/// The frame rate to hold (0 for none). Menus don't need more than 60 fps,
/// so they don't run the GPU flat out, even on high refresh displays; games
/// keep the chosen cap. Battery saver halves both.
fn frame_cap(chosen: u32, in_game: bool, battery_saver: bool) -> u32 {
    let limit = match (in_game, battery_saver) {
        (true, false) => u32::MAX,
        (true, true) | (false, false) => 60,
        (false, true) => 30,
    };
    match chosen {
        0 if limit == u32::MAX => 0,
        0 => limit,
        cap => cap.min(limit),
    }
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
        "infection" => Mode::Infection,
        "kingofthehill" | "koth" | "hill" => Mode::KingOfTheHill,
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

#[cfg(test)]
mod tap_input_regressions {
    use super::manual_fire;

    #[test]
    fn captured_click_survives_moving_the_cursor_over_the_hud() {
        assert!(manual_fire(false, true, true));
        assert!(manual_fire(false, true, false));
        assert!(!manual_fire(true, false, true));
        assert!(!manual_fire(false, false, false));
        assert!(manual_fire(true, false, false));
    }
}

#[cfg(test)]
mod frame_cap_regressions {
    use super::frame_cap;

    #[test]
    fn menus_hold_sixty_and_battery_saver_halves_the_caps() {
        assert_eq!(frame_cap(0, true, false), 0);
        assert_eq!(frame_cap(144, true, false), 144);
        assert_eq!(frame_cap(0, false, false), 60);
        assert_eq!(frame_cap(144, false, false), 60);
        assert_eq!(frame_cap(0, true, true), 60);
        assert_eq!(frame_cap(144, true, true), 60);
        assert_eq!(frame_cap(0, false, true), 30);
        assert_eq!(frame_cap(30, true, true), 30);
    }
}
