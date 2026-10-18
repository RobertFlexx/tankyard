use tankyard_core::{EntityId, EventKind, GameEvent, Tank, V2};

#[derive(Clone, Copy)]
pub struct Motion {
    pub velocity: V2,
    pub confidence: f32,
}
struct Opponent {
    id: EntityId,
    position: V2,
    tick: u64,
    motion: Motion,
    measured: V2,
    features: Option<[f32; 6]>,
}
pub struct Learning {
    pub build: crate::policy::BuildLearning,
    pub relationships: crate::relationships::Relationships,
    pub tactics: crate::tactics::TacticsLearning,
    neural: crate::neural::MotionNet,
    opponents: Vec<Opponent>,
    pub caution: f32,
    health: Option<f32>,
    kills: u32,
    deaths: u32,
    feedback_tick: Option<u64>,
    last_hit: Option<(u64, EntityId)>,
}
impl Default for Learning {
    fn default() -> Self {
        Self {
            build: Default::default(),
            relationships: Default::default(),
            tactics: Default::default(),
            neural: Default::default(),
            opponents: Vec::new(),
            caution: 0.35,
            health: None,
            kills: 0,
            deaths: 0,
            feedback_tick: None,
            last_hit: None,
        }
    }
}
impl Learning {
    pub(crate) fn inherit_models(&mut self, previous: &Self) {
        self.neural = previous.neural.clone();
        self.build.inherit_models(&previous.build);
        self.tactics.inherit(&previous.tactics);
    }
    pub fn feedback(&mut self, tank: &Tank, tick: u64, events: &[GameEvent]) {
        if self.feedback_tick == Some(tick) {
            return;
        }
        self.feedback_tick = Some(tick);
        self.tactics.observe(tank, tick);
        let health = tank.health / tank.max_health.max(1.);
        let hit = (tank.damaged_at, tank.last_attacker);
        if tank.alive && tank.last_attacker != 0 && self.last_hit != Some(hit) {
            let damage = self
                .health
                .map_or(0., |previous| (previous - health).max(0.));
            self.relationships
                .hurt_by(tank.last_attacker, damage, false, tick);
            self.last_hit = Some(hit);
        }
        // Destroy/Kill pairs are the bot's own death notice or kill credit.
        // Processing dead bots preserves the real killer before respawn; no
        // unseen player's location, motion or loadout is read here.
        for pair in events.windows(2) {
            if matches!(pair[0].kind, EventKind::Destroy)
                && matches!(pair[1].kind, EventKind::Kill)
                && pair[0].tick == pair[1].tick
                && pair[0].pos == pair[1].pos
            {
                if pair[0].subject == tank.id && tank.deaths > self.deaths {
                    self.relationships.hurt_by(pair[1].subject, 0., true, tick);
                } else if pair[1].subject == tank.id && tank.kills > self.kills {
                    self.relationships.defeated(pair[0].subject, tick);
                }
            }
        }
        self.combat(health, tank.kills, tank.deaths);
    }

    pub fn combat(&mut self, health: f32, kills: u32, deaths: u32) {
        if let Some(previous) = self.health {
            self.caution += (previous - health).max(0.) * 0.20;
            self.caution += deaths.saturating_sub(self.deaths).min(4) as f32 * 0.12;
            self.caution -= kills.saturating_sub(self.kills).min(4) as f32 * 0.04;
        }
        self.caution = self.caution.clamp(0.12, 0.9);
        self.health = Some(health);
        self.kills = kills;
        self.deaths = deaths;
    }
    pub fn observe(&mut self, id: EntityId, position: V2, tick: u64) -> Motion {
        if let Some(opponent) = self.opponents.iter_mut().find(|opponent| opponent.id == id) {
            let elapsed = tick.saturating_sub(opponent.tick) as f32 / 60.;
            if elapsed > 0. && elapsed <= 0.8 {
                let measured = ((position - opponent.position) / elapsed).clamp_length(1200.);
                let prediction = opponent.position + opponent.motion.velocity * elapsed;
                let tolerance = 12. + measured.length() * elapsed * 0.85;
                let accuracy = (1. - position.distance(prediction) / tolerance).clamp(0., 1.);
                opponent.motion.confidence += (accuracy - opponent.motion.confidence) * 0.18;
                let change = (measured - opponent.measured).clamp_length(200.0) / 200.0;
                if let Some(features) = opponent.features {
                    self.neural.train(features, [change.x, change.y]);
                }
                let features = [
                    (measured.x / 1200.0).clamp(-1.0, 1.0),
                    (measured.y / 1200.0).clamp(-1.0, 1.0),
                    change.x,
                    change.y,
                    (elapsed / 0.8).clamp(0.0, 1.0),
                    1.0,
                ];
                let learned = self.neural.predict(features);
                let strength = (self.neural.samples as f32 / 120.0).min(1.0)
                    * opponent.motion.confidence
                    * 0.25;
                let correction =
                    V2::new(learned[0], learned[1]).clamp_length(1.0) * (80.0 * strength);
                opponent.motion.velocity = (opponent.motion.velocity.lerp(measured, 0.55)
                    + correction)
                    .clamp_length(1200.0);
                opponent.measured = measured;
                opponent.features = Some(features);
            } else if elapsed > 0.8 {
                opponent.motion.velocity = V2::ZERO;
                opponent.motion.confidence *= 0.7;
                opponent.measured = V2::ZERO;
                opponent.features = None;
            }
            opponent.position = position;
            opponent.tick = tick;
            return opponent.motion;
        }
        if self.opponents.len() >= 12 {
            let oldest = self
                .opponents
                .iter()
                .enumerate()
                .min_by_key(|(_, opponent)| opponent.tick)
                .unwrap()
                .0;
            self.opponents.swap_remove(oldest);
        }
        let motion = Motion {
            velocity: V2::ZERO,
            confidence: 0.2,
        };
        self.opponents.push(Opponent {
            id,
            position,
            tick,
            motion,
            measured: V2::ZERO,
            features: None,
        });
        motion
    }
    pub fn confidence(&self, id: EntityId) -> f32 {
        self.opponents
            .iter()
            .find(|opponent| opponent.id == id)
            .map_or(0.2, |opponent| opponent.motion.confidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learns_motion_from_observations_and_reduces_lead_after_reversals() {
        let mut learning = Learning::default();
        let initial = learning.observe(1, V2::ZERO, 0);
        assert_eq!(initial.velocity, V2::ZERO);
        for i in 1..=30 {
            learning.observe(1, V2::new(i as f32 * 20., 0.), i * 12);
        }
        let steady = learning.confidence(1);
        assert!(learning.neural.samples > 20);
        assert!(steady > 0.85);
        let mut position = V2::new(600., 0.);
        for i in 31..=45 {
            position.x += if i % 2 == 0 { 20. } else { -20. };
            learning.observe(1, position, i * 12);
        }
        assert!(learning.confidence(1) < steady * 0.6);
        let unseen = learning.observe(1, V2::new(1000., 1000.), 2000);
        assert_eq!(unseen.velocity, V2::ZERO);
    }
    #[test]
    fn combat_experience_changes_spacing_without_changing_tank_stats() {
        let mut learning = Learning::default();
        learning.combat(1., 0, 0);
        learning.combat(0.5, 0, 0);
        learning.combat(1., 0, 1);
        assert!(learning.caution > 0.55);
        let cautious = learning.caution;
        learning.combat(1., 2, 1);
        assert!(learning.caution < cautious);
        for i in 2..100 {
            learning.combat(0., 2, i);
        }
        assert!(learning.caution <= 0.9);
    }
    #[test]
    fn opponent_memory_stays_bounded_in_crowded_arenas() {
        let mut learning = Learning::default();
        for id in 1..=500 {
            learning.observe(id, V2::new(id as f32, 0.), id);
        }
        assert_eq!(learning.opponents.len(), 12);
        assert!(learning.opponents.iter().all(|opponent| opponent.id >= 489));
    }
}

#[cfg(test)]
mod round_learning_regressions {
    use super::*;

    #[test]
    fn neural_weights_survive_a_round_without_retaining_player_sightings() {
        let mut trained = Learning::default();
        for tick in 1..=120 {
            trained.observe(17, V2::new(tick as f32 * 5., 50.), tick * 6);
        }
        assert!(trained.neural.samples > 100);
        let sample = [0.2, -0.2, 0.0, 0.0, 0.25, 1.0];
        let mut fresh = Learning::default();
        fresh.inherit_models(&trained);
        assert_eq!(fresh.neural.samples, trained.neural.samples);
        assert_eq!(fresh.neural.predict(sample), trained.neural.predict(sample));
        assert!(fresh.opponents.is_empty());
        assert!(fresh.feedback_tick.is_none());
        assert_eq!(fresh.caution, 0.35);
    }
}
