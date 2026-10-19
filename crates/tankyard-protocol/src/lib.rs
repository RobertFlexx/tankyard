//! Versioned, bounded application messages. Transport encryption is provided by
//! renet_netcode only when secure authentication is configured;
//! simulation authority always remains with the server.
use renet::{ChannelConfig, ConnectionConfig, SendType};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fmt, time::Duration};
use tankyard_content::Content;
use tankyard_core::{BossKind, EntityId, GameEvent, PlayerInput, Snapshot};
mod creator;
pub use creator::*;
mod community;
pub use community::*;
mod voice;
pub use voice::*;

pub const PROTOCOL_VERSION: u32 = 15;
pub const PROTOCOL_ID: u64 = 0x5441_4e4b_5941_000f;
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
        command: ServerCommand,
    },
    Spectate {
        round: u32,
        target: Option<EntityId>,
    },
}

impl ClientControl {
    pub fn valid(&self) -> bool {
        match self {
            Self::CreatorProof { signature } => signature.len() == 64,
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
                ..
            } => {
                name.len() <= 96
                    && password.len() <= 128
                    && account_token.as_ref().is_none_or(|s| s.len() <= 1024)
            }
            Self::UpgradeStat { stat, count } => *stat < 8 && (1..=7).contains(count),
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
            Self::Admin { command, .. } => command.valid(),
            Self::Spectate { round, target } => *round > 0 && target.is_none_or(|id| id > 0),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerControl {
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
pub fn game_data(bundle: &tankyard_mods::GameBundle) -> Result<ServerControl, DecodeError> {
    bundle
        .validate()
        .map_err(|_| DecodeError("invalid game data"))?;
    let bytes = encode(bundle, tankyard_mods::MAX_PACK_BYTES)?;
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
        .validate()
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
    let state: Snapshot = decode(&raw, MAX_STATE_BYTES)?;
    if !snapshot_valid(&state) {
        return Err(DecodeError("invalid snapshot values"));
    }
    Ok(state)
}

pub fn sanitize_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| {
            !c.is_control()
                && *c != '<'
                && *c != '>'
                && !matches!(
                    *c,
                    '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}'
                        | '\u{2066}'..='\u{2069}'
                )
        })
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
                && (0.25..=6.).contains(&t.reload_scale)
                && (0.25..=8.).contains(&t.damage_scale)
                && (0.0..=1.).contains(&t.knockback)
                && (0.0..=tankyard_core::STASIS_SECONDS).contains(&t.stasis)
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
                && (0.0..=30.).contains(&b.attack_timer)
                && (0.0..=0.91).contains(&b.attack_windup)
                && b.attack <= 2
                && (1..=3).contains(&b.phase)
                && b.team <= 4
        })
        && s.walls.iter().all(|w| {
            scalar(w.x) && scalar(w.y) && scalar(w.w) && scalar(w.h) && w.w >= 0. && w.h >= 0.
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
        && (0.0..=60.0).contains(&s.match_state.arena.remaining)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::V2;

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
        assert_eq!(bundle.fingerprint(), decoded.fingerprint());
        let bomb = (u32::MAX).to_le_bytes();
        assert!(decode_game_data(&bomb).is_err());
        let mut invalid = compressed;
        invalid[4..].fill(255);
        assert!(decode_game_data(&invalid).is_err());
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
