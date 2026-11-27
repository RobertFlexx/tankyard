use serde::{Deserialize, Serialize};
use tankyard_core::Mode;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSettings {
    pub mode: Mode,
    pub map: String,
    pub difficulty: String,
    pub bots: usize,
    pub party: Option<PartyMatchOptions>,
}
/// Only the bounded settings a party leader may change between rounds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyMatchOptions {
    pub width: u16,
    pub height: u16,
    pub dynamic_arena: bool,
    pub starting_level: u8,
}
impl PartyMatchOptions {
    pub fn valid(&self) -> bool {
        (900..=16000).contains(&self.width)
            && (700..=16000).contains(&self.height)
            && [1, 30, 45].contains(&self.starting_level)
    }
}
impl MatchSettings {
    pub fn valid(&self) -> bool {
        tankyard_mods::identifier(&self.map)
            && ["beginner", "normal", "advanced", "expert"]
                .iter()
                .any(|name| self.difficulty.eq_ignore_ascii_case(name))
            && self.bots <= 128
            && self
                .party
                .as_ref()
                .is_none_or(|party| party.valid() && (2..=8).contains(&self.bots))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerKind {
    #[default]
    Community,
    Official,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ServerRole {
    #[default]
    Player,
    Moderator,
    Admin,
    Owner,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ServerPermission {
    Warn,
    Announce,
    Kick,
    Ban,
    Mute,
    ManageRooms,
    ManageRoles,
    Configure,
    ReloadMods,
    ViewAudit,
}
impl ServerPermission {
    pub const ALL: [Self; 10] = [
        Self::Warn,
        Self::Announce,
        Self::Kick,
        Self::Ban,
        Self::Mute,
        Self::ManageRooms,
        Self::ManageRoles,
        Self::Configure,
        Self::ReloadMods,
        Self::ViewAudit,
    ];
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: u64,
    pub room: String,
    pub player: u64,
    pub name: String,
    #[serde(default)]
    pub game_creator: bool,
    #[serde(default)]
    pub server_badge: tankyard_core::ServerBadge,
    pub text: String,
    pub time: u64,
}
/// The central owner's own hull and gun settings for this round.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OwnerTank {
    pub class: String,
    pub weapons: tankyard_core::administration::WeaponTuning,
    #[serde(default)]
    pub invulnerable: bool,
}
impl OwnerTank {
    pub fn valid(&self) -> bool {
        tankyard_mods::identifier(&self.class) && self.weapons.valid()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerCommand {
    Warn {
        player: u64,
        reason: String,
    },
    Announce {
        text: String,
    },
    Kick {
        player: u64,
        reason: String,
    },
    Ban {
        player: u64,
        minutes: u32,
        reason: String,
    },
    Mute {
        player: u64,
        minutes: u32,
    },
    Nominate {
        player: u64,
        role: ServerRole,
        permissions: Vec<ServerPermission>,
    },
    CreateRoom {
        id: String,
        name: String,
        minimum_role: ServerRole,
    },
    DeleteRoom {
        id: String,
    },
    SetBots {
        count: usize,
    },
    ReloadMods,
    ReadAudit,
    ConfigureMatch {
        mode: Mode,
        map: String,
        difficulty: String,
    },
    RestartRound,
    ConfigureParty {
        settings: MatchSettings,
    },
    ConfigureRules {
        rules: tankyard_mods::GameRules,
    },
    /// List the bans in force. Newer commands stay at the end, so older
    /// servers keep reading every command they know.
    ReadBans,
    LiftBan {
        ban: u64,
    },
    ReadHealth,
    SummonBoss {
        boss: String,
    },
    DismissBosses,
    ReadOwnTank,
    SetOwnTank {
        tank: OwnerTank,
    },
}
impl ServerCommand {
    pub fn valid(&self) -> bool {
        let reason = |s: &str| {
            s.len() <= 640 && s.chars().count() <= 160 && !s.chars().any(char::is_control)
        };
        match self {
            Self::Warn { player, reason: r } => *player > 0 && reason(r) && !r.trim().is_empty(),
            Self::Announce { text } => {
                !text.trim().is_empty()
                    && text.len() <= 640
                    && text.chars().count() <= 160
                    && !text.chars().any(char::is_control)
            }
            Self::Kick { player, reason: r } => *player > 0 && reason(r),
            Self::Ban {
                player,
                minutes,
                reason: r,
            } => *player > 0 && (1..=525600).contains(minutes) && reason(r),
            Self::Mute { player, minutes } => *player > 0 && *minutes <= 1440,
            Self::Nominate {
                player,
                role,
                permissions,
            } => {
                *player > 0
                    && *role != ServerRole::Owner
                    && permissions.len() <= ServerPermission::ALL.len()
            }
            Self::CreateRoom { id, name, .. } => {
                tankyard_mods::identifier(id)
                    && !name.trim().is_empty()
                    && name.len() <= 48
                    && reason(name)
            }
            Self::DeleteRoom { id } => tankyard_mods::identifier(id),
            Self::SetBots { count } => *count <= 128,
            Self::ConfigureMatch {
                mode,
                map,
                difficulty,
            } => MatchSettings {
                mode: *mode,
                map: map.clone(),
                difficulty: difficulty.clone(),
                bots: 0,
                party: None,
            }
            .valid(),
            Self::ConfigureParty { settings } => settings.party.is_some() && settings.valid(),
            Self::ConfigureRules { rules } => rules.validate().is_ok(),
            Self::SummonBoss { boss } => tankyard_core::BossKind::from_id(boss).is_some(),
            Self::SetOwnTank { tank } => tank.valid(),
            Self::ReadOwnTank => true,
            Self::ReloadMods
            | Self::ReadAudit
            | Self::RestartRound
            | Self::ReadBans
            | Self::LiftBan { .. }
            | Self::ReadHealth
            | Self::DismissBosses => true,
        }
    }
    /// What this command does, as a line in the audit log. `player` names a
    /// player by entity and `ban` names a ban by its id; both fall back to the
    /// number when nothing better is known.
    pub fn describe(&self, player: impl Fn(u64) -> String, ban: impl Fn(u64) -> String) -> String {
        let because = |reason: &str| match reason.trim() {
            "" => String::new(),
            reason => format!(" ({reason})"),
        };
        match self {
            Self::Warn {
                player: who,
                reason,
            } => format!("Warned {}{}", player(*who), because(reason)),
            Self::Announce { text } => format!("Announced \"{}\"", text.trim()),
            Self::Kick {
                player: who,
                reason,
            } => format!("Kicked {}{}", player(*who), because(reason)),
            Self::Ban {
                player: who,
                minutes,
                reason,
            } => format!(
                "Banned {} for {}{}",
                player(*who),
                span(u64::from(*minutes)),
                because(reason)
            ),
            Self::Mute {
                player: who,
                minutes: 0,
            } => format!("Unmuted {}", player(*who)),
            Self::Mute {
                player: who,
                minutes,
            } => format!("Muted {} for {}", player(*who), span(u64::from(*minutes))),
            Self::Nominate {
                player: who,
                role,
                permissions,
            } => {
                let allowed: Vec<_> = permissions
                    .iter()
                    .map(|permission| words(&format!("{permission:?}")).to_lowercase())
                    .collect();
                let allowed = if allowed.is_empty() {
                    "nothing".to_owned()
                } else {
                    allowed.join(", ")
                };
                format!("Made {} {role:?} ({allowed})", player(*who))
            }
            Self::CreateRoom {
                id,
                name,
                minimum_role,
            } => format!("Opened room {name} ({id}) for {minimum_role:?} and up"),
            Self::DeleteRoom { id } => format!("Closed room {id}"),
            Self::SetBots { count } => format!("Set the bots to {count}"),
            Self::ReloadMods => "Reloaded mods".to_owned(),
            Self::ReadAudit => "Read the audit log".to_owned(),
            Self::ConfigureMatch {
                mode,
                map,
                difficulty,
            } => format!(
                "Set the match to {} on {map}, {difficulty} bots",
                mode.name()
            ),
            Self::RestartRound => "Restarted the round".to_owned(),
            Self::ConfigureParty { settings } => format!(
                "Set the party round to {} on {}",
                settings.mode.name(),
                settings.map
            ),
            Self::ConfigureRules { rules } => format!("Changed the game rules: {rules:?}"),
            Self::ReadBans => "Read the ban list".to_owned(),
            Self::LiftBan { ban: id } => format!("Lifted the ban on {}", ban(*id)),
            Self::ReadHealth => "Read server health".to_owned(),
            Self::SummonBoss { boss } => format!(
                "Summoned {}",
                tankyard_core::BossKind::from_id(boss).map_or(boss.as_str(), |kind| kind.name())
            ),
            Self::DismissBosses => "Dismissed every boss".to_owned(),
            Self::ReadOwnTank => "Read own tank settings".into(),
            Self::SetOwnTank { tank } => format!(
                "Set own tank to {} with {:.2}× damage, {:.2}× shot speed and invulnerability {}",
                tank.class,
                tank.weapons.damage,
                tank.weapons.shot_speed,
                if tank.invulnerable { "on" } else { "off" }
            ),
        }
    }
    pub fn permission(&self) -> ServerPermission {
        match self {
            Self::Warn { .. } => ServerPermission::Warn,
            Self::Announce { .. } => ServerPermission::Announce,
            Self::Kick { .. } => ServerPermission::Kick,
            Self::Ban { .. } | Self::ReadBans | Self::LiftBan { .. } => ServerPermission::Ban,
            Self::Mute { .. } => ServerPermission::Mute,
            Self::Nominate { .. } => ServerPermission::ManageRoles,
            Self::CreateRoom { .. } | Self::DeleteRoom { .. } => ServerPermission::ManageRooms,
            Self::SetBots { .. }
            | Self::ConfigureMatch { .. }
            | Self::ConfigureParty { .. }
            | Self::ConfigureRules { .. }
            | Self::RestartRound
            | Self::SummonBoss { .. }
            | Self::ReadOwnTank
            | Self::SetOwnTank { .. }
            | Self::DismissBosses => ServerPermission::Configure,
            Self::ReloadMods => ServerPermission::ReloadMods,
            Self::ReadAudit | Self::ReadHealth => ServerPermission::ViewAudit,
        }
    }
}

/// A length of time in minutes, the way staff read it: `45m`, `2h 5m`, `7d 3h`.
pub fn span(minutes: u64) -> String {
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}

/// `ManageRoles` as `Manage roles`.
fn words(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The most ban rows one answer carries.
pub const MOST_BAN_ROWS: usize = 64;

/// One ban in force, as staff see it. The address is masked: a ban list
/// tends to end up on a shared screen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BanRow {
    pub id: u64,
    pub who: String,
    pub reason: String,
    pub minutes_left: u32,
}
impl BanRow {
    pub fn valid(&self) -> bool {
        let line = |text: &str, chars: usize| {
            text.len() <= chars * 4
                && text.chars().count() <= chars
                && !text.chars().any(char::is_control)
        };
        line(&self.who, 96) && line(&self.reason, 160)
    }
}

/// How a server is running, for the staff who look after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerHealth {
    pub uptime_seconds: u64,
    pub humans: u16,
    pub bots: u16,
    pub arenas: u16,
    pub bosses: u16,
    pub tick_average_micros: u32,
    pub tick_p95_micros: u32,
    pub tick_budget_micros: u32,
    pub snapshot_kib: u64,
    pub rejected_packets: u64,
    pub bans: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeKind {
    Warning,
    Announcement,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerNotice {
    pub kind: NoticeKind,
    pub text: String,
}
impl ServerNotice {
    pub fn valid(&self) -> bool {
        !self.text.trim().is_empty()
            && self.text.len() <= 640
            && self.text.chars().count() <= 160
            && !self.text.chars().any(char::is_control)
    }
    pub fn seconds(&self) -> u64 {
        if self.kind == NoticeKind::Warning {
            12
        } else {
            5
        }
    }
}
