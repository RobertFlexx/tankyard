use crate::drones::DroneContacts;
use crate::physics::{
    Body, SpatialGrid, circle_rect, constrain_tank, move_tank, polygon_vertices_into, sweep_circle,
    sweep_polygon, sweep_rect,
};
use crate::*;
use std::collections::BTreeMap;
use tankyard_content::Content;
use tankyard_content::ProjectileKind;
pub struct World {
    pub config: WorldConfig,
    pub content: Content,
    pub map: tankyard_content::MapDef,
    pub tick: u64,
    pub tanks: Vec<Tank>,
    pub projectiles: Vec<Projectile>,
    pub targets: Vec<Target>,
    pub bosses: Vec<Boss>,
    pub walls: Vec<Rect>,
    pub objectives: Vec<Objective>,
    pub match_state: MatchState,
    pub events: Vec<GameEvent>,
    pub(crate) next_id: u64,
    pub(crate) rng: u64,
    pub(crate) grid: SpatialGrid,
    pub(crate) scratch: Vec<Body>,
    collision_hits: Vec<(f32, Body, EntityId)>,
    drone_contacts: DroneContacts,
    body_padding: f32,
    pub(crate) contributions: BTreeMap<EntityId, Vec<(EntityId, f32, u64)>>,
    pub(crate) mode_timer: f32,
    pub(crate) last_arena_event: ArenaEvent,
    pub(crate) score_fraction: [f32; 4],
    pub(crate) sandbox_god: Vec<EntityId>,
}
impl World {
    pub fn new(mut config: WorldConfig, content: Content) -> Self {
        if config.rules.validate().is_err() {
            config.rules = tankyard_mods::GameRules::default();
        }
        Self::with_maps(config, content, tankyard_content::builtin_maps())
    }
    pub fn with_bundle(
        mut config: WorldConfig,
        bundle: &tankyard_mods::GameBundle,
    ) -> Result<Self, String> {
        bundle.validate()?;
        config.rules = bundle.rules;
        config.modded = bundle.modded() || config.modded;
        config.mode_name = bundle.mode_name.clone();
        Ok(Self::with_maps(
            config,
            bundle.content.clone(),
            bundle.maps.clone(),
        ))
    }
    fn with_maps(
        mut config: WorldConfig,
        content: Content,
        maps: Vec<tankyard_content::MapDef>,
    ) -> Self {
        if !config.width.is_finite() {
            config.width = 3600.;
        }
        if !config.height.is_finite() {
            config.height = 2600.;
        }
        config.width = config.width.clamp(900., 16000.);
        config.height = config.height.clamp(700., 16000.);
        config.target_count = config.target_count.min(10000);
        config.max_projectiles = config.max_projectiles.clamp(100, 30000);
        config.spawn_protection = if config.spawn_protection.is_finite() {
            config.spawn_protection.clamp(0., 15.)
        } else {
            15.
        };
        config.time_limit = if config.time_limit.is_finite() {
            config.time_limit.clamp(10., 86400.)
        } else {
            600.
        };
        config.score_limit = config.score_limit.max(1);
        config.lives = config.lives.clamp(1, 10);
        let valid = if config.modded {
            content.validate_modded()
        } else {
            content.validate()
        };
        let content = if valid.is_ok() {
            content
        } else {
            Content::builtin()
        };
        if config.mode == Mode::Maze && config.map == "open" {
            config.map = "maze".into();
        }
        let map = maps
            .iter()
            .find(|m| m.id == config.map)
            .unwrap_or(&maps[0])
            .clone();
        config.map = map.id.clone();
        let mut world = Self {
            grid: SpatialGrid::new(config.width, config.height),
            drone_contacts: DroneContacts::new(config.width, config.height),
            rng: config.seed,
            next_id: 1,
            tick: 0,
            tanks: Vec::with_capacity(128),
            projectiles: Vec::with_capacity(config.max_projectiles),
            targets: Vec::with_capacity(config.target_count),
            bosses: vec![],
            walls: vec![],
            objectives: vec![],
            match_state: MatchState {
                elapsed: 0.,
                scores: [0; 4],
                winner: None,
                finished: false,
                round: 1,
                boss_wave: 0,
                safe_radius: config.width.max(config.height),
                announcement: config
                    .mode_name
                    .clone()
                    .unwrap_or_else(|| config.mode.name().into()),
                arena: ArenaStatus::default(),
            },
            config,
            content,
            map,
            events: Vec::with_capacity(256),
            scratch: Vec::with_capacity(128),
            collision_hits: Vec::with_capacity(32),
            body_padding: 0.0,
            contributions: BTreeMap::new(),
            mode_timer: 0.,
            last_arena_event: ArenaEvent::None,
            score_fraction: [0.; 4],
            sandbox_god: vec![],
        };
        world.initialize_map();
        world.initialize_mode();
        for _ in 0..world.config.target_count {
            world.spawn_target();
        }
        world
    }
    pub(crate) fn id(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("entity ID space exhausted");
        id
    }
    pub(crate) fn random(&mut self) -> f32 {
        self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u32 << 24) as f32
    }
    pub(crate) fn event(
        &mut self,
        kind: EventKind,
        pos: V2,
        subject: EntityId,
        value: u32,
        team: u8,
    ) {
        if self.events.len() < 512 {
            self.events.push(GameEvent {
                tick: self.tick,
                kind,
                pos,
                subject,
                value,
                team,
            });
        }
    }
    pub fn add_player(&mut self, name: &str, bot: bool) -> EntityId {
        let teams = self.config.mode.teams();
        let team = if teams == 0 {
            0
        } else {
            (1..=teams)
                .min_by_key(|t| self.tanks.iter().filter(|p| p.team == *t).count())
                .unwrap_or(1)
        };
        let pos = self.spawn_position(team);
        let id = self.id();
        let name = sanitize_name(name);
        let lives = self.config.lives;
        let protection = self.config.spawn_protection;
        self.tanks.push(Tank {
            id,
            name,
            pos,
            vel: V2::ZERO,
            aim: 0.,
            radius: 22.,
            health: 100.,
            max_health: 100.,
            xp: 0,
            score: 0,
            level: 1,
            stats: [0; 8],
            points: 0,
            class: "vector".into(),
            team,
            bot,
            alive: true,
            respawn_timer: 0.,
            protection,
            shield: 0.,
            ability_cooldown: 0.,
            last_input: 0,
            kills: 0,
            deaths: 0,
            lives,
            input: PlayerInput::default(),
            cooldowns: vec![],
            shot_count: 0,
            damaged_at: 0,
            last_attacker: 0,
            charge: 0.,
            boost: 0.,
            barrel_shots: vec![],
            barrel_aims: vec![],
            carrying: false,
            deployed_drones: 0,
            spawn_immunity: protection > 0.0,
        });
        if matches!(self.config.mode, Mode::Deathmatch | Mode::Sandbox) {
            self.set_level_unchecked(id, 30);
        } else if self.config.mode == Mode::CoopPve {
            self.set_level_unchecked(id, (12 + self.match_state.arena.wave * 3).clamp(15, 45));
        }
        self.refresh_stats(id, true);
        id
    }
    pub fn remove_player(&mut self, id: EntityId) {
        self.tanks.retain(|t| t.id != id);
        self.projectiles.retain(|p| p.owner != id);
        self.sandbox_god.retain(|&p| p != id);
        for o in &mut self.objectives {
            if o.carrier == Some(id) {
                o.carrier = None;
            }
        }
    }
    pub fn set_input(&mut self, id: EntityId, input: PlayerInput) -> bool {
        if !input.valid() {
            return false;
        }
        let Some(tank) = self.tanks.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        if input.sequence != tank.last_input
            && input.sequence.wrapping_sub(tank.last_input) > 0x80000000
        {
            return false;
        }
        tank.last_input = input.sequence;
        tank.input = PlayerInput {
            movement: input.movement.clamp_length(1.),
            ..input
        };
        true
    }
    pub fn upgrade_stat(&mut self, id: EntityId, index: usize) -> bool {
        if index >= 8 {
            return false;
        }
        let Some(t) = self.tanks.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        if t.points == 0 || t.stats[index] >= 7 || !t.alive {
            return false;
        }
        t.stats[index] += 1;
        t.points -= 1;
        self.refresh_stats(id, false);
        true
    }
    pub fn evolve(&mut self, id: EntityId, class: &str) -> bool {
        let Some(t) = self.tanks.iter().find(|t| t.id == id) else {
            return false;
        };
        let Some(def) = self.content.class(class) else {
            return false;
        };
        let Some(current) = self.content.class(&t.class) else {
            return false;
        };
        if !t.alive
            || t.level < def.min_level
            || !current.upgrades.iter().any(|u| u == class)
            || !def.parents.contains(&t.class)
        {
            return false;
        }
        self.set_class(id, class);
        true
    }
    pub(crate) fn set_class(&mut self, id: EntityId, class: &str) {
        if let Some(t) = self.tanks.iter_mut().find(|t| t.id == id) {
            t.class = class.into();
            t.cooldowns.clear();
            t.barrel_shots.clear();
            t.barrel_aims.clear();
            t.charge = 0.;
            let pos = t.pos;
            let team = t.team;
            self.refresh_stats(id, false);
            self.event(EventKind::Evolve, pos, id, 0, team);
        }
        if let Some(class) = self.content.class(class) {
            let mut kept = 0_u8;
            self.projectiles.retain(|p| {
                if p.owner != id || !p.kind.is_drone() {
                    return true;
                }
                if !class
                    .barrels
                    .iter()
                    .any(|barrel| barrel.projectile == p.kind)
                    || kept >= class.drone_limit
                {
                    return false;
                }
                kept += 1;
                true
            });
            if let Some(tank) = self.tanks.iter_mut().find(|tank| tank.id == id) {
                tank.deployed_drones = kept;
            }
        }
    }
    pub fn sandbox_level(&mut self, id: EntityId, level: u8) -> bool {
        if self.config.mode != Mode::Sandbox || !(1..=45).contains(&level) {
            return false;
        }
        if !self.tanks.iter().any(|t| t.id == id) {
            return false;
        }
        self.set_level_unchecked(id, level);
        true
    }
    pub fn sandbox_class(&mut self, id: EntityId, class: &str) -> bool {
        if self.config.mode != Mode::Sandbox
            || self.content.class(class).is_none()
            || !self.tanks.iter().any(|t| t.id == id)
        {
            return false;
        }
        self.set_level_unchecked(id, 45);
        self.set_class(id, class);
        true
    }
    pub fn sandbox_invulnerability(&mut self, id: EntityId, enabled: bool) -> bool {
        if self.config.mode != Mode::Sandbox || !self.tanks.iter().any(|t| t.id == id) {
            return false;
        }
        self.sandbox_god.retain(|&p| p != id);
        if enabled {
            self.sandbox_god.push(id);
        }
        true
    }
    pub(crate) fn set_level_unchecked(&mut self, id: EntityId, level: u8) {
        let Some(t) = self.tanks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        t.xp = xp_for_level(level);
        t.level = level;
        let allowed = points_for_level(level);
        if t.stats.iter().copied().sum::<u8>() > allowed {
            t.stats = [0; 8];
        }
        t.points = allowed.saturating_sub(t.stats.iter().copied().sum());
        self.refresh_stats(id, true);
    }
    pub(crate) fn refresh_stats(&mut self, id: EntityId, heal: bool) {
        let Some(t) = self.tanks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        let def = self.content.class(&t.class);
        let old = t.max_health;
        t.max_health = 100.
            * self.config.rules.health_multiplier
            * stat_scale(t.stats[0])
            * def.map_or(1., |c| c.multipliers[0]);
        t.radius = 22.
            * def.map_or(1., |c| {
                c.multipliers[0].sqrt().clamp(0.85, 1.6) * c.body_scale
            });
        t.health = if heal {
            t.max_health
        } else {
            (t.health + (t.max_health - old).max(0.)).min(t.max_health)
        };
    }
    pub(crate) fn award(&mut self, id: EntityId, xp: u32, score: u32) {
        let (xp, score) = if self.config.mode == Mode::RandomEvents
            && self.match_state.arena.event == ArenaEvent::DoublePoints
        {
            (xp.saturating_mul(2), score.saturating_mul(2))
        } else {
            (xp, score)
        };
        let xp = (xp as f64 * f64::from(self.config.rules.xp_multiplier)).min(f64::from(u32::MAX))
            as u32;
        let score = (score as f64 * f64::from(self.config.rules.score_multiplier))
            .min(f64::from(u32::MAX)) as u32;
        let Some(t) = self.tanks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        t.xp = t.xp.saturating_add(xp);
        t.score = t.score.saturating_add(score);
        let level = level_for_xp(t.xp);
        if level > t.level {
            let gain = points_for_level(level) - points_for_level(t.level);
            t.level = level;
            t.points = t.points.saturating_add(gain);
            let pos = t.pos;
            let team = t.team;
            self.event(EventKind::LevelUp, pos, id, level as u32, team);
        }
    }
    pub fn step(&mut self) {
        self.step_with(|_| {});
    }
    /// Run a server hook after clearing the previous frame's events. Events
    /// produced by the hook remain available to clients and later hooks.
    pub fn step_with(&mut self, before: impl FnOnce(&mut Self)) {
        self.events.clear();
        before(self);
        if self.match_state.finished {
            return;
        }
        self.tick += 1;
        self.match_state.elapsed += DT;
        self.update_tanks();
        self.update_targets();
        self.update_bosses();
        self.fire_weapons();
        self.rebuild_grid();
        self.update_projectiles();
        self.drone_contacts
            .resolve(&mut self.projectiles, &self.config);
        self.resolve_contacts();
        self.resolve_boss_contacts();
        for tank in self.tanks.iter_mut().filter(|tank| tank.alive) {
            constrain_tank(tank, &self.config, &self.walls);
        }
        self.resolve_deaths();
        self.update_mode();
        self.projectiles
            .retain(|p| p.lifetime > 0. && p.penetration > 0. && p.pos.finite());
        for tank in &mut self.tanks {
            tank.deployed_drones = 0;
        }
        for drone in self.projectiles.iter().filter(|p| p.kind.is_drone()) {
            if let Some(tank) = self.tanks.iter_mut().find(|tank| tank.id == drone.owner) {
                tank.deployed_drones = tank.deployed_drones.saturating_add(1);
            }
        }
        self.targets.retain(|t| t.health > 0.);
        self.bosses.retain(|b| b.health > 0.);
        if self.tick.is_multiple_of(15) {
            let count = self
                .config
                .target_count
                .saturating_sub(self.targets.len())
                .min(8);
            for _ in 0..count {
                self.spawn_target();
            }
        }
        if self.tick.is_multiple_of(300) {
            self.contributions.retain(|_, v| {
                v.retain(|(_, _, tick)| self.tick.saturating_sub(*tick) < 600);
                !v.is_empty()
            });
        }
    }
    fn update_tanks(&mut self) {
        let mut respawns = vec![];
        for tank in &mut self.tanks {
            if !tank.alive {
                tank.respawn_timer = (tank.respawn_timer - DT).max(0.);
                if tank.respawn_timer <= 0.
                    && (tank.bot || tank.input.ability)
                    && !(self.config.mode == Mode::Survival && tank.lives == 0)
                    && self.config.mode != Mode::CoopPve
                {
                    respawns.push(tank.id);
                }
                continue;
            }
            crate::physics::update_protection(tank, DT);
            tank.shield = (tank.shield - DT).max(0.);
            tank.boost = (tank.boost - DT).max(0.);
            tank.ability_cooldown = (tank.ability_cooldown - DT).max(0.);
            let def = self.content.class(&tank.class);
            let regen = 2. * stat_scale(tank.stats[1]) * def.map_or(1., |c| c.multipliers[1])
                + def.map_or(0., |c| c.passive_regen);
            if self.tick.saturating_sub(tank.damaged_at) > 180 {
                tank.health = (tank.health + regen * DT * self.config.rules.regen_multiplier)
                    .min(tank.max_health);
            }
            move_tank(tank, &self.content, &self.config, &self.walls, DT);
        }
        for id in respawns {
            let team = self.tanks.iter().find(|t| t.id == id).map_or(0, |t| t.team);
            let pos = self.spawn_position(team);
            if let Some(t) = self.tanks.iter_mut().find(|t| t.id == id) {
                t.alive = true;
                t.pos = pos;
                t.vel = V2::ZERO;
                t.input = PlayerInput::default();
                t.protection = self.config.spawn_protection;
                t.spawn_immunity = t.protection > 0.0;
                t.health = t.max_health;
                t.cooldowns.clear();
            }
        }
    }
    fn update_targets(&mut self) {
        for t in &mut self.targets {
            if t.kind != TargetKind::Mobile {
                t.vel *= (-2.5 * DT).exp();
            }
            t.vel = t.vel.clamp_length(120.0);
            for &wall in &self.walls {
                if let Some((normal, depth)) = circle_rect(t.pos, t.radius, wall) {
                    t.pos += normal * (depth + 0.01);
                    t.vel -= normal * t.vel.dot(normal).min(0.0);
                }
            }
            t.angle += DT * 0.16;
            let old = t.pos;
            t.pos += t.vel * DT;
            if t.pos.x < t.radius || t.pos.x > self.config.width - t.radius {
                t.vel.x = -t.vel.x;
            }
            if t.pos.y < t.radius || t.pos.y > self.config.height - t.radius {
                t.vel.y = -t.vel.y;
            }
            t.pos.x = t.pos.x.clamp(t.radius, self.config.width - t.radius);
            t.pos.y = t.pos.y.clamp(t.radius, self.config.height - t.radius);
            if self
                .walls
                .iter()
                .any(|&w| circle_rect(t.pos, t.radius, w).is_some())
            {
                t.pos = old;
                t.vel *= -1.;
            }
        }
    }
    fn rebuild_grid(&mut self) {
        self.grid.clear();
        self.body_padding = 0.0;
        for (i, t) in self.tanks.iter().enumerate() {
            if t.alive {
                self.grid.insert(t.pos, Body::Tank(i));
                self.body_padding = self.body_padding.max(t.radius);
            }
        }
        for (i, t) in self.targets.iter().enumerate() {
            if t.health > 0. {
                self.grid.insert(t.pos, Body::Target(i));
                self.body_padding = self.body_padding.max(t.radius);
            }
        }
        for (i, b) in self.bosses.iter().enumerate() {
            if b.health > 0. {
                self.grid.insert(b.pos, Body::Boss(i));
                self.body_padding = self.body_padding.max(b.radius);
            }
        }
        for (i, p) in self.projectiles.iter().enumerate() {
            if p.lifetime > 0. && p.penetration > 0. {
                self.grid.insert(p.pos, Body::Deployable(i));
                self.body_padding = self.body_padding.max(p.radius);
            }
        }
    }
    pub(crate) fn spawn_position(&mut self, team: u8) -> V2 {
        let mut best = V2::new(self.config.width * 0.5, self.config.height * 0.5);
        let mut best_safety = -1.;
        for _ in 0..64 {
            let (x, y) = (self.random(), self.random());
            let regions = || self.map.spawn_regions.iter().filter(|r| r.team == team);
            let count = regions().count();
            let region = regions().nth(((x * count as f32) as usize).min(count.saturating_sub(1)));
            let p = if let Some(region) = region {
                let rect = region.rect;
                V2::new(
                    (rect[0] + self.random() * rect[2]) * self.config.width,
                    (rect[1] + y * rect[3]) * self.config.height,
                )
            } else if team == 0 {
                V2::new(
                    70. + x * (self.config.width - 140.),
                    70. + y * (self.config.height - 140.),
                )
            } else {
                let home = self.team_home(team);
                V2::new(
                    (home.x + (x - 0.5) * 400.).clamp(50., self.config.width - 50.),
                    (home.y + (y - 0.5) * 400.).clamp(50., self.config.height - 50.),
                )
            };
            if self.walls.iter().any(|&w| circle_rect(p, 36., w).is_some()) {
                continue;
            }
            if self
                .targets
                .iter()
                .any(|target| p.distance(target.pos) < target.radius + 38.0)
                || self
                    .bosses
                    .iter()
                    .any(|boss| p.distance(boss.pos) < boss.radius + 60.0)
                || self
                    .tanks
                    .iter()
                    .any(|tank| tank.alive && p.distance(tank.pos) < tank.radius + 38.0)
            {
                continue;
            }
            let safety = self
                .tanks
                .iter()
                .filter(|t| t.alive && (team == 0 || t.team != team))
                .map(|t| p.distance(t.pos))
                .fold(10000., f32::min);
            if safety > best_safety {
                best = p;
                best_safety = safety;
            }
            if safety > 650. {
                break;
            }
        }
        best
    }
    pub fn team_home(&self, team: u8) -> V2 {
        if let Some(base) = self.map.team_bases.iter().find(|b| b.team == team) {
            return V2::new(
                base.position[0] * self.config.width,
                base.position[1] * self.config.height,
            );
        }
        match team {
            1 => V2::new(220., 220.),
            2 => V2::new(self.config.width - 220., self.config.height - 220.),
            3 => V2::new(self.config.width - 220., 220.),
            _ => V2::new(220., self.config.height - 220.),
        }
    }
    pub(crate) fn spawn_target(&mut self) {
        let roll = self.random();
        let (kind, radius, health, xp) = if roll < 0.003 {
            (TargetKind::Rare, 30., 450., 1800)
        } else if roll < 0.07 {
            (TargetKind::Fortress, 54., 650., 400)
        } else if roll < 0.19 {
            (TargetKind::Pentagon, 33., 150., 130)
        } else if roll < 0.3 {
            (TargetKind::Reinforced, 25., 190., 90)
        } else if roll < 0.42 {
            (TargetKind::Mobile, 19., 65., 55)
        } else if roll < 0.65 {
            (TargetKind::Triangle, 17., 75., 45)
        } else {
            (TargetKind::Square, 14., 35., 22)
        };
        for _ in 0..32 {
            let pos = V2::new(
                radius + 50. + self.random() * (self.config.width - 2. * radius - 100.),
                radius + 50. + self.random() * (self.config.height - 2. * radius - 100.),
            );
            if self
                .walls
                .iter()
                .any(|&w| circle_rect(pos, radius + 8., w).is_some())
                || self
                    .targets
                    .iter()
                    .any(|target| pos.distance(target.pos) < target.radius + radius + 4.0)
                || self
                    .tanks
                    .iter()
                    .any(|t| t.alive && pos.distance(t.pos) < 180.)
                || self
                    .objectives
                    .iter()
                    .any(|o| pos.distance(o.pos) < o.radius + radius)
            {
                continue;
            }
            let angle = self.random() * std::f32::consts::TAU;
            let vel = if kind == TargetKind::Mobile {
                V2::from_angle(angle) * 38.
            } else {
                V2::ZERO
            };
            let id = self.id();
            self.targets.push(Target {
                id,
                pos,
                vel,
                radius,
                health,
                max_health: health,
                kind,
                angle,
                xp,
            });
            break;
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.make_snapshot(None)
    }
    /// Refresh an offline display without discarding its entity buffers.
    pub fn snapshot_into(&self, snapshot: &mut Snapshot) {
        snapshot.tick = self.tick;
        snapshot.config.clone_from(&self.config);
        snapshot.tanks.clone_from(&self.tanks);
        snapshot.projectiles.clone_from(&self.projectiles);
        snapshot.targets.clone_from(&self.targets);
        snapshot.bosses.clone_from(&self.bosses);
        snapshot.walls.clone_from(&self.walls);
        snapshot.objectives.clone_from(&self.objectives);
        snapshot.match_state.clone_from(&self.match_state);
        snapshot.events.clone_from(&self.events);
    }
    pub fn snapshot_for(&self, center: V2, radius: f32) -> Snapshot {
        self.make_snapshot(Some((center, radius.clamp(400., 5000.))))
    }
    /// Reuse entity and per-tank buffers between server viewers and snapshots.
    pub fn snapshot_for_into(&self, snapshot: &mut Snapshot, center: V2, radius: f32) {
        let radius_squared = radius.clamp(400., 5000.).powi(2);
        let visible = |pos: V2| (pos - center).length_squared() < radius_squared;
        snapshot.tick = self.tick;
        snapshot.config.clone_from(&self.config);
        snapshot.tanks.clone_from(&self.tanks);
        clone_filtered(&mut snapshot.projectiles, &self.projectiles, |p| {
            visible(p.pos)
        });
        clone_filtered(&mut snapshot.targets, &self.targets, |t| visible(t.pos));
        snapshot.bosses.clone_from(&self.bosses);
        snapshot.walls.clone_from(&self.walls);
        snapshot.objectives.clone_from(&self.objectives);
        snapshot.match_state.clone_from(&self.match_state);
        clone_filtered(&mut snapshot.events, &self.events, |e| visible(e.pos));
    }
    fn make_snapshot(&self, view: Option<(V2, f32)>) -> Snapshot {
        let visible = |pos: V2| {
            view.is_none_or(|(center, radius)| (pos - center).length_squared() < radius * radius)
        };
        Snapshot {
            tick: self.tick,
            config: self.config.clone(),
            tanks: self.tanks.clone(),
            projectiles: self
                .projectiles
                .iter()
                .filter(|p| visible(p.pos))
                .cloned()
                .collect(),
            targets: self
                .targets
                .iter()
                .filter(|t| visible(t.pos))
                .cloned()
                .collect(),
            bosses: self.bosses.clone(),
            walls: self.walls.clone(),
            objectives: self.objectives.clone(),
            match_state: self.match_state.clone(),
            events: self
                .events
                .iter()
                .filter(|e| visible(e.pos))
                .cloned()
                .collect(),
        }
    }
    pub(crate) fn hostile(&self, team: u8, other: u8) -> bool {
        team == 0 || other == 0 || team != other || self.config.friendly_fire
    }
    pub(crate) fn damage_tank(
        &mut self,
        index: usize,
        damage: f32,
        owner: EntityId,
        knockback: V2,
    ) {
        let t = &mut self.tanks[index];
        if !t.alive || t.health <= 0. || t.protection > 0. || self.sandbox_god.contains(&t.id) {
            return;
        }
        let reduction = if t.shield > 0. { 0.18 } else { 1. };
        let amount = (damage * reduction).max(0.);
        t.vel = (t.vel + knockback).clamp_length(1200.);
        if amount <= 0.0 || !amount.is_finite() {
            return;
        }
        t.health -= amount;
        t.damaged_at = self.tick;
        t.last_attacker = owner;
        let id = t.id;
        let pos = t.pos;
        let team = t.team;
        self.record_damage(id, owner, amount);
        if amount > 0.5 {
            self.event(EventKind::Impact, pos, id, amount as u32, team);
        }
    }
    pub(crate) fn record_damage(&mut self, victim: EntityId, owner: EntityId, amount: f32) {
        if owner == 0 || amount <= 0.0 || !amount.is_finite() {
            return;
        }
        let list = self.contributions.entry(victim).or_default();
        if let Some(v) = list.iter_mut().find(|v| v.0 == owner) {
            v.1 += amount;
            v.2 = self.tick;
        } else if list.len() < 16 {
            list.push((owner, amount, self.tick));
        }
    }
    fn resolve_deaths(&mut self) {
        let dead: Vec<_> = self
            .tanks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.alive && t.health <= 0.)
            .map(|(i, _)| i)
            .collect();
        for i in dead {
            let (id, pos, team, killer, xp) = {
                let t = &mut self.tanks[i];
                t.alive = false;
                // Clear held attack/ability keys on death. Otherwise a stale
                // input could implicitly trigger redeployment after cooldown.
                t.input = PlayerInput::default();
                t.shield = 0.;
                t.boost = 0.;
                t.ability_cooldown = 0.;
                t.carrying = false;
                t.deaths += 1;
                t.respawn_timer = self.config.rules.respawn_seconds;
                t.lives = t.lives.saturating_sub(1);
                (
                    t.id,
                    t.pos,
                    t.team,
                    t.last_attacker,
                    (t.xp / 4 + 150).min(8000),
                )
            };
            let killer_team = self.tanks.iter().find(|t| t.id == killer).map(|t| t.team);
            self.tanks[i].last_attacker = 0;
            self.tanks[i].damaged_at = self.tick;
            if self.config.mode == Mode::Tag
                && let Some(kteam) = killer_team
            {
                self.tanks[i].team = kteam;
            }
            if let Some(k) = self.tanks.iter_mut().find(|t| t.id == killer) {
                k.kills += 1;
            }
            self.reward_contributors(id, killer, xp, xp);
            if let Some(kteam) = killer_team
                && kteam > 0
                && matches!(
                    self.config.mode,
                    Mode::TwoTeams | Mode::FourTeams | Mode::Deathmatch
                )
            {
                self.match_state.scores[kteam as usize - 1] += 1;
            }
            // Death costs actual levels and leaderboard score, not just a
            // percentage of XP (which barely changes level on a curved scale).
            // Re-enter at the beginning of the recovered level, with its normal
            // stat budget, and pick a new class path as you level again.
            if self.config.mode != Mode::CoopPve {
                let recovered_level = respawn_level(self.tanks[i].level);
                let recovered_score = xp_for_level(recovered_level);
                self.tanks[i].xp = recovered_score;
                self.tanks[i].score = recovered_score;
                self.tanks[i].level = recovered_level;
                self.tanks[i].class = "vector".into();
                self.tanks[i].stats = [0; 8];
                self.tanks[i].points = points_for_level(recovered_level);
            }
            self.tanks[i].cooldowns.clear();
            self.tanks[i].barrel_shots.clear();
            self.tanks[i].barrel_aims.clear();
            self.tanks[i].shot_count = 0;
            self.tanks[i].charge = 0.0;
            self.refresh_stats(id, true);
            for objective in &mut self.objectives {
                if objective.carrier == Some(id) {
                    objective.carrier = None;
                    objective.pos = pos;
                    objective.return_timer = 15.;
                }
            }
            self.event(EventKind::Destroy, pos, id, 0, team);
            self.event(EventKind::Kill, pos, killer, xp, killer_team.unwrap_or(0));
        }
        let dead_targets: Vec<_> = self
            .targets
            .iter()
            .filter(|t| t.health <= 0.)
            .map(|t| (t.id, t.pos, t.xp))
            .collect();
        for (id, pos, xp) in dead_targets {
            let killer = self
                .contributions
                .get(&id)
                .and_then(|c| c.iter().max_by(|a, b| a.2.cmp(&b.2)))
                .map_or(0, |c| c.0);
            self.reward_contributors(id, killer, xp, xp);
            self.event(EventKind::Destroy, pos, id, xp, 0);
        }
        let dead_bosses: Vec<_> = self
            .bosses
            .iter()
            .filter(|b| b.health <= 0.)
            .map(|b| (b.id, b.pos, b.team, b.mothership))
            .collect();
        for (id, pos, team, mother) in dead_bosses {
            let killer = self
                .contributions
                .get(&id)
                .and_then(|c| c.iter().max_by(|a, b| a.2.cmp(&b.2)))
                .map_or(0, |c| c.0);
            self.reward_contributors(id, killer, 12000, 12000);
            if self.config.mode == Mode::BossRush {
                let allies: Vec<_> = self.tanks.iter().map(|t| t.id).collect();
                for ally in allies {
                    self.award(ally, 3500, 3500);
                }
                self.mode_timer = 4.;
            }
            if mother && self.config.mode == Mode::Mothership {
                self.finish(Some(if team == 1 { 2 } else { 1 }), "Mothership destroyed");
            }
            self.event(EventKind::Destroy, pos, id, 12000, team);
        }
    }
    pub(crate) fn reward_contributors(
        &mut self,
        victim: EntityId,
        killer: EntityId,
        xp: u32,
        score: u32,
    ) {
        let entries = self.contributions.remove(&victim).unwrap_or_default();
        let valid: Vec<_> = entries
            .into_iter()
            .filter(|c| self.tick.saturating_sub(c.2) < 600)
            .collect();
        let total: f32 = valid.iter().map(|c| c.1).sum();
        for (id, damage, _) in valid {
            let share = if total > 0. { damage / total } else { 0. };
            let credit = if id == killer {
                0.5 + share * 0.5
            } else {
                share * 0.5
            };
            self.award(
                id,
                (xp as f32 * credit) as u32,
                (score as f32 * credit) as u32,
            );
        }
    }
    fn resolve_contacts(&mut self) {
        let mut contacts = Vec::new();
        for i in 0..self.tanks.len() {
            let a = &self.tanks[i];
            if !a.alive {
                continue;
            }
            self.grid.query(
                a.pos,
                a.pos,
                a.radius + self.body_padding,
                &mut self.scratch,
            );
            for body in &self.scratch {
                match *body {
                    Body::Tank(j) if j > i => {
                        let b = &self.tanks[j];
                        if !b.alive {
                            continue;
                        }
                        let d = b.pos - a.pos;
                        let distance = d.length();
                        let overlap = a.radius + b.radius - distance;
                        if overlap > 0. {
                            let normal = if distance > 0.001 {
                                d / distance
                            } else {
                                V2::new(1., 0.)
                            };
                            contacts.push((i, Some(j), None, normal, overlap));
                        }
                    }
                    Body::Target(j) => {
                        let b = &self.targets[j];
                        let d = b.pos - a.pos;
                        let distance = d.length();
                        let overlap = a.radius + b.radius - distance;
                        if overlap > 0. {
                            let normal = if distance > 0.001 {
                                d / distance
                            } else {
                                V2::new(1.0, 0.0)
                            };
                            contacts.push((i, None, Some(j), normal, overlap));
                        }
                    }
                    _ => {}
                }
            }
        }
        for (i, j, target, normal, _) in contacts {
            if let Some(j) = j {
                let (a, b) = self.tanks.split_at_mut(j);
                let delta = b[0].pos - a[i].pos;
                let distance = delta.length();
                let depth = a[i].radius + b[0].radius - distance;
                if depth <= 0.0 {
                    continue;
                }
                let normal = if distance > 0.001 {
                    delta / distance
                } else {
                    normal
                };
                a[i].pos -= normal * depth * 0.5;
                b[0].pos += normal * depth * 0.5;
                let closing = (b[0].vel - a[i].vel).dot(normal).min(0.0);
                a[i].vel += normal * closing * 0.55;
                b[0].vel -= normal * closing * 0.55;
                let id1 = a[i].id;
                let id2 = b[0].id;
                let team1 = a[i].team;
                let team2 = b[0].team;
                if self.hostile(team1, team2) {
                    let da = self.body_damage(i) * DT;
                    let db = self.body_damage(j) * DT;
                    self.damage_tank(i, db, id2, normal * -5.);
                    self.damage_tank(j, da, id1, normal * 5.);
                }
            } else if let Some(j) = target {
                if self.targets[j].health <= 0. {
                    continue;
                }
                let delta = self.targets[j].pos - self.tanks[i].pos;
                let distance = delta.length();
                let depth = self.tanks[i].radius + self.targets[j].radius - distance;
                if depth <= 0.0 {
                    continue;
                }
                let normal = if distance > 0.001 {
                    delta / distance
                } else {
                    normal
                };
                self.tanks[i].pos -= normal * depth * 0.6;
                self.targets[j].pos += normal * depth * 0.4;
                let damage = self.body_damage(i) * DT;
                self.targets[j].health -= damage;
                self.record_damage(self.targets[j].id, self.tanks[i].id, damage);
                self.damage_tank(i, 14. * DT, 0, normal * -3.);
            }
        }
    }
    fn body_damage(&self, i: usize) -> f32 {
        let t = &self.tanks[i];
        if t.protection > 0.0 {
            return 0.0;
        }
        30. * self.config.rules.damage_multiplier
            * stat_scale(t.stats[2])
            * self
                .content
                .class(&t.class)
                .map_or(1., |c| c.multipliers[2])
            * (if t.boost > 0. { 1.8 } else { 1. })
    }
    fn resolve_boss_contacts(&mut self) {
        let mut contacts = Vec::new();
        // Hull contacts have already moved tanks since the grid was built.
        // Boss populations are small and bounded; use current positions here.
        for (j, boss) in self
            .bosses
            .iter()
            .enumerate()
            .filter(|(_, boss)| boss.health > 0.0)
        {
            for (i, tank) in self.tanks.iter().enumerate().filter(|(_, tank)| tank.alive) {
                let delta = boss.pos - tank.pos;
                let radius = boss.radius + tank.radius;
                if delta.length_squared() >= radius * radius {
                    continue;
                }
                let distance = delta.length();
                let normal = if distance > 0.001 {
                    delta / distance
                } else {
                    V2::new(1.0, 0.0)
                };
                contacts.push((i, j, normal, radius - distance));
            }
        }
        for (i, j, normal, overlap) in contacts {
            self.tanks[i].pos -= normal * (overlap + 0.01);
            if self.hostile(self.tanks[i].team, self.bosses[j].team) {
                let damage = self.body_damage(i) * DT;
                let owner = self.tanks[i].id;
                let boss_id = self.bosses[j].id;
                if self.tanks[i].protection <= 0. {
                    self.bosses[j].health -= damage;
                    self.record_damage(boss_id, owner, damage);
                }
                self.damage_tank(i, 35. * DT, boss_id, normal * -7.);
            }
        }
    }
    fn update_projectiles(&mut self) {
        self.drone_contacts.prepare_walls(&self.walls);
        let mut spawned = Vec::new();
        let mut hits = std::mem::take(&mut self.collision_hits);
        let count = self.projectiles.len();
        let moving_padding = self
            .projectiles
            .iter()
            .map(|p| p.vel.length() * DT)
            .fold(0., f32::max);
        for i in 0..count {
            if self.projectiles[i].lifetime <= 0. || self.projectiles[i].penetration <= 0. {
                continue;
            }
            self.steer_projectile(i);
            if self.projectiles[i].lifetime <= 0.0 {
                continue;
            }
            let p = &mut self.projectiles[i];
            p.previous = p.pos;
            let flight_dt = if p.kind == ProjectileKind::Homing {
                p.lifetime.min(DT)
            } else {
                DT
            };
            p.age += flight_dt;
            p.lifetime -= DT;
            if matches!(
                p.kind,
                ProjectileKind::Trap
                    | ProjectileKind::Mine
                    | ProjectileKind::Barrier
                    | ProjectileKind::Turret
            ) {
                p.vel *= (-4. * DT).exp();
            }
            let start = p.pos;
            let mut end = start + p.vel * flight_dt;
            let radius = p.radius;
            if p.kind.is_drone() {
                self.drone_contacts.move_drone(p, &self.config);
                end = p.pos;
            } else {
                let mut wallhit = None;
                for wall in &self.walls {
                    if let Some((t, n)) = sweep_rect(start, end, radius, *wall)
                        && wallhit.is_none_or(|h: (f32, V2)| t < h.0)
                    {
                        wallhit = Some((t, n));
                    }
                }
                for (t, n) in boundary_hits(start, end, radius, &self.config)
                    .into_iter()
                    .flatten()
                {
                    if wallhit.is_none_or(|h| t < h.0) {
                        wallhit = Some((t, n));
                    }
                }
                if let Some((t, n)) = wallhit {
                    end = start + (end - start) * t;
                    if p.ricochets > 0 {
                        p.vel -= n * (2. * p.vel.dot(n));
                        p.ricochets -= 1;
                        end += n * 0.1;
                    } else {
                        p.lifetime = 0.;
                    }
                }
                p.pos = end;
            }
            let (owner, team, kind, damage, velocity) = (p.owner, p.team, p.kind, p.damage, p.vel);
            if kind == ProjectileKind::Turret
                && p.age > 0.6
                && (p.age * 2.) as u32 > ((p.age - DT) * 2.) as u32
                && let Some(aim) = self.nearest_aim(end, team, owner, 700.)
            {
                let shot = self.simple_projectile(
                    owner,
                    team,
                    end,
                    V2::from_angle(aim) * 450.,
                    ProjectileKind::Bullet,
                    damage * 0.6,
                    5.,
                    2.,
                    1.,
                );
                spawned.push(shot);
            }
            if kind == ProjectileKind::Mine && self.projectiles[i].age < 0.4 {
                continue;
            }
            self.grid.query(
                start,
                end,
                self.body_padding + radius + moving_padding,
                &mut self.scratch,
            );
            hits.clear();
            for body in &self.scratch {
                let (position, r, id, valid) = match *body {
                    Body::Tank(j) => {
                        let t = &self.tanks[j];
                        (
                            t.pos,
                            t.radius,
                            t.id,
                            t.alive
                                && t.health > 0.
                                && t.id != owner
                                && t.protection <= 0.
                                && self.hostile(team, t.team),
                        )
                    }
                    Body::Target(j) => {
                        let t = &self.targets[j];
                        (t.pos, t.radius, t.id, t.health > 0.)
                    }
                    Body::Boss(j) => {
                        let b = &self.bosses[j];
                        (
                            b.pos,
                            b.radius,
                            b.id,
                            b.health > 0. && b.id != owner && self.hostile(team, b.team),
                        )
                    }
                    Body::Deployable(j) => {
                        let d = &self.projectiles[j];
                        (
                            d.pos,
                            d.radius,
                            d.id,
                            j != i
                                && d.lifetime > 0.
                                && d.owner != owner
                                && self.hostile(team, d.team),
                        )
                    }
                };
                if !valid || self.projectiles[i].hit.contains(&id) {
                    continue;
                }
                if sweep_circle(start, end, position, r + radius).is_none()
                    && !matches!(body, Body::Deployable(_))
                {
                    continue;
                }
                let collision = if let Body::Target(j) = body {
                    let target = &self.targets[*j];
                    let sides = target_sides(target.kind);
                    let mut vertices = [V2::ZERO; 8];
                    let vertices = &mut vertices[..usize::from(sides)];
                    polygon_vertices_into(position, r, target.angle, vertices);
                    sweep_polygon(start, end, radius, vertices)
                } else if let Body::Deployable(j) = body {
                    let other = &self.projectiles[*j];
                    let (other_start, other_end) = if *j < i {
                        (other.previous, other.pos)
                    } else {
                        (other.pos, other.pos + other.vel * DT)
                    };
                    sweep_circle(start - other_start, end - other_end, V2::ZERO, r + radius)
                } else {
                    sweep_circle(start, end, position, r + radius)
                };
                if let Some(t) = collision {
                    hits.push((t, *body, id));
                }
            }
            hits.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
            for &(time, body, id) in &hits {
                if self.projectiles[i].penetration <= 0. {
                    break;
                }
                // Earlier impacts can destroy a candidate from this sweep.
                if match body {
                    Body::Tank(j) => self.tanks[j].health <= 0.0,
                    Body::Target(j) => self.targets[j].health <= 0.0,
                    Body::Boss(j) => self.bosses[j].health <= 0.0,
                    Body::Deployable(j) => {
                        self.projectiles[j].lifetime <= 0.0
                            || self.projectiles[j].penetration <= 0.0
                    }
                } {
                    continue;
                }
                let impact = start.lerp(end, time);
                if let Body::Tank(defender) = body
                    && self.parry_projectile(i, defender, impact)
                {
                    break;
                }
                if self.projectiles[i].hit.len() < 24 {
                    self.projectiles[i].hit.push(id);
                }
                let knock = velocity.normalized() * (damage * 0.9);
                let age = self.projectiles[i].age;
                let applied = damage
                    * (if matches!(kind, ProjectileKind::Bullet | ProjectileKind::Scatter)
                        && age > 1.
                    {
                        (1. - (age - 1.) * 0.08).max(0.6)
                    } else {
                        1.
                    });
                let penetration_cost = if let Body::Deployable(j) = body {
                    (self.projectiles[j].damage / 20.0).max(0.1)
                } else {
                    1.0
                };
                match body {
                    Body::Tank(j) => self.damage_tank(j, applied, owner, knock),
                    Body::Target(j) => {
                        self.targets[j].health -= applied;
                        self.targets[j].vel += knock * 0.08;
                        self.record_damage(id, owner, applied);
                        self.event(
                            EventKind::Impact,
                            self.targets[j].pos,
                            id,
                            applied as u32,
                            0,
                        );
                    }
                    Body::Boss(j) => {
                        self.bosses[j].health -= applied;
                        self.record_damage(id, owner, applied);
                        self.event(
                            EventKind::Impact,
                            self.bosses[j].pos,
                            id,
                            applied as u32,
                            self.bosses[j].team,
                        );
                    }
                    Body::Deployable(j) => {
                        // Each unordered projectile pair exchanges damage once.
                        // Without reciprocal history it was processed again
                        // when the second surviving projectile took its turn.
                        if self.projectiles[j].hit.len() < 24 {
                            let current_id = self.projectiles[i].id;
                            self.projectiles[j].hit.push(current_id);
                        }
                        self.projectiles[j].penetration -= applied / 20.;
                        if self.projectiles[j].penetration <= 0. {
                            self.projectiles[j].lifetime = 0.;
                        }
                    }
                }
                self.projectiles[i].penetration -= penetration_cost;
                if self.projectiles[i].penetration <= 0.0 {
                    self.projectiles[i].pos = impact;
                }
                if matches!(
                    kind,
                    ProjectileKind::Explosive | ProjectileKind::Mine | ProjectileKind::Fragment
                ) {
                    self.projectiles[i].pos = impact;
                    self.explode(impact, radius * 8. + 45., damage * 0.7, owner, team);
                    self.projectiles[i].lifetime = 0.;
                    if kind == ProjectileKind::Fragment {
                        for n in 0..6 {
                            let dir = V2::from_angle(
                                n as f32 * std::f32::consts::TAU / 6. + velocity.angle(),
                            );
                            let frag = self.simple_projectile(
                                owner,
                                team,
                                impact,
                                dir * 400.,
                                ProjectileKind::Scatter,
                                damage * 0.3,
                                4.,
                                0.7,
                                1.,
                            );
                            spawned.push(frag);
                        }
                    }
                    break;
                }
                if kind.is_drone() {
                    self.projectiles[i].hit.clear();
                    self.projectiles[i].pos = impact - velocity.normalized() * 12.;
                    self.projectiles[i].vel *= -0.4;
                    break;
                }
            }
            if self.projectiles[i].lifetime <= 0.
                && matches!(kind, ProjectileKind::Explosive | ProjectileKind::Fragment)
                && self.projectiles[i].hit.is_empty()
            {
                self.explode(end, radius * 8. + 45., damage * 0.6, owner, team);
            }
            if kind == ProjectileKind::Minion
                && let Some(shot) = self.fire_minion(i, spawned.len())
            {
                spawned.push(shot);
            }
        }
        let room = self
            .config
            .max_projectiles
            .saturating_sub(self.projectiles.len());
        self.projectiles.extend(spawned.into_iter().take(room));
        self.collision_hits = hits;
    }
    fn steer_projectile(&mut self, i: usize) {
        if self.projectiles[i].kind == ProjectileKind::Homing {
            self.steer_homing(i);
            return;
        }
        let (kind, pos, owner, team, speed, id) = {
            let p = &self.projectiles[i];
            (p.kind, p.pos, p.owner, p.team, p.cruise_speed, p.id)
        };
        if kind.is_drone() {
            let desired = if let Some(t) = self.tanks.iter().find(|t| t.id == owner && t.alive) {
                if t.input.alt_fire {
                    Some(t.pos + V2::from_angle(self.tick as f32 * 0.03 + drone_phase(id)) * 90.)
                } else if t.input.fire {
                    let cursor = t.pos + V2::from_angle(t.aim) * t.input.aim_distance.min(1600.);
                    if kind == ProjectileKind::Minion {
                        // Factory minions need room to shoot, not a damn pileup on the cursor.
                        // Each ID gets its own moving slot; the owner still chooses the target.
                        let orbit = drone_phase(id) + self.tick as f32 * DT * 0.8;
                        Some(cursor + V2::from_angle(orbit) * 90.0)
                    } else {
                        Some(cursor)
                    }
                } else if kind == ProjectileKind::Minion {
                    let orbit = drone_phase(id) + self.tick as f32 * DT * 0.7;
                    if let Some(target) = self.minion_target(pos, team, owner, 420.0) {
                        // Give the little bastards a firing circle instead of ramming.
                        Some(target + V2::from_angle(orbit) * 90.0)
                    } else {
                        Some(t.pos + V2::from_angle(orbit) * 110.0)
                    }
                } else {
                    self.nearest_position(pos, team, owner, 700.).or(Some(
                        t.pos + V2::from_angle(self.tick as f32 * 0.02 + drone_phase(id)) * 120.,
                    ))
                }
            } else if let Some(b) = self.bosses.iter().find(|b| b.id == owner && b.health > 0.) {
                self.nearest_position(pos, team, owner, 1000.)
                    .or(Some(b.pos))
            } else {
                self.projectiles[i].lifetime = 0.;
                None
            };
            if let Some(goal) = desired {
                let offset = goal - pos;
                // Arrive at a nearby cursor or guard slot without orbiting
                // forever at full speed. Distant commands keep full speed.
                let desired_speed = speed.max(170.) * (offset.length() / 90.).clamp(0.0, 1.0);
                let target = offset.normalized() * desired_speed;
                self.projectiles[i].vel = self.projectiles[i].vel.lerp(target, (DT * 5.).min(1.));
            }
        }
    }
    pub(crate) fn nearest_position(
        &self,
        pos: V2,
        team: u8,
        owner: EntityId,
        range: f32,
    ) -> Option<V2> {
        let mut best = range * range;
        let mut target = None;
        for t in &self.tanks {
            let d = (pos - t.pos).length_squared();
            if t.alive
                && t.id != owner
                && t.protection <= 0.
                && self.hostile(team, t.team)
                && d < best
            {
                best = d;
                target = Some(t.pos);
            }
        }
        for b in &self.bosses {
            let d = (pos - b.pos).length_squared();
            if b.health > 0. && self.hostile(team, b.team) && d < best {
                best = d;
                target = Some(b.pos);
            }
        }
        if target.is_none() {
            for t in &self.targets {
                let d = (pos - t.pos).length_squared();
                if t.health > 0. && d < best {
                    best = d;
                    target = Some(t.pos);
                }
            }
        }
        target
    }
    pub(crate) fn nearest_aim(
        &self,
        pos: V2,
        team: u8,
        owner: EntityId,
        range: f32,
    ) -> Option<f32> {
        self.nearest_position(pos, team, owner, range)
            .map(|p| (p - pos).angle())
    }
    pub(crate) fn explode(&mut self, pos: V2, radius: f32, damage: f32, owner: EntityId, team: u8) {
        self.grid.query(pos, pos, radius + 160., &mut self.scratch);
        let bodies = self.scratch.clone();
        for body in bodies {
            match body {
                Body::Tank(i) => {
                    let t = &self.tanks[i];
                    let distance = t.pos.distance(pos);
                    if t.id != owner && self.hostile(team, t.team) && distance < radius + t.radius {
                        let normal = (t.pos - pos).normalized();
                        self.damage_tank(
                            i,
                            damage * (1. - distance / (radius + t.radius)).max(0.15),
                            owner,
                            normal * damage * 2.,
                        );
                    }
                }
                Body::Target(i) => {
                    let t = &mut self.targets[i];
                    if t.pos.distance(pos) < radius + t.radius && t.health > 0. {
                        t.health -= damage;
                        let id = t.id;
                        self.record_damage(id, owner, damage);
                    }
                }
                Body::Boss(i) => {
                    let b = &mut self.bosses[i];
                    if b.id != owner
                        && b.pos.distance(pos) < radius + b.radius
                        && b.health > 0.
                        && (team == 0 || team != b.team)
                    {
                        b.health -= damage;
                        let id = b.id;
                        self.record_damage(id, owner, damage);
                    }
                }
                _ => {}
            }
        }
        self.event(EventKind::Destroy, pos, owner, radius as u32, team);
    }
}
fn drone_phase(id: EntityId) -> f32 {
    // Stable across projectile removals and vector compaction.
    (id % 4096) as f32 * 2.399_963_1
}
fn clone_filtered<T: Clone>(
    destination: &mut Vec<T>,
    source: &[T],
    mut keep: impl FnMut(&T) -> bool,
) {
    let mut count = 0;
    for item in source.iter().filter(|item| keep(item)) {
        if let Some(existing) = destination.get_mut(count) {
            existing.clone_from(item);
        } else {
            destination.push(item.clone());
        }
        count += 1;
    }
    destination.truncate(count);
}
pub fn sanitize_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    *c,
                    '<' | '>' | '\u{061c}' | '\u{200e}' | '\u{200f}'
                        | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
        })
        .take(24)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "Pilot".into()
    } else {
        clean.into()
    }
}
pub fn target_sides(kind: TargetKind) -> u8 {
    match kind {
        TargetKind::Square | TargetKind::Reinforced => 4,
        TargetKind::Triangle | TargetKind::Mobile => 3,
        TargetKind::Pentagon | TargetKind::Rare => 5,
        TargetKind::Fortress => 8,
        TargetKind::Custom(sides) => sides,
    }
}
fn boundary_hits(start: V2, end: V2, r: f32, config: &WorldConfig) -> [Option<(f32, V2)>; 2] {
    let d = end - start;
    let mut hits = [None; 2];
    for (index, (a, b, delta, low, high, axis)) in [
        (start.x, end.x, d.x, r, config.width - r, true),
        (start.y, end.y, d.y, r, config.height - r, false),
    ]
    .into_iter()
    .enumerate()
    {
        if b < low && delta < 0. {
            hits[index] = Some((
                ((low - a) / delta).clamp(0., 1.),
                if axis {
                    V2::new(1., 0.)
                } else {
                    V2::new(0., 1.)
                },
            ));
        }
        if b > high && delta > 0. {
            hits[index] = Some((
                ((high - a) / delta).clamp(0., 1.),
                if axis {
                    V2::new(-1., 0.)
                } else {
                    V2::new(0., -1.)
                },
            ));
        }
    }
    hits
}

#[cfg(test)]
mod name_hardening_tests {
    use super::*;

    #[test]
    fn offline_names_strip_unicode_direction_overrides() {
        assert_eq!(sanitize_name("\u{2066}Pilot\u{2069}"), "Pilot");
        assert_eq!(sanitize_name("bob\u{202e}txt"), "bobtxt");
        assert_eq!(sanitize_name("世界 艾"), "世界 艾");
    }
}
