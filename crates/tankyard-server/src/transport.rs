use crate::{LocalSession, NetError, ServerSettings, unix_time};
use renet::{Bytes, RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    time::Duration,
};
use tankyard_core::{DT, EntityId, EventKind, GameEvent, Mode, PlayerInput, TICK_RATE, V2};
use tankyard_protocol::*;

#[derive(Debug)]
pub(crate) struct Peer {
    pub entity: Option<EntityId>,
    pub connected_at: Duration,
    pub last_input_at: Duration,
    pub last_sequence: Option<u32>,
    pub control_tokens: f32,
    pub input_tokens: f32,
    pub disconnect_at: Option<Duration>,
}
#[derive(Default, Clone, Debug)]
pub struct ServerMetrics {
    pub ticks: u64,
    pub dropped_catchup_ticks: u64,
    pub snapshots_sent: u64,
    pub snapshots_skipped: u64,
    pub snapshot_bytes: u64,
    pub rejected_packets: u64,
    pub last_tick_micros: u64,
    pub max_tick_micros: u64,
    pub last_snapshot_micros: u64,
    pub max_snapshot_micros: u64,
    tick_samples: Vec<u64>,
    sample_index: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TickTiming {
    pub average_micros: f64,
    pub p95_micros: u64,
    pub p99_micros: u64,
    pub samples: usize,
}
impl ServerMetrics {
    fn record_tick(&mut self, micros: u64) {
        self.last_tick_micros = micros;
        self.max_tick_micros = self.max_tick_micros.max(micros);
        self.ticks += 1;
        if self.tick_samples.len() < 4096 {
            self.tick_samples.push(micros);
        } else {
            self.tick_samples[self.sample_index] = micros;
            self.sample_index = (self.sample_index + 1) % 4096;
        }
    }
    /// Percentiles of the most recent 4096 ticks. Sorting occurs only when
    /// diagnostics are requested, outside authoritative hot paths.
    pub fn tick_timing(&self) -> TickTiming {
        if self.tick_samples.is_empty() {
            return TickTiming::default();
        }
        let mut samples = self.tick_samples.clone();
        samples.sort_unstable();
        TickTiming {
            average_micros: samples.iter().map(|sample| *sample as f64).sum::<f64>()
                / samples.len() as f64,
            p95_micros: samples[(samples.len() - 1) * 95 / 100],
            p99_micros: samples[(samples.len() - 1) * 99 / 100],
            samples: samples.len(),
        }
    }
}

pub struct UdpServer {
    pub session: LocalSession,
    pub settings: ServerSettings,
    pub address: SocketAddr,
    pub metrics: ServerMetrics,
    pub(crate) server: RenetServer,
    pub(crate) transport: NetcodeServerTransport,
    pub(crate) peers: HashMap<u64, Peer>,
    pub(crate) bans: Vec<crate::admin::Ban>,
    pub(crate) elapsed: Duration,
    accumulator: Duration,
    snapshot_accumulator: Duration,
    pending_effects: Vec<GameEvent>,
    important_events: Vec<GameEvent>,
    client_ids: Vec<u64>,
    snapshot_buffer: tankyard_core::Snapshot,
    snapshot_encoder: SnapshotEncoder,
    finished_for: f32,
    pub(crate) shutting_down: bool,
    content_hash: u64,
    loaded_mods: tankyard_mods::LoadedMods,
    game_data: Bytes,
    native_plugins: Vec<fn() -> Box<dyn tankyard_core::modding::ServerPlugin>>,
    pub(crate) access: crate::access::Access,
    pub(crate) account_verifier: Option<std::sync::Arc<dyn crate::access::AccountVerifier>>,
    global_authority: Option<std::sync::Arc<dyn crate::access::GlobalAuthority>>,
    pub(crate) chat: crate::chat::Chat,
    accounts_checked_at: u64,
}

#[derive(Clone, Debug)]
pub struct AdminReply {
    pub message: String,
    pub shutdown: bool,
}

impl UdpServer {
    pub fn bind(mut settings: ServerSettings) -> Result<Self, NetError> {
        settings.validate()?;
        if settings.server_id.is_empty() {
            settings.server_id = renet_netcode::generate_random_bytes::<16>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
        }
        let access =
            crate::access::Access::new(settings.server_id.clone(), settings.access_file.clone())
                .map_err(NetError)?;
        let chat = crate::chat::Chat::new(settings.kind, settings.chat_enabled);
        let loaded_mods = tankyard_mods::load(&settings.mods).map_err(NetError)?;
        let mut session = LocalSession::with_mods(
            settings.world.clone(),
            settings.desired_bots(0),
            loaded_mods.clone(),
        )
        .map_err(NetError)?;
        session.bots.difficulty = settings.difficulty;
        let content_hash = session.bundle.fingerprint();
        let game_data = encode_game_data_message(&session.bundle)?;
        let socket = UdpSocket::bind(&settings.bind)?;
        crate::sockets::tune_udp(&socket, 1024 * 1024);
        let address = socket.local_addr()?;
        if settings.netcode_private_key.is_none() && !address.ip().is_loopback() {
            log::warn!(
                "Server is listening without secure netcode authentication. Do not expose private data or assume a password encrypts traffic."
            );
        }
        let transport = NetcodeServerTransport::new(
            ServerConfig {
                current_time: unix_time(),
                max_clients: settings.max_players,
                protocol_id: PROTOCOL_ID,
                public_addresses: if settings.public_addresses.is_empty() {
                    vec![address]
                } else {
                    settings.public_addresses.clone()
                },
                authentication: match &settings.netcode_private_key {
                    Some(key) => ServerAuthentication::Secure {
                        private_key: crate::config::parse_private_key(key)?,
                    },
                    None => ServerAuthentication::Unsecure,
                },
            },
            socket,
        )?;
        let bans = crate::admin::load_bans(&settings.ban_file)?;
        let snapshot_buffer = session.world.snapshot();
        let client_ids = Vec::with_capacity(settings.max_players);
        Ok(Self {
            session,
            settings,
            address,
            metrics: ServerMetrics {
                tick_samples: Vec::with_capacity(4096),
                ..Default::default()
            },
            server: RenetServer::new(connection_config()),
            transport,
            peers: HashMap::new(),
            bans,
            elapsed: Duration::ZERO,
            accumulator: Duration::ZERO,
            snapshot_accumulator: Duration::ZERO,
            pending_effects: Vec::with_capacity(1024),
            important_events: Vec::with_capacity(128),
            client_ids,
            snapshot_buffer,
            snapshot_encoder: SnapshotEncoder::default(),
            finished_for: 0.,
            shutting_down: false,
            content_hash,
            loaded_mods,
            game_data,
            native_plugins: vec![],
            access,
            account_verifier: None,
            global_authority: None,
            chat,
            accounts_checked_at: 0,
        })
    }

    /// Receive immediately, run fixed 60 Hz steps, send 20 Hz AOI snapshots.
    /// Catch-up work is bounded to eight ticks; excess time is measured and
    /// dropped rather than allowing an overloaded host to spiral indefinitely.
    pub fn update(&mut self, elapsed: Duration) -> Result<(), NetError> {
        if self.shutting_down {
            return Ok(());
        }
        self.elapsed += elapsed;
        self.server.update(elapsed);
        self.transport
            .update(elapsed, &mut self.server)
            .map_err(|e| NetError(e.to_string()))?;
        self.handle_connections();
        self.client_ids.clear();
        self.client_ids.extend(self.peers.keys().copied());
        for index in 0..self.client_ids.len() {
            let client = self.client_ids[index];
            if let Some(peer) = self.peers.get_mut(&client) {
                peer.control_tokens = (peer.control_tokens + elapsed.as_secs_f32() * 12.).min(24.);
                peer.input_tokens = (peer.input_tokens + elapsed.as_secs_f32() * 120.).min(256.);
            }
            self.receive_client(client);
        }
        self.expire_clients();
        self.sync_bots();
        let occupied = self
            .peers
            .values()
            .any(|peer| peer.entity.is_some() && peer.disconnect_at.is_none());
        let tick_time = Duration::from_secs_f64(1. / f64::from(TICK_RATE));
        // Transport and joins keep running; discard idle simulation time so
        // the first player does not inherit an empty arena's catch-up work.
        if occupied {
            self.accumulator += elapsed;
        } else {
            self.accumulator = Duration::ZERO;
        }
        let mut catchup = 0;
        while self.accumulator >= tick_time && catchup < 8 {
            self.accumulator -= tick_time;
            catchup += 1;
            let started = std::time::Instant::now();
            self.session.step();
            self.metrics
                .record_tick(started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
            self.replicate_events();
            if self.session.world.match_state.finished {
                self.finished_for += DT;
            }
            if self.finished_for >= self.settings.round_restart_seconds {
                self.restart_round();
            }
        }
        if self.accumulator >= tick_time {
            let skipped = (self.accumulator.as_secs_f64() / tick_time.as_secs_f64()) as u64;
            self.metrics.dropped_catchup_ticks += skipped;
            self.accumulator =
                Duration::from_secs_f64(self.accumulator.as_secs_f64() % tick_time.as_secs_f64());
            log::warn!("server overloaded: dropped {skipped} catch-up ticks");
        }
        self.snapshot_accumulator += elapsed;
        let snapshot_time = Duration::from_secs_f64(1. / f64::from(self.settings.snapshot_rate));
        if self.snapshot_accumulator >= snapshot_time {
            self.snapshot_accumulator = Duration::from_secs_f64(
                self.snapshot_accumulator.as_secs_f64() % snapshot_time.as_secs_f64(),
            );
            self.replicate_snapshots();
            self.pending_effects.clear();
        }
        self.transport.send_packets(&mut self.server);
        Ok(())
    }

    pub fn player_count(&self) -> usize {
        self.peers.values().filter(|p| p.entity.is_some()).count()
    }
    pub(crate) fn sync_bots(&mut self) {
        let humans = self
            .peers
            .values()
            .filter(|p| p.entity.is_some() && p.disconnect_at.is_none())
            .count();
        let desired = self.settings.desired_bots(humans);
        let world = &mut self.session.world;
        // Wave enemies are managed by co-op, not the arena population policy.
        let is_arena_bot = |tank: &&tankyard_core::Tank| {
            tank.bot && !(world.config.mode == Mode::CoopPve && tank.team == 2)
        };
        let current = world.tanks.iter().filter(is_arena_bot).count();
        if current > desired {
            let ids: Vec<_> = world
                .tanks
                .iter()
                .filter(is_arena_bot)
                .skip(desired)
                .map(|t| t.id)
                .collect();
            for id in ids {
                world.remove_player(id);
            }
        } else {
            for i in current..desired {
                world.add_player(&format!("Vector {:02}", i + 1), true);
            }
        }
    }
    pub fn player_id(&self, client: u64) -> Option<EntityId> {
        self.peers.get(&client)?.entity
    }
    pub fn set_account_verifier(
        &mut self,
        verifier: std::sync::Arc<dyn crate::access::AccountVerifier>,
    ) -> Result<(), NetError> {
        if self.player_count() > 0 || self.settings.netcode_private_key.is_none() {
            return Err(NetError(
                "configure account verification before players join, with authenticated transport"
                    .into(),
            ));
        }
        self.account_verifier = Some(verifier);
        Ok(())
    }
    pub fn set_global_authority(
        &mut self,
        gateway: std::sync::Arc<dyn crate::access::GlobalAuthority>,
    ) -> Result<(), NetError> {
        if self.settings.kind != ServerKind::Official
            || self.settings.netcode_private_key.is_none()
            || self.settings.global_audit_file.is_empty()
        {
            return Err(NetError(
                "global authority requires an official server, authenticated transport, and a private audit file".into(),
            ));
        }
        self.global_authority = Some(gateway);
        Ok(())
    }
    pub fn global_moderation(
        &mut self,
        credential: &str,
        action: crate::access::GlobalAction,
    ) -> Result<(), NetError> {
        let now = unix_time().as_secs();
        let staff = crate::access::authorize_global(
            self.settings.kind,
            self.global_authority.as_deref(),
            credential,
            &action,
            now,
        )
        .map_err(NetError)?;
        crate::access::audit(
            &self.settings.global_audit_file,
            &crate::access::AuditRecord {
                time: now,
                actor: &staff.account,
                action: "global_moderation",
                target: "backend",
                reason: &format!("{action:?}"),
            },
        )
        .map_err(NetError)?;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.global_authority
                .as_ref()
                .unwrap()
                .enforce(&staff, &action)
        }))
        .map_err(|_| NetError("global enforcement failed".into()))?
        .map_err(NetError)?;
        if let crate::access::GlobalAction::SuspendAccount { account, .. } = &action {
            for client in self.access.clients_for_account(account) {
                self.reject(client, "Account access is unavailable.");
            }
        }
        if let crate::access::GlobalAction::KickOfficialPlayer { client, reason } = action
            && self.peers.contains_key(&client)
        {
            self.reject(client, &reason);
        }
        Ok(())
    }

    fn handle_connections(&mut self) {
        while let Some(event) = self.server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    self.peers.insert(
                        client_id,
                        Peer {
                            entity: None,
                            connected_at: self.elapsed,
                            last_input_at: self.elapsed,
                            last_sequence: None,
                            control_tokens: 24.,
                            input_tokens: 256.,
                            disconnect_at: None,
                        },
                    );
                    let blocked = self
                        .transport
                        .client_addr(client_id)
                        .is_some_and(|address| {
                            self.bans.iter().any(|ban| {
                                ban.ip == address.ip() && ban.expires_at > unix_time().as_secs()
                            })
                        });
                    if blocked {
                        self.reject(client_id, "This address is temporarily banned.");
                    }
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    self.access.remove(client_id);
                    self.chat.remove(client_id);
                    if let Some(entity) = self.peers.remove(&client_id).and_then(|p| p.entity) {
                        self.session.world.remove_player(entity);
                        self.broadcast_control(&ServerControl::PlayerLeft { player: entity });
                    }
                    log::info!("client {client_id} disconnected: {reason}");
                }
            }
        }
    }

    fn receive_client(&mut self, client: u64) {
        if self
            .peers
            .get(&client)
            .is_none_or(|p| p.disconnect_at.is_some())
        {
            return;
        }
        for _ in 0..32 {
            let Some(bytes) = self.server.receive_message(client, CONTROL_CHANNEL) else {
                break;
            };
            if !self.take_token(client, true) {
                self.reject(client, "Control request rate exceeded.");
                return;
            }
            match decode_control(&bytes) {
                Ok(message) => self.handle_control(client, message),
                Err(_) => {
                    self.metrics.rejected_packets += 1;
                    self.reject(client, "Malformed control request.");
                    return;
                }
            }
            if self
                .peers
                .get(&client)
                .is_none_or(|p| p.disconnect_at.is_some())
            {
                return;
            }
        }
        let mut newest = None;
        for _ in 0..64 {
            let Some(bytes) = self.server.receive_message(client, REALTIME_CHANNEL) else {
                break;
            };
            if !self.take_token(client, false) {
                // Delayed UDP / WebSocket traffic can arrive in a burst. Drop
                // excess realtime state within the fixed work budget; never
                // disconnect a player because their packets arrived together.
                self.metrics.rejected_packets += 1;
                continue;
            }
            let mut input = match decode_input(&bytes) {
                Ok(input) => input,
                Err(_) => {
                    self.metrics.rejected_packets += 1;
                    self.reject(client, "Malformed player input.");
                    return;
                }
            };
            let Some(peer) = self.peers.get(&client) else {
                return;
            };
            if peer.entity.is_none() {
                continue;
            }
            let baseline = newest
                .map(|i: PlayerInput| i.sequence)
                .or(peer.last_sequence);
            if baseline.is_some_and(|last| {
                !sequence_newer(input.sequence, last) || input.sequence.wrapping_sub(last) > 4096
            }) {
                continue;
            }
            if let Some(previous) = newest {
                // A short click can share a delayed packet batch with its
                // release. Keep the tap while using the newest movement/aim.
                input.fire |= previous.fire;
                input.ability |= previous.ability;
            }
            newest = Some(input);
        }
        if let Some(input) = newest
            && let Some(peer) = self.peers.get_mut(&client)
            && let Some(entity) = peer.entity
        {
            // A fresh connection may start anywhere in the u32 sequence space.
            // Subsequent inputs still obey the wrap-aware bounded forward gap.
            if peer.last_sequence.is_none()
                && let Some(tank) = self.session.world.tanks.iter_mut().find(|t| t.id == entity)
            {
                tank.last_input = input.sequence;
            }
            if self.session.world.set_input(entity, input) {
                peer.last_sequence = Some(input.sequence);
                peer.last_input_at = self.elapsed;
            }
        }
    }

    fn take_token(&mut self, client: u64, control: bool) -> bool {
        let Some(peer) = self.peers.get_mut(&client) else {
            return false;
        };
        let tokens = if control {
            &mut peer.control_tokens
        } else {
            &mut peer.input_tokens
        };
        if *tokens < 1. {
            return false;
        }
        *tokens -= 1.;
        true
    }

    fn handle_control(&mut self, client: u64, message: ClientControl) {
        let entity = self.peers.get(&client).and_then(|p| p.entity);
        if let ClientControl::Hello {
            protocol,
            content: _,
            name,
            password,
            account_token,
        } = message
        {
            if entity.is_some() {
                self.reject(client, "Duplicate handshake.");
                return;
            }
            if protocol != PROTOCOL_VERSION {
                self.reject(client, "Incompatible protocol version.");
                return;
            }
            if password != self.settings.password {
                self.reject(client, "Incorrect server password.");
                return;
            }
            let account = if let Some(credential) = account_token {
                if self.settings.netcode_private_key.is_none() {
                    self.reject(client, "Accounts require authenticated transport.");
                    return;
                }
                let context = crate::access::AccountContext {
                    server_id: &self.settings.server_id,
                    client_id: client,
                    now: unix_time().as_secs(),
                    secure_transport: true,
                };
                match self
                    .account_verifier
                    .as_ref()
                    .ok_or("Account verification is unavailable.")
                    .and_then(|v| {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            v.verify(&credential, &context)
                        }))
                        .map_err(|_| "Account verification failed.")?
                        .map_err(|_| "Account verification failed.")
                    }) {
                    Ok(account) => Some(account),
                    Err(error) => {
                        self.reject(client, error);
                        return;
                    }
                }
            } else {
                None
            };
            if self.settings.require_accounts && account.is_none() {
                self.reject(client, "An account is required.");
                return;
            }
            if let Err(error) = self.access.add(client, account, unix_time().as_secs()) {
                self.reject(client, &error);
                return;
            }
            if self.chat.enabled() {
                let role = self.access.grant(client, unix_time().as_secs()).role;
                let _ = self.chat.join(client, "lobby", role);
            }
            let name = sanitize_name(&name);
            let player = self.session.world.add_player(&name, false);
            if let Some(peer) = self.peers.get_mut(&client) {
                peer.entity = Some(player);
            }
            self.welcome(client, player);
            self.broadcast_control(&ServerControl::PlayerJoined {
                player,
                name: name.clone(),
            });
            log::info!("{name} joined as entity {player} (client {client})");
            return;
        }
        let Some(player) = entity else {
            self.reject(
                client,
                "Complete the handshake before sending gameplay requests.",
            );
            return;
        };
        let permitted =
            self.settings.allow_sandbox && self.session.world.config.mode == Mode::Sandbox;
        let accepted = match message {
            ClientControl::Chat { room, text } => {
                let result = self.player_chat(client, room, text);
                self.community_result(client, result);
                return;
            }
            ClientControl::JoinRoom { room } => {
                let result = self.join_room(client, &room);
                self.community_result(client, result);
                return;
            }
            ClientControl::Admin { round, command } => {
                let result = if round == self.session.world.match_state.round {
                    self.server_command(client, command)
                } else {
                    Err(NetError("Match changed. Select the player again.".into()))
                };
                match result {
                    Ok(()) => self.send_control(
                        client,
                        &ServerControl::Action {
                            accepted: true,
                            reason: "Server command applied.".into(),
                        },
                    ),
                    Err(error) => self.community_result(client, Err(error)),
                }
                return;
            }
            ClientControl::UpgradeStat { stat, count } => {
                let mut accepted = false;
                for _ in 0..count {
                    if !self.session.world.upgrade_stat(player, usize::from(stat)) {
                        break;
                    }
                    accepted = true;
                }
                accepted
            }
            ClientControl::Evolve { class } => self.session.world.evolve(player, &class),
            ClientControl::SandboxLevel { level } => {
                permitted && self.session.world.sandbox_level(player, level)
            }
            ClientControl::SandboxClass { class } => {
                permitted && self.session.world.sandbox_class(player, &class)
            }
            ClientControl::SandboxInvulnerability { enabled } => {
                permitted && self.session.world.sandbox_invulnerability(player, enabled)
            }
            ClientControl::SandboxBoss { kind } => {
                if permitted && self.session.world.bosses.len() < 12 {
                    self.session.world.spawn_boss(kind);
                    true
                } else {
                    false
                }
            }
            ClientControl::Leave => {
                self.reject(client, "Disconnected.");
                return;
            }
            ClientControl::Hello { .. } => unreachable!(),
        };
        self.send_control(
            client,
            &ServerControl::Action {
                accepted,
                reason: if accepted {
                    String::new()
                } else {
                    "Upgrade is locked, invalid, or not permitted by this server.".into()
                },
            },
        );
        // Evolution events are emitted by validated control requests between
        // simulation steps; publish them before World::step clears its events.
        self.replicate_events();
    }

    fn welcome(&mut self, client: u64, player: EntityId) {
        self.send_control_bytes(client, self.game_data.clone());
        self.send_control(
            client,
            &ServerControl::Welcome {
                protocol: PROTOCOL_VERSION,
                content: self.content_hash,
                player,
                round: self.session.world.match_state.round,
                server_name: self.settings.name.clone(),
                sandbox_allowed: self.settings.allow_sandbox
                    && self.settings.world.mode == Mode::Sandbox,
                tick_rate: TICK_RATE,
            },
        );
        self.send_access(client);
        self.send_rooms(client);
        self.send_control(
            client,
            &ServerControl::MatchSettings {
                settings: self.match_settings(),
            },
        );
    }
    pub(crate) fn send_access(&mut self, client: u64) {
        let grant = self.access.grant(client, unix_time().as_secs());
        self.send_control(
            client,
            &ServerControl::Access {
                kind: self.settings.kind,
                chat_enabled: self.settings.kind == ServerKind::Community
                    && self.settings.chat_enabled,
                role: grant.role,
                permissions: grant.permissions,
            },
        );
    }
    pub fn reload_mods(&mut self) -> Result<(), NetError> {
        let loaded = tankyard_mods::load(&self.settings.mods).map_err(NetError)?;
        let mut candidate = LocalSession::with_mods(
            self.settings.world.clone(),
            self.settings.bot_count,
            loaded.clone(),
        )
        .map_err(NetError)?;
        for factory in &self.native_plugins {
            candidate.install_plugin(factory()).map_err(NetError)?;
        }
        let data = encode_game_data_message(&candidate.bundle)?;
        self.loaded_mods = loaded;
        self.game_data = data;
        self.restart_round();
        Ok(())
    }
    /// Trusted Rust extensions are registered before accepting players. The
    /// factory recreates them for each round; data hooks use runtime manifests.
    pub fn register_plugin(
        &mut self,
        factory: fn() -> Box<dyn tankyard_core::modding::ServerPlugin>,
    ) -> Result<(), NetError> {
        if self.player_count() > 0 {
            return Err(NetError("register plugins before players connect".into()));
        }
        self.session.install_plugin(factory()).map_err(NetError)?;
        self.game_data = encode_game_data_message(&self.session.bundle)?;
        self.content_hash = self.session.bundle.fingerprint();
        self.native_plugins.push(factory);
        Ok(())
    }

    fn expire_clients(&mut self) {
        let now = unix_time().as_secs();
        if self.elapsed.as_secs() != self.accounts_checked_at {
            self.accounts_checked_at = self.elapsed.as_secs();
            if let Some(verifier) = &self.account_verifier {
                let revoked: Vec<_> = self
                    .access
                    .accounts()
                    .into_iter()
                    .filter_map(|(client, account)| {
                        let context = crate::access::AccountContext {
                            server_id: &self.settings.server_id,
                            client_id: client,
                            now,
                            secure_transport: self.settings.netcode_private_key.is_some(),
                        };
                        let current =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                verifier.is_current(&account, &context)
                            }))
                            .unwrap_or(false);
                        (!current).then_some(client)
                    })
                    .collect();
                for client in revoked {
                    self.reject(client, "Account authorization expired or was revoked.");
                }
            }
        }
        for client in self.access.expire(now) {
            self.chat.refresh_role(client, ServerRole::Player);
            self.send_access(client);
            self.send_rooms(client);
        }
        for index in 0..self.client_ids.len() {
            let client = self.client_ids[index];
            let Some(peer) = self.peers.get(&client) else {
                continue;
            };
            if let Some(deadline) = peer.disconnect_at {
                if self.elapsed >= deadline {
                    self.server.disconnect(client);
                }
            } else if peer.entity.is_some()
                && self.settings.require_accounts
                && !self.access.account_valid(client, now)
            {
                self.reject(client, "Account authorization expired.");
            } else if peer.entity.is_none()
                && self.elapsed.saturating_sub(peer.connected_at).as_secs_f32()
                    > self.settings.handshake_timeout
            {
                self.reject(client, "Application handshake timed out.");
            } else if let Some(entity) = peer.entity
                && self
                    .elapsed
                    .saturating_sub(peer.last_input_at)
                    .as_secs_f32()
                    > self.settings.input_timeout
                && let Some(tank) = self.session.world.tanks.iter_mut().find(|t| t.id == entity)
            {
                tank.input.movement = V2::ZERO;
                tank.input.fire = false;
                tank.input.alt_fire = false;
                tank.input.ability = false;
            }
        }
    }

    fn replicate_snapshots(&mut self) {
        let started = std::time::Instant::now();
        let effect_radius = (self.settings.interest_radius + 300.).powi(2);
        for (&client, peer) in &self.peers {
            if peer.disconnect_at.is_some() {
                continue;
            }
            let Some(entity) = peer.entity else {
                continue;
            };
            let Some(tank) = self
                .session
                .world
                .tanks
                .iter()
                .find(|tank| tank.id == entity)
            else {
                continue;
            };
            let center = tank.pos;
            let snapshot = &mut self.snapshot_buffer;
            self.session
                .world
                .snapshot_for_into(snapshot, center, self.settings.interest_radius);
            snapshot.events.clear();
            snapshot.events.extend(
                self.pending_effects
                    .iter()
                    .filter(|e| (e.pos - center).length_squared() < effect_radius)
                    .take(2048)
                    .cloned(),
            );
            let bytes = match bounded_snapshot(snapshot, center, &mut self.snapshot_encoder) {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.metrics.snapshots_skipped += 1;
                    log::warn!("snapshot omitted for client {client}: {e}");
                    continue;
                }
            };
            if self
                .server
                .can_send_message(client, REALTIME_CHANNEL, bytes.len())
            {
                self.metrics.snapshots_sent += 1;
                self.metrics.snapshot_bytes += bytes.len() as u64;
                self.server.send_message(client, REALTIME_CHANNEL, bytes);
            } else {
                self.metrics.snapshots_skipped += 1;
            }
        }
        self.metrics.last_snapshot_micros =
            started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.metrics.max_snapshot_micros = self
            .metrics
            .max_snapshot_micros
            .max(self.metrics.last_snapshot_micros);
    }

    fn replicate_events(&mut self) {
        self.important_events.clear();
        for event in self.session.world.events.drain(..) {
            match event.kind {
                EventKind::LevelUp
                | EventKind::Evolve
                | EventKind::Capture
                | EventKind::Victory
                | EventKind::BossSpawn
                | EventKind::Kill => self.important_events.push(event),
                _ if self.pending_effects.len() < 4096 => self.pending_effects.push(event),
                _ => {}
            }
        }
        if self.important_events.is_empty() {
            return;
        }
        let packets = match encode_event_batches(
            self.session.world.match_state.round,
            &self.important_events,
        ) {
            Ok(packets) => packets,
            Err(_) => {
                log::error!("Unable to serialize a reliable event batch");
                return;
            }
        };
        let clients: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(&id, p)| (p.entity.is_some() && p.disconnect_at.is_none()).then_some(id))
            .collect();
        for bytes in packets {
            let bytes = Bytes::from(bytes);
            for &client in &clients {
                if self
                    .peers
                    .get(&client)
                    .is_some_and(|p| p.disconnect_at.is_some())
                {
                    continue;
                }
                if self
                    .server
                    .can_send_message(client, EVENT_CHANNEL, bytes.len())
                {
                    self.server
                        .send_message(client, EVENT_CHANNEL, bytes.clone());
                } else {
                    self.reject(client, "Client cannot keep up with reliable events.");
                }
            }
        }
    }

    pub(crate) fn send_control(&mut self, client: u64, message: &ServerControl) {
        if let Ok(bytes) = encode(message, MAX_SERVER_CONTROL_BYTES) {
            self.send_control_bytes(client, bytes.into());
        }
    }
    fn send_control_bytes(&mut self, client: u64, bytes: Bytes) {
        if self
            .server
            .can_send_message(client, CONTROL_CHANNEL, bytes.len())
        {
            self.server.send_message(client, CONTROL_CHANNEL, bytes);
        } else {
            self.server.disconnect(client);
        }
    }
    pub(crate) fn broadcast_control(&mut self, message: &ServerControl) {
        let Ok(bytes) = encode(message, MAX_SERVER_CONTROL_BYTES) else {
            return;
        };
        let bytes = Bytes::from(bytes);
        for (&client, peer) in &self.peers {
            if peer.entity.is_some() {
                if self
                    .server
                    .can_send_message(client, CONTROL_CHANNEL, bytes.len())
                {
                    self.server
                        .send_message(client, CONTROL_CHANNEL, bytes.clone());
                } else {
                    self.server.disconnect(client);
                }
            }
        }
    }
    pub(crate) fn reject(&mut self, client: u64, reason: &str) {
        self.access.remove(client);
        self.chat.remove(client);
        self.send_control(
            client,
            &ServerControl::Rejected {
                reason: reason.into(),
            },
        );
        if let Some(peer) = self.peers.get_mut(&client) {
            peer.disconnect_at = Some(self.elapsed + Duration::from_millis(250));
        }
    }

    pub(crate) fn restart_round(&mut self) {
        let players: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(&client, peer)| {
                if peer.disconnect_at.is_some() {
                    return None;
                }
                let tank = self
                    .session
                    .world
                    .tanks
                    .iter()
                    .find(|tank| Some(tank.id) == peer.entity)?;
                Some((client, tank.name.clone()))
            })
            .collect();
        let tick = self.session.world.tick;
        let mut config = self.settings.world.clone();
        config.seed = config
            .seed
            .wrapping_add(u64::from(self.session.world.match_state.round));
        let round = self.session.world.match_state.round.saturating_add(1);
        self.session = LocalSession::with_mods(
            config,
            self.settings.desired_bots(players.len()),
            self.loaded_mods.clone(),
        )
        .expect("previously validated mod pack");
        self.session.bots.difficulty = self.settings.difficulty;
        for factory in &self.native_plugins {
            self.session
                .install_plugin(factory())
                .expect("previously registered native plugin");
        }
        self.content_hash = self.session.bundle.fingerprint();
        self.game_data =
            encode_game_data_message(&self.session.bundle).expect("validated game data");
        self.session.world.tick = tick.saturating_add(1);
        self.session.world.match_state.round = round;
        self.finished_for = 0.;
        self.pending_effects.clear();
        for (client, name) in players {
            let player = self.session.world.add_player(&name, false);
            if let Some(peer) = self.peers.get_mut(&client) {
                peer.entity = Some(player);
                peer.last_sequence = None;
            }
            self.welcome(client, player);
        }
    }

    pub fn shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.broadcast_control(&ServerControl::Shutdown {
            reason: "Server is shutting down.".into(),
        });
        self.transport.send_packets(&mut self.server);
        self.transport.disconnect_all(&mut self.server);
        self.shutting_down = true;
    }
}

// Keeping the high-priority stuff in small ordered chunks means a crowded
// fight can't silently erase every kill and capture event in one huge blob.
fn encode_event_batches(round: u32, events: &[GameEvent]) -> Result<Vec<Vec<u8>>, DecodeError> {
    const MAX_EVENTS_PER_BATCH: usize = 128;
    events
        .chunks(MAX_EVENTS_PER_BATCH)
        .map(|events| {
            encode(
                &EventMessage {
                    round,
                    events: events.to_vec(),
                },
                MAX_EVENT_BYTES,
            )
        })
        .collect()
}

/// Overloaded arenas still send useful state. Keep nearby entities and reduce
/// the payload until it fits; one fixed truncation could still exceed the cap.
fn bounded_snapshot(
    snapshot: &mut tankyard_core::Snapshot,
    center: V2,
    encoder: &mut SnapshotEncoder,
) -> Result<Vec<u8>, DecodeError> {
    if let Ok(bytes) = encoder.encode(snapshot) {
        return Ok(bytes);
    }
    snapshot.projectiles.sort_unstable_by(|a, b| {
        (a.pos - center)
            .length_squared()
            .total_cmp(&(b.pos - center).length_squared())
            .then_with(|| a.id.cmp(&b.id))
    });
    snapshot.targets.sort_unstable_by(|a, b| {
        (a.pos - center)
            .length_squared()
            .total_cmp(&(b.pos - center).length_squared())
            .then_with(|| a.id.cmp(&b.id))
    });
    snapshot.events.truncate(128);
    loop {
        let previous = snapshot.projectiles.len() + snapshot.targets.len();
        snapshot
            .projectiles
            .truncate(snapshot.projectiles.len() / 2);
        snapshot.targets.truncate(snapshot.targets.len() / 2);
        if let Ok(bytes) = encoder.encode(snapshot) {
            return Ok(bytes);
        }
        if previous == 0 {
            return Err(DecodeError("essential state exceeds snapshot size limit"));
        }
    }
}

fn encode_game_data_message(bundle: &tankyard_mods::GameBundle) -> Result<Bytes, NetError> {
    let message = game_data(bundle).map_err(|e| NetError(e.to_string()))?;
    encode(&message, MAX_SERVER_CONTROL_BYTES)
        .map(Bytes::from)
        .map_err(|e| NetError(e.to_string()))
}
impl Drop for UdpServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_reliable_event_bursts_are_batched_without_losing_order() {
        let events: Vec<_> = (0..2048)
            .map(|n| GameEvent {
                tick: n,
                kind: EventKind::Kill,
                pos: V2::new(30.0, 40.0),
                subject: n + 1,
                value: 9,
                team: 2,
            })
            .collect();
        let batches = encode_event_batches(7, &events).unwrap();
        assert!(batches.len() > 1);
        assert!(batches.iter().all(|bytes| bytes.len() <= MAX_EVENT_BYTES));
        let decoded: Vec<GameEvent> = batches
            .iter()
            .flat_map(|bytes| {
                let message: EventMessage = decode(bytes, MAX_EVENT_BYTES).unwrap();
                assert_eq!(message.round, 7);
                message.events
            })
            .collect();
        assert_eq!(decoded.len(), events.len());
        assert!(
            decoded
                .iter()
                .enumerate()
                .all(|(i, e)| e.subject == (i as u64 + 1))
        );
    }

    #[test]
    fn timing_samples_are_bounded_and_percentiles_are_measured() {
        let mut metrics = ServerMetrics::default();
        for micros in 1..=100 {
            metrics.record_tick(micros);
        }
        let timing = metrics.tick_timing();
        assert_eq!(timing.samples, 100);
        assert_eq!(timing.average_micros, 50.5);
        assert_eq!(timing.p95_micros, 95);
        assert_eq!(timing.p99_micros, 99);
        for _ in 0..10000 {
            metrics.record_tick(7);
        }
        assert_eq!(metrics.tick_samples.len(), 4096);
        assert_eq!(metrics.tick_timing().p99_micros, 7);
        assert_eq!(metrics.max_tick_micros, 100);
    }
}

#[cfg(test)]
mod snapshot_regressions {
    use super::*;
    use tankyard_content::{Content, ProjectileKind};
    use tankyard_core::{Projectile, World, WorldConfig};

    #[test]
    fn overloaded_snapshots_preserve_players_and_nearest_projectiles() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let player = world.add_player("Player", false);
        let mut snapshot = world.snapshot();
        let center = V2::new(1000., 1000.);
        for n in 0..6000 {
            snapshot.projectiles.push(Projectile {
                id: n + 10000,
                owner: player,
                team: 0,
                pos: V2::new(100. + (n % 100) as f32 * 20., 100. + (n / 100) as f32 * 20.),
                previous: center,
                vel: V2::ZERO,
                cruise_speed: 0.,
                radius: 5.,
                damage: 20.,
                penetration: 3.,
                lifetime: 1.,
                kind: ProjectileKind::Bullet,
                ricochets: 0,
                age: 0.,
                hit: (0..24).map(|n| u64::MAX - n).collect(),
                gun: None,
                homing_target: 0,
            });
        }
        assert!(encode_snapshot(&snapshot).is_err());
        let bytes =
            bounded_snapshot(&mut snapshot, center, &mut SnapshotEncoder::default()).unwrap();
        let decoded = decode_snapshot(&bytes).unwrap();
        assert_eq!(decoded.tanks[0].id, player);
        assert!(!decoded.projectiles.is_empty() && decoded.projectiles.len() < 6000);
        assert!(
            decoded
                .projectiles
                .windows(2)
                .all(|pair| (pair[0].pos - center).length_squared()
                    <= (pair[1].pos - center).length_squared())
        );
    }
}
