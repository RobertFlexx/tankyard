use serde::{Deserialize, Serialize};
use tankyard_core::Mode;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSettings {
    pub mode: Mode,
    pub map: String,
    pub difficulty: String,
    pub bots: usize,
}
impl MatchSettings {
    pub fn valid(&self) -> bool {
        tankyard_mods::identifier(&self.map)
            && ["beginner", "normal", "advanced", "expert"]
                .iter()
                .any(|name| self.difficulty.eq_ignore_ascii_case(name))
            && self.bots <= 128
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
    pub const ALL: [Self; 8] = [
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
    pub text: String,
    pub time: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerCommand {
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
}
impl ServerCommand {
    pub fn valid(&self) -> bool {
        let reason = |s: &str| s.len() <= 160 && !s.chars().any(char::is_control);
        match self {
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
            }
            .valid(),
            Self::ReloadMods | Self::ReadAudit | Self::RestartRound => true,
        }
    }
    pub fn permission(&self) -> ServerPermission {
        match self {
            Self::Kick { .. } => ServerPermission::Kick,
            Self::Ban { .. } => ServerPermission::Ban,
            Self::Mute { .. } => ServerPermission::Mute,
            Self::Nominate { .. } => ServerPermission::ManageRoles,
            Self::CreateRoom { .. } | Self::DeleteRoom { .. } => ServerPermission::ManageRooms,
            Self::SetBots { .. } | Self::ConfigureMatch { .. } | Self::RestartRound => {
                ServerPermission::Configure
            }
            Self::ReloadMods => ServerPermission::ReloadMods,
            Self::ReadAudit => ServerPermission::ViewAudit,
        }
    }
}
