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
            Self::ReloadMods | Self::ReadAudit | Self::RestartRound => true,
        }
    }
    pub fn permission(&self) -> ServerPermission {
        match self {
            Self::Warn { .. } => ServerPermission::Warn,
            Self::Announce { .. } => ServerPermission::Announce,
            Self::Kick { .. } => ServerPermission::Kick,
            Self::Ban { .. } => ServerPermission::Ban,
            Self::Mute { .. } => ServerPermission::Mute,
            Self::Nominate { .. } => ServerPermission::ManageRoles,
            Self::CreateRoom { .. } | Self::DeleteRoom { .. } => ServerPermission::ManageRooms,
            Self::SetBots { .. }
            | Self::ConfigureMatch { .. }
            | Self::ConfigureParty { .. }
            | Self::RestartRound => ServerPermission::Configure,
            Self::ReloadMods => ServerPermission::ReloadMods,
            Self::ReadAudit => ServerPermission::ViewAudit,
        }
    }
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
