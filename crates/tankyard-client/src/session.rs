use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};
use tankyard_client::achievements::Tracker;
use tankyard_content::Content;
use tankyard_core::{
    BossKind, DT, EntityId, GameEvent, Mode, PlayerInput, Snapshot, Tank, V2, WorldConfig,
    physics::predict_tank,
};
use tankyard_server::{HostedServer, LocalSession, NetworkClient, ServerSettings};

pub enum Engine {
    Offline(Box<LocalSession>),
    Online {
        client: Box<NetworkClient>,
        host: Option<HostedServer>,
    },
}
pub struct Session {
    pub practice: Option<tankyard_client::practice::Run>,
    pub engine: Engine,
    pub player: EntityId,
    pub display: Option<Snapshot>,
    pub failure: Option<String>,
    pub notice: Option<String>,
    pub events: Vec<GameEvent>,
    predicted: Option<Tank>,
    predicted_tick: u64,
    pending: VecDeque<PlayerInput>,
    correction: V2,
    received_tick: Option<(u32, u64)>,
    received_at: f64,
    render_time: f32,
    pub previous_positions: HashMap<EntityId, (V2, f32)>,
    pub ticks: u64,
    pub dropped_ticks: u64,
    pub achievement_tracker: Tracker,
    pub bundle: tankyard_mods::GameBundle,
    pub bundle_revision: u64,
    owner_registered: bool,
    host_request: Option<std::sync::mpsc::Receiver<String>>,
}
impl Session {
    pub fn practice(index: usize, name: &str, seed: u64) -> Result<Self, String> {
        let challenge = *tankyard_client::practice::CHALLENGES
            .get(index)
            .ok_or("Choose a practice challenge.")?;
        let mut local = LocalSession::new(challenge.config(seed), challenge.bots);
        let player = local.world.add_player(name, false);
        if local.world.config.mode == tankyard_core::Mode::Sandbox {
            let ids = local
                .world
                .tanks
                .iter()
                .map(|tank| tank.id)
                .collect::<Vec<_>>();
            for id in ids {
                local.world.sandbox_class(id, challenge.class);
                local.world.sandbox_level(id, challenge.level);
            }
        }
        if challenge.id == "drone_drill" {
            for index in [6, 5, 4, 3, 7, 1, 0] {
                while local.world.upgrade_stat(player, index) {}
            }
        }
        let snapshot = local.world.snapshot();
        let run = tankyard_client::practice::Run::new(index, &snapshot, player)
            .ok_or("Could not start practice.")?;
        let mut session = Self::create(Engine::Offline(Box::new(local)), player, Some(snapshot));
        session.practice = Some(run);
        Ok(session)
    }
    pub fn offline(config: WorldConfig, bots: usize, name: &str) -> Self {
        let mut local = LocalSession::new(config, bots);
        let id = local.world.add_player(name, false);
        let snapshot = local.world.snapshot();
        Self::create(Engine::Offline(Box::new(local)), id, Some(snapshot))
    }
    pub fn offline_modded(
        config: WorldConfig,
        bots: usize,
        name: &str,
        paths: &[std::path::PathBuf],
    ) -> Result<Self, String> {
        let loaded = tankyard_mods::load(paths)?;
        let mut local = LocalSession::with_mods(config, bots, loaded)?;
        let id = local.world.add_player(name, false);
        let snapshot = local.world.snapshot();
        Ok(Self::create(
            Engine::Offline(Box::new(local)),
            id,
            Some(snapshot),
        ))
    }
    pub fn join(address: &str, name: &str, password: &str) -> Result<Self, String> {
        let client = NetworkClient::connect(address, name, password).map_err(|e| e.to_string())?;
        Ok(Self::create(
            Engine::Online {
                client: Box::new(client),
                host: None,
            },
            0,
            None,
        ))
    }
    pub fn join_token(
        path: &std::path::Path,
        name: &str,
        password: &str,
        credential: Option<String>,
    ) -> Result<Self, String> {
        let token =
            tankyard_server::accounts::load_connection_token(path).map_err(|e| e.to_string())?;
        let client = NetworkClient::connect_authenticated(token, name, password, credential)
            .map_err(|e| e.to_string())?;
        Ok(Self::create(
            Engine::Online {
                client: Box::new(client),
                host: None,
            },
            0,
            None,
        ))
    }
    pub fn host(
        config: WorldConfig,
        bots: usize,
        name: &str,
        password: &str,
        mods: Vec<std::path::PathBuf>,
        difficulty: &str,
    ) -> Result<Self, String> {
        let sandbox = config.mode == tankyard_core::Mode::Sandbox;
        let settings = ServerSettings {
            world: config,
            bot_count: bots,
            password: password.into(),
            name: format!("{}'s arena", name.chars().take(21).collect::<String>()),
            allow_sandbox: sandbox,
            difficulty: tankyard_server::difficulty_from_name(difficulty)?,
            mods,
            ..Default::default()
        };
        let host = HostedServer::start(settings).map_err(|e| e.to_string())?;
        let address = format!("127.0.0.1:{}", host.address.port());
        let client = NetworkClient::connect(&address, name, password).map_err(|e| e.to_string())?;
        Ok(Self::create(
            Engine::Online {
                client: Box::new(client),
                host: Some(host),
            },
            0,
            None,
        ))
    }
    fn create(engine: Engine, player: EntityId, display: Option<Snapshot>) -> Self {
        let bundle = match &engine {
            Engine::Offline(local) => local.bundle.clone(),
            Engine::Online { client, .. } => client.bundle.clone(),
        };
        let bundle_revision = bundle.fingerprint();
        let mut achievement_tracker = Tracker::default();
        if let Engine::Offline(local) = &engine {
            achievement_tracker.observe_world(&local.world, player);
        }
        Self {
            practice: None,
            engine,
            player,
            display,
            failure: None,
            notice: None,
            events: Vec::with_capacity(512),
            predicted: None,
            predicted_tick: 0,
            pending: VecDeque::with_capacity(256),
            correction: V2::ZERO,
            received_tick: None,
            received_at: 0.0,
            render_time: 0.0,
            previous_positions: HashMap::new(),
            ticks: 0,
            dropped_ticks: 0,
            achievement_tracker,
            bundle,
            bundle_revision,
            owner_registered: false,
            host_request: None,
        }
    }
    pub fn offline_mode(&self) -> bool {
        matches!(self.engine, Engine::Offline(_))
    }
    pub fn flush_network(&mut self) {
        if let Engine::Online { client, .. } = &mut self.engine
            && let Err(error) = client.flush()
        {
            self.failure = Some(error.to_string());
        }
    }
    pub fn update_network(&mut self, dt: f32, now: f64, _content: &Content) {
        let Engine::Online { client, host } = &mut self.engine else {
            return;
        };
        if let Some(error) = host.as_ref().and_then(HostedServer::error) {
            self.failure = Some(error);
        }
        if let Err(error) = client.update(Duration::from_secs_f32(dt)) {
            self.failure = Some(error.to_string());
        }
        if self.bundle_revision != client.bundle_revision {
            self.bundle.clone_from(&client.bundle);
            self.bundle_revision = client.bundle_revision;
        }
        let content = &self.bundle.content;
        if let Some(error) = &client.error {
            self.failure = Some(error.clone());
        }
        if let Some(reason) = client.last_action.take() {
            self.notice = Some(reason);
        }
        self.events.extend(
            client
                .events
                .iter()
                .take(4096_usize.saturating_sub(self.events.len()))
                .cloned(),
        );
        let Some(snapshot) = client.player_snapshot() else {
            self.achievement_tracker.suspend();
            return;
        };
        if let Some(id) = client.player_id {
            if !self.owner_registered
                && let Some(host) = host
            {
                self.owner_registered = true;
                match host.command_async(&format!("owner-client {}", client.client_id())) {
                    Ok(reply) => self.host_request = Some(reply),
                    Err(error) => self.notice = Some(error.to_string()),
                }
            }
            if let Some(receiver) = self.host_request.take() {
                match receiver.try_recv() {
                    Ok(reply) => {
                        if reply.starts_with("Error:") {
                            self.notice = Some(reply);
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => self.host_request = Some(receiver),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.notice = Some("Host command was interrupted.".into())
                    }
                }
            }
            if self.player != id {
                self.pending.clear();
                self.predicted = None;
                self.correction = V2::ZERO;
            }
            self.player = id;
        }
        if Some((snapshot.match_state.round, snapshot.tick)) != self.received_tick {
            if self
                .display
                .as_ref()
                .is_some_and(|previous| previous.match_state.round != snapshot.match_state.round)
            {
                self.pending.clear();
                self.predicted = None;
                self.correction = V2::ZERO;
            }
            self.events.extend(
                snapshot
                    .events
                    .iter()
                    .take(4096_usize.saturating_sub(self.events.len()))
                    .cloned(),
            );
            self.received_tick = Some((snapshot.match_state.round, snapshot.tick));
            self.received_at = now;
            self.previous_positions.clear();
            if let Some(previous) = &client.previous_snapshot
                && previous.match_state.round == snapshot.match_state.round
            {
                for t in previous.tanks.iter().filter(|t| t.alive) {
                    self.previous_positions.insert(t.id, (t.pos, t.aim));
                }
                for p in &previous.projectiles {
                    self.previous_positions.insert(p.id, (p.pos, p.vel.angle()));
                }
                for target in &previous.targets {
                    self.previous_positions
                        .insert(target.id, (target.pos, target.angle));
                }
                for boss in &previous.bosses {
                    self.previous_positions
                        .insert(boss.id, (boss.pos, boss.aim));
                }
            }
            if let Some(authoritative) = snapshot.tanks.iter().find(|t| t.id == self.player) {
                self.achievement_tracker
                    .observe(snapshot, self.player, content, 0, 0);
                while self.pending.front().is_some_and(|input| {
                    authoritative.last_input.wrapping_sub(input.sequence) < 0x80000000
                }) {
                    self.pending.pop_front();
                }
                let old = self
                    .predicted
                    .as_ref()
                    .filter(|t| t.alive == authoritative.alive)
                    .map(|t| t.pos + self.correction)
                    .unwrap_or(authoritative.pos);
                let mut predicted = authoritative.clone();
                self.predicted_tick = snapshot.tick;
                for input in &self.pending {
                    predicted.input = *input;
                    self.predicted_tick += 1;
                    predict_tank(
                        &mut predicted,
                        content,
                        &snapshot.config,
                        &snapshot.walls,
                        self.predicted_tick,
                        DT,
                    );
                }
                let error = old - predicted.pos;
                self.correction = if error.length() < 150.0 {
                    error
                } else {
                    V2::ZERO
                };
                self.predicted = Some(predicted);
            }
            self.display = Some(snapshot.clone());
        }
        self.correction *= (-12.0 * dt).exp();
        let Some(display) = &mut self.display else {
            return;
        };
        let interval = client
            .previous_snapshot
            .as_ref()
            .map(|previous| snapshot.tick.saturating_sub(previous.tick).max(1) as f64 / 60.0)
            .unwrap_or(0.05);
        let alpha = ((now - self.received_at) / interval).clamp(0.0, 1.0) as f32;
        let extrapolation = ((now - self.received_at - interval) as f32).clamp(0.0, 0.075);
        self.render_time = client
            .previous_snapshot
            .as_ref()
            .filter(|previous| previous.match_state.round == snapshot.match_state.round)
            .map_or(snapshot.tick as f32 * DT, |previous| {
                previous.tick as f32 * DT
                    + snapshot.tick.saturating_sub(previous.tick) as f32 * DT * alpha
            })
            + extrapolation;
        for (tank, latest) in display.tanks.iter_mut().zip(&snapshot.tanks) {
            if tank.id == self.player {
                if let Some(predicted) = &self.predicted {
                    tank.pos = predicted.pos + self.correction;
                    tank.aim = predicted.aim;
                    tank.vel = predicted.vel;
                }
            } else if let Some((previous, angle)) = self.previous_positions.get(&tank.id) {
                tank.pos = extrapolate_position(
                    interpolate_position(*previous, latest.pos, alpha),
                    latest.vel,
                    extrapolation,
                    latest.radius,
                    snapshot,
                );
                tank.aim = lerp_angle(*angle, latest.aim, alpha);
            }
        }
        for (projectile, latest) in display.projectiles.iter_mut().zip(&snapshot.projectiles) {
            if let Some((previous, _)) = self.previous_positions.get(&projectile.id) {
                projectile.pos = extrapolate_position(
                    previous.lerp(latest.pos, alpha),
                    latest.vel,
                    extrapolation,
                    latest.radius,
                    snapshot,
                );
            }
        }
        for (target, latest) in display.targets.iter_mut().zip(&snapshot.targets) {
            if let Some((previous, angle)) = self.previous_positions.get(&target.id) {
                target.pos = extrapolate_position(
                    interpolate_position(*previous, latest.pos, alpha),
                    latest.vel,
                    extrapolation,
                    latest.radius,
                    snapshot,
                );
                target.angle = lerp_angle(*angle, latest.angle, alpha);
            }
        }
        for (boss, latest) in display.bosses.iter_mut().zip(&snapshot.bosses) {
            if let Some((previous, _)) = self.previous_positions.get(&boss.id) {
                boss.pos = extrapolate_position(
                    interpolate_position(*previous, latest.pos, alpha),
                    latest.vel,
                    extrapolation,
                    latest.radius,
                    snapshot,
                );
            }
        }
    }
    pub fn tick(&mut self, input: PlayerInput, content: &Content) {
        if self.practice.as_ref().is_some_and(|run| run.done()) {
            return;
        }
        match &mut self.engine {
            Engine::Offline(local) => {
                self.achievement_tracker
                    .observe_world(&local.world, self.player);
                self.previous_positions.clear();
                for tank in local.world.tanks.iter().filter(|t| t.alive) {
                    self.previous_positions
                        .insert(tank.id, (tank.pos, tank.aim));
                }
                for target in &local.world.targets {
                    self.previous_positions
                        .insert(target.id, (target.pos, target.angle));
                }
                for boss in &local.world.bosses {
                    self.previous_positions
                        .insert(boss.id, (boss.pos, boss.aim));
                }
                local.world.set_input(self.player, input);
                local.step();
                self.achievement_tracker
                    .observe_world(&local.world, self.player);
                self.events.extend(
                    local
                        .world
                        .events
                        .iter()
                        .take(4096_usize.saturating_sub(self.events.len()))
                        .cloned(),
                );
                if let Some(display) = &mut self.display {
                    local.world.snapshot_into(display);
                } else {
                    self.display = Some(local.world.snapshot());
                }
                if let (Some(run), Some(snapshot)) = (&mut self.practice, &self.display) {
                    run.observe(snapshot);
                }
            }
            Engine::Online { client, .. } => {
                if client.send_input(input) {
                    self.pending.push_back(input);
                    if self.pending.len() > 256 {
                        self.pending.pop_front();
                    }
                    if let (Some(predicted), Some(snapshot)) =
                        (&mut self.predicted, &client.snapshot)
                    {
                        predicted.input = input;
                        self.predicted_tick += 1;
                        predict_tank(
                            predicted,
                            content,
                            &snapshot.config,
                            &snapshot.walls,
                            self.predicted_tick,
                            DT,
                        );
                    }
                }
            }
        }
        self.ticks += 1;
    }
    /// Show the latest local prediction in the same frame as its input.
    pub fn present_prediction(&mut self, fraction: f32) {
        if self.offline_mode() {
            return;
        }
        if let (Some(predicted), Some(display)) = (&self.predicted, &mut self.display)
            && let Some(tank) = display.tanks.iter_mut().find(|t| t.id == self.player)
        {
            let mut position =
                predicted.pos + self.correction + predicted.vel * (DT * fraction.clamp(0.0, 1.0));
            let mut contact: f32 = 1.0;
            for &wall in &display.walls {
                if let Some((time, _)) =
                    tankyard_core::physics::sweep_rect(predicted.pos, position, tank.radius, wall)
                {
                    contact = contact.min(time);
                }
            }
            position = predicted.pos.lerp(position, contact);
            tank.pos = V2::new(
                position
                    .x
                    .clamp(tank.radius, display.config.width - tank.radius),
                position
                    .y
                    .clamp(tank.radius, display.config.height - tank.radius),
            );
            tank.aim = predicted.aim;
            tank.vel = predicted.vel;
            tank.ability_cooldown = predicted.ability_cooldown;
            tank.boost = predicted.boost;
            tank.shield = predicted.shield;
            tank.protection = predicted.protection;
            tank.spawn_immunity = predicted.spawn_immunity;
            tank.barrel_shots.clone_from(&predicted.barrel_shots);
            tank.cooldowns.clone_from(&predicted.cooldowns);
            tank.charge = predicted.charge;
            tank.input = predicted.input;
        }
    }
    pub fn render_fraction(&self, fraction: f32) -> f32 {
        if self.offline_mode() {
            fraction.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
    pub fn animation_time(&self, fraction: f32) -> f32 {
        if self.offline_mode() {
            self.display.as_ref().map_or(0.0, |snapshot| {
                (snapshot.tick.saturating_sub(1) as f32 + fraction.clamp(0.0, 1.0)) * DT
            })
        } else {
            self.render_time
        }
    }
    pub fn local_animation_time(&self, fraction: f32) -> f32 {
        if self.offline_mode() {
            self.animation_time(fraction)
        } else {
            (self.predicted_tick as f32 + fraction.clamp(0.0, 1.0)) * DT
        }
    }
    pub fn neutral_input(&self, sequence: u32) -> PlayerInput {
        let aim = self
            .display
            .as_ref()
            .and_then(|snapshot| snapshot.tanks.iter().find(|tank| tank.id == self.player))
            .map_or(0.0, |tank| tank.aim);
        PlayerInput {
            sequence,
            aim,
            ..Default::default()
        }
    }
    pub fn position(&self, id: EntityId, current: V2, fraction: f32) -> V2 {
        self.previous_positions
            .get(&id)
            .map_or(current, |&(previous, _)| {
                interpolate_position(previous, current, self.render_fraction(fraction))
            })
    }
    pub fn upgrade_stat(&mut self, index: usize, fill: bool) {
        let count = self
            .display
            .as_ref()
            .and_then(|s| s.tanks.iter().find(|t| t.id == self.player && t.alive))
            .and_then(|t| {
                t.stats
                    .get(index)
                    .map(|spent| 7_u8.saturating_sub(*spent).min(t.points))
            })
            .unwrap_or(0)
            .min(if fill { 7 } else { 1 });
        if count == 0 {
            return;
        }
        match &mut self.engine {
            Engine::Offline(local) => {
                for _ in 0..count {
                    local.world.upgrade_stat(self.player, index);
                }
                self.achievement_tracker
                    .observe_world(&local.world, self.player);
                self.display = Some(local.world.snapshot());
            }
            Engine::Online { client, .. } => {
                client.upgrade_stat_amount(index, count);
            }
        }
    }
    pub fn evolve(&mut self, class: &str) {
        match &mut self.engine {
            Engine::Offline(local) => {
                if local.world.evolve(self.player, class) {
                    self.achievement_tracker
                        .observe_world(&local.world, self.player);
                    self.events.extend(local.world.events.last().cloned());
                    self.display = Some(local.world.snapshot());
                }
            }
            Engine::Online { client, .. } => {
                client.evolve(class);
            }
        }
    }
    pub fn sandbox_level(&mut self, level: u8) {
        if self.practice.is_some() {
            return;
        }
        match &mut self.engine {
            Engine::Offline(local) => {
                local.world.sandbox_level(self.player, level);
                self.display = Some(local.world.snapshot());
            }
            Engine::Online { client, .. } => {
                client.sandbox_level(level);
            }
        }
    }
    pub fn sandbox_adjust_level(&mut self, step: i16) {
        if self.practice.is_some() {
            return;
        }
        let Some(tank) = self
            .display
            .as_ref()
            .filter(|snapshot| {
                snapshot.config.mode == Mode::Sandbox && !snapshot.match_state.finished
            })
            .and_then(|snapshot| {
                snapshot
                    .tanks
                    .iter()
                    .find(|tank| tank.id == self.player && tank.alive)
            })
        else {
            return;
        };
        let level = i16::from(tank.level)
            .saturating_add(step)
            .clamp(1, i16::from(tankyard_core::MAX_LEVEL)) as u8;
        if level != tank.level {
            self.sandbox_level(level);
        }
    }
    pub fn sandbox_class(&mut self, class: &str) {
        if self.practice.is_some() {
            return;
        }
        match &mut self.engine {
            Engine::Offline(local) => {
                if local.world.sandbox_class(self.player, class) {
                    self.events.extend(local.world.events.last().cloned());
                    self.display = Some(local.world.snapshot());
                }
            }
            Engine::Online { client, .. } => {
                client.sandbox_class(class);
            }
        }
    }
    pub fn sandbox_invulnerability(&mut self, enabled: bool) {
        if self.practice.is_some() {
            return;
        }
        match &mut self.engine {
            Engine::Offline(local) => {
                local.world.sandbox_invulnerability(self.player, enabled);
            }
            Engine::Online { client, .. } => {
                client.sandbox_invulnerability(enabled);
            }
        }
    }
    pub fn sandbox_boss(&mut self, kind: BossKind) {
        if self.practice.is_some() {
            return;
        }
        match &mut self.engine {
            Engine::Offline(local) => {
                if local.world.bosses.len() < 12 {
                    local.world.spawn_boss(kind);
                    self.events.extend(local.world.events.last().cloned());
                    self.display = Some(local.world.snapshot());
                }
            }
            Engine::Online { client, .. } => {
                client.sandbox_boss(kind);
            }
        }
    }
    pub fn add_bot(&mut self) {
        if self.practice.is_some() {
            return;
        }
        match &mut self.engine {
            Engine::Offline(local) => {
                if local.world.tanks.iter().filter(|t| t.bot).count() < 64 {
                    local.world.add_player("Practice bot", true);
                }
            }
            Engine::Online {
                host: Some(host), ..
            } => {
                if self.host_request.is_some() {
                    return;
                }
                let count = self
                    .display
                    .as_ref()
                    .map_or(1, |s| s.tanks.iter().filter(|t| t.bot).count() + 1);
                match host.command_async(&format!("bots {count}")) {
                    Ok(reply) => self.host_request = Some(reply),
                    Err(error) => self.notice = Some(error.to_string()),
                }
            }
            _ => {}
        }
    }
    pub fn diagnostics(&self) -> Option<(f64, f64, f64, f64)> {
        if let Engine::Online { client, .. } = &self.engine {
            let d = client.diagnostics();
            Some((
                d.ping_ms,
                d.packet_loss,
                d.sent_bytes_per_sec,
                d.received_bytes_per_sec,
            ))
        } else {
            None
        }
    }
    pub fn close(&mut self) {
        if let Engine::Online { client, host } = &mut self.engine {
            client.disconnect();
            if let Some(server) = host {
                server.shutdown();
            }
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(super) fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let delta =
        (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    a + delta * t
}
pub fn interpolate_position(previous: V2, current: V2, fraction: f32) -> V2 {
    if previous.distance(current) > 150.0 {
        current
    } else {
        previous.lerp(current, fraction.clamp(0.0, 1.0))
    }
}

/// Bridge brief packet gaps, then stop rather than inventing continuing motion.
fn extrapolate_position(
    position: V2,
    velocity: V2,
    seconds: f32,
    radius: f32,
    snapshot: &Snapshot,
) -> V2 {
    if seconds <= 0.0 {
        return position;
    }
    let end = position + velocity * seconds.min(0.075);
    let mut contact: f32 = 1.0;
    for &wall in &snapshot.walls {
        if let Some((time, _)) = tankyard_core::physics::sweep_rect(position, end, radius, wall) {
            contact = contact.min(time);
        }
    }
    let position = position.lerp(end, contact);
    let edge_x = radius.min(snapshot.config.width * 0.5);
    let edge_y = radius.min(snapshot.config.height * 0.5);
    V2::new(
        position.x.clamp(edge_x, snapshot.config.width - edge_x),
        position.y.clamp(edge_y, snapshot.config.height - edge_y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_spawn_protection_display_ends_without_waiting_for_a_server_snapshot() {
        let content = Content::builtin();
        let config = WorldConfig {
            target_count: 0,
            ..Default::default()
        };
        let mut world = tankyard_core::World::new(config.clone(), content.clone());
        let id = world.add_player("Pilot", false);
        let snapshot = world.snapshot();
        let client = NetworkClient::connect("127.0.0.1:9", "Pilot", "").unwrap();
        let mut session = Session::create(
            Engine::Online {
                client: Box::new(client),
                host: None,
            },
            id,
            Some(snapshot),
        );
        let mut predicted = world.tanks[0].clone();
        assert!(predicted.spawn_immunity);
        predicted.input.movement = V2::new(1.0, 0.0);
        predict_tank(&mut predicted, &content, &config, &[], 1, DT);
        session.predicted = Some(predicted);
        session.present_prediction(0.5);
        let displayed = &session.display.as_ref().unwrap().tanks[0];
        assert!(!displayed.spawn_immunity);
        assert_eq!(displayed.protection, 0.0);
    }

    #[test]
    fn online_reload_display_uses_predicted_firing_instead_of_the_last_snapshot() {
        let content = Content::builtin();
        let config = WorldConfig {
            target_count: 0,
            ..Default::default()
        };
        let mut world = tankyard_core::World::new(config.clone(), content.clone());
        let id = world.add_player("Pilot", false);
        let client = NetworkClient::connect("127.0.0.1:9", "Pilot", "").unwrap();
        let mut session = Session::create(
            Engine::Online {
                client: Box::new(client),
                host: None,
            },
            id,
            Some(world.snapshot()),
        );
        let mut predicted = world.tanks[0].clone();
        predicted.input.fire = true;
        predict_tank(&mut predicted, &content, &config, &[], 1, DT);
        session.predicted = Some(predicted);
        session.present_prediction(0.5);
        let displayed = &session.display.as_ref().unwrap().tanks[0];
        let progress = tankyard_core::reload_progress(
            displayed,
            content.class(&displayed.class).unwrap(),
            &config,
        )
        .unwrap();
        assert!(
            progress < 0.04,
            "a local shot should drain the bar immediately"
        );
        assert!(displayed.input.fire);
        assert_eq!(
            world.tanks[0].shot_count, 0,
            "the authoritative snapshot is still before the shot"
        );
    }
    #[test]
    fn sandbox_level_shortcuts_respect_bounds_and_game_state() {
        let mut session = Session::offline(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                ..Default::default()
            },
            0,
            "Pilot",
        );
        session.sandbox_adjust_level(1);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].level, 31);
        session.sandbox_adjust_level(15);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].level, 46);
        session.sandbox_adjust_level(i16::MAX);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].level, 55);
        session.sandbox_adjust_level(i16::MIN);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].level, 1);
        session.display.as_mut().unwrap().tanks[0].alive = false;
        session.sandbox_adjust_level(15);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].level, 1);
        session.display.as_mut().unwrap().tanks[0].alive = true;
        session.display.as_mut().unwrap().match_state.finished = true;
        session.sandbox_adjust_level(15);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].level, 1);
        let mut normal = Session::offline(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            0,
            "Pilot",
        );
        normal.sandbox_adjust_level(15);
        assert_eq!(normal.display.as_ref().unwrap().tanks[0].level, 1);
    }

    #[test]
    fn practice_loadouts_match_the_challenge_and_sandbox_commands_are_blocked() {
        for (index, challenge) in tankyard_client::practice::CHALLENGES.iter().enumerate() {
            let mut session = Session::practice(index, "Pilot", 42).unwrap();
            if let Engine::Offline(local) = &session.engine {
                assert_eq!(local.world.tanks.len(), challenge.bots + 1);
                assert!(
                    local
                        .world
                        .tanks
                        .iter()
                        .all(|tank| tank.level == challenge.level && tank.class == challenge.class)
                );
                let human = local.world.tanks.iter().find(|tank| !tank.bot).unwrap();
                if challenge.id == "drone_drill" {
                    assert_eq!(human.points, 0);
                }
            }
            session.sandbox_level(45);
            session.sandbox_adjust_level(15);
            session.sandbox_class("annihilator");
            session.sandbox_invulnerability(true);
            session.sandbox_boss(BossKind::Sentinel);
            session.add_bot();
            if let Engine::Offline(local) = &session.engine {
                assert_eq!(local.world.tanks.len(), challenge.bots + 1);
                assert!(local.world.bosses.is_empty());
                let human = local.world.tanks.iter().find(|tank| !tank.bot).unwrap();
                assert_eq!(human.level, challenge.level);
                assert_eq!(human.class, challenge.class);
            }
        }
        assert!(Session::practice(4, "Pilot", 42).is_err());
    }
    #[test]
    fn finished_practice_freezes_and_does_not_award_normal_achievements() {
        for index in [0, 3] {
            let mut session = Session::practice(index, "Pilot", 42).unwrap();
            if let Engine::Offline(local) = &mut session.engine {
                let human = local.world.tanks.iter_mut().find(|tank| !tank.bot).unwrap();
                human.score = 500;
                human.kills = 3;
                if index == 3 {
                    human.alive = false;
                }
            }
            session.tick(PlayerInput::default(), &Content::builtin());
            assert!(session.practice.as_ref().unwrap().done());
            let tick = session.display.as_ref().unwrap().tick;
            session.tick(
                PlayerInput {
                    movement: V2::new(1., 0.),
                    fire: true,
                    ability: true,
                    ..Default::default()
                },
                &Content::builtin(),
            );
            assert_eq!(session.display.as_ref().unwrap().tick, tick);
            assert_eq!(session.achievement_tracker.session_totals(), (0, 0));
            assert_eq!(session.achievement_tracker.highest_score(), 0);
        }
    }
    #[test]
    fn shift_upgrades_use_available_points_and_stop_at_the_stat_limit() {
        let mut session = Session::offline(
            WorldConfig {
                mode: tankyard_core::Mode::Sandbox,
                target_count: 0,
                ..Default::default()
            },
            0,
            "Pilot",
        );
        session.upgrade_stat(3, true);
        let tank = &session.display.as_ref().unwrap().tanks[0];
        assert_eq!(tank.stats[3], 7);
        let remaining = tank.points;
        session.upgrade_stat(3, true);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].points, remaining);
        session.upgrade_stat(8, true);
        assert_eq!(session.display.as_ref().unwrap().tanks[0].points, remaining);
        if let Engine::Offline(local) = &mut session.engine {
            local.world.tanks[0].points = 2;
        }
        session.tick(PlayerInput::default(), &Content::builtin());
        session.upgrade_stat(4, true);
        let tank = &session.display.as_ref().unwrap().tanks[0];
        assert_eq!(tank.stats[4], 2);
        assert_eq!(tank.points, 0);
    }

    #[test]
    fn packet_gap_motion_is_bounded_and_stops_at_walls_and_arena_edges() {
        let mut session = Session::offline(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            0,
            "Pilot",
        );
        let snapshot = session.display.as_mut().unwrap();
        let origin = V2::new(500., 500.);
        let velocity = V2::new(1000., 0.);
        let capped = extrapolate_position(origin, velocity, 10., 22., snapshot);
        assert!((capped.x - 575.).abs() < 0.001);
        snapshot.walls.push(tankyard_core::Rect {
            x: 550.,
            y: 400.,
            w: 20.,
            h: 200.,
        });
        let contact = extrapolate_position(origin, velocity, 0.075, 22., snapshot);
        assert!((contact.x - 528.).abs() < 0.001);
        snapshot.walls.clear();
        let edge = extrapolate_position(
            V2::new(snapshot.config.width - 30., 500.),
            velocity,
            0.075,
            22.,
            snapshot,
        );
        assert_eq!(edge.x, snapshot.config.width - 22.);
        assert_eq!(
            extrapolate_position(origin, velocity, 0., 22., snapshot),
            origin
        );
        assert_eq!(
            extrapolate_position(origin, velocity, 0.075, 2000., snapshot),
            V2::new(snapshot.config.width * 0.5, snapshot.config.height * 0.5)
        );
    }

    #[test]
    fn catchup_keeps_events_from_all_simulation_ticks() {
        let config = WorldConfig {
            target_count: 0,
            ..Default::default()
        };
        let content = Content::builtin();
        let mut session = Session::offline(config, 0, "Pilot");
        for sequence in 1..=40 {
            session.tick(
                PlayerInput {
                    sequence,
                    fire: true,
                    ..Default::default()
                },
                &content,
            );
        }
        let shot_ticks: Vec<_> = session
            .events
            .iter()
            .filter(|event| matches!(event.kind, tankyard_core::EventKind::Shot))
            .map(|event| event.tick)
            .collect();
        assert!(
            shot_ticks.len() >= 2,
            "all firing events must survive rendering catch-up"
        );
        assert!(shot_ticks.windows(2).all(|pair| pair[1] > pair[0]));
        assert_eq!(session.display.as_ref().unwrap().tick, 40);
    }

    #[test]
    fn angle_interpolation_takes_short_path_across_wrap() {
        let interpolated = lerp_angle(3.1, -3.1, 0.5);
        assert!((interpolated - std::f32::consts::PI).abs() < 0.001);
    }

    #[test]
    fn neutral_input_keeps_aim_and_releases_controls() {
        let mut session = Session::offline(WorldConfig::default(), 0, "Pilot");
        let tank = &mut session.display.as_mut().unwrap().tanks[0];
        tank.aim = 1.75;
        tank.input = PlayerInput {
            movement: V2::new(1.0, 0.0),
            fire: true,
            alt_fire: true,
            ability: true,
            ..Default::default()
        };
        let input = session.neutral_input(42);
        assert_eq!(input.sequence, 42);
        assert_eq!(input.aim, 1.75);
        assert_eq!(input.movement, V2::ZERO);
        assert!(!input.fire && !input.alt_fire && !input.ability);
    }

    #[test]
    fn highest_score_survives_death_and_respawn() {
        let content = Content::builtin();
        let mut session = Session::offline(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            0,
            "Pilot",
        );
        if let Engine::Offline(local) = &mut session.engine {
            local.world.tanks[0].score = 50_000;
            local.world.tanks[0].health = 0.0;
        }
        session.tick(PlayerInput::default(), &content);
        assert_eq!(session.achievement_tracker.highest_score(), 50_000);
        let tank = &session.display.as_ref().unwrap().tanks[0];
        assert!(!tank.alive && tank.score < 50_000);

        for sequence in 1..=200 {
            session.tick(
                PlayerInput {
                    sequence,
                    ability: true,
                    ..Default::default()
                },
                &content,
            );
        }
        assert!(session.display.as_ref().unwrap().tanks[0].alive);
        assert_eq!(session.achievement_tracker.highest_score(), 50_000);
    }

    #[test]
    fn living_idle_player_gets_no_bot_achievements_locally_or_from_wire_snapshots() {
        use tankyard_client::achievements::{Feed, unlock};
        use tankyard_core::Rect;
        use tankyard_protocol::{decode_snapshot, encode_snapshot};

        let content = Content::builtin();
        let mut session = Session::offline(
            WorldConfig {
                width: 1500.,
                height: 1100.,
                target_count: 36,
                seed: 42,
                ..Default::default()
            },
            8,
            "Pilot",
        );
        if let Engine::Offline(local) = &mut session.engine {
            // Keep the living player away from body-contact damage and shape
            // rewards while the bots play a real, unmodified FFA match.
            let player = local
                .world
                .tanks
                .iter_mut()
                .find(|t| t.id == session.player)
                .unwrap();
            player.pos = V2::new(35., 35.);
            player.protection = 3600.;
            local.world.walls.push(Rect {
                x: 80.,
                y: 0.,
                w: 20.,
                h: 100.,
            });
            local.world.walls.push(Rect {
                x: 0.,
                y: 80.,
                w: 100.,
                h: 20.,
            });
            local
                .world
                .targets
                .retain(|t| t.pos.x > 130. || t.pos.y > 130.);
        }
        let mut remote = Tracker::default();
        let mut offline_ids = vec![];
        let mut remote_ids = vec![];
        let mut offline_feed = Feed::default();
        let mut remote_feed = Feed::default();
        for sequence in 1..=12_000 {
            session.tick(
                PlayerInput {
                    sequence,
                    ..Default::default()
                },
                &content,
            );
            if sequence % 3 == 0 {
                let snapshot = session.display.as_ref().unwrap();
                let received = decode_snapshot(&encode_snapshot(snapshot).unwrap())
                    .unwrap_or_else(|error| panic!("{error} at tick {}", snapshot.tick));
                let progress = remote
                    .observe(&received, session.player, &content, 0, 0)
                    .unwrap();
                unlock(progress, &mut remote_ids, &mut remote_feed);
                unlock(
                    session.achievement_tracker.progress(0, 0).unwrap(),
                    &mut offline_ids,
                    &mut offline_feed,
                );
            }
        }
        let snapshot = session.display.as_ref().unwrap();
        let player = snapshot
            .tanks
            .iter()
            .find(|t| t.id == session.player)
            .unwrap();
        assert!(player.alive);
        assert_eq!((player.kills, player.score, player.level), (0, 0, 1));
        assert!(
            snapshot.tanks.iter().any(|t| t.bot && t.kills > 0),
            "bots must actually fight"
        );
        assert!(
            snapshot.tanks.iter().any(|t| t.bot && t.class != "vector"),
            "bots must actually upgrade"
        );
        assert_eq!(offline_ids, ["first_round"]);
        assert_eq!(remote_ids, offline_ids);
        assert_eq!(session.achievement_tracker.session_totals(), (1, 0));
        assert_eq!(remote.session_totals(), (1, 0));
    }

    #[test]
    fn sandbox_and_modded_sessions_never_record_personal_progress() {
        for config in [
            WorldConfig {
                mode: tankyard_core::Mode::Sandbox,
                ..Default::default()
            },
            WorldConfig {
                modded: true,
                ..Default::default()
            },
        ] {
            let mut session = Session::offline(config, 2, "Pilot");
            if let Engine::Offline(local) = &mut session.engine {
                let player = local
                    .world
                    .tanks
                    .iter_mut()
                    .find(|t| t.id == session.player)
                    .unwrap();
                player.kills = 100;
                player.score = 50_000;
            }
            session.tick(PlayerInput::default(), &Content::builtin());
            assert!(session.achievement_tracker.progress(0, 0).is_none());
            assert_eq!(session.achievement_tracker.session_totals(), (0, 0));
            assert_eq!(session.achievement_tracker.highest_score(), 0);
        }
    }
}
