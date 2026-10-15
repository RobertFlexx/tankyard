use crate::{NetError, unix_time};
use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport, generate_random_bytes};
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::Duration,
};
use tankyard_core::{BossKind, EntityId, GameEvent, PlayerInput, Snapshot};
use tankyard_protocol::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct NetworkDiagnostics {
    pub ping_ms: f64,
    pub packet_loss: f64,
    pub sent_bytes_per_sec: f64,
    pub received_bytes_per_sec: f64,
    pub server_tick_rate: u32,
}

pub struct NetworkClient {
    pub player_id: Option<EntityId>,
    pub player_round: Option<u32>,
    pub snapshot: Option<Snapshot>,
    pub previous_snapshot: Option<Snapshot>,
    pub events: Vec<GameEvent>,
    pub error: Option<String>,
    pub last_action: Option<String>,
    pub match_settings: Option<MatchSettings>,
    pub server_name: String,
    pub sandbox_allowed: bool,
    pub address: SocketAddr,
    pub bundle: tankyard_mods::GameBundle,
    pub bundle_revision: u64,
    pub kind: ServerKind,
    pub chat_enabled: bool,
    pub role: ServerRole,
    pub permissions: Vec<ServerPermission>,
    pub rooms: Vec<RoomInfo>,
    pub chat_messages: std::collections::VecDeque<ChatMessage>,
    pub chat_revision: u64,
    pub audit_lines: Vec<String>,
    client: RenetClient,
    transport: NetcodeClientTransport,
    hello: ClientControl,
    hello_sent: bool,
    tick_rate: u32,
    event_queue: Vec<GameEvent>,
    handshake_elapsed: Duration,
}

impl NetworkClient {
    pub fn connect(address: &str, name: &str, password: &str) -> Result<Self, NetError> {
        if address.len() > 256 || password.len() > 128 {
            return Err(NetError("server address or password is too long".into()));
        }
        let address = address
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| NetError("server address did not resolve".into()))?;
        let socket = UdpSocket::bind(if address.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })?;
        let client_id = u64::from_le_bytes(generate_random_bytes());
        Self::connect_with_auth(
            address,
            name,
            password,
            ClientAuthentication::Unsecure {
                protocol_id: PROTOCOL_ID,
                client_id,
                server_addr: address,
                user_data: None,
            },
            None,
            socket,
        )
    }
    pub fn connect_authenticated(
        token: renet_netcode::ConnectToken,
        name: &str,
        password: &str,
        account_token: Option<String>,
    ) -> Result<Self, NetError> {
        if token.protocol_id != PROTOCOL_ID
            || token.expire_timestamp <= unix_time().as_secs()
            || account_token.as_ref().is_some_and(|s| s.len() > 1024)
        {
            return Err(NetError("invalid or expired connection token".into()));
        }
        let address = token
            .server_addresses
            .iter()
            .flatten()
            .next()
            .copied()
            .ok_or_else(|| NetError("connection token has no server address".into()))?;
        let socket = UdpSocket::bind(if address.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })?;
        Self::connect_with_auth(
            address,
            name,
            password,
            ClientAuthentication::Secure {
                connect_token: token,
            },
            account_token,
            socket,
        )
    }
    fn connect_with_auth(
        address: SocketAddr,
        name: &str,
        password: &str,
        authentication: ClientAuthentication,
        account_token: Option<String>,
        socket: UdpSocket,
    ) -> Result<Self, NetError> {
        if password.len() > 128 {
            return Err(NetError("password exceeds 128 bytes".into()));
        }
        let transport = NetcodeClientTransport::new(unix_time(), authentication, socket)
            .map_err(|e| NetError(e.to_string()))?;
        let bundle = tankyard_mods::GameBundle::builtin();
        let bundle_revision = bundle.fingerprint();
        Ok(Self {
            player_id: None,
            player_round: None,
            snapshot: None,
            previous_snapshot: None,
            events: Vec::new(),
            error: None,
            last_action: None,
            match_settings: None,
            server_name: String::new(),
            sandbox_allowed: false,
            address,
            client: RenetClient::new(connection_config()),
            transport,
            hello: ClientControl::Hello {
                protocol: PROTOCOL_VERSION,
                content: bundle_revision,
                name: sanitize_name(name),
                password: password.into(),
                account_token,
            },
            hello_sent: false,
            tick_rate: 60,
            event_queue: Vec::with_capacity(128),
            handshake_elapsed: Duration::ZERO,
            bundle,
            bundle_revision,
            kind: ServerKind::Community,
            chat_enabled: false,
            role: ServerRole::Player,
            permissions: vec![],
            rooms: vec![],
            chat_messages: std::collections::VecDeque::with_capacity(64),
            chat_revision: 0,
            audit_lines: Vec::new(),
        })
    }
    pub fn update(&mut self, elapsed: Duration) -> Result<(), NetError> {
        self.events.clear();
        if self.player_id.is_none() {
            self.handshake_elapsed += elapsed;
            if self.handshake_elapsed > Duration::from_secs(15) {
                return self.fail("Server handshake timed out. Check the address, UDP firewall, and server version.".into());
            }
        }
        self.client.update(elapsed);
        if let Err(e) = self.transport.update(elapsed, &mut self.client) {
            return self.fail(e.to_string());
        }
        if let Some(reason) = self.transport.disconnect_reason() {
            return self.fail(reason.to_string());
        }
        if let Some(reason) = self.client.disconnect_reason() {
            return self.fail(reason.to_string());
        }
        if self.client.is_connected() && !self.hello_sent {
            self.hello_sent = self.queue_control(self.hello.clone());
        }
        let mut received_game_data = false;
        for _ in 0..128 {
            let Some(bytes) = self.client.receive_message(CONTROL_CHANNEL) else {
                break;
            };
            let control: ServerControl = match decode(&bytes, MAX_SERVER_CONTROL_BYTES) {
                Ok(control) => control,
                Err(e) => return self.fail(e.to_string()),
            };
            match control {
                ServerControl::Audit { lines } => {
                    if lines.len() > 32
                        || lines
                            .iter()
                            .any(|line| line.len() > 1024 || line.chars().any(char::is_control))
                    {
                        return self.fail("Invalid server audit data.".into());
                    }
                    self.audit_lines = lines;
                }
                ServerControl::Access {
                    kind,
                    chat_enabled,
                    role,
                    permissions,
                } => {
                    if permissions.len() > 8
                        || permissions
                            .iter()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            != permissions.len()
                    {
                        return self.fail("Invalid server permission data.".into());
                    }
                    self.kind = kind;
                    self.chat_enabled = kind == ServerKind::Community && chat_enabled;
                    self.role = role;
                    self.permissions = permissions;
                }
                ServerControl::Rooms { rooms } => {
                    if rooms.len() > 8
                        || rooms.iter().any(|r| {
                            !tankyard_mods::identifier(&r.id)
                                || r.name.len() > 48
                                || r.name.chars().any(char::is_control)
                        })
                    {
                        return self.fail("Invalid chat room list.".into());
                    }
                    self.rooms = rooms;
                }
                ServerControl::Chat { message } => {
                    if !self.chat_enabled
                        || message.id == 0
                        || !tankyard_mods::identifier(&message.room)
                        || message.text.chars().count() > 160
                        || message.text.len() > 640
                        || message.name.len() > 96
                        || message.name.chars().any(char::is_control)
                        || message.text.chars().any(char::is_control)
                    {
                        return self.fail("Invalid server chat message.".into());
                    }
                    if self.chat_messages.iter().any(|m| m.id == message.id) {
                        continue;
                    }
                    self.chat_messages.push_back(message);
                    self.chat_messages.make_contiguous().sort_by_key(|m| m.id);
                    if self.chat_messages.len() > 64 {
                        self.chat_messages.pop_front();
                    }
                    self.chat_revision = self.chat_revision.wrapping_add(1);
                }
                ServerControl::GameData { compressed } => {
                    if received_game_data {
                        return self.fail("Too many game data updates in one frame.".into());
                    }
                    received_game_data = true;
                    self.bundle = match decode_game_data(&compressed) {
                        Ok(bundle) => bundle,
                        Err(e) => return self.fail(e.to_string()),
                    };
                    self.bundle_revision = self.bundle.fingerprint();
                }
                ServerControl::Welcome {
                    protocol,
                    content,
                    player,
                    round,
                    server_name,
                    sandbox_allowed,
                    tick_rate,
                } => {
                    if protocol != PROTOCOL_VERSION
                        || content != self.bundle_revision
                        || tick_rate != 60
                        || server_name.len() > 96
                        || server_name.chars().any(char::is_control)
                        || player == 0
                        || round == 0
                        || self.player_round.is_some_and(|previous| round < previous)
                        || (self.player_round == Some(round)
                            && self.player_id.is_some_and(|previous| player != previous))
                    {
                        return self
                            .fail("Server gameplay content or protocol is incompatible.".into());
                    }
                    if self.player_id != Some(player) || self.player_round != Some(round) {
                        self.snapshot = None;
                        self.previous_snapshot = None;
                        self.event_queue.clear();
                        self.events.clear();
                    }
                    self.player_id = Some(player);
                    self.player_round = Some(round);
                    self.server_name = server_name;
                    self.sandbox_allowed = sandbox_allowed;
                    self.tick_rate = tick_rate;
                }
                ServerControl::Rejected { reason } | ServerControl::Shutdown { reason } => {
                    if reason.len() > 1024 || reason.chars().any(char::is_control) {
                        return self.fail("Invalid server response.".into());
                    }
                    return self.fail(reason);
                }
                ServerControl::Action { accepted, reason } => {
                    if reason.len() > 1024 || reason.chars().any(char::is_control) {
                        return self.fail("Invalid server response.".into());
                    }
                    self.last_action = (!accepted || !reason.is_empty()).then_some(reason);
                }
                ServerControl::MatchSettings { settings } => {
                    if !settings.valid()
                        || !self.bundle.maps.iter().any(|map| map.id == settings.map)
                    {
                        return self.fail("Invalid server match settings.".into());
                    }
                    self.match_settings = Some(settings);
                }
                ServerControl::PlayerJoined { .. } | ServerControl::PlayerLeft { .. } => {}
            }
        }
        for _ in 0..128 {
            let Some(bytes) = self.client.receive_message(EVENT_CHANNEL) else {
                break;
            };
            let events: EventMessage = match decode(&bytes, MAX_EVENT_BYTES) {
                Ok(events) => events,
                Err(e) => return self.fail(e.to_string()),
            };
            if self.player_round != Some(events.round) {
                continue;
            }
            if self.event_queue.len() + events.events.len() > 4096 {
                return self.fail("Server event queue exceeds safety limit.".into());
            }
            self.events.extend(events.events.iter().cloned());
            self.event_queue.extend(events.events);
        }
        let mut latest = None;
        for _ in 0..32 {
            let Some(bytes) = self.client.receive_message(REALTIME_CHANNEL) else {
                break;
            };
            let snapshot = match decode_snapshot(&bytes) {
                Ok(snapshot) => snapshot,
                Err(e) => return self.fail(e.to_string()),
            };
            // Control and realtime packets can arrive in either order. IDs
            // may be reused in a new round, so wait for its Welcome first.
            if self.player_round != Some(snapshot.match_state.round) {
                continue;
            }
            if !snapshot_matches_player(&snapshot, self.player_id, self.player_round) {
                return self.fail("Snapshot does not contain the assigned player.".into());
            }
            if snapshot.config.rules != self.bundle.rules
                || snapshot
                    .tanks
                    .iter()
                    .any(|t| self.bundle.content.class(&t.class).is_none())
            {
                return self.fail("Snapshot does not match the server's game data.".into());
            }
            let baseline = latest
                .as_ref()
                .map(|s: &Snapshot| s.tick)
                .or_else(|| self.snapshot.as_ref().map(|s| s.tick));
            if baseline.is_none_or(|tick| snapshot.tick > tick) {
                latest = Some(snapshot);
            }
        }
        if let Some(mut snapshot) = latest {
            self.event_queue
                .retain(|e| e.tick.saturating_add(120) >= snapshot.tick);
            snapshot.events.append(&mut self.event_queue);
            self.previous_snapshot = self.snapshot.take();
            self.snapshot = Some(snapshot);
        }
        self.flush()
    }
    /// Send inputs and UI commands queued after this frame's network update.
    pub fn flush(&mut self) -> Result<(), NetError> {
        if let Err(error) = self.transport.send_packets(&mut self.client) {
            return self.fail(error.to_string());
        }
        Ok(())
    }
    pub fn send_input(&mut self, input: PlayerInput) -> bool {
        if !self.is_ready() || !input.valid() {
            return false;
        }
        let Ok(bytes) = encode(&InputMessage { input }, MAX_INPUT_BYTES) else {
            return false;
        };
        if !self.client.can_send_message(REALTIME_CHANNEL, bytes.len()) {
            return false;
        }
        self.client.send_message(REALTIME_CHANNEL, bytes);
        true
    }
    pub fn upgrade_stat(&mut self, stat: usize) -> bool {
        self.upgrade_stat_amount(stat, 1)
    }
    pub fn upgrade_stat_amount(&mut self, stat: usize, count: u8) -> bool {
        stat < 8
            && self.queue_control(ClientControl::UpgradeStat {
                stat: stat as u8,
                count,
            })
    }
    pub fn evolve(&mut self, class: &str) -> bool {
        self.queue_control(ClientControl::Evolve {
            class: class.into(),
        })
    }
    pub fn send_chat(&mut self, room: &str, text: &str) -> bool {
        self.chat_enabled
            && self.queue_control(ClientControl::Chat {
                room: room.into(),
                text: text.into(),
            })
    }
    pub fn join_room(&mut self, room: &str) -> bool {
        self.chat_enabled && self.queue_control(ClientControl::JoinRoom { room: room.into() })
    }
    pub fn admin(&mut self, command: ServerCommand) -> bool {
        let Some(round) = self.snapshot.as_ref().map(|s| s.match_state.round) else {
            return false;
        };
        self.permissions.contains(&command.permission())
            && self.queue_control(ClientControl::Admin { round, command })
    }
    pub fn sandbox_level(&mut self, level: u8) -> bool {
        self.sandbox_allowed && self.queue_control(ClientControl::SandboxLevel { level })
    }
    pub fn sandbox_class(&mut self, class: &str) -> bool {
        self.sandbox_allowed
            && self.queue_control(ClientControl::SandboxClass {
                class: class.into(),
            })
    }
    pub fn sandbox_invulnerability(&mut self, enabled: bool) -> bool {
        self.sandbox_allowed
            && self.queue_control(ClientControl::SandboxInvulnerability { enabled })
    }
    pub fn sandbox_boss(&mut self, kind: BossKind) -> bool {
        self.sandbox_allowed && self.queue_control(ClientControl::SandboxBoss { kind })
    }
    pub fn is_ready(&self) -> bool {
        self.client.is_connected() && self.player_snapshot().is_some()
    }
    pub fn player_snapshot(&self) -> Option<&Snapshot> {
        if self.error.is_some() {
            return None;
        }
        self.snapshot
            .as_ref()
            .filter(|snapshot| snapshot_matches_player(snapshot, self.player_id, self.player_round))
    }
    pub fn is_connecting(&self) -> bool {
        self.client.is_connecting() || (self.client.is_connected() && self.player_id.is_none())
    }
    pub fn client_id(&self) -> u64 {
        self.transport.client_id()
    }
    pub fn diagnostics(&self) -> NetworkDiagnostics {
        NetworkDiagnostics {
            ping_ms: self.client.rtt() * 1000.,
            packet_loss: self.client.packet_loss(),
            sent_bytes_per_sec: self.client.bytes_sent_per_sec(),
            received_bytes_per_sec: self.client.bytes_received_per_sec(),
            server_tick_rate: self.tick_rate,
        }
    }
    pub fn disconnect(&mut self) {
        self.transport.disconnect();
        self.client.disconnect();
    }
    fn queue_control(&mut self, control: ClientControl) -> bool {
        if !self.client.is_connected() || !control.valid() {
            return false;
        }
        let Ok(bytes) = encode(&control, MAX_CONTROL_BYTES) else {
            return false;
        };
        if !self.client.can_send_message(CONTROL_CHANNEL, bytes.len()) {
            return false;
        }
        self.client.send_message(CONTROL_CHANNEL, bytes);
        true
    }
    fn fail<T>(&mut self, reason: String) -> Result<T, NetError> {
        if self.error.is_none() {
            self.error = Some(reason);
        }
        self.disconnect();
        Err(NetError(
            self.error.clone().unwrap_or_else(|| "Disconnected".into()),
        ))
    }
}
impl Drop for NetworkClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn snapshot_matches_player(
    snapshot: &Snapshot,
    player: Option<EntityId>,
    round: Option<u32>,
) -> bool {
    round == Some(snapshot.match_state.round)
        && player.is_some_and(|id| {
            id != 0 && snapshot.tanks.iter().any(|tank| tank.id == id && !tank.bot)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;
    use tankyard_core::{World, WorldConfig};

    #[test]
    fn round_assignments_reject_reordered_snapshots_with_reused_human_and_bot_ids() {
        let config = WorldConfig {
            target_count: 0,
            ..Default::default()
        };
        let mut old = World::new(config.clone(), Content::builtin());
        let old_player = old.add_player("Pilot", false);
        let old_other = old.add_player("Other", false);
        old.tanks[1].kills = 100;
        old.tick = 100;
        let old_snapshot = decode_snapshot(&encode_snapshot(&old.snapshot()).unwrap()).unwrap();
        assert!(snapshot_matches_player(
            &old_snapshot,
            Some(old_player),
            Some(1)
        ));

        let mut next = World::new(config, Content::builtin());
        let next_other = next.add_player("Other", false);
        let next_player = next.add_player("Pilot", false);
        next.match_state.round = 2;
        next.tick = 101;
        assert_eq!(old_player, next_other);
        assert_eq!(old_other, next_player);
        let welcome = ServerControl::Welcome {
            protocol: PROTOCOL_VERSION,
            content: tankyard_mods::GameBundle::builtin().fingerprint(),
            player: next_player,
            round: 2,
            server_name: "Test".into(),
            sandbox_allowed: false,
            tick_rate: 60,
        };
        let welcome: ServerControl = decode(
            &encode(&welcome, MAX_SERVER_CONTROL_BYTES).unwrap(),
            MAX_SERVER_CONTROL_BYTES,
        )
        .unwrap();
        let ServerControl::Welcome { player, round, .. } = welcome else {
            unreachable!()
        };
        let next_snapshot = decode_snapshot(&encode_snapshot(&next.snapshot()).unwrap()).unwrap();
        // Snapshot first: the previous assignment must not observe Other.
        assert!(!snapshot_matches_player(
            &next_snapshot,
            Some(old_player),
            Some(1)
        ));
        // Welcome first: its new ID belongs to Other in the old snapshot.
        assert!(!snapshot_matches_player(
            &old_snapshot,
            Some(player),
            Some(round)
        ));
        assert!(snapshot_matches_player(
            &next_snapshot,
            Some(player),
            Some(round)
        ));
        next.tanks[1].bot = true;
        assert!(!snapshot_matches_player(
            &next.snapshot(),
            Some(player),
            Some(round)
        ));
        next.tanks.clear();
        assert!(!snapshot_matches_player(
            &next.snapshot(),
            Some(player),
            Some(round)
        ));
    }
}
