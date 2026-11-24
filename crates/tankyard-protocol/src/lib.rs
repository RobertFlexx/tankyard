//! Versioned, bounded application messages. Transport encryption is provided by
//! renet_netcode only when secure authentication is configured;
//! simulation authority always remains with the server.
use renet::{ChannelConfig, ConnectionConfig, SendType};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{borrow::Cow, fmt, time::Duration};
use tankyard_content::Content;
use tankyard_core::{BossKind, EntityId, GameEvent, PlayerInput, Snapshot};
mod creator;
pub use creator::*;
mod administration;
pub use administration::*;
mod community;
pub use community::*;
mod voice;
pub use voice::*;
mod party;
pub use party::*;
mod status;
pub use status::*;

/// Version 34 carries the Even, Odd and Most gates, eight conditions a
/// trigger, On input change, Gate to counter and rift events that name their
/// owner, which version 33 can't hold, so clients and servers update
/// together. Every Tank Yard 2.0 release uses it; 1.7.0 used version 33, 1.6.0
/// version 32, 1.5.0 to 1.5.2 version 31, 1.4.0 version 30, 1.3.0 version 29,
/// and 1.1.0 and 1.2.0 version 28.
pub const PROTOCOL_VERSION: u32 = 34;
/// The oldest client version servers accept.
pub const MIN_PROTOCOL_VERSION: u32 = 34;
pub const CLIENT_OUTDATED: &str = "This server runs a newer Tank Yard. Update the game to join it.";
pub const SERVER_OUTDATED: &str =
    "This server runs an older Tank Yard. Its host needs to update it.";
/// Why a server turned a client of `protocol` away, in words a player can
/// act on.
pub fn protocol_mismatch(protocol: u32) -> &'static str {
    if protocol < MIN_PROTOCOL_VERSION {
        CLIENT_OUTDATED
    } else {
        SERVER_OUTDATED
    }
}
/// The first version that reads `ServerControl::ModHud`.
pub const MOD_HUD_PROTOCOL: u32 = 23;
pub const PROTOCOL_ID: u64 = 0x5441_4e4b_5941_0016;
pub const CONTROL_CHANNEL: u8 = 0;
pub const REALTIME_CHANNEL: u8 = 1;
pub const EVENT_CHANNEL: u8 = 2;
pub const VOICE_CHANNEL: u8 = 3;
pub const MAX_CONTROL_BYTES: usize = 2048;
pub const MAX_SERVER_CONTROL_BYTES: usize = tankyard_mods::MAX_PACK_BYTES + 8192;
pub const MAX_INPUT_BYTES: usize = 128;
pub const MAX_STATE_BYTES: usize = 512 * 1024;
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
pub const MAX_NAME_CHARS: usize = 24;

pub fn connection_config() -> ConnectionConfig {
    let reliable = |id, memory| ChannelConfig {
        channel_id: id,
        max_memory_usage_bytes: memory,
        send_type: SendType::ReliableOrdered {
            resend_time: Duration::from_millis(150),
        },
    };
    let realtime = || ChannelConfig {
        channel_id: REALTIME_CHANNEL,
        max_memory_usage_bytes: MAX_STATE_BYTES * 2,
        send_type: SendType::Unreliable,
    };
    ConnectionConfig {
        available_bytes_per_tick: 32 * 1024,
        server_channels_config: vec![
            reliable(CONTROL_CHANNEL, MAX_SERVER_CONTROL_BYTES * 2),
            reliable(EVENT_CHANNEL, 256 * 1024),
            realtime(),
            ChannelConfig {
                channel_id: VOICE_CHANNEL,
                max_memory_usage_bytes: 64 * 1024,
                send_type: SendType::Unreliable,
            },
        ],
        client_channels_config: vec![
            reliable(CONTROL_CHANNEL, MAX_CONTROL_BYTES * 64),
            ChannelConfig {
                channel_id: REALTIME_CHANNEL,
                max_memory_usage_bytes: 16 * 1024,
                send_type: SendType::Unreliable,
            },
            ChannelConfig {
                channel_id: VOICE_CHANNEL,
                max_memory_usage_bytes: 8 * 1024,
                send_type: SendType::Unreliable,
            },
        ],
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientControl {
    PartyPresence {
        in_menu: bool,
    },
    CreatorProof {
        signature: Vec<u8>,
    },
    /// Optional host extension. Only a cryptographically verified creator may
    /// invoke it; standard servers install no handler.
    OwnerExtension {
        round: u32,
        sequence: u64,
        payload: Vec<u8>,
        signature: Vec<u8>,
    },
    Hello {
        protocol: u32,
        content: u64,
        name: String,
        password: String,
        account_token: Option<String>,
        admin_key: [u8; 32],
        party_proof: Option<Vec<u8>>,
        /// The lobby the player picked from the server's status, if any.
        /// The server honors it while that lobby has room.
        arena: Option<u8>,
    },
    UpgradeStat {
        stat: u8,
        count: u8,
    },
    Evolve {
        class: String,
    },
    SandboxLevel {
        level: u8,
    },
    SandboxClass {
        class: String,
    },
    SandboxInvulnerability {
        enabled: bool,
    },
    SandboxBoss {
        kind: BossKind,
    },
    Leave,
    Chat {
        room: String,
        text: String,
    },
    JoinRoom {
        room: String,
    },
    Admin {
        round: u32,
        sequence: u64,
        command: ServerCommand,
        signature: Vec<u8>,
    },
    CentralOwnerProof {
        signature: Vec<u8>,
    },
    Spectate {
        round: u32,
        target: Option<EntityId>,
    },
    PartyReady {
        ready: bool,
    },
    /// Pick choice `option` of the HUD widget numbered `menu`.
    Choose {
        menu: u32,
        option: u8,
    },
}

impl ClientControl {
    pub fn valid(&self) -> bool {
        match self {
            Self::PartyPresence { .. } | Self::PartyReady { .. } => true,
            Self::CreatorProof { signature } | Self::CentralOwnerProof { signature } => {
                signature.len() == 64
            }
            Self::OwnerExtension {
                round,
                payload,
                signature,
                ..
            } => {
                *round > 0 && !payload.is_empty() && payload.len() <= 1024 && signature.len() == 64
            }
            Self::Hello {
                name,
                password,
                account_token,
                party_proof,
                ..
            } => {
                name.len() <= 96
                    && password.len() <= 128
                    && account_token.as_ref().is_none_or(|s| s.len() <= 1024)
                    && party_proof.as_ref().is_none_or(|s| s.len() == 64)
            }
            Self::UpgradeStat { stat, count } => *stat < 8 && (1..=7).contains(count),
            Self::Choose { menu, option } => {
                *menu != 0 && usize::from(*option) < tankyard_mods::HUD_CHOICES
            }
            Self::Evolve { class } | Self::SandboxClass { class } => valid_class_id(class),
            Self::SandboxLevel { level } => (1..=tankyard_core::MAX_LEVEL).contains(level),
            Self::SandboxInvulnerability { .. } | Self::SandboxBoss { .. } | Self::Leave => true,
            Self::Chat { room, text } => {
                tankyard_mods::identifier(room)
                    && text.len() <= 640
                    && text.chars().count() <= 160
                    && !text.chars().any(char::is_control)
            }
            Self::JoinRoom { room } => tankyard_mods::identifier(room),
            Self::Admin {
                round,
                command,
                signature,
                ..
            } => *round > 0 && command.valid() && signature.len() == 64,
            Self::Spectate { round, target } => *round > 0 && target.is_none_or(|id| id > 0),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerControl {
    PartyRoster {
        members: Vec<PartyMember>,
    },
    PartyChallenge {
        nonce: [u8; 32],
        server_id: String,
    },
    MapRotation {
        remaining: u32,
    },
    AdminChallenge {
        nonce: [u8; 32],
        server_id: String,
        central_owner: bool,
    },
    Notice {
        notice: ServerNotice,
    },
    CreatorChallenge {
        nonce: [u8; 32],
        owner_extension: bool,
        server_id: String,
    },
    Access {
        kind: ServerKind,
        chat_enabled: bool,
        role: ServerRole,
        permissions: Vec<ServerPermission>,
    },
    Rooms {
        rooms: Vec<RoomInfo>,
    },
    Chat {
        message: ChatMessage,
    },
    Audit {
        lines: Vec<String>,
    },
    GameData {
        compressed: Vec<u8>,
    },
    Welcome {
        protocol: u32,
        content: u64,
        player: EntityId,
        round: u32,
        server_name: String,
        sandbox_allowed: bool,
        tick_rate: u32,
    },
    Rejected {
        reason: String,
    },
    Action {
        accepted: bool,
        reason: String,
    },
    PlayerJoined {
        player: EntityId,
        name: String,
    },
    PlayerLeft {
        player: EntityId,
    },
    Shutdown {
        reason: String,
    },
    MatchSettings {
        settings: MatchSettings,
    },
    VoiceStatus {
        enabled: bool,
        muted: bool,
    },
    /// Every HUD widget behavior mods show this player, replacing the last
    /// list. Sent only to clients of `MOD_HUD_PROTOCOL` or later.
    ModHud {
        widgets: Vec<tankyard_mods::HudWidget>,
    },
    /// The answer to [`ServerCommand::ReadBans`]. Only clients that asked get
    /// one, so older clients never see this message.
    Bans {
        bans: Vec<BanRow>,
    },
    /// The answer to [`ServerCommand::ReadHealth`].
    Health {
        health: ServerHealth,
    },
}
/// Whether a HUD list from a server is one this client should draw.
pub fn valid_mod_hud(widgets: &[tankyard_mods::HudWidget]) -> bool {
    widgets.len() <= tankyard_mods::MAX_HUD_SENT && widgets.iter().all(|w| w.valid())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputMessage {
    pub input: PlayerInput,
}

/// At most three chronological input frames. Repeating recent frames recovers
/// short taps after UDP loss without resending old movement as the current one.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct InputBundle {
    pub inputs: [PlayerInput; 3],
    pub count: u8,
}
impl InputBundle {
    pub fn push(&mut self, input: PlayerInput) {
        if self.count < 3 {
            self.inputs[usize::from(self.count)] = input;
            self.count += 1;
        } else {
            self.inputs.rotate_left(1);
            self.inputs[2] = input;
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = PlayerInput> + '_ {
        self.inputs
            .iter()
            .copied()
            .take(usize::from(self.count.min(3)))
    }
    fn valid(&self) -> bool {
        (1..=3).contains(&self.count) && self.iter().all(PlayerInput::valid)
    }
}
/// What players receive of `bundle`. Map triggers and the paths they play
/// run only on the server, so they stay behind: players see their effects
/// in snapshots, newer trigger kinds never reach clients that predate them,
/// and the fields triggers leave out of their files when unused can't
/// travel in the binary format at all.
pub fn players_bundle(bundle: &tankyard_mods::GameBundle) -> Cow<'_, tankyard_mods::GameBundle> {
    if bundle
        .maps
        .iter()
        .all(|map| map.triggers.is_empty() && map.paths.is_empty())
    {
        return Cow::Borrowed(bundle);
    }
    let mut stripped = bundle.clone();
    for map in &mut stripped.maps {
        map.triggers.clear();
        map.paths.clear();
    }
    Cow::Owned(stripped)
}
/// The game data message for `bundle`, as players receive it.
pub fn game_data(bundle: &tankyard_mods::GameBundle) -> Result<ServerControl, DecodeError> {
    // Validate gameplay on the authored catalog, where moving doors still
    // have their triggers. The client validates the stripped presentation.
    bundle
        .validate()
        .map_err(|_| DecodeError("invalid game data"))?;
    let bundle = players_bundle(bundle);
    let bytes = encode(&*bundle, tankyard_mods::MAX_PACK_BYTES)?;
    Ok(ServerControl::GameData {
        compressed: lz4_flex::compress_prepend_size(&bytes),
    })
}
pub fn decode_game_data(bytes: &[u8]) -> Result<tankyard_mods::GameBundle, DecodeError> {
    if bytes.len() < 4 || bytes.len() > MAX_SERVER_CONTROL_BYTES {
        return Err(DecodeError("invalid game data size"));
    }
    let size = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    if size > tankyard_mods::MAX_PACK_BYTES {
        return Err(DecodeError("game data decompression limit exceeded"));
    }
    let bytes = lz4_flex::decompress_size_prepended(bytes)
        .map_err(|_| DecodeError("invalid compressed game data"))?;
    let bundle: tankyard_mods::GameBundle = decode(&bytes, tankyard_mods::MAX_PACK_BYTES)?;
    bundle
        .validate_for_players()
        .map_err(|_| DecodeError("invalid server game data"))?;
    Ok(bundle)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventMessage {
    pub round: u32,
    pub events: Vec<GameEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub &'static str);
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for DecodeError {}

pub fn encode<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, DecodeError> {
    let bytes =
        postcard::to_stdvec(value).map_err(|_| DecodeError("message serialization failed"))?;
    if bytes.len() > limit {
        return Err(DecodeError("message exceeds size limit"));
    }
    Ok(bytes)
}
pub fn decode<T: DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T, DecodeError> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err(DecodeError("invalid message size"));
    }
    let (value, remaining) =
        postcard::take_from_bytes(bytes).map_err(|_| DecodeError("malformed message"))?;
    if !remaining.is_empty() {
        return Err(DecodeError("trailing data"));
    }
    Ok(value)
}
pub fn decode_control(bytes: &[u8]) -> Result<ClientControl, DecodeError> {
    let message: ClientControl = decode(bytes, MAX_CONTROL_BYTES)?;
    if !message.valid() {
        return Err(DecodeError("invalid control request"));
    }
    Ok(message)
}
pub fn decode_input(bytes: &[u8]) -> Result<PlayerInput, DecodeError> {
    let message: InputMessage = decode(bytes, MAX_INPUT_BYTES)?;
    if !message.input.valid() {
        return Err(DecodeError("invalid input values"));
    }
    Ok(message.input)
}

pub fn decode_inputs(bytes: &[u8]) -> Result<InputBundle, DecodeError> {
    // A single frame occupies at most 25 bytes (five-byte sequence, four
    // four-byte floats, four booleans). A fixed three-frame bundle is >=64.
    if bytes.len() <= 25 {
        let mut bundle = InputBundle::default();
        bundle.push(decode_input(bytes)?);
        return Ok(bundle);
    }
    let bundle: InputBundle = decode(bytes, MAX_INPUT_BYTES)?;
    if !bundle.valid() {
        return Err(DecodeError("invalid input history"));
    }
    Ok(bundle)
}

/// Independent LZ4 blocks make dropped or reordered snapshots harmless. No
/// unbounded decompression is performed: the original size is checked first.
pub fn encode_snapshot(snapshot: &Snapshot) -> Result<Vec<u8>, DecodeError> {
    let raw = encode(snapshot, MAX_STATE_BYTES)?;
    compress_snapshot(&raw)
}

/// Reuse a bounded serialization buffer across a server's viewers and ticks.
/// Compressed messages remain independent blocks with the same wire format.
pub struct SnapshotEncoder {
    raw: Box<[u8]>,
}
impl Default for SnapshotEncoder {
    fn default() -> Self {
        Self {
            raw: vec![0; MAX_STATE_BYTES].into_boxed_slice(),
        }
    }
}
impl SnapshotEncoder {
    pub fn encode(&mut self, snapshot: &Snapshot) -> Result<Vec<u8>, DecodeError> {
        let raw = postcard::to_slice(snapshot, &mut self.raw)
            .map_err(|_| DecodeError("state serialization exceeds buffer or failed"))?;
        compress_snapshot(raw)
    }
}

fn compress_snapshot(raw: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let compressed = lz4_flex::compress_prepend_size(raw);
    if compressed.len() > MAX_STATE_BYTES {
        return Err(DecodeError("compressed state exceeds limit"));
    }
    Ok(compressed)
}
pub fn decode_snapshot(bytes: &[u8]) -> Result<Snapshot, DecodeError> {
    if bytes.len() < 4 || bytes.len() > MAX_STATE_BYTES {
        return Err(DecodeError("invalid snapshot size"));
    }
    let raw_size = u32::from_le_bytes(
        bytes[..4]
            .try_into()
            .map_err(|_| DecodeError("missing state size"))?,
    ) as usize;
    if raw_size > MAX_STATE_BYTES || raw_size == 0 {
        return Err(DecodeError("invalid decompressed size"));
    }
    let raw = lz4_flex::decompress_size_prepended(bytes)
        .map_err(|_| DecodeError("invalid compressed state"))?;
    let mut state: Snapshot = decode(&raw, MAX_STATE_BYTES)?;
    // The Railgunner travels as a Siege Engine of a size no other boss has.
    for boss in &mut state.bosses {
        boss.kind = boss.kind.from_wire(boss.radius);
    }
    if !snapshot_valid(&state) {
        return Err(DecodeError("invalid snapshot values"));
    }
    Ok(state)
}

pub fn sanitize_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|&c| !tankyard_core::invisible_char(c) && c != '<' && c != '>')
        .take(MAX_NAME_CHARS)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "Pilot".into()
    } else {
        clean.into()
    }
}
pub fn valid_class_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
pub fn sequence_newer(a: u32, b: u32) -> bool {
    a != b && a.wrapping_sub(b) < (1 << 31)
}

/// Compatibility checksum, not a security primitive. Covers actual content
/// rather than assuming that identical executable versions have identical data.
pub fn content_fingerprint(content: &Content) -> u64 {
    let Ok(bytes) = postcard::to_stdvec(content) else {
        return 0;
    };
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3)
    })
}

fn snapshot_valid(s: &Snapshot) -> bool {
    let scalar = |v: f32| v.is_finite() && v.abs() <= 1_000_000_000_000.;
    let vector =
        |v: tankyard_core::V2| v.finite() && v.x.abs() <= 1_000_000. && v.y.abs() <= 1_000_000.;
    let radius = |v: f32| scalar(v) && (0.0..=2000.0).contains(&v);
    scalar(s.config.width)
        && scalar(s.config.height)
        && (100.0..=20000.0).contains(&s.config.width)
        && (100.0..=20000.0).contains(&s.config.height)
        && (0.5..=1.0).contains(&s.config.arena_scale)
        && scalar(s.config.spawn_protection)
        && scalar(s.config.time_limit)
        && s.config.map.len() <= 48
        && s.config
            .mode_name
            .as_ref()
            .is_none_or(|n| n.len() <= 48 && !n.chars().any(char::is_control))
        && s.tanks.len() <= 512
        && s.projectiles.len() <= 20000
        && s.targets.len() <= 5000
        && s.bosses.len() <= 64
        && s.walls.len() <= 4096
        && s.objectives.len() <= 64
        && s.events.len() <= 8192
        && s.tanks.iter().all(|t| {
            vector(t.pos)
                && vector(t.vel)
                && scalar(t.aim)
                && scalar(t.health)
                && scalar(t.max_health)
                && t.max_health > 0.0
                && radius(t.radius)
                && scalar(t.respawn_timer)
                && scalar(t.protection)
                && scalar(t.shield)
                && scalar(t.ability_cooldown)
                && scalar(t.charge)
                && scalar(t.boost)
                && (0.25..=6.).contains(&t.speed_scale)
                && (0.3..=2.5).contains(&t.terrain)
                && (0.25..=6.).contains(&t.reload_scale)
                && (0.25..=8.).contains(&t.damage_scale)
                && (0.0..=1.).contains(&t.knockback)
                && (0.0..=tankyard_core::STASIS_SECONDS).contains(&t.stasis)
                && (0.0..=tankyard_core::POCKET_SECONDS).contains(&t.pocket)
                && t.rift_shots <= 2
                && (0.0..=tankyard_core::RIFT_ARMED).contains(&t.rift_armed)
                && t.pocket_echoes.len() <= tankyard_core::POCKET_ECHO_LIMIT
                && (t.pocket > 0. || t.pocket_echoes.is_empty())
                && t.pocket_echoes.iter().all(|echo| {
                    vector(echo.pos)
                        && vector(echo.previous)
                        && vector(echo.vel)
                        && scalar(echo.aim)
                        && (4.0..=24.0).contains(&echo.radius)
                        && (0.0..=tankyard_core::POCKET_ECHO_WAKE).contains(&echo.wake)
                        && valid_class_id(&echo.class)
                })
                && (!t.creator_boss || t.game_creator)
                && t.creator_attacks.iter().all(|v| {
                    v.is_finite() && (0.0..=tankyard_core::creator_boss::MAX_COOLDOWN).contains(v)
                })
                && t.stats.iter().all(|stat| *stat <= 7)
                && t.points <= 38
                && t.team <= 4
                && t.input.valid()
                && t.name.len() <= 96
                && valid_class_id(&t.class)
                && t.cooldowns.len() <= 128
                && t.cooldowns.iter().all(|v| scalar(*v))
                && t.barrel_shots.len() <= 128
                && t.barrel_aims.len() <= 128
                && t.barrel_aims.iter().all(|v| scalar(*v))
                && t.cooldowns.len() == t.barrel_shots.len()
                && t.cooldowns.len() == t.barrel_aims.len()
                && (1..=tankyard_core::MAX_LEVEL).contains(&t.level)
        })
        && s.projectiles.iter().all(|p| {
            vector(p.pos)
                && vector(p.previous)
                && vector(p.vel)
                && scalar(p.cruise_speed)
                && p.cruise_speed >= 0.0
                && radius(p.radius)
                && scalar(p.damage)
                && scalar(p.penetration)
                && scalar(p.lifetime)
                && scalar(p.age)
                && p.frozen.is_none_or(|f| {
                    f.anchor > 0 && p.stasis_used && vector(f.offset) && vector(f.velocity)
                })
                && p.hit.len() <= 24
        })
        && s.targets.iter().all(|t| {
            vector(t.pos)
                && vector(t.vel)
                && radius(t.radius)
                && scalar(t.health)
                && scalar(t.max_health)
                && t.max_health > 0.0
                && scalar(t.angle)
                && match t.kind {
                    tankyard_core::TargetKind::Custom(sides) => (3..=12).contains(&sides),
                    _ => true,
                }
        })
        && s.bosses.iter().all(|b| {
            vector(b.pos)
                && vector(b.vel)
                && radius(b.radius)
                && scalar(b.health)
                && scalar(b.max_health)
                && b.max_health > 0.0
                && scalar(b.timer)
                && scalar(b.aim)
                && vector(b.goal)
                && scalar(b.tactic_time)
                && scalar(b.arrival)
                && (-tankyard_core::BOSS_AIM_HOLD..=30.).contains(&b.attack_timer)
                && (0.0..=b.kind.windup()).contains(&b.attack_windup)
                && b.attack <= 2
                && (1..=3).contains(&b.phase)
                && b.team <= 4
        })
        && s.walls.iter().all(|w| {
            scalar(w.x) && scalar(w.y) && scalar(w.w) && scalar(w.h) && w.w >= 0. && w.h >= 0.
        })
        && (s.wall_groups.is_empty() || s.wall_groups.len() == s.walls.len())
        && s.wall_groups
            .iter()
            .all(|&group| group <= tankyard_content::MAP_GROUPS)
        && s.match_state.tweens.len() <= tankyard_content::TWEEN_SLOTS
        && s.match_state.tweens.iter().all(|t| t.valid())
        && s.match_state.bodies.len() <= usize::from(tankyard_content::MAP_GROUPS)
        && s.match_state.bodies.iter().all(|b| b.valid())
        && s.match_state.shown_counters.len() <= usize::from(tankyard_content::MAP_COUNTERS)
        && s.match_state
            .shown_counters
            .iter()
            .all(|&(counter, value)| {
                counter >= 1 && value.unsigned_abs() <= tankyard_content::COUNTER_LIMIT as u32
            })
        && s.rifts.len() <= tankyard_core::MAX_RIFTS
        && s.rifts.iter().all(|r| {
            vector(r.pos)
                && scalar(r.angle)
                && (0.0..=1_000_000.).contains(&r.age)
                && (-tankyard_core::RIFT_CLOSING..=tankyard_core::RIFT_SECONDS).contains(&r.life)
        })
        && s.rift_shots.len() <= 512
        && s.rift_shots.iter().all(|shot| {
            vector(shot.pos)
                && vector(shot.vel)
                && (0.0..=tankyard_core::RIFT_RANGE).contains(&shot.range)
        })
        && s.objectives.iter().all(|o| {
            vector(o.pos)
                && vector(o.home)
                && radius(o.radius)
                && scalar(o.progress)
                && scalar(o.return_timer)
                && o.team <= 4
        })
        && scalar(s.match_state.elapsed)
        && s.config.rules.validate().is_ok()
        && scalar(s.match_state.safe_radius)
        && s.match_state.winner.is_none_or(|team| team <= 4)
        && s.match_state.boss_wave <= 6
        && s.match_state.announcement.len() <= 256
        && scalar(s.match_state.arena.remaining)
        && (0.0..=if s.config.mode == tankyard_core::Mode::Infection {
            tankyard_core::INFECTION_SECONDS
        } else {
            60.
        })
            .contains(&s.match_state.arena.remaining)
        && s.match_state.arena.wave <= tankyard_core::PVE_WAVES
        && s.match_state.arena.enemies <= 512
        && s.events.iter().all(|e| vector(e.pos) && e.team <= 4)
}

#[cfg(test)]
mod name_hardening_tests {
    use super::*;

    #[test]
    fn name_rejects_bidi_spoofing_without_breaking_utf8() {
        assert_eq!(sanitize_name("abc\u{202e}txt"), "abctxt");
        assert_eq!(sanitize_name("\u{2066}Mira\u{2069}"), "Mira");
        assert_eq!(sanitize_name("世界 Sol"), "世界 Sol");
        assert_eq!(sanitize_name("<>\u{202e}"), "Pilot");
        assert_eq!(sanitize_name("\u{3164}\u{200b}"), "Pilot");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::V2;

    #[test]
    fn fallen_overlords_ready_special_survives_the_wire_while_turning() {
        use tankyard_core::{BossKind, DT, Mode, World, WorldConfig};
        let mut world = World::new(
            WorldConfig {
                mode: Mode::BossRush,
                boss_rush: vec![BossKind::FallenOverlord],
                target_count: 0,
                spawn_protection: 0.,
                dynamic_arena: false,
                ..Default::default()
            },
            Content::builtin(),
        );
        for (name, x) in [("Leader", 1800.), ("Friend", 1850.)] {
            world.add_player(name, false);
            let tank = world.tanks.last_mut().unwrap();
            tank.pos = V2::new(x, 1600.);
            tank.level = 45;
        }
        let boss = &mut world.bosses[0];
        boss.pos = V2::new(1800., 1000.);
        boss.aim = -std::f32::consts::FRAC_PI_2;
        boss.arrival = 0.;
        boss.attack_timer = DT;
        let mut waited_for_aim = false;
        let mut warned = false;
        let mut fired = false;
        for tick in 0..240 {
            for tank in &mut world.tanks {
                tank.health = tank.max_health;
            }
            world.step();
            let snapshot = world.snapshot();
            let boss = &snapshot.bosses[0];
            waited_for_aim |= boss.attack_timer < 0.;
            fired |= warned && boss.attack_windup == 0.;
            warned |= boss.attack_windup > 0.;
            let restored = decode_snapshot(&encode_snapshot(&snapshot).unwrap());
            assert!(
                restored.is_ok(),
                "Fallen Overlord tick {tick}, timer {}, warning {}: {restored:?}",
                boss.attack_timer,
                boss.attack_windup,
            );
        }
        assert!(waited_for_aim && warned && fired);
    }

    #[test]
    fn every_boss_warning_survives_the_wire_and_invalid_timers_do_not() {
        use tankyard_core::{BOSS_AIM_HOLD, BossKind, DT, World, WorldConfig};
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Pilot", false);
        for kind in BossKind::ALL {
            world.bosses.clear();
            assert_ne!(world.spawn_boss(kind), 0);
            let mut snapshot = world.snapshot();
            for phase in 1..=3 {
                snapshot.bosses[0].phase = phase;
                snapshot.bosses[0].attack = phase - 1;
                snapshot.bosses[0].attack_windup = kind.windup();
                for timer in [-BOSS_AIM_HOLD, -DT, 0., 7., 30.] {
                    snapshot.bosses[0].attack_timer = timer;
                    let restored = decode_snapshot(&encode_snapshot(&snapshot).unwrap());
                    assert!(restored.is_ok(), "{kind:?}, phase {phase}: {restored:?}");
                }
            }
            for timer in [
                -BOSS_AIM_HOLD - DT,
                30. + DT,
                f32::NAN,
                f32::NEG_INFINITY,
                f32::INFINITY,
            ] {
                snapshot.bosses[0].attack_timer = timer;
                assert!(
                    decode_snapshot(&encode_snapshot(&snapshot).unwrap()).is_err(),
                    "{kind:?} accepted invalid attack timer {timer}",
                );
            }
            snapshot.bosses[0].attack_timer = 7.;
            for windup in [
                -DT,
                kind.windup() + DT,
                f32::NAN,
                f32::NEG_INFINITY,
                f32::INFINITY,
            ] {
                snapshot.bosses[0].attack_windup = windup;
                assert!(
                    decode_snapshot(&encode_snapshot(&snapshot).unwrap()).is_err(),
                    "{kind:?} accepted invalid warning timer {windup}",
                );
            }
        }
    }

    #[test]
    fn a_railgunner_travels_as_a_siege_engine_older_clients_can_read() {
        use tankyard_core::{BossKind, RAIL_RADIUS, World, WorldConfig};
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Pilot", false);
        assert_ne!(world.spawn_boss(BossKind::Railgunner), 0);
        assert_ne!(world.spawn_boss(BossKind::SiegeEngine), 0);
        assert_eq!(
            BossKind::Railgunner.windup(),
            BossKind::SiegeEngine.windup(),
            "an older client checks the warning timer against the Siege Engine's"
        );

        let bytes = encode_snapshot(&world.snapshot()).unwrap();
        // An older client stops here: no step turns the kind back.
        let raw = lz4_flex::decompress_size_prepended(&bytes).unwrap();
        let older: Snapshot = decode(&raw, MAX_STATE_BYTES).unwrap();
        assert!(older.bosses.iter().all(|b| b.kind == BossKind::SiegeEngine));
        assert!(snapshot_valid(&older));

        let current = decode_snapshot(&bytes).unwrap();
        let kinds: Vec<_> = current.bosses.iter().map(|b| (b.kind, b.radius)).collect();
        assert_eq!(
            kinds,
            [
                (BossKind::Railgunner, RAIL_RADIUS),
                (BossKind::SiegeEngine, BossKind::SiegeEngine.radius()),
            ]
        );
    }

    #[test]
    fn a_clash_stays_an_ordinary_parry_on_the_wire() {
        use tankyard_core::{ClashStyle, EventKind, GameEvent, clash_names, clash_of, clash_value};
        assert_eq!(PROTOCOL_VERSION, 34, "2.0 releases share one protocol");
        for style in ClashStyle::ALL {
            for boss in [41, 41 + 8192, u64::MAX - 1] {
                let event = GameEvent {
                    tick: 45,
                    kind: EventKind::Parry,
                    subject: 7,
                    value: clash_value(style, boss, 76_540.),
                    pos: V2::new(100., 200.),
                    team: 1,
                    elimination: None,
                };
                let bytes = encode(&event, MAX_EVENT_BYTES).unwrap();
                let received: GameEvent = decode(&bytes, MAX_EVENT_BYTES).unwrap();
                assert!(matches!(received.kind, EventKind::Parry));
                assert_eq!(received.subject, event.subject);
                let (read, bits, damage) = clash_of(received.value).unwrap();
                assert_eq!(read, style);
                assert!(clash_names(bits, boss));
                assert_eq!(damage, 76_540);
            }
        }
    }

    #[test]
    fn all_boss_phases_remain_decodable_in_moving_two_player_fights() {
        use tankyard_core::{BossKind, DT, Mode, World, WorldConfig};
        for kind in BossKind::ALL {
            let mut world = World::new(
                WorldConfig {
                    mode: Mode::BossRush,
                    boss_rush: vec![kind],
                    target_count: 0,
                    spawn_protection: 0.,
                    dynamic_arena: false,
                    ..Default::default()
                },
                Content::builtin(),
            );
            for name in ["Leader", "Friend"] {
                world.add_player(name, false);
                let tank = world.tanks.last_mut().unwrap();
                tank.pos = V2::new(1800., 1600.);
                tank.level = 45;
                // Keep targets alive through lethal rounds so every phase
                // exercises a full attack cycle rather than an empty arena.
                tank.max_health = 1e9;
                tank.health = tank.max_health;
            }
            world.bosses[0].pos = V2::new(1800., 1000.);
            world.bosses[0].arrival = 0.;
            for (phase, fraction) in [(1, 1.), (2, 0.5), (3, 0.2)] {
                let boss = &mut world.bosses[0];
                boss.health = boss.max_health * fraction;
                boss.attack_timer = 0.;
                let mut warned = false;
                let mut fired = false;
                for tick in 0..720 {
                    for (index, tank) in world.tanks.iter_mut().enumerate() {
                        tank.health = tank.max_health;
                        tank.input.movement =
                            V2::from_angle(tick as f32 * DT * 1.7 + index as f32 * 3.);
                    }
                    world.step();
                    let boss = &world.bosses[0];
                    assert_eq!(boss.phase, phase, "{kind:?}");
                    fired |= warned && boss.attack_windup == 0.;
                    warned |= boss.attack_windup > 0.;
                    // The host sends snapshots at 20 Hz while simulating at 60.
                    if tick % 3 == 0 {
                        let restored =
                            decode_snapshot(&encode_snapshot(&world.snapshot()).unwrap());
                        assert!(
                            restored.is_ok(),
                            "{kind:?}, phase {phase}, tick {tick}: {restored:?}",
                        );
                    }
                }
                assert!(
                    warned && fired,
                    "{kind:?}, phase {phase} exercised its special"
                );
            }
        }
    }

    #[test]
    fn a_bosss_host_only_steering_never_travels_in_a_snapshot() {
        let mut world = tankyard_core::World::new(
            tankyard_core::WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        world.add_player("Pilot", false);
        assert_ne!(world.spawn_boss(tankyard_core::BossKind::Sentinel), 0);
        let boss = world.bosses.remove(0);
        let plain = postcard::to_stdvec(&boss).unwrap();
        let mut steering = boss;
        steering.dodge = V2::new(0.7, -0.4);
        assert_eq!(postcard::to_stdvec(&steering).unwrap(), plain);
        let decoded: tankyard_core::Boss = postcard::from_bytes(&plain).unwrap();
        assert_eq!(decoded.dodge, V2::ZERO);
    }

    #[test]
    fn input_history_recovers_taps_across_wrap_and_rejects_malformed_bundles() {
        let mut history = InputBundle::default();
        for sequence in [u32::MAX - 1, u32::MAX, 0, 1] {
            history.push(PlayerInput {
                sequence,
                fire: sequence == u32::MAX,
                steer_return: sequence == u32::MAX,
                ability: sequence == 0,
                movement: V2::new(0.5, -0.5),
                ..Default::default()
            });
        }
        let bytes = encode(&history, MAX_INPUT_BYTES).unwrap();
        assert!(bytes.len() < 80, "history must fit in a small datagram");
        let decoded = decode_inputs(&bytes).unwrap();
        assert_eq!(
            decoded.iter().map(|i| i.sequence).collect::<Vec<_>>(),
            [u32::MAX, 0, 1]
        );
        assert!(decoded.inputs[0].fire && decoded.inputs[1].ability);
        assert!(decoded.inputs[0].steer_return && !decoded.inputs[2].steer_return);
        for count in [0, 4, 255] {
            history.count = count;
            assert!(decode_inputs(&encode(&history, MAX_INPUT_BYTES).unwrap()).is_err());
        }
        history.count = 3;
        history.inputs[1].aim = f32::NAN;
        assert!(decode_inputs(&encode(&history, MAX_INPUT_BYTES).unwrap()).is_err());
        assert!(decode_inputs(&[0; MAX_INPUT_BYTES + 1]).is_err());
        let single = InputMessage {
            input: PlayerInput {
                sequence: u32::MAX,
                ..Default::default()
            },
        };
        let single = decode_inputs(&encode(&single, MAX_INPUT_BYTES).unwrap()).unwrap();
        assert_eq!(single.count, 1);
        assert_eq!(single.inputs[0].sequence, u32::MAX);
    }
    #[test]
    fn pocket_doubles_round_trip_and_invalid_private_state_is_rejected() {
        use tankyard_core::{Mode, PlayerInput, World, WorldConfig};
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                spawn_protection: 0.,
                dynamic_arena: false,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Bacteria", false);
        assert!(world.sandbox_class(id, "bacteria"));
        world.tanks[0].input = PlayerInput {
            ability: true,
            ..Default::default()
        };
        world.step();
        let snapshot = world.snapshot();
        let restored = decode_snapshot(&encode_snapshot(&snapshot).unwrap()).unwrap();
        assert_eq!(
            restored.tanks[0].pocket_echoes.len(),
            tankyard_core::POCKET_ECHO_LIMIT
        );
        for (expected, actual) in snapshot.tanks[0]
            .pocket_echoes
            .iter()
            .zip(&restored.tanks[0].pocket_echoes)
        {
            assert_eq!(expected.class, actual.class);
            assert_eq!(expected.pos, actual.pos);
            assert_eq!(expected.wake, actual.wake);
        }
        for (radius, wake) in [(f32::NAN, 0.), (25., 0.), (0., 0.), (20., -1.), (20., 3.1)] {
            let mut bad = snapshot.clone();
            bad.tanks[0].pocket_echoes[0].radius = radius;
            bad.tanks[0].pocket_echoes[0].wake = wake;
            assert!(decode_snapshot(&encode_snapshot(&bad).unwrap()).is_err());
        }
        let mut bad = snapshot.clone();
        bad.tanks[0].pocket_echoes[0].pos.x = f32::NAN;
        assert!(decode_snapshot(&encode_snapshot(&bad).unwrap()).is_err());
        let mut bad = snapshot.clone();
        let extra = bad.tanks[0].pocket_echoes[0].clone();
        bad.tanks[0].pocket_echoes.push(extra);
        assert!(decode_snapshot(&encode_snapshot(&bad).unwrap()).is_err());
        let mut bad = snapshot;
        bad.tanks[0].pocket = 0.;
        assert!(decode_snapshot(&encode_snapshot(&bad).unwrap()).is_err());
    }
    #[test]
    fn arena_status_spawn_immunity_and_reflected_shots_survive_the_wire() {
        use tankyard_core::{ArenaEvent, Mode, World, WorldConfig};
        for mode in [Mode::RandomEvents, Mode::CoopPve] {
            let mut world = World::new(
                WorldConfig {
                    mode,
                    target_count: 0,
                    ..Default::default()
                },
                Content::builtin(),
            );
            world.add_player("Pilot", false);
            let mut snapshot = world.snapshot();
            snapshot.match_state.arena.wave = 8;
            snapshot.match_state.arena.enemies = 24;
            snapshot.match_state.arena.remaining = 12.5;
            snapshot.match_state.arena.event = ArenaEvent::Regeneration;
            let decoded = decode_snapshot(&encode_snapshot(&snapshot).unwrap()).unwrap();
            assert_eq!(decoded.config.mode, mode);
            assert!(decoded.tanks[0].spawn_immunity);
            assert_eq!(decoded.match_state.arena.wave, 8);
            assert_eq!(decoded.match_state.arena.enemies, 24);
            assert_eq!(decoded.match_state.arena.remaining, 12.5);
            assert_eq!(decoded.match_state.arena.event, ArenaEvent::Regeneration);
            for value in [f32::NAN, -1.0, 61.0] {
                snapshot.match_state.arena.remaining = value;
                assert!(decode_snapshot(&encode_snapshot(&snapshot).unwrap()).is_err());
            }
        }
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Riposte", false);
        assert!(world.sandbox_class(id, "riposte"));
        world.tanks[0].input.movement = V2::new(1., 0.);
        world.tanks[0].input.fire = true;
        world.step();
        world.projectiles[0].kind = tankyard_content::ProjectileKind::Homing;
        world.projectiles[0].homing_target = 123;
        let decoded = decode_snapshot(&encode_snapshot(&world.snapshot()).unwrap()).unwrap();
        assert_eq!(decoded.projectiles[0].homing_target, 123);
        assert_eq!(decoded.projectiles[0].owner, id);
        assert!(!decoded.tanks[0].spawn_immunity);
    }
    #[test]
    fn outbreak_and_hill_timers_round_trip_with_mode_specific_limits() {
        for (mode, limit) in [
            (
                tankyard_core::Mode::Infection,
                tankyard_core::INFECTION_SECONDS,
            ),
            (
                tankyard_core::Mode::KingOfTheHill,
                tankyard_core::HILL_SECONDS,
            ),
        ] {
            let world = tankyard_core::World::new(
                tankyard_core::WorldConfig {
                    mode,
                    target_count: 0,
                    ..Default::default()
                },
                Content::builtin(),
            );
            let mut snapshot = world.snapshot();
            snapshot.match_state.arena.remaining = limit;
            let result = decode_snapshot(&encode_snapshot(&snapshot).unwrap()).unwrap();
            assert_eq!(result.config.mode, mode);
            assert_eq!(result.match_state.arena.remaining, limit);
            snapshot.match_state.arena.remaining = 181.;
            assert!(decode_snapshot(&encode_snapshot(&snapshot).unwrap()).is_err());
        }
    }

    #[test]
    fn reused_snapshot_encoder_preserves_wire_format_and_recovers_after_overflow() {
        let mut world =
            tankyard_core::World::new(tankyard_core::WorldConfig::default(), Content::builtin());
        world.add_player("世界 Mira", false);
        let mut encoder = SnapshotEncoder::default();
        let mut previous: Option<(u64, Vec<u8>)> = None;
        for _ in 0..6 {
            world.step();
            let state = world.snapshot();
            let bytes = encoder.encode(&state).unwrap();
            assert_eq!(bytes, encode_snapshot(&state).unwrap());
            assert_eq!(decode_snapshot(&bytes).unwrap().tick, state.tick);
            if let Some((tick, bytes)) = previous {
                assert_eq!(decode_snapshot(&bytes).unwrap().tick, tick);
            }
            previous = Some((state.tick, bytes));
        }
        let mut state = world.snapshot();
        state.tanks[0].name = "x".repeat(MAX_STATE_BYTES);
        assert!(encoder.encode(&state).is_err());
        state.tanks[0].name = "Mira".into();
        assert_eq!(
            encoder.encode(&state).unwrap(),
            encode_snapshot(&state).unwrap()
        );
    }

    #[test]
    fn match_controls_and_current_settings_roundtrip_with_bounded_values() {
        let command = ClientControl::Admin {
            sequence: 0,
            signature: vec![0; 64],
            round: 3,
            command: ServerCommand::ConfigureMatch {
                mode: tankyard_core::Mode::Domination,
                map: "maze".into(),
                difficulty: "Advanced".into(),
            },
        };
        let decoded = decode_control(&encode(&command, MAX_CONTROL_BYTES).unwrap()).unwrap();
        assert!(matches!(
            decoded,
            ClientControl::Admin {
                round: 3,
                sequence: _,
                signature: _,
                command: ServerCommand::ConfigureMatch {
                    mode: tankyard_core::Mode::Domination,
                    ..
                }
            }
        ));
        let settings = MatchSettings {
            mode: tankyard_core::Mode::Domination,
            map: "maze".into(),
            difficulty: "Advanced".into(),
            bots: 24,
            party: None,
        };
        let message = ServerControl::MatchSettings {
            settings: settings.clone(),
        };
        let ServerControl::MatchSettings { settings: received } = decode::<ServerControl>(
            &encode(&message, MAX_SERVER_CONTROL_BYTES).unwrap(),
            MAX_SERVER_CONTROL_BYTES,
        )
        .unwrap() else {
            panic!("wrong control variant")
        };
        assert_eq!(received, settings);
        assert!(received.valid());
        let mut invalid = settings;
        invalid.bots = 129;
        assert!(!invalid.valid());
    }
    #[test]
    fn reused_interest_snapshots_match_fresh_snapshots_after_viewer_and_world_changes() {
        use tankyard_core::{Mode, PlayerInput, World, WorldConfig};
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 36,
                ..Default::default()
            },
            Content::builtin(),
        );
        let first = world.add_player("One", false);
        let second = world.add_player("Two", false);
        world.tanks[0].pos = V2::new(500., 500.);
        world.tanks[1].pos = V2::new(2800., 2000.);
        let mut reused = world.snapshot();
        for class in ["fork", "bulwark", "hive"] {
            assert!(world.sandbox_class(first, class));
            world.set_input(
                first,
                PlayerInput {
                    fire: true,
                    ..Default::default()
                },
            );
            for _ in 0..30 {
                world.step();
            }
            for center in [V2::new(500., 500.), V2::new(2800., 2000.)] {
                world.snapshot_for_into(&mut reused, center, 450.);
                let fresh = world.snapshot_for(center, 450.);
                assert_eq!(
                    encode_snapshot(&reused).unwrap(),
                    encode_snapshot(&fresh).unwrap()
                );
            }
            world.remove_player(second);
            // Exercise the same trimming used to bound overloaded snapshots.
            reused.projectiles.clear();
            reused.targets.truncate(1);
        }
    }
    #[test]
    fn snapshots_remain_valid_when_switching_between_guns_and_melee_tanks() {
        let mut world = tankyard_core::World::new(
            tankyard_core::WorldConfig {
                mode: tankyard_core::Mode::Sandbox,
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let player = world.add_player("Pilot", false);
        world.step();
        assert!(!world.tanks[0].barrel_shots.is_empty());
        for class in [
            "bulwark",
            "fork",
            "counterguard",
            "reprisal",
            "commodore",
            "juggernaut",
            "vector",
        ] {
            assert!(world.sandbox_class(player, class), "{class}");
            for _ in 0..2 {
                let snapshot = world.snapshot();
                let decoded = decode_snapshot(&encode_snapshot(&snapshot).unwrap());
                assert!(decoded.is_ok(), "{class}: {decoded:?}");
                world.step();
            }
        }
    }
    #[test]
    fn mod_game_data_roundtrips_and_rejects_decompression_bombs() {
        let mut bundle = tankyard_mods::GameBundle::builtin();
        bundle.rules.movement_multiplier = 1.25;
        let ServerControl::GameData { compressed } = game_data(&bundle).unwrap() else {
            unreachable!()
        };
        let decoded = decode_game_data(&compressed).unwrap();
        assert_eq!(players_bundle(&bundle).fingerprint(), decoded.fingerprint());
        // Built-in arenas with logic arrive without it: the server runs it.
        assert!(bundle.maps.iter().any(|map| !map.triggers.is_empty()));
        assert!(decoded.maps.iter().all(|map| map.triggers.is_empty()));
        assert_eq!(decoded.maps.len(), bundle.maps.len());
        let bomb = (u32::MAX).to_le_bytes();
        assert!(decode_game_data(&bomb).is_err());
        let mut invalid = compressed;
        invalid[4..].fill(255);
        assert!(decode_game_data(&invalid).is_err());
    }
    #[test]
    fn closed_moving_gates_reach_players_without_server_logic() {
        use tankyard_content::{
            MapBlock, MapProcedure, MapTrigger, SpawnRegion, TriggerAction, TriggerWhen,
        };
        let mut bundle = tankyard_mods::GameBundle::builtin();
        bundle.maps.truncate(1);
        let map = &mut bundle.maps[0];
        map.procedural = MapProcedure::None;
        map.walls.clear();
        map.blocks = vec![MapBlock {
            rect: [0.49, 0., 0.02, 1.],
            group: 1,
            style: Default::default(),
            color: None,
        }];
        map.spawn_regions = vec![SpawnRegion {
            team: 0,
            rect: [0.1, 0.4, 0.1, 0.2],
        }];
        map.team_bases.clear();
        map.objective_positions.clear();
        map.paths.clear();
        map.bodies.clear();
        map.triggers = vec![MapTrigger {
            when: TriggerWhen::Start,
            action: TriggerAction::Hide,
            group: 1,
            ..Default::default()
        }];
        bundle.validate().unwrap();
        assert!(
            players_bundle(&bundle)
                .validate()
                .unwrap_err()
                .contains("disconnected")
        );
        let ServerControl::GameData { compressed } = game_data(&bundle).unwrap() else {
            unreachable!()
        };
        let received = decode_game_data(&compressed).unwrap();
        assert_eq!(received.maps[0].blocks, bundle.maps[0].blocks);
        assert!(received.maps[0].triggers.is_empty());
        assert_eq!(
            received.fingerprint(),
            players_bundle(&bundle).fingerprint()
        );

        // A genuinely disconnected authored map must still be rejected by
        // the server. Removing logic cannot become a way around that check.
        bundle.maps[0].triggers.clear();
        assert!(game_data(&bundle).is_err());

        // Player data still rejects malformed geometry and resource bombs.
        let mut bad = received;
        bad.maps[0].blocks[0].rect[0] = f32::NAN;
        let bytes = encode(&bad, tankyard_mods::MAX_PACK_BYTES).unwrap();
        assert!(decode_game_data(&lz4_flex::compress_prepend_size(&bytes)).is_err());
        bad.maps[0].blocks[0].rect[0] = 0.49;
        bad.maps[0].width = 16000.;
        bad.maps[0].height = 16000.;
        let block = bad.maps[0].blocks[0].clone();
        bad.maps[0].blocks = vec![block; 2048];
        assert!(bad.validate_for_players().unwrap_err().contains("budget"));
    }

    #[test]
    fn bounded_control_and_trailing_data() {
        let request = ClientControl::UpgradeStat { stat: 3, count: 1 };
        let mut bytes = encode(&request, MAX_CONTROL_BYTES).unwrap();
        assert!(decode_control(&bytes).is_ok());
        bytes.push(0);
        assert!(decode_control(&bytes).is_err());
        assert!(decode_control(&vec![0; MAX_CONTROL_BYTES + 1]).is_err());
        let invalid = encode(
            &ClientControl::UpgradeStat { stat: 8, count: 1 },
            MAX_CONTROL_BYTES,
        )
        .unwrap();
        assert!(decode_control(&invalid).is_err());
        for count in [0, 8, 255] {
            let invalid = encode(
                &ClientControl::UpgradeStat { stat: 0, count },
                MAX_CONTROL_BYTES,
            )
            .unwrap();
            assert!(decode_control(&invalid).is_err());
        }
    }
    #[test]
    fn rejects_nonfinite_input() {
        let input = PlayerInput {
            movement: V2::new(f32::NAN, 0.),
            ..Default::default()
        };
        assert!(decode_input(&encode(&InputMessage { input }, MAX_INPUT_BYTES).unwrap()).is_err());
        for distance in [f32::NAN, f32::INFINITY, -1., 5001.] {
            let input = PlayerInput {
                aim_distance: distance,
                ..Default::default()
            };
            assert!(
                decode_input(&encode(&InputMessage { input }, MAX_INPUT_BYTES).unwrap()).is_err()
            );
        }
    }
    #[test]
    fn compressed_bomb_and_malformed_input() {
        assert!(decode_snapshot(&u32::MAX.to_le_bytes()).is_err());
        for len in 0..128 {
            let bytes = vec![255; len];
            assert!(decode_control(&bytes).is_err());
            assert!(decode_snapshot(&bytes).is_err());
        }
    }
    #[test]
    fn wraparound_sequence_and_names() {
        assert!(sequence_newer(0, u32::MAX));
        assert!(!sequence_newer(5, 6));
        assert!(!sequence_newer(5, 5));
        assert_eq!(sanitize_name("\n<>"), "Pilot");
        assert_eq!(sanitize_name(&"a".repeat(100)).len(), MAX_NAME_CHARS);
    }

    #[test]
    fn full_world_snapshot_roundtrip_and_content_compatibility() {
        use tankyard_core::{Mode, World, WorldConfig};
        let content = Content::builtin();
        assert_eq!(
            content_fingerprint(&content),
            content_fingerprint(&content.clone())
        );
        let mut changed = content.clone();
        changed.classes[0].multipliers[0] += 0.5;
        assert_ne!(content_fingerprint(&content), content_fingerprint(&changed));
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 64,
                ..Default::default()
            },
            content,
        );
        let id = world.add_player("Protocol Pilot", false);
        assert!(world.set_input(
            id,
            PlayerInput {
                sequence: 123,
                movement: V2::new(0.5, -0.5),
                fire: true,
                ..Default::default()
            }
        ));
        for _ in 0..30 {
            world.step();
        }
        let snapshot = world.snapshot();
        let bytes = encode_snapshot(&snapshot).unwrap();
        let restored = decode_snapshot(&bytes).unwrap();
        assert_eq!(restored.tick, snapshot.tick);
        assert_eq!(restored.tanks[0].id, id);
        assert_eq!(restored.tanks[0].last_input, 123);
        assert_eq!(restored.tanks[0].pos, snapshot.tanks[0].pos);
        assert_eq!(restored.projectiles.len(), snapshot.projectiles.len());
        assert_eq!(restored.targets.len(), snapshot.targets.len());
    }

    #[test]
    fn deterministic_byte_fuzz_never_panics() {
        let mut state = 7u64;
        for length in 0..2048 {
            let bytes: Vec<_> = (0..length)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                })
                .collect();
            let _ = decode_control(&bytes);
            let _ = decode_input(&bytes);
            let _ = decode_snapshot(&bytes);
            let _ = decode_voice(&bytes);
            let _ = decode_voice_packet(&bytes);
        }
    }
}

#[cfg(test)]
mod state_regressions {
    use super::*;
    use tankyard_core::{World, WorldConfig};

    #[test]
    fn malformed_health_and_mount_arrays_are_rejected_before_prediction() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("State", false);
        let mut snapshot = world.snapshot();
        snapshot.tanks[0].max_health = 0.0;
        assert!(decode_snapshot(&encode_snapshot(&snapshot).unwrap()).is_err());
        snapshot.tanks[0].max_health = 100.0;
        snapshot.tanks[0].cooldowns.push(0.0);
        assert!(decode_snapshot(&encode_snapshot(&snapshot).unwrap()).is_err());
    }
}

#[cfg(test)]
mod concealment_tests {
    use super::*;
    use tankyard_core::{World, WorldConfig};

    #[test]
    fn concealed_distant_tanks_keep_a_valid_compact_snapshot() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let viewer = world.add_player("Viewer", false);
        world.add_player("Far", false);
        world.sandbox_level(viewer, 1);
        for _ in 0..30 {
            world.step();
        }
        let mut snapshot = world.snapshot();
        let full = encode(&snapshot, MAX_STATE_BYTES).unwrap().len();
        let far = &mut snapshot.tanks[1];
        far.input.aim = 1.2;
        far.input.aim_distance = 800.;
        tankyard_core::conceal_distant_tank(far);
        assert_eq!(far.input.aim_distance, PlayerInput::default().aim_distance);
        assert!(far.cooldowns.is_empty() && far.barrel_aims.is_empty());
        assert_eq!(far.pos.x % 96., 0.);
        assert!(encode(&snapshot, MAX_STATE_BYTES).unwrap().len() < full);
        assert!(decode_snapshot(&encode_snapshot(&snapshot).unwrap()).is_ok());
    }
}
