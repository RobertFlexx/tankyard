//! Server-authoritative utility decisions, tactical steering and cached A*.
//!
//! All bots send the same input, stat and evolution commands as humans. Their
//! difficulty changes perception, reaction delay, aim and avoidance. Combat stats
//! follow the same rules. During a match, bots estimate visible tanks' motion and
//! adjust spacing after damage, kills, and deaths. Outcome networks influence
//! legal tank and skill choices. That memory lasts across lives, within a match.
//! Each bot also has a stable personality and its own verdicts on the classes
//! it has played, so bots differ in temperament and choices like people do.
mod aiming;
mod builds;
mod combat;
mod difficulty;
mod learning;
mod navigation;
mod neural;
mod objectives;
mod perception;
mod personality;
mod policy;
mod relationships;
mod steering;
mod tactics;

use aiming::*;
use builds::progression_commands;
use combat::decide;
pub use difficulty::Difficulty;
use difficulty::Skill;
use navigation::{Navigation, clear_line};
use objectives::objective_goal;
use perception::{Perception, Seen};
use personality::Personality;
use std::collections::{HashMap, VecDeque};
use steering::*;
use tankyard_content::{Ability, ClassDef, ProjectileKind};
use tankyard_core::{EntityId, Mode, PlayerInput, Tank, V2, World, physics};

/// Seconds the hidden boost takes to climb one whole difficulty level.
const BOOST_SECONDS_PER_LEVEL: f32 = 45.;

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
    parry_threat: Option<EntityId>,
    parry_miss: bool,
    parry_timing: f32,
    escape: V2,
    steering: V2,
    /// Where and when a hostile tank was last seen, so a hurt bot keeps
    /// backing off briefly after losing sight instead of freezing.
    last_threat: Option<(V2, u64)>,
    /// Where something worth doing was last seen, for purposeful roaming.
    interest: Option<(V2, u64)>,
    /// Circle-strafe direction around the current target: 1 or -1.
    orbit: f32,
    /// Position at the last progress check, for detecting a stuck bot.
    progress: (V2, u64),
    stuck_checks: u8,
    /// A sidestep that frees a stuck bot, until the given tick.
    unstick: Option<(V2, u64)>,
}

enum Command {
    Stat(EntityId, usize),
    Evolve(EntityId, String),
}

pub struct BotController {
    pub difficulty: Difficulty,
    /// Let bots play up when a human's tank far outclasses them.
    pub adaptive: bool,
    /// How far above `difficulty` they currently play, eased every tick.
    boost: f32,
    brains: HashMap<EntityId, Brain>,
    perception: Perception,
    navigation: Navigation,
    /// Ticks since the route grid last followed gliding cover.
    navigation_age: u32,
    seen: Vec<Seen>,
    commands: Vec<Command>,
    inputs: Vec<(EntityId, PlayerInput)>,
}

impl BotController {
    pub fn new(difficulty: Difficulty) -> Self {
        Self {
            difficulty,
            adaptive: true,
            boost: 0.,
            brains: HashMap::new(),
            perception: Perception::default(),
            navigation: Navigation::default(),
            navigation_age: 0,
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

    /// The skill level bots play at right now, on the `Difficulty::level`
    /// scale.
    pub fn skill(&self) -> f32 {
        (self.difficulty.level() + self.boost).min(Difficulty::Expert.level())
    }

    /// Ease the boost toward what the match calls for. Once a human sits
    /// well above the bots' average level, bots climb toward Expert; as the
    /// gap closes they settle back. Each tick moves only a tiny step, so
    /// there's no moment where they suddenly get better.
    fn adapt(&mut self, world: &World) {
        let room = Difficulty::Expert.level() - self.difficulty.level();
        let target = if !self.adaptive || world.config.mode == Mode::Sandbox || room <= 0. {
            0.
        } else {
            let (sum, count) = world
                .tanks
                .iter()
                .filter(|t| t.bot)
                .fold((0., 0.), |(sum, n), t| (sum + f32::from(t.level), n + 1.));
            let strongest = world
                .tanks
                .iter()
                .filter(|t| !t.bot && t.alive)
                .map(|t| f32::from(t.level))
                .fold(None, |best: Option<f32>, level| {
                    Some(best.map_or(level, |b| b.max(level)))
                });
            match strongest {
                Some(top) if count > 0. => {
                    // 12 levels ahead starts it; 36 ahead is the full boost.
                    let gap = top - sum / count;
                    ((gap - 12.) / 24.).clamp(0., 1.) * room
                }
                _ => 0.,
            }
        };
        let step = 1. / (BOOST_SECONDS_PER_LEVEL * tankyard_core::TICK_RATE as f32);
        self.boost += (target - self.boost).clamp(-step, step);
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
        // Gliding cover would rebuild the route grid every tick. Routes only
        // need it a few times a second; steering still meets the real walls.
        self.navigation_age = self.navigation_age.saturating_add(1);
        if !world.cover_gliding() || self.navigation_age >= 30 {
            self.navigation_age = 0;
            self.navigation
                .refresh(world.config.width, world.config.height, &world.walls);
        }
        self.perception.rebuild(world);
        self.commands.clear();
        self.inputs.clear();
        self.adapt(world);
        let difficulty = Skill(self.skill());
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
            let dodge_period = difficulty.dodge_period();
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
                let opportunity = if class.ability.is_parry() {
                    parry_threat(world, tank, &self.seen)
                } else {
                    None
                };
                brain.parry_eta = opportunity.map(|(_, eta)| eta);
                if opportunity.map(|(id, _)| id) != brain.parry_threat {
                    brain.parry_threat = opportunity.map(|(id, _)| id);
                    if let Some((shot, _)) = opportunity {
                        let roll = noise(
                            tank.id.wrapping_mul(97)
                                ^ shot.wrapping_mul(193)
                                ^ u64::from(tank.deaths),
                        );
                        let miss = difficulty.miss_chance();
                        brain.parry_miss = roll < miss;
                        brain.parry_timing = 0.52 + noise(shot ^ tank.id.wrapping_mul(31)) * 0.34;
                    }
                }
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
            movement += guarded_base_repulsion(world, tank);
            movement = human_motion(world, tank, class, brain, movement);
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
                    steer_return: false,
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

fn is_melee(class: &ClassDef) -> bool {
    class.multipliers[2] >= 2.
        && (class.ai_range < 120.
            || class
                .barrels
                .iter()
                .all(|barrel| barrel.pattern == tankyard_content::FirePattern::Auto))
}

fn hostile(team: u8, other: u8) -> bool {
    team == 0 || other == 0 || team != other
}

/// Bots respect enemy guardian zones the way experienced players do: they
/// fight at the edge instead of driving into a garrisoned base.
fn noise(mut value: u64) -> f32 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((value ^ (value >> 31)) >> 40) as f32 / 16_777_216.
}

fn wrapped_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_hurt_bot_with_nobody_around_keeps_moving_instead_of_camping() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                width: 2400.,
                height: 1800.,
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let bot = world.add_player("Hurt", true);
        let tank = world.tanks.iter_mut().find(|tank| tank.id == bot).unwrap();
        tank.health = tank.max_health * 0.2;
        let mut bots = BotController::new(Difficulty::Normal);
        let mut still = 0;
        let mut travelled = 0.;
        for _ in 0..1200 {
            let before = world.tanks[0].pos;
            bots.update(&mut world);
            world.step();
            let step = world.tanks[0].pos.distance(before);
            travelled += step;
            if step < 0.3 {
                still += 1;
            }
        }
        assert!(still < 120, "stood still for {still} of 1200 ticks");
        assert!(travelled > 1500., "travelled only {travelled}");
        assert_ne!(bots.state(bot), Some(BotState::Retreating));
    }

    #[test]
    fn bots_circle_strafe_their_target_instead_of_parking_at_range() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                width: 1600.,
                height: 1200.,
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        let bot = world.add_player("Strafer", true);
        let dummy = world.add_player("Dummy", false);
        world.sandbox_invulnerability(dummy, true);
        world.sandbox_invulnerability(bot, true);
        for (id, pos) in [(bot, V2::new(500., 600.)), (dummy, V2::new(800., 600.))] {
            world
                .tanks
                .iter_mut()
                .find(|tank| tank.id == id)
                .unwrap()
                .pos = pos;
        }
        let mut bots = BotController::new(Difficulty::Normal);
        let mut swept = 0.;
        let mut previous: Option<f32> = None;
        for _ in 0..900 {
            bots.update(&mut world);
            world.step();
            let (b, d) = (
                world.tanks.iter().find(|t| t.id == bot).unwrap().pos,
                world.tanks.iter().find(|t| t.id == dummy).unwrap().pos,
            );
            let angle = (b - d).angle();
            if let Some(previous) = previous {
                swept += wrapped_angle(angle - previous).abs();
            }
            previous = Some(angle);
        }
        assert!(swept > 1.5, "swept only {swept} radians around the target");
    }

    #[test]
    fn outbreak_bots_evade_observed_threats_and_patrol_without_hidden_positions() {
        let mut world = World::new(
            tankyard_core::WorldConfig {
                mode: Mode::Infection,
                target_count: 0,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        world.add_player("Survivor", true);
        world.add_player("Infected", true);
        world.match_state.arena.intermission = false;
        world.match_state.arena.wave = 1;
        world.tanks[0].pos = V2::new(1500., 1200.);
        world.tanks[1].pos = V2::new(1600., 1200.);
        world.tanks[1].team = 2;
        let threat = TargetSample {
            id: world.tanks[1].id,
            pos: world.tanks[1].pos,
            velocity: V2::ZERO,
            radius: world.tanks[1].radius,
        };
        let (_, goal, state) = objective_goal(&world, &world.tanks[0], Some(threat)).unwrap();
        assert_eq!(state, BotState::Repositioning);
        assert!(goal.distance(threat.pos) > world.tanks[0].pos.distance(threat.pos));
        let (_, patrol, state) = objective_goal(&world, &world.tanks[1], None).unwrap();
        assert_eq!(state, BotState::Exploring);
        world.tanks[0].pos = V2::new(3000., 2200.);
        assert_eq!(
            patrol,
            objective_goal(&world, &world.tanks[1], None).unwrap().1
        );
        assert!(objective_goal(&world, &world.tanks[1], Some(threat)).is_none());
    }
    #[test]
    fn hill_bots_retarget_the_objective_after_it_moves() {
        let mut world = World::new(
            tankyard_core::WorldConfig {
                mode: Mode::KingOfTheHill,
                target_count: 0,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        world.add_player("Defender", true);
        let (_, first, state) = objective_goal(&world, &world.tanks[0], None).unwrap();
        assert_eq!(state, BotState::Capturing);
        world.objectives[0].pos += V2::new(500., 300.);
        let (_, second, _) = objective_goal(&world, &world.tanks[0], None).unwrap();
        assert!(first.distance(second) > 500.);
    }
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
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
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
        let (_, goal, state) = objective_goal(&world, &world.tanks[0], None).unwrap();
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
    fn skill_matches_each_difficulty_and_blends_between_them() {
        for difficulty in Difficulty::ALL {
            let skill = Skill(difficulty.level());
            assert_eq!(skill.reaction_ticks(), difficulty.reaction_ticks());
            assert_eq!(skill.perception_radius(), difficulty.perception_radius());
            assert_eq!(skill.aim_error(), difficulty.aim_error());
            assert_eq!(skill.miss_chance(), difficulty.miss_chance());
            assert_eq!(skill.pressure_limit(), difficulty.pressure_limit());
        }
        let between = Skill(1.5);
        assert!(between.aim_error() < Difficulty::Normal.aim_error());
        assert!(between.aim_error() > Difficulty::Advanced.aim_error());
        // "Never retreat from pressure" eases into a finite limit.
        let limit = Skill(0.5).pressure_limit();
        assert!(limit.is_finite() && limit > Difficulty::Normal.pressure_limit());
    }

    #[test]
    fn bots_ease_up_against_an_overpowered_human_and_settle_back() {
        let mut world = World::new(
            tankyard_core::WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let human = world.add_player("Human", false);
        world.add_player("Bot", true);
        let mut bots = BotController::new(Difficulty::Normal);
        let second = tankyard_core::TICK_RATE as usize;
        // Even levels: no boost.
        for _ in 0..second * 5 {
            bots.adapt(&world);
        }
        assert_eq!(bots.skill(), 1.);
        world
            .tanks
            .iter_mut()
            .find(|t| t.id == human)
            .unwrap()
            .level = 45;
        let mut last = bots.skill();
        for _ in 0..second {
            bots.adapt(&world);
            // Never a visible jump from one tick to the next.
            assert!(bots.skill() - last < 0.001);
            last = bots.skill();
        }
        assert!(last > 1. && last < 1.05);
        for _ in 0..second * 200 {
            bots.adapt(&world);
        }
        assert_eq!(bots.skill(), 3., "a far stronger human meets Expert play");
        world
            .tanks
            .iter_mut()
            .find(|t| t.id == human)
            .unwrap()
            .level = 1;
        for _ in 0..second * 30 {
            bots.adapt(&world);
        }
        assert!(bots.skill() < 3. && bots.skill() > 1.);
        for _ in 0..second * 200 {
            bots.adapt(&world);
        }
        assert_eq!(bots.skill(), 1.);
        // Practice trials and benchmarks keep a fixed difficulty.
        bots.adaptive = false;
        world
            .tanks
            .iter_mut()
            .find(|t| t.id == human)
            .unwrap()
            .level = 45;
        for _ in 0..second * 200 {
            bots.adapt(&world);
        }
        assert_eq!(bots.skill(), 1.);
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
    fn parry_bots_recognize_approaching_smashers_but_do_not_parry_retreating_hulls() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Defender", true);
        world.add_player("Smasher", false);
        world.tanks[0].class = "riposte".into();
        world.tanks[0].pos = V2::new(1000., 1000.);
        world.tanks[1].class = "bulwark".into();
        world.tanks[1].pos = V2::new(1080., 1000.);
        world.tanks[1].vel = V2::new(-300., 0.);
        assert!(
            parry_opportunity(&world, &world.tanks[0], &[Seen::Tank(1)])
                .is_some_and(|eta| eta < 0.18)
        );
        world.tanks[1].vel.x = 300.;
        assert!(parry_opportunity(&world, &world.tanks[0], &[Seen::Tank(1)]).is_none());
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
            frozen: None,
            stasis_used: false,
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
            frozen: None,
            stasis_used: false,
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
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
            &[Seen::Tank(1)],
            &mut neutral,
        );
        assert_ne!(neutral.state, BotState::Retreating);
        neutral.learning.relationships.hurt_by(feared, 0.8, true, 0);
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
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
            Difficulty::Expert.into(),
            &[Seen::Tank(1)],
            &mut brain,
        );
        assert_eq!(brain.goal, world.tanks[1].pos);
        world.tanks[0].health = world.tanks[0].max_health * 0.2;
        decide(
            &world,
            &world.tanks[0],
            class,
            Difficulty::Expert.into(),
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
        assert!(parry_opportunity(&world, &world.tanks[0], &seen).is_some());
        world.projectiles[0].kind = ProjectileKind::Minion;
        assert!(parry_opportunity(&world, &world.tanks[0], &seen).is_some());
        world.projectiles[0].kind = ProjectileKind::Trap;
        assert!(parry_opportunity(&world, &world.tanks[0], &seen).is_none());
    }

    #[test]
    fn parry_controllers_make_consistent_human_errors_instead_of_frame_by_frame_rerolls() {
        let mut successes = Vec::new();
        for difficulty in [Difficulty::Beginner, Difficulty::Expert] {
            let mut world = pursuit_world(ProjectileKind::Bullet);
            world.config.mode = Mode::Sandbox;
            let bot = world.tanks[0].id;
            assert!(world.sandbox_class(bot, "reprisal"));
            world.tanks[0].pos = V2::new(1000., 1000.);
            world.tanks[0].vel = V2::ZERO;
            world.projectiles[0].pos = V2::new(1055., 1000.);
            world.projectiles[0].vel = V2::new(-400., 0.);
            let mut controller = BotController::new(difficulty);
            let mut count = 0;
            let period = if difficulty == Difficulty::Beginner {
                12
            } else {
                4
            };
            for attempt in 1..=400 {
                world.tick = attempt * 60 + (period - bot % period) % period;
                world.projectiles[0].id = 1000 + attempt;
                controller.update(&mut world);
                let input = world.tanks[0].input;
                count += usize::from(input.ability);
                controller.update(&mut world);
                assert_eq!(world.tanks[0].input.ability, input.ability);
            }
            successes.push(count);
        }
        assert!(
            (140..=270).contains(&successes[0]),
            "beginner: {}",
            successes[0]
        );
        assert!(
            (320..=390).contains(&successes[1]),
            "expert: {}",
            successes[1]
        );
        assert!(successes[1] > successes[0] + 70);
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
            .filter_map(|tank| objective_goal(&world, tank, None))
            .map(|(_, _, state)| state)
            .collect::<Vec<_>>();
        assert!(roles.contains(&BotState::BossEngagement));
        assert!(roles.contains(&BotState::Defending));
        for boss in &mut world.bosses {
            boss.health = boss.max_health * 0.5;
        }
        assert!(world.tanks.iter().all(|tank| {
            objective_goal(&world, tank, None)
                .is_some_and(|(_, _, state)| state == BotState::Defending)
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
    fn bots_back_away_from_guarded_enemy_bases_but_not_their_own() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::TwoTeams,
                target_count: 0,
                dynamic_arena: false,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let bot = world.add_player("Raider", true);
        world.assign_team(bot, 2);
        let base = world.config.team_base(1).unwrap();
        world.tanks[0].pos = V2::new(base.x + base.w + 60., base.center().y);
        let push = guarded_base_repulsion(&world, &world.tanks[0]);
        assert!(push.x > 0.5 && push.y.abs() < 0.01, "{push:?}");
        world.tanks[0].pos = world.config.bounds().center();
        assert_eq!(guarded_base_repulsion(&world, &world.tanks[0]), V2::ZERO);
        world.assign_team(bot, 1);
        world.tanks[0].pos = V2::new(base.x + base.w + 60., base.center().y);
        assert_eq!(guarded_base_repulsion(&world, &world.tanks[0]), V2::ZERO);
        world.config.mode = Mode::CaptureCore;
        world.assign_team(bot, 2);
        world.tanks[0].pos = V2::new(base.x + base.w + 60., base.center().y);
        assert_eq!(guarded_base_repulsion(&world, &world.tanks[0]), V2::ZERO);
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
