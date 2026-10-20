use crate::NetError;
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, net::ToSocketAddrs, path::Path};
use tankyard_ai::Difficulty;
use tankyard_core::{Mode, WorldConfig};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSettings {
    pub kind: tankyard_protocol::ServerKind,
    pub server_id: String,
    pub chat_enabled: bool,
    pub voice_enabled: bool,
    /// Official public arenas enable signed creator tools by default. Community
    /// hosts must explicitly opt in; false always overrides the official default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_admin: Option<bool>,
    /// Separate, signed owner identity for the central arena only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub central_owner_admin: Option<bool>,
    pub access_file: String,
    pub audit_file: String,
    pub global_audit_file: String,
    pub require_accounts: bool,
    pub netcode_private_key: Option<String>,
    pub name: String,
    pub bind: String,
    pub public_addresses: Vec<std::net::SocketAddr>,
    pub max_players: usize,
    pub bot_count: usize,
    pub adaptive_bots: bool,
    pub difficulty: Difficulty,
    pub password: String,
    pub allow_sandbox: bool,
    pub snapshot_rate: u32,
    pub interest_radius: f32,
    pub handshake_timeout: f32,
    pub input_timeout: f32,
    pub ban_file: String,
    /// Shared relay addresses must never receive an address-wide player ban.
    pub shared_relay_ips: Vec<std::net::IpAddr>,
    pub round_restart_seconds: f32,
    pub world: WorldConfig,
    pub mods: Vec<std::path::PathBuf>,
}
impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            kind: tankyard_protocol::ServerKind::Community,
            server_id: String::new(),
            chat_enabled: true,
            voice_enabled: true,
            creator_admin: None,
            central_owner_admin: None,
            access_file: String::new(),
            audit_file: String::new(),
            global_audit_file: String::new(),
            require_accounts: false,
            netcode_private_key: None,
            name: "Tank Yard Community Arena".into(),
            bind: "0.0.0.0:5000".into(),
            public_addresses: vec![],
            max_players: 64,
            bot_count: 12,
            adaptive_bots: true,
            difficulty: Difficulty::Normal,
            password: String::new(),
            allow_sandbox: false,
            snapshot_rate: 20,
            interest_radius: 1350.,
            handshake_timeout: 8.,
            input_timeout: 0.5,
            ban_file: "bans.json".into(),
            shared_relay_ips: vec![],
            round_restart_seconds: 10.,
            world: WorldConfig::default(),
            mods: vec![],
        }
    }
}
impl ServerSettings {
    pub fn central() -> Self {
        // Shipped defaults also work in minimal Docker images without configs/.
        toml::from_str(include_str!("../../../configs/central.toml"))
            .expect("built-in central server configuration")
    }
    pub fn central_owner_enabled(&self) -> bool {
        self.central_owner_admin.unwrap_or(
            self.server_id == "tankyard-central"
                && self.kind == tankyard_protocol::ServerKind::Official,
        ) && self.kind == tankyard_protocol::ServerKind::Official
            && self.server_id == "tankyard-central"
            && self.password.is_empty()
    }
    pub fn creator_admin_enabled(&self) -> bool {
        self.password.is_empty()
            && self
                .creator_admin
                .unwrap_or(self.kind == tankyard_protocol::ServerKind::Official)
    }
    pub(crate) fn creator_audit_file(&self) -> &str {
        if self.audit_file.is_empty() && self.kind == tankyard_protocol::ServerKind::Official {
            "creator-audit.jsonl"
        } else {
            &self.audit_file
        }
    }
    pub(crate) fn shared_relay(&self, ip: std::net::IpAddr) -> bool {
        ip.is_loopback() || self.shared_relay_ips.contains(&ip)
    }
    /// Fill quiet online arenas to six participants; humans always take priority.
    pub fn desired_bots(&self, humans: usize) -> usize {
        if self.adaptive_bots {
            self.bot_count.min(6usize.saturating_sub(humans))
        } else {
            self.bot_count
        }
    }
    pub(crate) fn configured_match(
        &self,
        maps: &[tankyard_content::MapDef],
        mode: Mode,
        map: &str,
        difficulty: Difficulty,
    ) -> Result<Self, NetError> {
        if mode == Mode::Sandbox && !self.allow_sandbox {
            return Err(NetError(
                "Sandbox is disabled by the host configuration.".into(),
            ));
        }
        let map = if mode == Mode::Maze && map == "open" {
            "maze"
        } else {
            map
        };
        if !maps.iter().any(|candidate| candidate.id == map) {
            return Err(NetError("Map is not available on this server.".into()));
        }
        let mut candidate = self.clone();
        candidate.world.mode = mode;
        candidate.world.map = map.into();
        candidate.difficulty = difficulty;
        Ok(candidate)
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self, NetError> {
        let file = fs::File::open(path)?;
        if file.metadata()?.len() > 64 * 1024 {
            return Err(NetError("server configuration exceeds 64 KiB".into()));
        }
        let mut bytes = Vec::new();
        file.take(64 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 {
            return Err(NetError("server configuration exceeds 64 KiB".into()));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| NetError("server configuration is not UTF-8".into()))?;
        let settings: Self = toml::from_str(&text)
            .map_err(|e| NetError(format!("invalid server configuration: {e}")))?;
        settings.validate()?;
        Ok(settings)
    }
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), NetError> {
        self.validate()?;
        let text = toml::to_string_pretty(self).map_err(|e| NetError(e.to_string()))?;
        crate::storage::atomic_write(path.as_ref(), text.as_bytes())?;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), NetError> {
        if self.central_owner_admin == Some(true) && !self.central_owner_enabled() {
            return Err(NetError("central owner administration requires the password-free official tankyard-central profile".into()));
        }
        if self.creator_admin == Some(true)
            && (!self.password.is_empty()
                || (self.kind != tankyard_protocol::ServerKind::Official
                    && (self.audit_file.is_empty() || self.server_id.is_empty())))
        {
            return Err(NetError(
                "creator administration requires a password-free public arena; community hosts must also set a server ID and audit_file".into(),
            ));
        }
        if self.kind == tankyard_protocol::ServerKind::Official && !self.access_file.is_empty() {
            return Err(NetError(
                "community access files cannot grant roles on official servers".into(),
            ));
        }
        if (!self.server_id.is_empty() && !crate::access::account_id(&self.server_id))
            || (!self.access_file.is_empty() && self.server_id.is_empty())
            || self.access_file.len() > 512
            || self.audit_file.len() > 512
            || self.global_audit_file.len() > 512
            || (!self.global_audit_file.is_empty() && self.audit_file == self.global_audit_file)
        {
            return Err(NetError(
                "invalid server ID or moderation storage path".into(),
            ));
        }
        if let Some(key) = &self.netcode_private_key {
            parse_private_key(key)?;
        }
        if self.require_accounts && self.netcode_private_key.is_none() {
            return Err(NetError(
                "accounts require authenticated netcode transport".into(),
            ));
        }
        self.world.rules.validate().map_err(NetError)?;
        if self.mods.len() > tankyard_mods::MAX_MODS
            || self.mods.iter().any(|p| p.as_os_str().len() > 512)
        {
            return Err(NetError("invalid mod directory list".into()));
        }
        let invalid = |reason: &str| Err(NetError(reason.into()));
        if self.bind.to_socket_addrs()?.next().is_none() {
            return invalid("bind address did not resolve");
        }
        if self.public_addresses.len() > 8
            || self
                .public_addresses
                .iter()
                .any(|a| a.port() == 0 || a.ip().is_unspecified() || a.ip().is_multicast())
        {
            return invalid("invalid public transport address");
        }
        if self.name.is_empty() || self.name.len() > 96 || self.name.chars().any(char::is_control) {
            return invalid("server name must contain 1 to 96 printable bytes");
        }
        if !(1..=128).contains(&self.max_players)
            || self.bot_count > 128
            || self.max_players + self.bot_count > 256
        {
            return invalid("player/bot limits exceed 128 each or 256 total");
        }
        if self.password.len() > 128 {
            return invalid("password exceeds 128 bytes");
        }
        if self.shared_relay_ips.len() > 16
            || self
                .shared_relay_ips
                .iter()
                .any(|ip| ip.is_unspecified() || ip.is_multicast())
        {
            return invalid("shared_relay_ips must contain at most 16 unicast addresses");
        }
        if !(1..=30).contains(&self.snapshot_rate) {
            return invalid("snapshot_rate must be between 1 and 30");
        }
        if !self.interest_radius.is_finite() || !(400.0..=5000.0).contains(&self.interest_radius) {
            return invalid("interest_radius must be between 400 and 5000");
        }
        if !self.handshake_timeout.is_finite() || !(2.0..=30.0).contains(&self.handshake_timeout) {
            return invalid("handshake_timeout must be between 2 and 30 seconds");
        }
        if !self.input_timeout.is_finite() || !(0.1..=2.0).contains(&self.input_timeout) {
            return invalid("input_timeout must be between 0.1 and 2 seconds");
        }
        if !self.round_restart_seconds.is_finite()
            || !(2.0..=60.0).contains(&self.round_restart_seconds)
        {
            return invalid("round_restart_seconds must be between 2 and 60");
        }
        let world = &self.world;
        if !world.width.is_finite()
            || !world.height.is_finite()
            || !(900.0..=16000.0).contains(&world.width)
            || !(700.0..=16000.0).contains(&world.height)
        {
            return invalid("arena width must be 900..16000 and height 700..16000");
        }
        if world.target_count > 5000 || world.max_projectiles > 20000 || world.max_projectiles < 100
        {
            return invalid("entity population exceeds supported limits");
        }
        if !world.spawn_protection.is_finite() || !(0.0..=30.0).contains(&world.spawn_protection) {
            return invalid("spawn protection must be between 0 and 30 seconds");
        }
        if !world.time_limit.is_finite()
            || !(10.0..=86400.0).contains(&world.time_limit)
            || world.score_limit == 0
        {
            return invalid("invalid time or score limit");
        }
        if !(1..=10).contains(&world.lives) {
            return invalid("lives must be between 1 and 10");
        }
        if !tankyard_mods::identifier(&world.map) {
            return invalid("invalid map identifier");
        }
        let maps = if self.mods.is_empty() {
            tankyard_content::builtin_maps()
        } else {
            tankyard_mods::load(&self.mods)
                .map_err(NetError)?
                .bundle
                .maps
        };
        if !maps.iter().any(|map| map.id == world.map) {
            return invalid("unknown map identifier");
        }
        Ok(())
    }
}
impl std::fmt::Debug for ServerSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerSettings")
            .field("kind", &self.kind)
            .field("server_id", &self.server_id)
            .field("name", &self.name)
            .field("bind", &self.bind)
            .field("world", &self.world)
            .field("max_players", &self.max_players)
            .field("bot_count", &self.bot_count)
            .field("adaptive_bots", &self.adaptive_bots)
            .field("creator_admin", &self.creator_admin)
            .field("require_accounts", &self.require_accounts)
            .field("password", &"[redacted]")
            .field(
                "netcode_private_key",
                &self.netcode_private_key.as_ref().map(|_| "[redacted]"),
            )
            .finish_non_exhaustive()
    }
}
pub fn parse_private_key(value: &str) -> Result<[u8; 32], NetError> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(NetError(
            "netcode_private_key must contain exactly 64 hexadecimal characters".into(),
        ));
    }
    let mut bytes = [0; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|_| NetError("invalid transport key".into()))?;
    }
    Ok(bytes)
}
pub fn mode_from_name(name: &str) -> Result<Mode, String> {
    match name.to_ascii_lowercase().as_str() {
        "ffa" | "free-for-all" => Ok(Mode::Ffa),
        "two-teams" | "teams" => Ok(Mode::TwoTeams),
        "four-teams" => Ok(Mode::FourTeams),
        "maze" => Ok(Mode::Maze),
        "domination" => Ok(Mode::Domination),
        "survival" => Ok(Mode::Survival),
        "tag" => Ok(Mode::Tag),
        "mothership" => Ok(Mode::Mothership),
        "capture-core" => Ok(Mode::CaptureCore),
        "boss-rush" => Ok(Mode::BossRush),
        "deathmatch" | "tdm" => Ok(Mode::Deathmatch),
        "sandbox" => Ok(Mode::Sandbox),
        "random-events" | "events" | "random_events" => Ok(Mode::RandomEvents),
        "coop-pve" | "co-op-pve" | "pve" | "coop_pve" => Ok(Mode::CoopPve),
        _ => Err(format!("unknown mode {name}")),
    }
}

pub fn difficulty_from_name(name: &str) -> Result<Difficulty, String> {
    match name.to_ascii_lowercase().as_str() {
        "beginner" => Ok(Difficulty::Beginner),
        "normal" => Ok(Difficulty::Normal),
        "advanced" => Ok(Difficulty::Advanced),
        "expert" => Ok(Difficulty::Expert),
        _ => Err(format!("unknown difficulty {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn match_changes_validate_host_policy_and_map_before_replacing_settings() {
        let settings = ServerSettings::default();
        let maps = tankyard_content::builtin_maps();
        assert!(
            settings
                .configured_match(&maps, Mode::Ffa, "missing", Difficulty::Expert)
                .is_err()
        );
        assert!(
            settings
                .configured_match(&maps, Mode::Sandbox, "open", Difficulty::Expert)
                .is_err()
        );
        assert_eq!(settings.world.mode, Mode::Ffa);
        assert_eq!(settings.difficulty, Difficulty::Normal);
        let candidate = settings
            .configured_match(&maps, Mode::Maze, "open", Difficulty::Expert)
            .unwrap();
        assert_eq!(candidate.world.map, "maze");
        let mut session = crate::LocalSession::new(candidate.world, 2);
        session.bots.difficulty = candidate.difficulty;
        session.step();
        assert_eq!(session.world.config.mode, Mode::Maze);
        assert_eq!(session.world.map.id, "maze");
        assert_eq!(session.bots.difficulty, Difficulty::Expert);
    }
    #[test]
    fn config_roundtrip_and_limits() {
        let settings = ServerSettings::default();
        assert!(settings.validate().is_ok());
        let text = toml::to_string(&settings).unwrap();
        let mut decoded: ServerSettings = toml::from_str(&text).unwrap();
        assert!(decoded.validate().is_ok());
        decoded.world.width = f32::NAN;
        assert!(decoded.validate().is_err());
    }
    #[test]
    fn shipped_public_and_private_host_policies_validate() {
        for (text, enabled) in [
            (include_str!("../../../configs/server.toml"), false),
            (include_str!("../../../configs/official.toml"), true),
            (include_str!("../../../configs/central.toml"), true),
        ] {
            let settings: ServerSettings = toml::from_str(text).unwrap();
            settings.validate().unwrap();
            assert_eq!(settings.creator_admin_enabled(), enabled);
        }
    }
    #[test]
    fn creator_policy_defaults_to_official_public_arenas_and_respects_revocation() {
        let mut settings = ServerSettings::default();
        assert!(!settings.creator_admin_enabled());
        // Only operator-owned configuration chooses this policy. A client's
        // URL, username, rank, or claimed hostname is never consulted.
        settings.kind = tankyard_protocol::ServerKind::Official;
        assert!(settings.creator_admin_enabled());
        assert!(settings.validate().is_ok());
        assert_eq!(settings.creator_audit_file(), "creator-audit.jsonl");
        settings.password = "private-match".into();
        assert!(!settings.creator_admin_enabled());
        assert!(settings.validate().is_ok());
        settings.creator_admin = Some(true);
        assert!(settings.validate().is_err());
        settings.password.clear();
        settings.creator_admin = Some(false);
        assert!(!settings.creator_admin_enabled());
        settings.kind = tankyard_protocol::ServerKind::Community;
        settings.creator_admin = Some(true);
        assert!(settings.validate().is_err());
        settings.server_id = "consenting-public-host".into();
        settings.audit_file = "creator-audit.jsonl".into();
        assert!(settings.validate().is_ok());
        assert!(settings.creator_admin_enabled());
        settings.password = "private-match".into();
        assert!(settings.validate().is_err());
        settings.creator_admin = Some(false);
        assert!(settings.validate().is_ok());
        let legacy: ServerSettings = toml::from_str("name = 'Existing host'").unwrap();
        assert!(!legacy.creator_admin_enabled());
        let central = ServerSettings::central();
        central.validate().unwrap();
        assert_eq!(central.creator_admin, None);
        assert!(central.creator_admin_enabled());
    }
    #[test]
    fn credentials_are_redacted_and_accounts_need_secure_transport() {
        let mut settings = ServerSettings {
            password: "never-print-this".into(),
            require_accounts: true,
            ..Default::default()
        };
        assert!(settings.validate().is_err());
        settings.netcode_private_key = Some("09".repeat(32));
        assert!(settings.validate().is_ok());
        let debug = format!("{settings:?}");
        assert!(!debug.contains("never-print-this"));
        assert!(!debug.contains(&"09".repeat(32)));
        settings.kind = tankyard_protocol::ServerKind::Official;
        settings.access_file = "community-access.json".into();
        assert!(settings.validate().is_err());
        settings.access_file.clear();
        settings.audit_file = "one.jsonl".into();
        settings.global_audit_file = "one.jsonl".into();
        assert!(settings.validate().is_err());
    }
}
