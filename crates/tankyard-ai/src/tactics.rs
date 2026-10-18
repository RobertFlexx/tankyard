//! Contextual outcome learning chooses combat spacing and flanking maneuvers.
//! Safety, legal inputs, visibility and objectives remain simulation constraints.
use crate::{neural::Network, noise};
use std::collections::VecDeque;
use tankyard_core::Tank;

const ACTIONS: usize = 6;
type Features = [f32; 14];
struct Trial {
    tick: u64,
    health: f32,
    kills: u32,
    deaths: u32,
    score: u32,
    action: usize,
    features: Features,
}
#[derive(Default)]
pub struct TacticsLearning {
    model: Network<14, 1>,
    trial: Option<Trial>,
    replay: VecDeque<(Features, f32)>,
    outcomes: u32,
}
fn features(context: [f32; 8], action: usize) -> Features {
    std::array::from_fn(|i| {
        if i < 8 {
            context[i]
        } else {
            f32::from(i - 8 == action)
        }
    })
}
impl TacticsLearning {
    pub fn inherit(&mut self, previous: &Self) {
        self.model = previous.model.clone();
        self.outcomes = previous.outcomes;
    }
    pub fn observe(&mut self, tank: &Tank, tick: u64) {
        let Some(trial) = &self.trial else {
            return;
        };
        if tick.saturating_sub(trial.tick) < 90 && tank.deaths == trial.deaths {
            return;
        }
        let health = tank.health / tank.max_health.max(1.);
        let reward = (tank.kills.saturating_sub(trial.kills).min(2) as f32 * 0.55
            + (tank.score.saturating_sub(trial.score) as f32 / 1800.).min(0.2)
            - (trial.health - health).max(0.) * 0.9
            - if tank.deaths > trial.deaths { 0.9 } else { 0. }
            + if tank.alive { 0.015 } else { 0. })
        .clamp(-1., 1.);
        let feature = trial.features;
        self.trial = None;
        self.record(feature, reward, tick);
    }
    fn record(&mut self, feature: Features, reward: f32, tick: u64) {
        if self.replay.len() == 48 {
            self.replay.pop_front();
        }
        self.replay.push_back((feature, reward));
        self.outcomes = self.outcomes.saturating_add(1);
        self.model.train(feature, [reward]);
        for offset in 0..3 {
            let index = ((noise(tick.wrapping_add(offset * 29)) * self.replay.len() as f32)
                as usize)
                .min(self.replay.len() - 1);
            let (feature, reward) = self.replay[index];
            self.model.train(feature, [reward]);
        }
    }
    pub fn choose(
        &mut self,
        tank: &Tank,
        tick: u64,
        context: [f32; 8],
        legal: [bool; ACTIONS],
    ) -> usize {
        if let Some(trial) = &self.trial
            && legal[trial.action]
        {
            return trial.action;
        }
        let epoch = tick / 90;
        let seed = tank.id.wrapping_mul(107)
            ^ epoch.wrapping_mul(79)
            ^ u64::from(tank.deaths).wrapping_mul(7919);
        let exploring = noise(seed ^ 0xe771) < 0.15;
        let score = |action| {
            let preference = noise(seed ^ (action as u64).wrapping_mul(43)) * 0.18;
            let learned = self.model.predict(features(context, action))[0];
            preference
                + if exploring {
                    0.
                } else {
                    learned * (self.outcomes as f32 / 20.).min(1.)
                }
        };
        let action = (0..ACTIONS)
            .filter(|&a| legal[a])
            .max_by(|&a, &b| score(a).total_cmp(&score(b)))
            .unwrap_or(0);
        self.trial = Some(Trial {
            tick,
            health: tank.health / tank.max_health.max(1.),
            kills: tank.kills,
            deaths: tank.deaths,
            score: tank.score,
            action,
            features: features(context, action),
        });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradient_learning_changes_actual_choices_and_remembers_across_rounds() {
        let mut learner = TacticsLearning::default();
        let context = [0.7, 0.3, 0., 0.8, 0.2, 0.4, 0., 1.];
        for tick in 0..2500 {
            let action = tick as usize % 2;
            learner.record(
                features(context, action),
                if action == 0 { -0.8 } else { 0.8 },
                tick,
            );
        }
        assert!(
            learner.model.predict(features(context, 1))[0]
                > learner.model.predict(features(context, 0))[0] + 0.6
        );
        let mut world =
            tankyard_core::World::new(Default::default(), tankyard_content::Content::builtin());
        world.add_player("Learner", true);
        let mut preferred = 0;
        for tick in 0..100 {
            learner.trial = None;
            preferred += usize::from(
                learner.choose(
                    &world.tanks[0],
                    tick * 90,
                    context,
                    [true, true, false, false, false, false],
                ) == 1,
            );
        }
        assert!(preferred > 80);
        let mut next = TacticsLearning::default();
        next.inherit(&learner);
        assert_eq!(
            next.model.predict(features(context, 1)),
            learner.model.predict(features(context, 1))
        );
        assert!(next.trial.is_none() && next.replay.is_empty());
        assert_eq!(learner.replay.len(), 48);
    }
}
