use directories::ProjectDirs;
use macroquad::prelude::KeyCode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CENTRAL_SERVER: &str = "tankyard.kokonico.me";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerSort {
    #[default]
    Saved,
    Address,
    ReverseAddress,
}
impl ServerSort {
    pub fn next(self) -> Self {
        match self {
            Self::Saved => Self::Address,
            Self::Address => Self::ReverseAddress,
            Self::ReverseAddress => Self::Saved,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ServerList {
    #[default]
    Favorites,
    Recent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub player_name: String,
    pub server_address: String,
    pub favorites: Vec<String>,
    pub recent_servers: Vec<String>,
    pub favorite_sort: ServerSort,
    pub recent_sort: ServerSort,
    pub width: u32,
    pub height: u32,
    pub window_position: Option<[i32; 2]>,
    pub native_resolution: bool,
    pub fullscreen: bool,
    pub vsync: bool,
    pub anti_aliasing: u8,
    pub shape_quality: u8,
    pub anisotropic_filtering: u8,
    pub camera_smoothing: f32,
    pub frame_cap: u32,
    pub ui_scale: f32,
    pub effects: f32,
    pub shake: f32,
    pub reduced_motion: bool,
    pub colorblind: bool,
    pub high_contrast: bool,
    pub dark_arena: bool,
    pub theme_chosen: bool,
    pub master_volume: f32,
    pub effects_volume: f32,
    pub voice_volume: f32,
    pub auto_fire: bool,
    pub bot_difficulty: usize,
    pub show_fps: bool,
    pub bindings: [String; 6],
    pub games_played: u32,
    pub total_kills: u32,
    pub highest_score: u32,
    pub unlocked_achievements: Vec<String>,
    pub achievement_mode: tankyard_client::achievements::AchievementMode,
    pub practice_records: Vec<tankyard_client::practice::Record>,
    pub enabled_mods: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            player_name: "Pilot".into(),
            server_address: CENTRAL_SERVER.into(),
            favorites: vec![],
            recent_servers: vec![],
            favorite_sort: ServerSort::Saved,
            recent_sort: ServerSort::Saved,
            width: 1280,
            height: 800,
            window_position: None,
            native_resolution: false,
            fullscreen: false,
            vsync: true,
            anti_aliasing: 4,
            shape_quality: 2,
            anisotropic_filtering: 1,
            camera_smoothing: 0.75,
            frame_cap: 0,
            ui_scale: 1.0,
            effects: 0.8,
            shake: 0.25,
            reduced_motion: false,
            colorblind: false,
            high_contrast: false,
            dark_arena: false,
            theme_chosen: false,
            master_volume: 0.65,
            effects_volume: 0.7,
            voice_volume: 0.8,
            auto_fire: false,
            bot_difficulty: 1,
            show_fps: false,
            bindings: ["W", "S", "A", "D", "Space", "E"].map(str::to_owned),
            games_played: 0,
            total_kills: 0,
            highest_score: 0,
            unlocked_achievements: vec![],
            achievement_mode: Default::default(),
            practice_records: vec![],
            enabled_mods: vec![],
        }
    }
}
impl Settings {
    pub fn reset_preferences(&mut self) {
        *self = Self {
            player_name: std::mem::take(&mut self.player_name),
            server_address: std::mem::take(&mut self.server_address),
            favorites: std::mem::take(&mut self.favorites),
            recent_servers: std::mem::take(&mut self.recent_servers),
            window_position: self.window_position,
            theme_chosen: true,
            games_played: self.games_played,
            total_kills: self.total_kills,
            highest_score: self.highest_score,
            unlocked_achievements: std::mem::take(&mut self.unlocked_achievements),
            practice_records: std::mem::take(&mut self.practice_records),
            enabled_mods: std::mem::take(&mut self.enabled_mods),
            ..Self::default()
        };
    }
    pub fn use_safe_graphics(&mut self) {
        self.native_resolution = false;
        self.fullscreen = false;
        self.width = 1280;
        self.height = 800;
        self.anti_aliasing = 1;
        self.anisotropic_filtering = 1;
    }
    pub fn record_session(&mut self, tracker: &tankyard_client::achievements::Tracker) {
        let (rounds, kills) = tracker.session_totals();
        self.games_played = self.games_played.saturating_add(rounds);
        self.total_kills = self.total_kills.saturating_add(kills);
        self.highest_score = self.highest_score.max(tracker.highest_score());
    }
    pub fn path() -> PathBuf {
        ProjectDirs::from("org", "Tank Yard", "Tank Yard")
            .map(|p| p.config_dir().join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("tankyard-settings.json"))
    }
    pub fn load() -> Self {
        let current = Self::path();
        let legacy = ProjectDirs::from("org", "Shellvector", "Shellvector")
            .map(|p| p.config_dir().join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("shellvector-settings.json"));
        let path = if current.exists() { current } else { legacy };
        let mut value = std::fs::metadata(&path)
            .ok()
            .filter(|metadata| metadata.len() <= 1024 * 1024)
            .and_then(|_| std::fs::read(&path).ok())
            .and_then(|bytes| {
                let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
                let mut value: Self = serde_json::from_value(json.clone()).ok()?;
                // Existing installations have already chosen a preference.
                if json.get("theme_chosen").is_none() {
                    value.theme_chosen = true;
                }
                Some(value)
            })
            .unwrap_or_default();
        value.validate();
        value
    }
    pub fn save(&mut self) -> Result<(), String> {
        self.validate();
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())
    }
    pub fn validate(&mut self) {
        self.bot_difficulty = self.bot_difficulty.min(3);
        let mut ids = std::collections::HashSet::new();
        self.practice_records.retain(|record| {
            tankyard_client::practice::CHALLENGES
                .iter()
                .any(|challenge| {
                    challenge.id == record.id
                        && record.best_ticks <= challenge.seconds * 60
                        && record.best_progress <= challenge.target()
                        && record.completed <= record.attempts
                })
                && ids.insert(record.id.clone())
        });
        self.practice_records
            .truncate(tankyard_client::practice::CHALLENGES.len());
        self.width = self.width.clamp(800, 7680);
        self.height = self.height.clamp(600, 4320);
        if self.window_position.is_some_and(|position| {
            position
                .into_iter()
                .any(|coordinate| !(-131072..=131072).contains(&coordinate))
        }) {
            self.window_position = None;
        }
        self.frame_cap = self.frame_cap.min(1000);
        if !matches!(self.anti_aliasing, 1 | 4 | 8) {
            self.anti_aliasing = 4;
        }
        self.shape_quality = self.shape_quality.min(2);
        if !matches!(self.anisotropic_filtering, 1 | 2 | 4 | 8 | 16) {
            self.anisotropic_filtering = 1;
        }
        self.camera_smoothing = finite_clamp(self.camera_smoothing, 0.0, 1.0, 0.75);
        self.ui_scale = finite_clamp(self.ui_scale, 0.8, 1.4, 1.0);
        for value in [
            &mut self.effects,
            &mut self.shake,
            &mut self.master_volume,
            &mut self.effects_volume,
            &mut self.voice_volume,
        ] {
            *value = finite_clamp(*value, 0.0, 1.0, 0.5);
        }
        self.player_name = self
            .player_name
            .chars()
            .filter(|c| !c.is_control())
            .take(24)
            .collect();
        if self.player_name.trim().is_empty() {
            self.player_name = "Pilot".into();
        }
        self.server_address = clean_address(&self.server_address);
        clean_servers(&mut self.favorites, 32);
        clean_servers(&mut self.recent_servers, 12);
        self.unlocked_achievements.retain(|id| {
            tankyard_client::achievements::ACHIEVEMENTS
                .iter()
                .any(|achievement| achievement.id == id)
        });
        self.unlocked_achievements.sort_unstable();
        self.unlocked_achievements.dedup();
        self.enabled_mods.truncate(tankyard_mods::MAX_MODS);
        self.enabled_mods
            .retain(|p| !p.is_empty() && p.len() <= 512 && !p.chars().any(char::is_control));
        for (index, binding) in self.bindings.iter_mut().enumerate() {
            if key_from_name(binding).is_none() {
                *binding = Settings::default().bindings[index].clone();
            }
        }
    }
    pub fn remember_server(&mut self, address: &str) {
        let address = clean_address(address);
        if address.is_empty() {
            return;
        }
        self.recent_servers.retain(|s| !same_server(s, &address));
        self.recent_servers.insert(0, address);
        self.recent_servers.truncate(12);
    }
    pub fn add_favorite(&mut self, address: &str) -> Result<(), String> {
        let address = clean_address(address);
        if address.is_empty() {
            return Err("Enter a server address first.".into());
        }
        if self.favorites.iter().any(|s| same_server(s, &address)) {
            return Ok(());
        }
        if self.favorites.len() >= 32 {
            return Err("Favorites are full. Remove a saved server first.".into());
        }
        self.favorites.push(address);
        Ok(())
    }
    pub fn server_sort(&self, list: ServerList) -> ServerSort {
        match list {
            ServerList::Favorites => self.favorite_sort,
            ServerList::Recent => self.recent_sort,
        }
    }
    pub fn cycle_server_sort(&mut self, list: ServerList) {
        let sort = match list {
            ServerList::Favorites => &mut self.favorite_sort,
            ServerList::Recent => &mut self.recent_sort,
        };
        *sort = sort.next();
    }
    pub fn servers(&self, list: ServerList) -> &[String] {
        match list {
            ServerList::Favorites => &self.favorites,
            ServerList::Recent => &self.recent_servers,
        }
    }
    pub fn sorted_servers(&self, list: ServerList) -> Vec<String> {
        let mut result = self.servers(list).to_vec();
        match self.server_sort(list) {
            ServerSort::Saved => {}
            ServerSort::Address => result.sort_by_cached_key(|a| a.to_ascii_lowercase()),
            ServerSort::ReverseAddress => {
                result.sort_by_cached_key(|a| std::cmp::Reverse(a.to_ascii_lowercase()))
            }
        }
        result
    }
    pub fn remove_server(&mut self, list: ServerList, address: &str) {
        let servers = match list {
            ServerList::Favorites => &mut self.favorites,
            ServerList::Recent => &mut self.recent_servers,
        };
        servers.retain(|s| !same_server(s, address));
    }
    pub fn move_favorite(&mut self, address: &str, direction: isize) {
        if let Some(index) = self.favorites.iter().position(|s| same_server(s, address)) {
            let target = index
                .saturating_add_signed(direction)
                .min(self.favorites.len() - 1);
            self.favorites.swap(index, target);
            self.favorite_sort = ServerSort::Saved;
        }
    }
    pub fn key(&self, index: usize) -> KeyCode {
        key_from_name(&self.bindings[index]).unwrap_or(KeyCode::Space)
    }
}
fn clean_servers(servers: &mut Vec<String>, limit: usize) {
    let mut seen = std::collections::HashSet::new();
    for address in servers.iter_mut() {
        *address = clean_address(address);
    }
    servers.retain(|address| !address.is_empty() && seen.insert(server_key(address)));
    servers.truncate(limit);
}
pub fn same_server(a: &str, b: &str) -> bool {
    server_key(a) == server_key(b)
}
fn server_key(value: &str) -> String {
    let value = value.trim();
    let (scheme, rest) = value
        .split_once("://")
        .map_or((String::new(), value), |(scheme, rest)| {
            (format!("{}://", scheme.to_ascii_lowercase()), rest)
        });
    let (authority, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    // Hosts are case insensitive; WebSocket paths and query strings are not.
    format!("{scheme}{}{path}", authority.to_ascii_lowercase())
}
fn clean_address(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(128)
        .collect()
}
fn finite_clamp(value: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        default
    }
}
pub fn key_from_name(name: &str) -> Option<KeyCode> {
    Some(match name {
        "W" => KeyCode::W,
        "S" => KeyCode::S,
        "A" => KeyCode::A,
        "D" => KeyCode::D,
        "E" => KeyCode::E,
        "Q" => KeyCode::Q,
        "R" => KeyCode::R,
        "F" => KeyCode::F,
        "Z" => KeyCode::Z,
        "X" => KeyCode::X,
        "C" => KeyCode::C,
        "V" => KeyCode::V,
        "I" => KeyCode::I,
        "J" => KeyCode::J,
        "K" => KeyCode::K,
        "L" => KeyCode::L,
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Left" => KeyCode::Left,
        "Right" => KeyCode::Right,
        "Space" => KeyCode::Space,
        "LeftShift" => KeyCode::LeftShift,
        "LeftControl" => KeyCode::LeftControl,
        _ => return None,
    })
}
pub fn bindable_pressed() -> Option<String> {
    [
        "W",
        "S",
        "A",
        "D",
        "E",
        "Q",
        "R",
        "F",
        "Z",
        "X",
        "C",
        "V",
        "I",
        "J",
        "K",
        "L",
        "Up",
        "Down",
        "Left",
        "Right",
        "Space",
        "LeftShift",
        "LeftControl",
    ]
    .into_iter()
    .find(|name| macroquad::prelude::is_key_pressed(key_from_name(name).unwrap()))
    .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoring_preferences_keeps_identity_servers_mods_and_earned_progress() {
        let mut settings: Settings = serde_json::from_str(r#"{
            "player_name":"Mira","server_address":"arena.example:5000",
            "favorites":["favorite.example:5000"],"recent_servers":["recent.example:5000"],
            "window_position":[-1800,75],"enabled_mods":["mods/fast_arena"],
            "games_played":12,"total_kills":55,"highest_score":9000,
            "unlocked_achievements":["meet_central_owner"],
            "practice_records":[{"id":"shape_sprint","attempts":3,"completed":1,"best_ticks":2400,"best_progress":500}],
            "width":1920,"height":1080,"fullscreen":true,"native_resolution":true,
            "ui_scale":1.4,"master_volume":0.0,"colorblind":true,"high_contrast":true,
            "reduced_motion":true,"auto_fire":true,"dark_arena":true,
            "bindings":["Up","Down","Left","Right","Q","R"]
        }"#).unwrap();
        let before = serde_json::to_value(&settings).unwrap();
        settings.reset_preferences();
        settings.validate();
        let after = serde_json::to_value(&settings).unwrap();
        let defaults = Settings::default();
        assert_eq!(
            (settings.width, settings.height),
            (defaults.width, defaults.height)
        );
        assert_eq!(settings.bindings, defaults.bindings);
        assert_eq!(settings.ui_scale, defaults.ui_scale);
        assert_eq!(settings.master_volume, defaults.master_volume);
        assert!(!settings.fullscreen && !settings.native_resolution && !settings.auto_fire);
        assert!(
            !settings.colorblind
                && !settings.high_contrast
                && !settings.reduced_motion
                && !settings.dark_arena
        );
        assert!(settings.theme_chosen);
        for field in [
            "player_name",
            "server_address",
            "favorites",
            "recent_servers",
            "window_position",
            "enabled_mods",
            "games_played",
            "total_kills",
            "highest_score",
            "unlocked_achievements",
            "practice_records",
        ] {
            assert_eq!(before[field], after[field], "{field}");
        }
    }

    #[test]
    fn profile_records_only_observed_player_progress_even_without_a_final_snapshot() {
        use crate::session::{Engine, Session};
        use tankyard_content::Content;
        use tankyard_core::{PlayerInput, WorldConfig};

        let mut settings = Settings::default();
        settings.record_session(&tankyard_client::achievements::Tracker::default());
        assert_eq!(
            (
                settings.games_played,
                settings.total_kills,
                settings.highest_score
            ),
            (0, 0, 0)
        );
        let mut session = Session::offline(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            1,
            "Pilot",
        );
        if let Engine::Offline(local) = &mut session.engine {
            for tank in &mut local.world.tanks {
                tank.kills = if tank.bot { 100 } else { 2 };
                tank.score = if tank.bot { 50_000 } else { 500 };
            }
        }
        session.tick(PlayerInput::default(), &Content::builtin());
        session.display = None;
        settings.record_session(&session.achievement_tracker);
        assert_eq!(
            (
                settings.games_played,
                settings.total_kills,
                settings.highest_score
            ),
            (1, 2, 500)
        );
    }

    #[test]
    fn old_profiles_keep_safe_defaults() {
        let mut settings: Settings = serde_json::from_str(
            r#"{"player_name":"Old pilot","width":0,"master_volume":4.0,"future_option":true}"#,
        )
        .unwrap();
        settings.validate();
        assert_eq!(settings.player_name, "Old pilot");
        assert_eq!(settings.width, 800);
        assert_eq!(settings.master_volume, 1.0);
        assert_eq!(settings.bindings[0], "W");
        assert!(!settings.dark_arena);
        assert!(settings.unlocked_achievements.is_empty());
        assert!(settings.practice_records.is_empty());
    }

    #[test]
    fn practice_records_round_trip_and_reject_invalid_or_duplicate_entries() {
        let mut settings: Settings = serde_json::from_str(
            r#"{"practice_records":[
            {"id":"shape_sprint","attempts":3,"completed":1,"best_ticks":2400,"best_progress":500},
            {"id":"shape_sprint","attempts":7,"completed":2,"best_ticks":2000,"best_progress":500},
            {"id":"unknown","attempts":1,"completed":0,"best_ticks":0,"best_progress":0},
            {"id":"drone_drill","attempts":1,"completed":2,"best_ticks":60,"best_progress":1500},
            {"id":"skirmish","attempts":1,"completed":0,"best_ticks":0,"best_progress":999}
        ]}"#,
        )
        .unwrap();
        settings.validate();
        assert_eq!(settings.practice_records.len(), 1);
        assert_eq!(settings.practice_records[0].attempts, 3);
        let loaded: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(loaded.practice_records[0].best_ticks, 2400);
        assert_eq!(loaded.games_played, 0);
    }

    #[test]
    fn validation_bounds_numbers_and_unicode_without_panicking() {
        let mut settings = Settings {
            ui_scale: f32::NAN,
            effects: f32::INFINITY,
            player_name: "\n\t".into(),
            server_address: "界".repeat(200),
            favorites: vec![" \n ".into()],
            ..Default::default()
        };
        settings.bindings[0] = "invalid-key".into();
        settings.validate();
        assert_eq!(settings.ui_scale, 1.0);
        assert!(settings.effects.is_finite());
        assert_eq!(settings.player_name, "Pilot");
        assert_eq!(settings.server_address.chars().count(), 128);
        assert!(settings.favorites.is_empty());
        assert_eq!(settings.key(0), KeyCode::W);
    }

    #[test]
    fn default_server_is_central_and_saved_manual_addresses_are_preserved() {
        assert_eq!(Settings::default().server_address, CENTRAL_SERVER);
        assert_eq!(
            tankyard_server::websocket::websocket_url(CENTRAL_SERVER),
            Some("wss://tankyard.kokonico.me".into())
        );
        let mut saved: Settings =
            serde_json::from_str(r#"{"server_address":"127.0.0.1:5000"}"#).unwrap();
        saved.validate();
        assert_eq!(saved.server_address, "127.0.0.1:5000");
    }

    #[test]
    fn recent_servers_are_unique_and_bounded() {
        let mut settings = Settings::default();
        for index in 0..25 {
            settings.remember_server(&format!("192.168.1.{index}:5000"));
        }
        settings.remember_server("192.168.1.24:5000");
        assert_eq!(settings.recent_servers.len(), 12);
        assert_eq!(settings.recent_servers[0], "192.168.1.24:5000");
        assert_eq!(
            settings
                .recent_servers
                .iter()
                .filter(|s| s.as_str() == "192.168.1.24:5000")
                .count(),
            1
        );
    }

    #[test]
    fn saved_server_migration_deduplicates_before_limits_and_keeps_lists_independent() {
        let mut settings: Settings = serde_json::from_str(
            r#"{
            "favorites":[" Arena.Example:5000 ","arena.example:5000","  ","other:5000"],
            "recent_servers":["arena.example:5000"," recent:5000 ","RECENT:5000"]
        }"#,
        )
        .unwrap();
        settings.validate();
        assert_eq!(settings.favorites, ["Arena.Example:5000", "other:5000"]);
        assert_eq!(
            settings.recent_servers,
            ["arena.example:5000", "recent:5000"]
        );
        settings.remove_server(ServerList::Recent, "ARENA.EXAMPLE:5000");
        assert_eq!(settings.favorites.len(), 2);
        assert_eq!(settings.recent_servers, ["recent:5000"]);
        settings.remove_server(ServerList::Favorites, "arena.example:5000");
        assert_eq!(settings.favorites, ["other:5000"]);
    }

    #[test]
    fn server_sorting_persists_without_rewriting_connection_recency() {
        let mut settings = Settings::default();
        for address in ["zeta:5000", "Alpha:5000", "beta:5000"] {
            settings.add_favorite(address).unwrap();
            settings.remember_server(address);
        }
        settings.cycle_server_sort(ServerList::Recent);
        assert_eq!(
            settings.sorted_servers(ServerList::Recent),
            ["Alpha:5000", "beta:5000", "zeta:5000"]
        );
        assert_eq!(settings.recent_servers[0], "beta:5000");
        settings.cycle_server_sort(ServerList::Favorites);
        settings.cycle_server_sort(ServerList::Favorites);
        assert_eq!(
            settings.sorted_servers(ServerList::Favorites),
            ["zeta:5000", "beta:5000", "Alpha:5000"]
        );
        let loaded: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(loaded.favorite_sort, ServerSort::ReverseAddress);
        assert_eq!(loaded.recent_sort, ServerSort::Address);
        settings.remember_server(" ALPHA:5000 ");
        assert_eq!(
            settings.recent_servers,
            ["ALPHA:5000", "beta:5000", "zeta:5000"]
        );
    }

    #[test]
    fn server_deduplication_preserves_case_sensitive_websocket_paths_and_queries() {
        let mut settings = Settings::default();
        for address in [
            "wss://Arena.example/Room?room=A",
            "wss://arena.EXAMPLE/Room?room=A",
            "wss://arena.example/room?room=A",
            "wss://arena.example/Room?room=a",
        ] {
            settings.add_favorite(address).unwrap();
            settings.remember_server(address);
        }
        settings.validate();
        assert_eq!(settings.favorites.len(), 3);
        assert_eq!(settings.recent_servers.len(), 3);
        settings.remove_server(ServerList::Favorites, "wss://ARENA.EXAMPLE/Room?room=A");
        assert_eq!(settings.favorites.len(), 2);
        assert!(
            settings
                .favorites
                .iter()
                .any(|s| s.ends_with("room?room=A"))
        );
        assert!(
            settings
                .favorites
                .iter()
                .any(|s| s.ends_with("Room?room=a"))
        );
    }

    #[test]
    fn favorite_reordering_handles_bounds_capacity_and_empty_addresses() {
        let mut settings = Settings::default();
        assert!(settings.add_favorite(" \n").is_err());
        for i in 0..32 {
            settings.add_favorite(&format!("arena-{i}:5000")).unwrap();
        }
        assert!(settings.add_favorite("extra:5000").is_err());
        assert!(settings.add_favorite(" ARENA-0:5000 ").is_ok());
        settings.move_favorite("arena-0:5000", -1);
        settings.move_favorite("arena-31:5000", 1);
        settings.move_favorite("arena-2:5000", -1);
        assert_eq!(settings.favorites[1], "arena-2:5000");
        assert_eq!(settings.favorite_sort, ServerSort::Saved);
        settings.remove_server(ServerList::Favorites, "arena-31:5000");
        settings.add_favorite("extra:5000").unwrap();
        assert_eq!(settings.favorites.len(), 32);
    }

    #[test]
    fn achievement_profile_discards_unknown_ids_and_duplicates() {
        let mut settings: Settings = serde_json::from_str(
            r#"{"unlocked_achievements":["first_kill","unknown","first_kill","level_45"]}"#,
        )
        .unwrap();
        settings.validate();
        assert_eq!(settings.unlocked_achievements, ["first_kill", "level_45"]);
        let saved = serde_json::to_string(&settings).unwrap();
        let loaded: Settings = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded.unlocked_achievements, settings.unlocked_achievements);
        assert_eq!(
            loaded.achievement_mode,
            tankyard_client::achievements::AchievementMode::PerMatch
        );
    }
}
