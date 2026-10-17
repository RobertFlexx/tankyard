//! Server-authoritative utility decisions, tactical steering and cached A*.
//!
//! All bots send the same input, stat and evolution commands as humans. Their
//! difficulty changes perception, reaction delay, aim and avoidance. Combat stats
//! follow the same rules. During a match, bots estimate visible tanks' motion and
//! adjust spacing after damage, kills, and deaths. Outcome networks influence
//! legal tank and skill choices. That memory lasts across lives, within a match.
mod learning;
mod navigation;
mod neural;
mod perception;
mod policy;
mod relationships;

use navigation::{Navigation, clear_line};
use perception::{Perception, Seen};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tankyard_content::{Ability, ClassDef};
use tankyard_core::{EntityId, Mode, PlayerInput, Tank, V2, World, physics};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Beginner,
    #[default]
    Normal,
    Advanced,
    Expert,
}

impl Difficulty {
    pub const ALL: [Self; 4] = [Self::Beginner, Self::Normal, Self::Advanced, Self::Expert];

    pub fn name(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Normal => "Normal",
            Self::Advanced => "Advanced",
            Self::Expert => "Expert",
        }
    }

    fn reaction_ticks(self) -> u64 {
        match self {
            Self::Beginner => 20,
            Self::Normal => 12,
            Self::Advanced => 8,
            Self::Expert => 7,
        }
    }

    fn perception_radius(self) -> f32 {
        match self {
            Self::Beginner => 420.,
            Self::Normal => 500.,
            Self::Advanced => 560.,
            Self::Expert => 620.,
        }
    }

    fn prediction(self) -> f32 {
        match self {
            Self::Beginner => 0.15,
            Self::Normal => 0.55,
            Self::Advanced => 0.8,
            Self::Expert => 0.9,
        }
    }

    fn aim_error(self) -> f32 {
        match self {
            Self::Beginner => 0.12,
            Self::Normal => 0.075,
            Self::Advanced => 0.045,
            Self::Expert => 0.03,
        }
    }

    fn dodge_strength(self) -> f32 {
        match self {
            Self::Beginner => 0.2,
            Self::Normal => 0.5,
            Self::Advanced => 0.75,
            Self::Expert => 0.9,
        }
    }

    fn retreat_health(self) -> f32 {
        match self {
            Self::Beginner => 0.2,
            Self::Normal => 0.32,
            Self::Advanced => 0.42,
            Self::Expert => 0.38,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BotState {
    Farming,
    #[default]
    Exploring,
    Attacking,
    Defending,
    Retreating,
    Capturing,
    Escorting,
    BossEngagement,
    Repositioning,
}

#[derive(Clone, Copy)]
struct TargetSample {
    id: EntityId,
    pos: V2,
    velocity: V2,
    radius: f32,
}

#[derive(Default)]
struct Brain {
    learning: learning::Learning,
    deaths: u32,
    initialized: bool,
    state: BotState,
    goal: V2,
    target: Option<TargetSample>,
    path: VecDeque<V2>,
    path_cell: Option<usize>,
    next_path: u64,
    last_path: u64,
    last_decision_tick: u64,
    dodge: V2,
    danger: f32,
    parry_eta: Option<f32>,
    escape: V2,
    steering: V2,
}

enum Command {
    Stat(EntityId, usize),
    Evolve(EntityId, String),
}

pub struct BotController {
    pub difficulty: Difficulty,
    brains: HashMap<EntityId, Brain>,
    perception: Perception,
    navigation: Navigation,
    seen: Vec<Seen>,
    commands: Vec<Command>,
    inputs: Vec<(EntityId, PlayerInput)>,
}

impl BotController {
    pub fn new(difficulty: Difficulty) -> Self {
        Self {
            difficulty,
            brains: HashMap::new(),
            perception: Perception::default(),
            navigation: Navigation::default(),
            seen: Vec::with_capacity(256),
            commands: Vec::with_capacity(128),
            inputs: Vec::with_capacity(128),
        }
    }

    /// Keep learned weights when a server starts its next round. New bots get
    /// fresh tactical state and no information about past players or positions.
    pub fn inherit_models(&mut self, previous: &Self, pairs: &[(EntityId, EntityId)]) {
        for &(old_id, new_id) in pairs {
            if let Some(old) = previous.brains.get(&old_id) {
                self.brains
                    .entry(new_id)
                    .or_default()
                    .learning
                    .inherit_models(&old.learning);
            }
        }
    }

    pub fn state(&self, id: EntityId) -> Option<BotState> {
        self.brains.get(&id).map(|brain| brain.state)
    }

    pub fn target(&self, id: EntityId) -> Option<EntityId> {
        self.brains
            .get(&id)
            .and_then(|brain| brain.target.map(|target| target.id))
    }

    /// Call once immediately before each fixed authoritative simulation tick.
    pub fn update(&mut self, world: &mut World) {
        if world.match_state.finished {
            return;
        }
        self.brains
            .retain(|id, _| world.tanks.iter().any(|tank| tank.id == *id && tank.bot));
        // Feedback also runs for freshly destroyed bots, while kill notices
        // still identify the opponent. That memory survives the tactical reset.
        for tank in world.tanks.iter().filter(|tank| tank.bot) {
            let brain = self.brains.entry(tank.id).or_default();
            brain.learning.feedback(tank, world.tick, &world.events);
            if let Some(class) = world.content.class(&tank.class)
                && (tank.alive || brain.deaths != tank.deaths)
            {
                brain.learning.build.observe(
                    world.tick,
                    tank,
                    class,
                    brain.target.is_some() || brain.danger > 0.25,
                );
            }
            if brain.deaths != tank.deaths {
                let learning = std::mem::take(&mut brain.learning);
                *brain = Brain {
                    learning,
                    deaths: tank.deaths,
                    ..Default::default()
                };
            }
        }
        if !world.tanks.iter().any(|tank| tank.bot && tank.alive) {
            return;
        }
        self.navigation
            .refresh(world.config.width, world.config.height, &world.walls);
        self.perception.rebuild(world);
        self.commands.clear();
        self.inputs.clear();
        let difficulty = self.difficulty;
        for tank in world.tanks.iter().filter(|tank| tank.bot && tank.alive) {
            let Some(class) = world.content.class(&tank.class) else {
                continue;
            };
            let brain = self.brains.entry(tank.id).or_default();
            if brain.target.is_some_and(|target| {
                !self.perception.target_valid(target.id, tank.team)
                    || tank.pos.distance(target.pos) > difficulty.perception_radius()
            }) {
                // Invalid or out-of-sight samples stop firing immediately,
                // without waiting for the next tactical decision.
                brain.target = None;
                brain.initialized = false;
            }
            let period = difficulty.reaction_ticks();
            // IDs stagger the expensive utility decisions across the tick budget.
            if !brain.initialized
                || world
                    .tick
                    .wrapping_add(tank.id.wrapping_mul(7))
                    .is_multiple_of(period)
            {
                self.perception
                    .query(tank.pos, difficulty.perception_radius(), &mut self.seen);
                decide(world, tank, class, difficulty, &self.seen, brain);
                brain.last_decision_tick = world.tick;
                progression_commands(
                    world,
                    tank,
                    class,
                    &brain.learning.build,
                    &mut self.commands,
                );
                brain.initialized = true;
            }
            let dodge_period = match difficulty {
                Difficulty::Beginner => 12,
                Difficulty::Normal => 6,
                Difficulty::Advanced => 5,
                Difficulty::Expert => 4,
            };
            if world
                .tick
                .wrapping_add(tank.id)
                .is_multiple_of(dodge_period)
            {
                self.perception.query(tank.pos, 340., &mut self.seen);
                let (dodge, danger, escape) = projectile_avoidance(world, tank, &self.seen);
                let next_dodge = dodge * difficulty.dodge_strength();
                // High-priority evasion ramps up quickly but does not jerk the
                // bot across its whole steering envelope in one tick.
                brain.dodge = brain.dodge.lerp(next_dodge, 0.55);
                brain.danger = danger;
                brain.escape = escape;
                brain.parry_eta = if class.ability.is_parry() {
                    parry_opportunity(world, tank, &self.seen)
                } else {
                    None
                };
            }
            let mut movement = navigate(world, tank, brain, &mut self.navigation);
            // Guided threats can turn after the current collision prediction.
            // Give visible nearby swarms room instead of driving into them.
            let retreat_weight = (brain.escape.length() * 0.9).min(0.9);
            movement =
                movement * (1. - retreat_weight) + brain.escape.normalized() * retreat_weight;
            self.perception.query(tank.pos, 110., &mut self.seen);
            movement += local_steering(world, tank, class, &self.seen);
            movement += brain.dodge;
            movement = movement.clamp_length(1.);
            let steering_rate: f32 = if brain.danger > 0.75 { 21.0 } else { 7.5 };
            let smoothing = 1.0 - (-steering_rate / 60.0).exp();
            brain.steering = brain.steering.lerp(movement, smoothing).clamp_length(1.);
            movement = brain.steering;

            let (mut desired_aim, mut fire, target_distance) = if let Some(target) = brain.target {
                // Continue the last observed motion between decisions.
                let sample_age =
                    (world.tick.saturating_sub(brain.last_decision_tick) as f32 / 60.0).min(0.45);
                let estimated = target.pos + target.velocity * sample_age;
                let speed = class
                    .barrels
                    .iter()
                    .filter(|barrel| barrel.speed > 0. && barrel.angle.abs() < 0.8)
                    .map(|barrel| barrel.speed)
                    .next()
                    .unwrap_or(350.)
                    * tankyard_core::stat_scale(tank.stats[3])
                    * class.multipliers[3];
                let confidence = brain.learning.confidence(target.id);
                let horizon = 0.12 + confidence * difficulty.prediction() * 0.6;
                let offset = (intercept(tank.pos, estimated, target.velocity, speed) - estimated)
                    .clamp_length(target.velocity.length() * horizon);
                let lead =
                    estimated + offset * difficulty.prediction() * (0.35 + confidence * 0.65);
                let drift_time = world.tick as f32 / (36. + (tank.id % 23) as f32);
                let epoch = drift_time.floor() as u64;
                let fraction = drift_time.fract();
                let blend = fraction * fraction * (3. - 2. * fraction);
                let drift = noise(tank.id ^ epoch.wrapping_mul(37)) * (1. - blend)
                    + noise(tank.id ^ epoch.wrapping_add(1).wrapping_mul(37)) * blend;
                let error = (drift * 2. - 1.) * difficulty.aim_error() * (1.5 - confidence * 0.5);
                let distance = tank.pos.distance(estimated);
                let range = class.ai_range.max(100.) * 1.6 + target.radius;
                let fire = distance < range.min(difficulty.perception_radius())
                    && clear_line(tank.pos, estimated, 5., &world.walls);
                ((lead - tank.pos).angle() + error, fire, distance)
            } else {
                ((brain.goal - tank.pos).angle(), false, f32::MAX)
            };
            let escaping = brain.state == BotState::Retreating
                && matches!(class.ability, Ability::Dash | Ability::Blink);
            if escaping {
                desired_aim = (brain.goal - tank.pos).angle();
                fire = false;
            }
            let turn_rate = 3.5 + difficulty.prediction() * 7.;
            let difference = wrapped_angle(desired_aim - tank.aim);
            let aim = tank.aim + difference.clamp(-turn_rate / 60., turn_rate / 60.);
            // Heavy, slowly reloading cannons should wait until nearly aligned
            // rather than wasting their only shell while rotating toward prey.
            if class.barrels.iter().any(|barrel| barrel.recoil > 70.0)
                && class.barrels.len() <= 2
                && wrapped_angle(desired_aim - aim).abs() > 0.14
            {
                fire = false;
            }
            if class.barrels.iter().any(|barrel| {
                barrel.angle.abs() < 0.8 && barrel.pattern != tankyard_content::FirePattern::Auto
            }) && wrapped_angle(desired_aim - aim).abs() > 0.22
            {
                fire = false;
            }
            if brain.state == BotState::Retreating && target_distance > 300.0 {
                fire = false;
            }
            let deploy = class.has_deployables()
                && matches!(
                    brain.state,
                    BotState::Capturing | BotState::Defending | BotState::Escorting
                )
                && tank.pos.distance(brain.goal) < 250.;
            let retreat = brain.state == BotState::Retreating;
            let facing = !matches!(class.ability, Ability::Dash | Ability::Blink)
                || wrapped_angle(desired_aim - aim).abs() < 0.25;
            let ability = tank.ability_cooldown <= 0.
                && facing
                && use_ability(class, tank, brain, target_distance);
            self.inputs.push((
                tank.id,
                PlayerInput {
                    sequence: tank.last_input.wrapping_add(1),
                    movement,
                    aim,
                    aim_distance: if class.barrels.iter().any(|b| b.projectile.is_drone()) {
                        target_distance.clamp(150., difficulty.perception_radius())
                    } else {
                        difficulty.perception_radius()
                    },
                    fire: fire || deploy,
                    alt_fire: retreat && class.barrels.iter().any(|b| b.projectile.is_drone()),
                    ability,
                },
            ));
        }
        for command in self.commands.drain(..) {
            match command {
                Command::Stat(id, index) => {
                    world.upgrade_stat(id, index);
                }
                Command::Evolve(id, class) => {
                    world.evolve(id, &class);
                }
            }
        }
        for (id, input) in self.inputs.drain(..) {
            world.set_input(id, input);
        }
    }
}

fn hostile(team: u8, other: u8) -> bool {
    team == 0 || other == 0 || team != other
}

fn noise(mut value: u64) -> f32 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((value ^ (value >> 31)) >> 40) as f32 / 16_777_216.
}

fn wrapped_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn progression_commands(
    world: &World,
    tank: &Tank,
    class: &ClassDef,
    learning: &policy::BuildLearning,
    commands: &mut Vec<Command>,
) {
    let melee = class.multipliers[2] >= 2.;
    let drones = class
        .barrels
        .iter()
        .any(|barrel| barrel.projectile.is_drone());
    let desired = if melee {
        [7, 5, 7, 2, 2, 2, 1, 7]
    } else if drones {
        [5, 3, 0, 2, 6, 7, 7, 3]
    } else if class.has_deployables() {
        [6, 3, 1, 1, 7, 6, 6, 3]
    } else {
        [4, 2, 0, 4, 5, 7, 7, 4]
    };
    let mut points = tank.points;
    let mut stats = tank.stats;
    while points > 0 {
        let next = (0..8)
            .filter(|&index| stats[index] < 7 && stats[index] < desired[index])
            .min_by(|&a, &b| {
                let ratio_a = f32::from(stats[a]) / f32::from(desired[a].max(1))
                    - learning.skill_value(class, stats, a);
                let ratio_b = f32::from(stats[b]) / f32::from(desired[b].max(1))
                    - learning.skill_value(class, stats, b);
                ratio_a.total_cmp(&ratio_b).then_with(|| b.cmp(&a))
            })
            .or_else(|| (0..8).find(|&index| stats[index] < 7));
        let Some(index) = next else {
            break;
        };
        commands.push(Command::Stat(tank.id, index));
        stats[index] += 1;
        points -= 1;
    }
    let eligible = || {
        class.upgrades.iter().filter(|id| {
            world
                .content
                .class(id)
                .is_some_and(|next| tank.level >= next.min_level)
        })
    };
    if let Some(next) = eligible()
        .filter_map(|id| world.content.class(id))
        .max_by(|a, b| {
            learning
                .class_score(tank, a)
                .total_cmp(&learning.class_score(tank, b))
        })
    {
        commands.push(Command::Evolve(tank.id, next.id.clone()));
    }
}

fn decide(
    world: &World,
    tank: &Tank,
    class: &ClassDef,
    difficulty: Difficulty,
    seen: &[Seen],
    brain: &mut Brain,
) {
    let health = tank.health / tank.max_health.max(1.);
    let melee = class.multipliers[2] >= 2.
        && (class.ai_range < 120.
            || class
                .barrels
                .iter()
                .all(|barrel| barrel.pattern == tankyard_content::FirePattern::Auto));
    let mut best_target: Option<(f32, TargetSample, BotState)> = None;
    let mut nearest_enemy: Option<(f32, TargetSample)> = None;
    let mut enemy_pressure = 0.;
    let mut support = 0.;
    let mut threat_center = V2::ZERO;
    let mut nearby_enemies = 0;
    for &item in seen {
        let (sample, utility, state) = match item {
            Seen::Tank(index) => {
                let other = &world.tanks[index];
                if other.id == tank.id || !other.alive || other.protection > 0. {
                    continue;
                }
                let distance = tank.pos.distance(other.pos);
                if distance > difficulty.perception_radius() {
                    continue;
                }
                let strength = (f32::from(other.level) + 12.) / (f32::from(tank.level) + 12.)
                    * (other.max_health / tank.max_health.max(1.)).sqrt();
                let weak = 1. - (other.health / other.max_health.max(1.)).clamp(0., 1.);
                let visible = clear_line(tank.pos, other.pos, tank.radius * 0.35, &world.walls);
                let mut pressure = strength.clamp(0.3, 2.)
                    * (1. - weak * 0.65)
                    * (1. - distance / 850.).clamp(0., 1.);
                if visible && !other.bot {
                    brain
                        .learning
                        .build
                        .observe_player(other, &world.content, world.tick);
                }
                if !hostile(tank.team, other.team) {
                    if visible {
                        support += pressure;
                    }
                    continue;
                }
                if !visible {
                    continue;
                }
                let motion = brain.learning.observe(other.id, other.pos, world.tick);
                let disposition = brain.learning.relationships.get(other.id, world.tick);
                pressure *= 1. + disposition.fear * 0.4;
                let sample = TargetSample {
                    id: other.id,
                    pos: other.pos,
                    velocity: motion.velocity,
                    radius: other.radius,
                };
                if nearest_enemy.is_none_or(|(previous, _)| distance < previous) {
                    nearest_enemy = Some((distance, sample));
                }
                if pressure > 0. {
                    nearby_enemies += 1;
                    enemy_pressure += pressure;
                    threat_center += other.pos * pressure;
                }
                let awareness = match difficulty {
                    Difficulty::Beginner => 0.15,
                    Difficulty::Normal => 0.6,
                    Difficulty::Advanced => 0.9,
                    Difficulty::Expert => 1.,
                };
                let caution = 1. / (1. + (strength - 1.).max(0.) * awareness)
                    * if strength > 1.6 && health < 0.7 {
                        0.35
                    } else {
                        1.
                    };
                let persistence = if brain.target.is_some_and(|target| target.id == other.id) {
                    1.30
                } else {
                    1.0
                };
                // A grudge only encourages a fight the bot can reasonably take.
                // Fear of a stronger player outweighs that urge; neither can
                // reveal a hidden opponent or override recovery/objectives.
                let boldness = noise(tank.id.wrapping_mul(103));
                let manageable =
                    ((health - 0.4) / 0.6).clamp(0., 1.) * (1.8 - strength).clamp(0., 1.);
                let rivalry = 1. + disposition.animosity * manageable * (0.25 + boldness * 0.45);
                let respect = 1.
                    / (1. + disposition.fear * strength.clamp(0.5, 2.) * (0.65 - boldness * 0.5));
                let utility = (1.15 + weak * 1.2) * caution * persistence * rivalry * respect
                    / (1. + distance / 420.);
                (sample, utility, BotState::Attacking)
            }
            Seen::Target(index) => {
                if world.config.mode == Mode::CoopPve && tank.team == 2 {
                    continue;
                }
                let target = &world.targets[index];
                if target.health <= 0. {
                    continue;
                }
                let distance = tank.pos.distance(target.pos);
                if distance > difficulty.perception_radius() {
                    continue;
                }
                let efficiency = (target.xp as f32 / target.health.max(20.)).clamp(0.3, 3.);
                let visible = clear_line(tank.pos, target.pos, 4.0, &world.walls);
                if !visible {
                    continue;
                }
                let persistence = if brain
                    .target
                    .is_some_and(|previous| previous.id == target.id)
                {
                    1.22
                } else {
                    1.0
                };
                let utility = (0.4 + efficiency * 0.18) * persistence / (1. + distance / 300.);
                (
                    TargetSample {
                        id: target.id,
                        pos: target.pos,
                        velocity: target.vel,
                        radius: target.radius,
                    },
                    utility,
                    BotState::Farming,
                )
            }
            Seen::Boss(index) => {
                let boss = &world.bosses[index];
                if boss.health <= 0. || !hostile(tank.team, boss.team) {
                    continue;
                }
                let distance = tank.pos.distance(boss.pos);
                if distance > difficulty.perception_radius()
                    || !clear_line(tank.pos, boss.pos, tank.radius * 0.35, &world.walls)
                {
                    continue;
                }
                let priority = if matches!(
                    world.config.mode,
                    Mode::BossRush | Mode::Mothership | Mode::CoopPve
                ) {
                    2.0
                } else {
                    0.75
                };
                (
                    TargetSample {
                        id: boss.id,
                        pos: boss.pos,
                        velocity: boss.vel,
                        radius: boss.radius,
                    },
                    priority / (1. + distance / 650.),
                    BotState::BossEngagement,
                )
            }
            Seen::Projectile(_) => continue,
        };
        if best_target
            .as_ref()
            .is_none_or(|(best, _, _)| utility > *best)
        {
            best_target = Some((utility, sample, state));
        }
    }
    let wander_epoch = world.tick / 240;
    let mut goal = V2::new(
        100. + noise(tank.id ^ wander_epoch.wrapping_mul(31)) * (world.config.width - 200.),
        100. + noise(tank.id.wrapping_mul(17) ^ wander_epoch.wrapping_mul(43))
            * (world.config.height - 200.),
    );
    let mut state = BotState::Exploring;
    let mut priority = 0.06;
    let mut target = best_target.map(|(_, sample, _)| sample);
    if let Some((utility, sample, action)) = best_target {
        priority = utility;
        state = action;
        let distance = tank.pos.distance(sample.pos);
        let disposition = brain.learning.relationships.get(sample.id, world.tick);
        let preferred = if melee {
            0.
        } else {
            (class.ai_range.clamp(100., 600.)
                * 0.82
                * (0.9 + noise(tank.id.wrapping_mul(53)) * 0.2)
                * (1. + (brain.learning.caution - 0.35) * 0.3)
                * (1. + disposition.fear * 0.22)
                + sample.radius)
                .min(difficulty.perception_radius() * 0.82)
        };
        let outward = (tank.pos - sample.pos).normalized();
        let epoch = world.tick / (180 + tank.id % 121);
        let side = if noise(tank.id ^ epoch.wrapping_mul(17)) > 0.5 {
            1.
        } else {
            -1.
        };
        let strafe = V2::new(-outward.y, outward.x) * side;
        goal = if melee {
            let lead = (distance / (tank.vel.length().max(180.) + 100.)).clamp(0.12, 0.4);
            let intercept = sample.pos + sample.velocity * lead;
            let dangerous_front = world
                .tanks
                .iter()
                .find(|other| other.id == sample.id)
                .is_some_and(|other| {
                    let facing = V2::from_angle(other.aim).dot(outward) > 0.75;
                    facing
                        && world.content.class(&other.class).is_some_and(|enemy| {
                            enemy
                                .barrels
                                .iter()
                                .any(|barrel| barrel.damage >= 25. || barrel.recoil >= 70.)
                        })
                });
            if distance > 155. && dangerous_front {
                intercept + strafe * (distance * 0.3).min(100.)
            } else {
                intercept
            }
        } else if distance > preferred * 1.25 {
            sample.pos + outward * preferred
        } else {
            sample.pos + outward * preferred + strafe * (90. + difficulty.prediction() * 90.)
        };
    }
    let personal_fear = nearest_enemy.map_or(0., |(_, enemy)| {
        brain.learning.relationships.get(enemy.id, world.tick).fear
    });
    let recovering = health
        < difficulty.retreat_health()
            + (brain.learning.caution - 0.35) * 0.12
            + personal_fear * 0.12
        || (brain.state == BotState::Retreating && health < 0.68);
    let pressure_limit = match difficulty {
        Difficulty::Beginner => f32::INFINITY,
        Difficulty::Normal => 2.2,
        Difficulty::Advanced => 1.9,
        Difficulty::Expert => 1.7,
    };
    let outnumbered =
        nearby_enemies >= 2 && health < 0.85 && enemy_pressure > (1. + support) * pressure_limit;
    if (recovering && (nearest_enemy.is_some() || brain.state == BotState::Retreating))
        || outnumbered
    {
        priority = 5.;
        state = if recovering {
            BotState::Retreating
        } else {
            BotState::Repositioning
        };
        goal = if recovering && tank.team > 0 && world.config.mode.teams() > 1 {
            world.team_home(tank.team)
        } else if let Some((_, enemy)) = nearest_enemy {
            let threat = if enemy_pressure > 0. {
                threat_center / enemy_pressure
            } else {
                enemy.pos
            };
            tank.pos + (tank.pos - threat).normalized() * 650.
        } else {
            brain.goal
        };
        // Cover the escape against the pursuer rather than keeping an old
        // farming target while enemies are in contact range.
        target = nearest_enemy.map(|(_, enemy)| enemy);
    }
    if let Some((_, objective_goal, action)) = objective_goal(world, tank)
        .filter(|(objective_priority, _, _)| priority < 5. && *objective_priority > priority)
    {
        state = action;
        goal = objective_goal;
    }
    if world.config.mode == Mode::Survival {
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        if tank.pos.distance(center) > world.match_state.safe_radius - tank.radius - 95. {
            state = BotState::Repositioning;
            goal = center;
        }
    }
    if state == BotState::Exploring {
        target = None;
    }
    brain.state = state;
    brain.goal = V2::new(
        goal.x.clamp(48., world.config.width - 48.),
        goal.y.clamp(48., world.config.height - 48.),
    );
    brain.target = target;
}

fn objective_goal(world: &World, tank: &Tank) -> Option<(f32, V2, BotState)> {
    match world.config.mode {
        Mode::CoopPve => {
            let enemy = world
                .tanks
                .iter()
                .filter(|other| other.alive && other.health > 0.0 && hostile(tank.team, other.team))
                .map(|other| other.pos)
                .chain(
                    world
                        .bosses
                        .iter()
                        .filter(|boss| boss.health > 0.0 && hostile(tank.team, boss.team))
                        .map(|boss| boss.pos),
                )
                .min_by(|a, b| tank.pos.distance(*a).total_cmp(&tank.pos.distance(*b)));
            if let Some(enemy) = enemy {
                return (tank.pos.distance(enemy) > 650.0).then_some((
                    1.1,
                    enemy,
                    BotState::Attacking,
                ));
            }
            world
                .tanks
                .iter()
                .filter(|other| other.alive && !other.bot && other.team == tank.team)
                .min_by(|a, b| {
                    tank.pos
                        .distance(a.pos)
                        .total_cmp(&tank.pos.distance(b.pos))
                })
                .filter(|human| tank.pos.distance(human.pos) > 260.0)
                .map(|human| (0.7, human.pos, BotState::Escorting))
        }
        Mode::Domination => world
            .objectives
            .iter()
            .map(|objective| {
                let desired = objective.team != tank.team || objective.contested;
                let priority = if desired { 1.5 } else { 0.12 };
                let coverage = world
                    .tanks
                    .iter()
                    .filter(|ally| {
                        ally.id != tank.id
                            && ally.alive
                            && ally.team == tank.team
                            && ally.pos.distance(objective.pos) < objective.radius + 240.
                    })
                    .count() as f32;
                let state = if objective.team == tank.team {
                    BotState::Defending
                } else {
                    BotState::Capturing
                };
                (
                    priority
                        / ((1. + tank.pos.distance(objective.pos) / 1800.)
                            * (1. + coverage * 0.45)),
                    objective.pos,
                    state,
                )
            })
            .max_by(|a, b| a.0.total_cmp(&b.0)),
        Mode::CaptureCore => {
            if world
                .objectives
                .iter()
                .any(|objective| objective.carrier == Some(tank.id))
            {
                return Some((4., world.team_home(tank.team), BotState::Escorting));
            }
            let mut best: Option<(f32, V2, BotState)> = None;
            for objective in &world.objectives {
                let candidate = if let Some(carrier) = objective.carrier {
                    if let Some(player) = world
                        .tanks
                        .iter()
                        .find(|player| player.id == carrier && player.alive)
                    {
                        if player.team == tank.team {
                            (
                                1.5,
                                player.pos + (tank.pos - player.pos).normalized() * 110.,
                                BotState::Escorting,
                            )
                        } else {
                            (2.2, player.pos, BotState::Defending)
                        }
                    } else {
                        continue;
                    }
                } else if objective.team != tank.team {
                    (1.75, objective.pos, BotState::Capturing)
                } else if objective.pos.distance(objective.home) > 30. {
                    (2.5, objective.pos, BotState::Defending)
                } else {
                    continue;
                };
                let utility = candidate.0 / (1. + tank.pos.distance(candidate.1) / 2200.);
                let candidate = (utility, candidate.1, candidate.2);
                if best
                    .as_ref()
                    .is_none_or(|previous| candidate.0 > previous.0)
                {
                    best = Some(candidate);
                }
            }
            best
        }
        Mode::Mothership => {
            let friendly = world
                .bosses
                .iter()
                .find(|boss| boss.mothership && boss.team == tank.team);
            if let Some(boss) = friendly {
                let damaged = boss.health / boss.max_health.max(1.) < 0.75;
                if damaged || noise(tank.id) < 0.3 {
                    let priority = if damaged { 1.8 } else { 0.65 };
                    let offset = V2::from_angle(noise(tank.id) * std::f32::consts::TAU)
                        * (boss.radius + 150.);
                    return Some((priority, boss.pos + offset, BotState::Defending));
                }
            }
            // Most healthy-team bots assault the other ship instead of camping
            // indefinitely outside perception range of the opposing command.
            world
                .bosses
                .iter()
                .filter(|boss| boss.mothership && hostile(tank.team, boss.team))
                .map(|boss| {
                    let distance = tank.pos.distance(boss.pos);
                    let approach =
                        boss.pos + (tank.pos - boss.pos).normalized() * (boss.radius + 300.);
                    (
                        1.35 / (1. + distance / 2500.),
                        approach,
                        BotState::BossEngagement,
                    )
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
        }
        _ => None,
    }
}

fn navigate(world: &World, tank: &Tank, brain: &mut Brain, navigation: &mut Navigation) -> V2 {
    let goal_distance = tank.pos.distance(brain.goal);
    if goal_distance < 20. {
        return V2::ZERO;
    }
    if clear_line(tank.pos, brain.goal, tank.radius + 4., &world.walls) {
        brain.path.clear();
        return (brain.goal - tank.pos).normalized() * (goal_distance / 90.).min(1.);
    }
    let cell = navigation.cell_of(brain.goal);
    if world.tick >= brain.next_path
        || (brain.path_cell != Some(cell) && world.tick.saturating_sub(brain.last_path) >= 24)
        || (brain.path.is_empty() && world.tick.saturating_sub(brain.last_path) >= 30)
    {
        brain.path = navigation
            .route(tank.pos, brain.goal, &world.walls, tank.radius + 4.)
            .into();
        brain.path_cell = Some(cell);
        brain.last_path = world.tick;
        brain.next_path = world.tick + 120 + tank.id % 45;
    }
    while brain
        .path
        .front()
        .is_some_and(|&waypoint| tank.pos.distance(waypoint) < 32.)
    {
        brain.path.pop_front();
    }
    while brain.path.len() >= 2
        && clear_line(tank.pos, brain.path[1], tank.radius + 4., &world.walls)
    {
        brain.path.pop_front();
    }
    brain
        .path
        .front()
        .map_or(V2::ZERO, |&next| (next - tank.pos).normalized())
}

fn local_steering(world: &World, tank: &Tank, class: &ClassDef, seen: &[Seen]) -> V2 {
    let mut steer = V2::ZERO;
    let forward = tank.pos + tank.vel * 0.2;
    for &wall in &world.walls {
        if let Some((normal, depth)) = physics::circle_rect(forward, tank.radius + 20., wall) {
            steer += normal * (depth / 45.).clamp(0., 0.65);
        }
    }
    for &item in seen {
        if let Seen::Tank(index) = item {
            let ally = &world.tanks[index];
            if ally.id == tank.id || !ally.alive || tank.team == 0 || ally.team != tank.team {
                continue;
            }
            let delta = tank.pos - ally.pos;
            let distance = delta.length();
            if distance < tank.radius + ally.radius + 30. {
                steer += delta.normalized() * 0.25;
            }
        } else if let Seen::Target(index) = item {
            if class.multipliers[2] >= 2. {
                continue;
            }
            // Keep a farming approach out of accidental contact-damage duels.
            let target = &world.targets[index];
            let delta = tank.pos - target.pos;
            if delta.length() < target.radius + tank.radius + 12. {
                steer += delta.normalized() * 0.45;
            }
        }
    }
    let margin = 65.;
    if tank.pos.x < margin {
        steer.x += 0.4;
    }
    if tank.pos.y < margin {
        steer.y += 0.4;
    }
    if tank.pos.x > world.config.width - margin {
        steer.x -= 0.4;
    }
    if tank.pos.y > world.config.height - margin {
        steer.y -= 0.4;
    }
    steer.clamp_length(0.9)
}

/// Threat estimate for a projectile relative to a moving tank. Only shots
/// with a plausible future impact influence steering. The tangent direction
/// costs less travel than reversing, which feels more like a skilled player.
fn dodge_threat(relative_position: V2, relative_velocity: V2, safety: f32) -> (V2, f32) {
    let speed_sq = relative_velocity.length_squared();
    if speed_sq < 1.0 {
        return (V2::ZERO, 0.0);
    }
    // Minimum distance on the next 0.8 seconds, not merely current distance.
    let arrival = (-relative_position.dot(relative_velocity) / speed_sq).clamp(0.0, 0.80);
    let closest = relative_position + relative_velocity * arrival;
    let distance = closest.length();
    if distance >= safety || arrival <= 0.0 && distance >= safety * 0.6 {
        return (V2::ZERO, 0.0);
    }
    let travel = relative_velocity.normalized();
    let tangent = V2::new(-travel.y, travel.x);
    let side = if tangent.dot(relative_position) >= 0.0 {
        -1.0
    } else {
        1.0
    };
    let strength =
        (1.0 - distance / safety).clamp(0.0, 1.0) * (1.0 - arrival / 1.10).clamp(0.25, 1.0);
    (tangent * side, strength)
}

fn projectile_avoidance(world: &World, tank: &Tank, seen: &[Seen]) -> (V2, f32, V2) {
    let mut dodge = V2::ZERO;
    let mut danger: f32 = 0.;
    let mut escape = V2::ZERO;
    for &item in seen {
        let Seen::Projectile(index) = item else {
            continue;
        };
        let projectile = &world.projectiles[index];
        if projectile.owner == tank.id
            || !hostile(tank.team, projectile.team)
            || projectile.damage <= 0.
            || projectile.lifetime <= 0.
            || projectile.penetration <= 0.
        {
            continue;
        }
        let delta = projectile.pos - tank.pos;
        if delta.length_squared() > 340.0 * 340.0
            || !clear_line(tank.pos, projectile.pos, projectile.radius, &world.walls)
        {
            continue;
        }
        let (direction, risk) = dodge_threat(
            delta,
            projectile.vel - tank.vel,
            tank.radius + projectile.radius + 22.0,
        );
        // Collision risk determines whether to dodge even a modest bullet.
        // Damage only increases urgency; it must not suppress lane avoidance.
        let severity = (projectile.damage / (tank.health.max(1.0) * 0.25))
            .sqrt()
            .clamp(1.0, 1.8);
        dodge += direction * risk * severity;
        danger = danger.max((risk * severity).min(1.0));
        let distance = delta.length();
        let drone = projectile.kind.is_drone();
        let approaching_homing = projectile.kind == tankyard_content::ProjectileKind::Homing
            && projectile.vel.dot(delta) < 0.
            && distance
                < projectile.vel.length() * projectile.lifetime + tank.radius + projectile.radius;
        if drone || approaching_homing {
            // A drone can change direction even if its current lane misses.
            // Homing shots already flying away or about to expire are ignored.
            let pursuit = ((340. - distance) / 260.).clamp(0., 1.);
            let away = (tank.pos - projectile.pos).normalized();
            dodge += away * pursuit * severity;
            escape += away * pursuit * 1.4;
            danger = danger.max((pursuit * severity).min(1.));
        }
    }
    (dodge.clamp_length(2.), danger, escape.clamp_length(1.))
}

fn parry_opportunity(world: &World, tank: &Tank, seen: &[Seen]) -> Option<f32> {
    seen.iter()
        .filter_map(|item| {
            let Seen::Projectile(index) = *item else {
                return None;
            };
            let shot = &world.projectiles[index];
            if shot.owner == tank.id
                || !hostile(tank.team, shot.team)
                || shot.damage <= 0.
                || shot.lifetime <= 0.
                || shot.penetration <= 0.
                || matches!(
                    shot.kind,
                    tankyard_content::ProjectileKind::Drone
                        | tankyard_content::ProjectileKind::Minion
                        | tankyard_content::ProjectileKind::Trap
                        | tankyard_content::ProjectileKind::Mine
                        | tankyard_content::ProjectileKind::Barrier
                        | tankyard_content::ProjectileKind::Turret
                )
            {
                return None;
            }
            let delta = shot.pos - tank.pos;
            if delta.length_squared() > 340. * 340.
                || !clear_line(tank.pos, shot.pos, shot.radius, &world.walls)
            {
                return None;
            }
            let velocity = shot.vel - tank.vel;
            let safety = tank.radius + shot.radius + 2.;
            let a = velocity.length_squared();
            let b = 2. * delta.dot(velocity);
            let c = delta.length_squared() - safety * safety;
            if c <= 0. {
                return Some(0.);
            }
            let discriminant = b * b - 4. * a * c;
            if a < 0.01 || b >= 0. || discriminant < 0. {
                return None;
            }
            let eta = (-b - discriminant.sqrt()) / (2. * a);
            ((0. ..=0.3).contains(&eta) && eta < shot.lifetime).then_some(eta)
        })
        .min_by(f32::total_cmp)
}

fn use_ability(class: &ClassDef, tank: &Tank, brain: &Brain, distance: f32) -> bool {
    let health = tank.health / tank.max_health.max(1.);
    let ability = class.ability;
    match ability {
        Ability::None => false,
        Ability::Dash => {
            brain.state == BotState::Retreating
                || if class.multipliers[2] >= 2. {
                    health > 0.5
                        && (80. ..240.).contains(&distance)
                        && brain.danger < 0.55
                        && brain.state == BotState::Attacking
                } else {
                    distance > 100. && distance < 420.
                }
        }
        Ability::Shield => brain.danger > 0.3 || (distance < 350. && health < 0.75),
        Ability::Parry | Ability::CounterParry | Ability::VolleyParry => brain
            .parry_eta
            .is_some_and(|eta| eta <= ability.parry_window().unwrap_or(0.) * 0.85),
        Ability::Heal => health < 0.72 || (brain.state == BotState::Escorting && distance < 500.),
        Ability::Nova => distance < 190. || brain.danger > 0.6,
        Ability::Recall => brain.state == BotState::Retreating || (health < 0.5 && distance < 200.),
        Ability::Cloak => {
            brain.state == BotState::Retreating || (distance > 250. && distance < 650.)
        }
        Ability::Overdrive => brain.target.is_some() && distance < 650.,
        Ability::Blink => {
            brain.state == BotState::Retreating || (distance > 170. && distance < 650.)
        }
        Ability::Fortify => {
            brain.danger > 0.3
                || distance < 230.
                || (matches!(brain.state, BotState::Capturing | BotState::Defending)
                    && tank.pos.distance(brain.goal) < 90.)
        }
        Ability::Magnet => distance < 280. || brain.danger > 0.4,
    }
}

/// Solve the intercept quadratic, including the nearly linear case. No client
/// prediction or hidden bonuses are involved; bots estimate ordinary shot travel.
fn intercept(origin: V2, position: V2, velocity: V2, speed: f32) -> V2 {
    let delta = position - origin;
    let a = velocity.length_squared() - speed * speed;
    let b = 2. * delta.dot(velocity);
    let c = delta.length_squared();
    let time = if a.abs() < 0.001 {
        if b.abs() > 0.001 {
            (-c / b).max(0.)
        } else {
            0.
        }
    } else {
        let discriminant = b * b - 4. * a * c;
        if discriminant < 0. {
            0.
        } else {
            let root = discriminant.sqrt();
            let first = (-b - root) / (2. * a);
            let second = (-b + root) / (2. * a);
            [first, second]
                .into_iter()
                .filter(|&t| t >= 0. && t.is_finite())
                .min_by(f32::total_cmp)
                .unwrap_or(0.)
        }
    };
    position + velocity * time.clamp(0., 2.)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::{Content, ProjectileKind};
    use tankyard_core::{BossKind, WorldConfig};

    #[test]
    fn coop_raiders_seek_the_squad_beyond_sight_and_teammates_seek_enemies() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::CoopPve,
                target_count: 0,
                spawn_protection: 0.0,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Pilot", false);
        let ally = world.add_player("Ally", true);
        for _ in 0..481 {
            world.step();
        }
        let raider = world.tanks.iter().find(|tank| tank.team == 2).unwrap().id;
        world.tanks[0].pos = V2::new(400.0, 400.0);
        world.tanks[1].pos = V2::new(600.0, 400.0);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == raider)
            .unwrap()
            .pos = V2::new(2200.0, 1900.0);
        let mut controller = BotController::new(Difficulty::Normal);
        controller.update(&mut world);
        assert_eq!(controller.state(raider), Some(BotState::Attacking));
        assert_eq!(controller.state(ally), Some(BotState::Attacking));
        let start = world
            .tanks
            .iter()
            .find(|tank| tank.id == raider)
            .unwrap()
            .pos
            .distance(world.tanks[0].pos);
        for _ in 0..120 {
            controller.update(&mut world);
            world.step();
        }
        let end = world
            .tanks
            .iter()
            .find(|tank| tank.id == raider)
            .unwrap()
            .pos
            .distance(world.tanks[0].pos);
        assert!(
            end < start - 200.0,
            "raiders must approach instead of wandering"
        );
        assert_ne!(controller.target(ally), Some(world.tanks[0].id));
    }

    #[test]
    fn bots_do_not_read_motion_through_walls_and_keep_experience_after_death() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let bot = world.add_player("Bot", true);
        let enemy = world.add_player("Opponent", false);
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[1].pos = V2::new(1200., 1000.);
        let mut controller = BotController::new(Difficulty::Expert);
        for i in 0..180 {
            world.tick = i;
            world.tanks[1].pos.x = 1200. + i as f32;
            controller.update(&mut world);
        }
        assert!(controller.brains[&bot].learning.confidence(enemy) > 0.7);
        world.tanks[0].alive = false;
        world.tanks[0].deaths += 1;
        controller.update(&mut world);
        assert!(controller.brains.contains_key(&bot));
        world.tanks[0].alive = true;
        world.tick += 30;
        controller.update(&mut world);
        assert!(controller.brains[&bot].learning.caution > 0.35);
        world.walls.push(tankyard_core::Rect {
            x: 1100.,
            y: 0.,
            w: 40.,
            h: 2500.,
        });
        world.tick += 30;
        controller.brains.get_mut(&bot).unwrap().initialized = false;
        controller.update(&mut world);
        assert_ne!(controller.target(bot), Some(enemy));
    }

    #[test]
    fn an_outnumbered_bot_repositions_and_friendly_support_changes_the_decision() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::TwoTeams,
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Bot", true);
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[0].team = 1;
        world.tanks[0].health *= 0.75;
        for offset in [-50., 0., 50.] {
            world.add_player("Opponent", false);
            let other = world.tanks.last_mut().unwrap();
            other.team = 2;
            other.pos = V2::new(1200., 1000. + offset);
        }
        let mut brain = Brain::default();
        let class = world.content.class(&world.tanks[0].class).unwrap();
        let seen = (0..world.tanks.len()).map(Seen::Tank).collect::<Vec<_>>();
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &seen,
            &mut brain,
        );
        assert_eq!(brain.state, BotState::Repositioning);
        assert!(brain.goal.x < world.tanks[0].pos.x);
        assert_ne!(brain.target.unwrap().id, id);

        for offset in [-60., 60.] {
            world.add_player("Ally", false);
            let ally = world.tanks.last_mut().unwrap();
            ally.team = 1;
            ally.pos = V2::new(1000., 1000. + offset);
        }
        let seen = (0..world.tanks.len()).map(Seen::Tank).collect::<Vec<_>>();
        let class = world.content.class(&world.tanks[0].class).unwrap();
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &seen,
            &mut brain,
        );
        assert_eq!(brain.state, BotState::Attacking);
    }

    #[test]
    fn retreating_bots_cover_the_pursuer_instead_of_a_cached_farming_target() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Bot", true);
        let enemy = world.add_player("Pursuer", false);
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[0].health = world.tanks[0].max_health * 0.2;
        world.tanks[1].pos = V2::new(1200., 1000.);
        world.tanks[1].level = 45;
        world.tanks[1].max_health = 1000.;
        world.tanks[1].health = 1000.;
        world.targets.push(tankyard_core::Target {
            id: 10000,
            pos: V2::new(1000., 1100.),
            vel: V2::ZERO,
            radius: 20.,
            health: 35.,
            max_health: 35.,
            kind: tankyard_core::TargetKind::Square,
            angle: 0.,
            xp: 22,
        });
        let mut brain = Brain::default();
        let class = world.content.class(&world.tanks[0].class).unwrap();
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1), Seen::Target(0)],
            &mut brain,
        );
        assert_eq!(brain.state, BotState::Retreating);
        assert_eq!(brain.target.unwrap().id, enemy);
        assert!(brain.goal.x < world.tanks[0].pos.x);
    }

    #[test]
    fn domination_bots_prefer_an_uncovered_objective() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Domination,
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Bot", true);
        world.objectives.truncate(2);
        world.objectives[0].pos = V2::new(800., 1000.);
        world.objectives[1].pos = V2::new(1200., 1000.);
        for objective in &mut world.objectives {
            objective.team = 0;
        }
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[0].team = 1;
        for _ in 0..3 {
            world.add_player("Holding point", true);
            let ally = world.tanks.last_mut().unwrap();
            ally.team = 1;
            ally.pos = V2::new(800., 1000.);
        }
        // Keep the other point outside the coverage radius of the first.
        world.objectives[1].pos = V2::new(1700., 1000.);
        let (_, goal, state) = objective_goal(&world, &world.tanks[0]).unwrap();
        assert_eq!(state, BotState::Capturing);
        assert_eq!(goal, world.objectives[1].pos);
    }

    #[test]
    fn prediction_handles_perpendicular_and_unreachable_targets() {
        let predicted = intercept(V2::ZERO, V2::new(100., 0.), V2::new(0., 50.), 100.);
        assert!(predicted.y > 50. && predicted.y < 60.);
        assert!(intercept(V2::ZERO, V2::new(100., 0.), V2::new(1000., 0.), 100.).finite());
        assert!(intercept(V2::ZERO, V2::ZERO, V2::ZERO, 0.).finite());
    }

    #[test]
    fn bots_issue_legal_commands_and_evolve_without_stat_bonuses() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 24,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Test bot", true);
        world.sandbox_level(id, 45);
        let mut bots = BotController::new(Difficulty::Expert);
        for _ in 0..180 {
            bots.update(&mut world);
            world.step();
        }
        let tank = world.tanks.iter().find(|tank| tank.id == id).unwrap();
        assert!(tank.input.valid());
        assert!(tank.stats.iter().all(|&point| point <= 7));
        assert_eq!(tank.stats.iter().sum::<u8>() + tank.points, 33);
        assert_eq!(world.content.class(&tank.class).unwrap().tier, 3);
        assert!(tank.pos.finite());
        assert!(bots.state(id).is_some());
    }

    #[test]
    fn every_class_is_usable_by_the_shared_bot_controller() {
        let content = Content::builtin();
        for class in &content.classes {
            let mut world = World::new(
                WorldConfig {
                    mode: Mode::Sandbox,
                    target_count: 0,
                    spawn_protection: 0.,
                    ..Default::default()
                },
                content.clone(),
            );
            let id = world.add_player("Class verification", true);
            world.sandbox_class(id, &class.id);
            world.sandbox_level(id, class.min_level);
            let enemy = world.add_player("Target", false);
            world.sandbox_invulnerability(enemy, true);
            let tank = world.tanks.iter_mut().find(|tank| tank.id == id).unwrap();
            tank.pos = V2::new(1000., 1000.);
            let initial = tank.pos;
            world
                .tanks
                .iter_mut()
                .find(|tank| tank.id == enemy)
                .unwrap()
                .pos = V2::new(1200., 1000.);
            let mut controller = BotController::new(Difficulty::Expert);
            let mut shot = false;
            for _ in 0..360 {
                controller.update(&mut world);
                world.step();
                shot |= world.events.iter().any(|event| {
                    event.subject == id && matches!(event.kind, tankyard_core::EventKind::Shot)
                });
                assert!(
                    world
                        .tanks
                        .iter()
                        .all(|tank| tank.pos.finite() && tank.input.valid()),
                    "{} produced invalid state",
                    class.id
                );
                assert!(
                    world
                        .projectiles
                        .iter()
                        .all(|projectile| projectile.pos.finite() && projectile.vel.finite()),
                    "{} produced invalid projectiles",
                    class.id
                );
            }
            let tank = world.tanks.iter().find(|tank| tank.id == id).unwrap();
            assert_eq!(
                tank.class, class.id,
                "class verification evolved away from the class under test"
            );
            assert!(
                tank.pos.distance(initial) > 5.,
                "{} could not move",
                class.id
            );
            if !class.barrels.is_empty() {
                assert!(shot, "{} could not fire", class.id);
            }
        }
    }

    #[test]
    fn cooperative_bots_never_target_allies_and_engage_bosses() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::BossRush,
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let first = world.add_player("One", true);
        let second = world.add_player("Two", true);
        world.spawn_boss(BossKind::Sentinel);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == first)
            .unwrap()
            .pos = V2::new(1700., 1300.);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == second)
            .unwrap()
            .pos = V2::new(1720., 1300.);
        let mut controller = BotController::new(Difficulty::Expert);
        controller.update(&mut world);
        for id in [first, second] {
            let brain = controller.brains.get(&id).unwrap();
            assert!(
                !brain
                    .target
                    .is_some_and(|target| target.id == first || target.id == second)
            );
            assert!(matches!(
                brain.state,
                BotState::BossEngagement | BotState::Exploring
            ));
        }
    }

    #[test]
    fn every_difficulty_uses_identical_authoritative_stats_and_damage() {
        let mut expected = None;
        for difficulty in Difficulty::ALL {
            let mut world = World::new(
                WorldConfig {
                    mode: Mode::Ffa,
                    target_count: 0,
                    spawn_protection: 0.,
                    ..Default::default()
                },
                Content::builtin(),
            );
            let bot = world.add_player("Difficulty test", true);
            let enemy = world.add_player("Enemy", false);
            world
                .tanks
                .iter_mut()
                .find(|tank| tank.id == bot)
                .unwrap()
                .pos = V2::new(1000., 1000.);
            world
                .tanks
                .iter_mut()
                .find(|tank| tank.id == enemy)
                .unwrap()
                .pos = V2::new(1300., 1000.);
            let mut controller = BotController::new(difficulty);
            controller.update(&mut world);
            world.step();
            let tank = world.tanks.iter().find(|tank| tank.id == bot).unwrap();
            let shot = world
                .projectiles
                .iter()
                .find(|projectile| projectile.owner == bot)
                .unwrap();
            let actual = (tank.max_health, tank.stats, shot.damage, shot.penetration);
            if let Some(expected) = expected {
                assert_eq!(actual, expected);
            }
            expected = Some(actual);
            assert!(tank.input.valid());
        }
        assert!(Difficulty::Beginner.reaction_ticks() > Difficulty::Expert.reaction_ticks());
        assert!(Difficulty::Expert.prediction() > Difficulty::Beginner.prediction());
    }

    #[test]
    fn projectile_dodge_anticipates_an_incoming_hit() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Dodge", true);
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.projectiles.push(tankyard_core::Projectile {
            id: 10000,
            owner: id + 1,
            team: 0,
            pos: V2::new(1200., 1000.),
            previous: V2::new(1206., 1000.),
            vel: V2::new(-400., 0.),
            cruise_speed: 400.,
            radius: 6.,
            damage: 10.,
            penetration: 1.,
            lifetime: 2.,
            kind: ProjectileKind::Bullet,
            ricochets: 0,
            age: 0.,
            hit: vec![],
            gun: None,
            homing_target: 0,
        });
        let (dodge, danger, escape) =
            projectile_avoidance(&world, &world.tanks[0], &[Seen::Projectile(0)]);
        assert!(danger > 0.5);
        assert!(
            dodge.y.abs() > 0.5,
            "an exact incoming lane requires a sideways dodge"
        );
        assert!(dodge.finite());
        assert_eq!(escape, V2::ZERO);
    }

    #[test]
    fn bots_only_target_and_fire_at_enemies_inside_their_shorter_sight() {
        for difficulty in Difficulty::ALL {
            let mut world = World::new(
                WorldConfig {
                    target_count: 0,
                    spawn_protection: 0.,
                    ..Default::default()
                },
                Content::builtin(),
            );
            let bot = world.add_player("Bot", true);
            let enemy = world.add_player("Opponent", false);
            world.tanks[0].pos = V2::new(1000., 1000.);
            world.tanks[1].pos = V2::new(1000. + difficulty.perception_radius() + 20., 1000.);
            let mut controller = BotController::new(difficulty);
            controller.update(&mut world);
            assert_eq!(controller.target(bot), None);
            assert!(!world.tanks[0].input.fire);
            world.tanks[1].pos = V2::new(1300., 1000.);
            controller.brains.get_mut(&bot).unwrap().initialized = false;
            controller.update(&mut world);
            assert_eq!(controller.target(bot), Some(enemy));
            assert!(world.tanks[0].input.fire);
            // Walking away must invalidate the remembered target immediately.
            world.tanks[0].pos = V2::new(300., 1000.);
            controller.update(&mut world);
            assert_eq!(controller.target(bot), None);
            assert!(!world.tanks[0].input.fire);
            assert!(world.tanks[0].input.aim_distance <= difficulty.perception_radius());
        }
    }

    fn pursuit_world(kind: ProjectileKind) -> World {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                mode: Mode::TwoTeams,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Bot", true);
        let owner = world.add_player("Opponent", false);
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[0].team = 1;
        world.tanks[1].pos = V2::new(1250., 1000.);
        world.tanks[1].team = 2;
        world.projectiles.push(tankyard_core::Projectile {
            id: 10000,
            owner,
            team: 2,
            pos: V2::new(1200., 1000.),
            previous: V2::new(1203., 1000.),
            vel: V2::new(-180., 0.),
            cruise_speed: 180.,
            radius: 8.,
            damage: 10.,
            penetration: 3.,
            lifetime: 2.,
            kind,
            ricochets: 0,
            age: 0.,
            hit: vec![],
            gun: None,
            homing_target: 0,
        });
        world
    }

    #[test]
    fn bots_withdraw_from_visible_homing_rounds_and_drones() {
        for kind in [
            ProjectileKind::Homing,
            ProjectileKind::Drone,
            ProjectileKind::Minion,
        ] {
            let mut world = pursuit_world(kind);
            let mut controller = BotController::new(Difficulty::Normal);
            for tick in 0..36 {
                world.tick = tick;
                controller.update(&mut world);
            }
            assert!(
                world.tanks[0].input.movement.x < -0.15,
                "must retreat from {kind:?}"
            );
            assert!(world.tanks[0].input.valid());
            let (_, danger, escape) =
                projectile_avoidance(&world, &world.tanks[0], &[Seen::Projectile(0)]);
            assert!(danger > 0.5 && escape.x < -0.5);
        }
    }

    #[test]
    fn pursuit_avoidance_ignores_friends_hidden_threats_and_expiring_or_departing_homing() {
        let mut world = pursuit_world(ProjectileKind::Homing);
        let seen = [Seen::Projectile(0)];
        world.projectiles[0].team = 1;
        assert_eq!(
            projectile_avoidance(&world, &world.tanks[0], &seen),
            (V2::ZERO, 0., V2::ZERO)
        );
        world.projectiles[0].team = 2;
        world.walls.push(tankyard_core::Rect {
            x: 1100.,
            y: 950.,
            w: 20.,
            h: 100.,
        });
        assert_eq!(
            projectile_avoidance(&world, &world.tanks[0], &seen),
            (V2::ZERO, 0., V2::ZERO)
        );
        world.walls.clear();
        world.projectiles[0].vel.x = 180.;
        assert_eq!(
            projectile_avoidance(&world, &world.tanks[0], &seen),
            (V2::ZERO, 0., V2::ZERO)
        );
        world.projectiles[0].vel.x = -180.;
        world.projectiles[0].lifetime = 0.1;
        assert_eq!(
            projectile_avoidance(&world, &world.tanks[0], &seen).2,
            V2::ZERO
        );
        world.projectiles[0].lifetime = 2.;
        world.projectiles[0].pos.x = 1500.;
        assert_eq!(
            projectile_avoidance(&world, &world.tanks[0], &seen),
            (V2::ZERO, 0., V2::ZERO)
        );
    }

    #[test]
    fn fear_and_animosity_change_visible_choices_without_overriding_sight() {
        let mut world = pursuit_world(ProjectileKind::Bullet);
        world.projectiles.clear();
        world.add_player("Other", false);
        world.tanks[2].team = 2;
        world.tanks[1].pos = V2::new(1200., 1000.);
        world.tanks[2].pos = V2::new(1000., 1200.);
        // A bold bot sometimes favors the opponent it holds a grudge against.
        world.tanks[0].id = (100..200).find(|id| noise(id * 103) > 0.95).unwrap();
        let remembered = world.tanks[2].id;
        let mut brain = Brain::default();
        brain
            .learning
            .relationships
            .hurt_by(remembered, 0., true, 0);
        let class = world.content.class(&world.tanks[0].class).unwrap();
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1), Seen::Tank(2)],
            &mut brain,
        );
        assert_eq!(brain.target.unwrap().id, remembered);
        // Recognition follows identity through a rename, not the display name.
        world.tanks[2].name = "Renamed".into();
        assert!(brain.learning.relationships.get(remembered, 1).animosity > 0.2);
        world.walls.push(tankyard_core::Rect {
            x: 950.,
            y: 1100.,
            w: 100.,
            h: 20.,
        });
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1), Seen::Tank(2)],
            &mut brain,
        );
        assert_ne!(brain.target.map(|sample| sample.id), Some(remembered));
        world.walls.clear();
        world.tanks[2].pos = V2::new(1000., 1800.);
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1), Seen::Tank(2)],
            &mut brain,
        );
        assert_ne!(brain.target.map(|sample| sample.id), Some(remembered));

        // Fear of this particular nearby opponent changes when to withdraw.
        world.tanks[0].health = world.tanks[0].max_health * 0.44;
        let feared = world.tanks[1].id;
        let mut neutral = Brain::default();
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1)],
            &mut neutral,
        );
        assert_ne!(neutral.state, BotState::Retreating);
        neutral.learning.relationships.hurt_by(feared, 0.8, true, 0);
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1)],
            &mut neutral,
        );
        assert_eq!(neutral.state, BotState::Retreating);
    }

    #[test]
    fn a_real_death_records_the_killer_and_recognition_survives_respawn() {
        let mut world = pursuit_world(ProjectileKind::Bullet);
        let bot = world.tanks[0].id;
        let killer = world.tanks[1].id;
        let mut controller = BotController::new(Difficulty::Normal);
        controller.update(&mut world);
        world.tanks[0].health = 1.;
        world.projectiles[0].pos = V2::new(1010., 1000.);
        world.projectiles[0].previous = world.projectiles[0].pos;
        world.step();
        assert!(!world.tanks[0].alive);
        controller.update(&mut world);
        assert!(
            controller.brains[&bot]
                .learning
                .relationships
                .get(killer, world.tick)
                .fear
                > 0.29
        );
        for _ in 0..200 {
            world.step();
            controller.update(&mut world);
        }
        assert!(world.tanks[0].alive);
        assert!(
            controller.brains[&bot]
                .learning
                .relationships
                .get(killer, world.tick)
                .fear
                > 0.25
        );
    }

    #[test]
    fn auto_smasher_bots_use_their_body_instead_of_circling_with_a_small_gun() {
        let mut world = pursuit_world(ProjectileKind::Bullet);
        world.projectiles.clear();
        world.config.mode = Mode::Sandbox;
        let bot = world.tanks[0].id;
        assert!(world.sandbox_class(bot, "auto_smasher"));
        world.tanks[1].level = 45;
        world.tanks[1].max_health = world.tanks[0].max_health;
        world.tanks[1].health = world.tanks[1].max_health;
        let mut brain = Brain::default();
        let class = world.content.class("auto_smasher").unwrap();
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1)],
            &mut brain,
        );
        assert_eq!(brain.state, BotState::Attacking);
        assert_eq!(brain.goal, world.tanks[1].pos);
    }

    #[test]
    fn smashers_flank_heavy_cannons_commit_up_close_and_retreat_when_hurt() {
        let mut world = pursuit_world(ProjectileKind::Bullet);
        world.projectiles.clear();
        world.config.mode = Mode::Sandbox;
        let bot = world.tanks[0].id;
        let enemy = world.tanks[1].id;
        assert!(world.sandbox_class(bot, "juggernaut"));
        assert!(world.sandbox_class(enemy, "hammer"));
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[1].pos = V2::new(1300., 1000.);
        world.tanks[1].aim = std::f32::consts::PI;
        let class = world.content.class("juggernaut").unwrap();
        let mut brain = Brain::default();
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1)],
            &mut brain,
        );
        assert_eq!(brain.state, BotState::Attacking);
        assert!(brain.goal.x > world.tanks[0].pos.x);
        assert!((brain.goal.y - world.tanks[1].pos.y).abs() > 50.);
        world.tanks[1].pos.x = 1100.;
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1)],
            &mut brain,
        );
        assert_eq!(brain.goal, world.tanks[1].pos);
        world.tanks[0].health = world.tanks[0].max_health * 0.2;
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert,
            &[Seen::Tank(1)],
            &mut brain,
        );
        assert_eq!(brain.state, BotState::Retreating);
        assert!(brain.goal.x < world.tanks[0].pos.x);
    }

    #[test]
    fn parry_bots_wait_for_impact_and_ignore_hidden_or_expiring_shots() {
        let mut world = pursuit_world(ProjectileKind::Bullet);
        world.config.mode = Mode::Sandbox;
        let id = world.tanks[0].id;
        assert!(world.sandbox_class(id, "reprisal"));
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[0].vel = V2::ZERO;
        world.projectiles[0].pos = V2::new(1100., 1000.);
        world.projectiles[0].vel = V2::new(-400., 0.);
        let seen = [Seen::Projectile(0)];
        let class = world.content.class("reprisal").unwrap();
        let mut brain = Brain {
            parry_eta: parry_opportunity(&world, &world.tanks[0], &seen),
            ..Default::default()
        };
        assert!(brain.parry_eta.unwrap() > 0.14);
        assert!(!use_ability(class, &world.tanks[0], &brain, 100.));
        world.projectiles[0].pos.x = 1060.;
        brain.parry_eta = parry_opportunity(&world, &world.tanks[0], &seen);
        assert!(use_ability(class, &world.tanks[0], &brain, 60.));
        world.projectiles[0].lifetime = 0.001;
        assert!(parry_opportunity(&world, &world.tanks[0], &seen).is_none());
        world.projectiles[0].lifetime = 2.;
        world.walls.push(tankyard_core::Rect {
            x: 1035.,
            y: 950.,
            w: 10.,
            h: 100.,
        });
        assert!(parry_opportunity(&world, &world.tanks[0], &seen).is_none());
        world.walls.clear();
        world.projectiles[0].kind = ProjectileKind::Drone;
        assert!(parry_opportunity(&world, &world.tanks[0], &seen).is_none());
    }

    #[test]
    fn bots_capture_domination_objectives_in_the_real_mode_loop() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Domination,
                width: 1200.,
                height: 800.,
                target_count: 0,
                score_limit: 1000,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Capture", true);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == id)
            .unwrap()
            .pos = V2::new(300., 230.);
        let mut controller = BotController::new(Difficulty::Normal);
        for _ in 0..1200 {
            controller.update(&mut world);
            world.step();
        }
        assert!(world.objectives.iter().any(|objective| objective.team == 1));
        assert!(world.match_state.scores[0] > 0);
    }

    #[test]
    fn a_bot_transports_and_scores_a_core_without_scripted_position_changes() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::CaptureCore,
                width: 900.,
                height: 700.,
                target_count: 0,
                score_limit: 1000,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Courier", true);
        let home = world.team_home(1);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == id)
            .unwrap()
            .pos = home;
        let mut controller = BotController::new(Difficulty::Advanced);
        for _ in 0..1200 {
            controller.update(&mut world);
            world.step();
        }
        assert!(
            world.match_state.scores[0] > 0,
            "bot failed to carry the enemy core home"
        );
    }

    #[test]
    fn bot_navigation_reaches_the_other_side_of_cover() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                width: 1200.,
                height: 800.,
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.walls.push(tankyard_core::Rect {
            x: 440.,
            y: 180.,
            w: 100.,
            h: 360.,
        });
        let bot = world.add_player("Navigator", true);
        world.sandbox_level(bot, 1);
        let enemy = world.add_player("Destination", false);
        world.sandbox_invulnerability(enemy, true);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == bot)
            .unwrap()
            .pos = V2::new(220., 350.);
        world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == enemy)
            .unwrap()
            .pos = V2::new(1050., 350.);
        let mut controller = BotController::new(Difficulty::Expert);
        for _ in 0..720 {
            controller.update(&mut world);
            world.step();
            let tank = world.tanks.iter().find(|tank| tank.id == bot).unwrap();
            assert!(world.position_clear(tank.pos, tank.radius));
        }
        assert!(
            world
                .tanks
                .iter()
                .find(|tank| tank.id == bot)
                .unwrap()
                .pos
                .x
                > 600.
        );
    }

    #[test]
    fn mothership_teams_include_assault_and_defense_roles() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Mothership,
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        for _ in 0..12 {
            world.add_player("Ship squad", true);
        }
        let roles = world
            .tanks
            .iter()
            .filter_map(|tank| objective_goal(&world, tank))
            .map(|(_, _, state)| state)
            .collect::<Vec<_>>();
        assert!(roles.contains(&BotState::BossEngagement));
        assert!(roles.contains(&BotState::Defending));
        for boss in &mut world.bosses {
            boss.health = boss.max_health * 0.5;
        }
        assert!(world.tanks.iter().all(|tank| {
            objective_goal(&world, tank).is_some_and(|(_, _, state)| state == BotState::Defending)
        }));
    }

    #[test]
    fn expert_farms_safely_instead_of_rushing_a_much_stronger_opponent() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let bot = world.add_player("Cautious", true);
        let enemy = world.add_player("Overwhelming", false);
        world.tanks[0].pos = V2::new(1000., 1000.);
        let stronger = world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == enemy)
            .unwrap();
        stronger.pos = V2::new(1100., 1000.);
        stronger.level = 45;
        stronger.health = 1000.;
        stronger.max_health = 1000.;
        world.targets.push(tankyard_core::Target {
            id: 10000,
            pos: V2::new(1000., 1250.),
            vel: V2::ZERO,
            radius: 20.,
            health: 35.,
            max_health: 35.,
            kind: tankyard_core::TargetKind::Square,
            angle: 0.,
            xp: 22,
        });
        let mut controller = BotController::new(Difficulty::Expert);
        controller.update(&mut world);
        assert_eq!(controller.state(bot), Some(BotState::Farming));
        assert_eq!(controller.target(bot), Some(10000));
    }
}

#[cfg(test)]
mod tactical_tests {
    use super::*;
    #[test]
    fn projectile_on_intercept_course_causes_dodge() {
        let (direction, risk) = dodge_threat(V2::new(100.0, 0.0), V2::new(-400.0, 0.0), 30.0);
        assert!(risk > 0.0);
        assert!(direction.length() > 0.9);
        assert!(direction.x.abs() < 0.001);
    }
    #[test]
    fn projectile_moving_away_does_not_trigger_dodge() {
        let (_, risk) = dodge_threat(V2::new(100.0, 0.0), V2::new(400.0, 0.0), 30.0);
        assert_eq!(risk, 0.0);
    }
    #[test]
    fn intercept_uses_target_velocity() {
        let point = intercept(V2::ZERO, V2::new(100., 0.), V2::new(0., 20.), 100.);
        assert!(point.y > 0.0 && point.x == 100.0);
    }
}

#[cfg(test)]
mod combat_regressions {
    use super::*;
    use tankyard_content::Content;
    use tankyard_core::WorldConfig;

    #[test]
    fn bots_farm_evolve_and_kill_each_other_without_player_input() {
        let mut world = World::new(
            WorldConfig {
                width: 1500.,
                height: 1100.,
                target_count: 36,
                seed: 42,
                ..Default::default()
            },
            Content::builtin(),
        );
        for n in 0..8 {
            world.add_player(&format!("Bot {n}"), true);
        }
        let mut bots = BotController::new(Difficulty::Normal);
        let mut attacked = false;
        let mut evolved = false;
        for _ in 0..12000 {
            bots.update(&mut world);
            attacked |= world
                .tanks
                .iter()
                .any(|tank| bots.state(tank.id) == Some(BotState::Attacking));
            world.step();
            evolved |= world.tanks.iter().any(|tank| tank.class != "vector");
        }
        assert!(attacked && evolved);
        assert!(
            world.tanks.iter().map(|tank| tank.kills).sum::<u32>() > 0,
            "autonomous combat must produce kills"
        );
        assert!(
            world
                .tanks
                .iter()
                .all(|tank| tank.input.valid() && tank.pos.finite())
        );
    }

    #[test]
    fn a_tag_conversion_immediately_invalidates_a_cached_enemy() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Tag,
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let bot = world.add_player("Bot", true);
        let enemy = world.add_player("Enemy", false);
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[1].pos = V2::new(1200., 1000.);
        let mut bots = BotController::new(Difficulty::Expert);
        bots.update(&mut world);
        assert_eq!(bots.target(bot), Some(enemy));
        world.tanks[1].team = world.tanks[0].team;
        world.tick += 1;
        bots.update(&mut world);
        assert_ne!(bots.target(bot), Some(enemy));
    }
}
