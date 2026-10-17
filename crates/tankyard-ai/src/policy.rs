//! Outcome learning supplements legal, procedural build choices with own results
//! and visible human class results. Private skill allocations/inputs stay unread.
use std::collections::VecDeque;
use tankyard_content::{ClassDef, Content};
use tankyard_core::Tank;

use crate::{neural::Network, noise};

const REPLAY_LIMIT: usize = 40;
const WINDOW_TICKS: u64 = 120;

#[derive(Clone, Copy)]
struct Experience {
    class: [f32; 8],
    build: [f32; 13],
    reward: f32,
}

struct Observation {
    tick: u64,
    class_key: u64,
    stats: [u8; 8],
    health: f32,
    score: u32,
    kills: u32,
    deaths: u32,
    engaged: bool,
    experience: Experience,
}

struct VisiblePlayer {
    id: u64,
    start: u64,
    last_seen: u64,
    class_key: u64,
    class: [f32; 8],
    score: u32,
    kills: u32,
    health: f32,
}

#[derive(Default)]
pub struct BuildLearning {
    classes: Network<8, 1>,
    builds: Network<13, 1>,
    observation: Option<Observation>,
    replay: VecDeque<Experience>,
    outcomes: u32,
    class_outcomes: u32,
    players: Vec<VisiblePlayer>,
}

fn class_key(class: &ClassDef) -> u64 {
    class.id.bytes().fold(0xcbf29ce484222325, |key, byte| {
        (key ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn class_features(class: &ClassDef) -> [f32; 8] {
    let damage_rate: f32 = class
        .barrels
        .iter()
        .map(|barrel| barrel.damage * f32::from(barrel.count) / barrel.cooldown.max(0.05))
        .sum();
    [
        (damage_rate * class.multipliers[5] * class.multipliers[6] / 150.).min(1.),
        (class.multipliers[0] / 3.).min(1.),
        (class.multipliers[2] / 5.).min(1.),
        (class.multipliers[7] / 2.).min(1.),
        (class.ai_range / 900.).min(1.),
        f32::from(
            class
                .barrels
                .iter()
                .any(|barrel| barrel.projectile.is_drone()),
        ),
        f32::from(class.has_deployables()),
        1.,
    ]
}

fn build_features(class: &ClassDef, stats: [u8; 8]) -> [f32; 13] {
    let role = class_features(class);
    std::array::from_fn(|index| match index {
        0..=7 => f32::from(stats[index].min(7)) / 7.,
        8 => f32::from(class.multipliers[2] >= 2.),
        9 => role[5],
        10 => role[6],
        11 => role[4],
        _ => 1.,
    })
}

impl BuildLearning {
    /// Learn class traits from a human only while that person stays visible.
    /// Hidden gaps discard the sample; private skill allocations are not copied.
    pub fn observe_player(&mut self, player: &Tank, content: &Content, tick: u64) {
        if player.bot || !player.alive {
            return;
        }
        let Some(class) = content.class(&player.class) else {
            return;
        };
        let current = VisiblePlayer {
            id: player.id,
            start: tick,
            last_seen: tick,
            class_key: class_key(class),
            class: class_features(class),
            score: player.score,
            kills: player.kills,
            health: player.health / player.max_health.max(1.),
        };
        let index = if let Some(index) = self.players.iter().position(|seen| seen.id == player.id) {
            index
        } else {
            if self.players.len() == 8 {
                let oldest = self
                    .players
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, seen)| seen.last_seen)
                    .unwrap()
                    .0;
                self.players.swap_remove(oldest);
            }
            self.players.push(current);
            return;
        };
        let previous = &mut self.players[index];
        if tick.saturating_sub(previous.last_seen) > 30 || previous.class_key != current.class_key {
            *previous = current;
            return;
        }
        previous.last_seen = tick;
        if tick.saturating_sub(previous.start) < WINDOW_TICKS {
            return;
        }
        let score = player.score.saturating_sub(previous.score);
        let kills = player.kills.saturating_sub(previous.kills);
        let damage = (previous.health - current.health).max(0.);
        if score > 0 || kills > 0 || damage > 0. {
            let reward = ((score as f32 / 2000.).min(0.15) + kills.min(2) as f32 * 0.35
                - damage * 0.35)
                .clamp(-0.75, 0.75);
            self.classes.train(previous.class, [reward]);
            self.class_outcomes = self.class_outcomes.saturating_add(1);
        }
        *previous = current;
    }

    pub fn observe(&mut self, tick: u64, tank: &Tank, class: &ClassDef, engaged: bool) {
        let key = class_key(class);
        if let Some(previous) = &mut self.observation {
            previous.engaged |= engaged;
            let elapsed = tick.saturating_sub(previous.tick);
            let died = tank.deaths > previous.deaths;
            let changed = previous.class_key != key || previous.stats != tank.stats;
            if elapsed < WINDOW_TICKS && !died && !changed {
                return;
            }
            // Discard very short loadouts rather than crediting an upgrade's
            // healing or a kill that happened before that build was equipped.
            if elapsed >= 60 || died {
                let health = tank.health / tank.max_health.max(1.);
                let damage = (previous.health - health).max(0.);
                let score = tank.score.saturating_sub(previous.score);
                let kills = tank.kills.saturating_sub(previous.kills);
                if previous.engaged || damage > 0. || score > 0 || died {
                    let reward = (kills.min(2) as f32 * 0.5 + (score as f32 / 1500.).min(0.25)
                        - damage * 0.6
                        - if died { 0.85 } else { 0. }
                        + if previous.engaged && !died { 0.025 } else { 0. })
                    .clamp(-1., 1.);
                    let experience = Experience {
                        reward,
                        ..previous.experience
                    };
                    self.record(experience, tick);
                }
            }
        }
        self.observation = Some(Observation {
            tick,
            class_key: key,
            stats: tank.stats,
            health: tank.health / tank.max_health.max(1.),
            score: tank.score,
            kills: tank.kills,
            deaths: tank.deaths,
            engaged,
            experience: Experience {
                class: class_features(class),
                build: build_features(class, tank.stats),
                reward: 0.,
            },
        });
    }

    fn record(&mut self, experience: Experience, tick: u64) {
        if self.replay.len() == REPLAY_LIMIT {
            self.replay.pop_front();
        }
        self.replay.push_back(experience);
        self.outcomes = self.outcomes.saturating_add(1);
        self.class_outcomes = self.class_outcomes.saturating_add(1);
        self.train(experience);
        // Three replay samples reduce forgetting with a fixed CPU/memory budget.
        for offset in 0..3 {
            let index = (noise(tick.wrapping_add(offset * 37)) * self.replay.len() as f32) as usize;
            self.train(self.replay[index.min(self.replay.len() - 1)]);
        }
    }

    fn train(&mut self, experience: Experience) {
        self.classes.train(experience.class, [experience.reward]);
        self.builds.train(experience.build, [experience.reward]);
    }

    fn confidence(&self) -> f32 {
        (self.outcomes as f32 / 24.).min(1.)
    }

    /// A stable preference gives untrained bots varied builds. A small fraction
    /// of lives try a different branch, so learning cannot lock out alternatives.
    pub fn class_score(&self, tank: &Tank, candidate: &ClassDef) -> f32 {
        let life = tank.id.wrapping_mul(19) ^ u64::from(tank.deaths).wrapping_mul(7919);
        let preference = noise(life ^ class_key(candidate)) * 0.3;
        if noise(life ^ 0xe771) < 0.12 {
            return preference;
        }
        preference
            + self.classes.predict(class_features(candidate))[0]
                * (self.class_outcomes as f32 / 24.).min(1.)
                * 0.45
    }

    /// Only a bounded nudge to a sensible class-specific allocation. This cannot
    /// create points, exceed seven per stat, or change the tank's combat values.
    pub fn skill_value(&self, class: &ClassDef, stats: [u8; 8], index: usize) -> f32 {
        if self.outcomes == 0 {
            return 0.;
        }
        let before = self.builds.predict(build_features(class, stats))[0];
        let mut after = stats;
        after[index] = (after[index] + 1).min(7);
        let after = self.builds.predict(build_features(class, after))[0];
        ((after - before) * 8.).clamp(-0.18, 0.18) * self.confidence()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;
    use tankyard_core::{World, WorldConfig};

    #[test]
    fn own_outcomes_train_models_idle_time_does_not_and_memory_stays_bounded() {
        let mut world = World::new(WorldConfig::default(), Content::builtin());
        world.add_player("Learner", true);
        let tank = &mut world.tanks[0];
        let class = world.content.class(&tank.class).unwrap();
        let mut learning = BuildLearning::default();
        for tick in (0..=1200).step_by(120) {
            learning.observe(tick, tank, class, false);
        }
        assert_eq!(learning.outcomes, 0);
        tank.score += 150;
        tank.kills += 1;
        learning.observe(1320, tank, class, true);
        assert_eq!(learning.outcomes, 1);
        assert!(learning.classes.samples > 0 && learning.builds.samples > 0);
        assert!(learning.replay[0].reward > 0.5);
        // A respawn's restored health and reset score must still penalize death.
        tank.deaths += 1;
        tank.score = 0;
        learning.observe(1321, tank, class, false);
        assert!(learning.replay.back().unwrap().reward < -0.8);
        for tick in (1440..14400).step_by(120) {
            tank.health *= 0.99;
            learning.observe(tick, tank, class, true);
        }
        assert_eq!(learning.replay.len(), REPLAY_LIMIT);
        assert!(learning.classes.samples < 500);
    }

    #[test]
    fn learns_visible_human_results_but_never_hidden_gaps_or_private_skills() {
        let mut world = World::new(WorldConfig::default(), Content::builtin());
        world.add_player("Human", false);
        let player = &mut world.tanks[0];
        let mut learning = BuildLearning::default();
        learning.observe_player(player, &world.content, 0);
        player.score = 1000;
        player.kills = 1;
        // Reappearing after two seconds cannot credit an unseen fight.
        learning.observe_player(player, &world.content, 120);
        assert_eq!(learning.class_outcomes, 0);
        for tick in (132..=240).step_by(12) {
            player.score += 15;
            learning.observe_player(player, &world.content, tick);
        }
        assert_eq!(learning.class_outcomes, 1);
        assert_eq!(learning.classes.samples, 1);
        assert_eq!(
            learning.builds.samples, 0,
            "human skill allocations stay private"
        );
        player.stats = [7; 8];
        learning.observe_player(player, &world.content, 252);
        assert_eq!(learning.builds.samples, 0);
        for id in 10..100 {
            player.id = id;
            learning.observe_player(player, &world.content, 260);
        }
        assert_eq!(learning.players.len(), 8);
    }

    #[test]
    fn neural_training_changes_tank_preferences_and_skill_allocation() {
        let content = Content::builtin();
        let good = content.class("anchor").unwrap();
        let bad = content.class("hive").unwrap();
        let mut world = World::new(WorldConfig::default(), content.clone());
        world.add_player("Learner", true);
        let tank = &world.tanks[0];
        let mut learning = BuildLearning::default();
        // Real SGD on contrasting results must affect the scores used by the
        // controller, not just a disconnected prediction or sample counter.
        for i in 0..2500 {
            let positive = i % 2 == 0;
            let class = if positive { good } else { bad };
            let mut stats = [0; 8];
            stats[if positive { 5 } else { 1 }] = 5;
            learning.record(
                Experience {
                    class: class_features(class),
                    build: build_features(class, stats),
                    reward: if positive { 0.8 } else { -0.8 },
                },
                i,
            );
        }
        assert!(learning.class_score(tank, good) > learning.class_score(tank, bad));
        let mut choice = content.class("vector").unwrap().clone();
        choice.upgrades = vec![good.id.clone(), bad.id.clone()];
        let mut eligible_tank = tank.clone();
        eligible_tank.level = 15;
        eligible_tank.points = 0;
        let mut commands = Vec::new();
        crate::progression_commands(&world, &eligible_tank, &choice, &learning, &mut commands);
        assert!(matches!(&commands[0], crate::Command::Evolve(_, id) if id == &good.id));

        // Hold the class fixed to prove the skill model learns allocation,
        // rather than merely recognizing the class used in the first examples.
        for i in 0..2500 {
            let positive = i % 2 == 0;
            let mut stats = [0; 8];
            stats[if positive { 5 } else { 1 }] = 5;
            learning.record(
                Experience {
                    class: class_features(good),
                    build: build_features(good, stats),
                    reward: if positive { 0.8 } else { -0.8 },
                },
                i + 2500,
            );
        }
        assert!(learning.skill_value(good, [0; 8], 5) > learning.skill_value(good, [0; 8], 1));
        eligible_tank.points = 1;
        eligible_tank.class = good.id.clone();
        commands.clear();
        crate::progression_commands(&world, &eligible_tank, good, &learning, &mut commands);
        assert!(matches!(commands[0], crate::Command::Stat(_, 5)));
        for index in 0..8 {
            assert!(learning.skill_value(good, [0; 8], index).abs() <= 0.18);
        }
    }
}
