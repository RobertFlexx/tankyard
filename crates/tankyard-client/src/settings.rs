use directories::ProjectDirs;
use macroquad::prelude::KeyCode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CENTRAL_SERVER: &str = "tankyard.kokonico.me";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub player_name: String,
    pub server_address: String,
    pub favorites: Vec<String>,
    pub recent_servers: Vec<String>,
    pub width: u32,
    pub height: u32,
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
    pub ui_volume: f32,
    pub music_volume: f32,
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
            width: 1280,
            height: 800,
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
            ui_volume: 0.6,
            music_volume: 0.0,
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
    pub fn use_safe_graphics(&mut self) {
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
            &mut self.ui_volume,
            &mut self.music_volume,
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
        self.favorites.truncate(32);
        self.recent_servers.truncate(12);
        for address in self.favorites.iter_mut().chain(&mut self.recent_servers) {
            *address = clean_address(address);
        }
        self.favorites.retain(|address| !address.is_empty());
        self.recent_servers.retain(|address| !address.is_empty());
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
        self.recent_servers.retain(|s| s != address);
        self.recent_servers.insert(0, address.to_owned());
        self.recent_servers.truncate(12);
    }
    pub fn key(&self, index: usize) -> KeyCode {
        key_from_name(&self.bindings[index]).unwrap_or(KeyCode::Space)
    }
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
