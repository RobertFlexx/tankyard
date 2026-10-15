use tankyard_core::{EntityId, V2};

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
}
pub struct Learning {
    opponents: Vec<Opponent>,
    pub caution: f32,
    health: Option<f32>,
    kills: u32,
    deaths: u32,
}
impl Default for Learning {
    fn default() -> Self {
        Self {
            opponents: Vec::new(),
            caution: 0.35,
            health: None,
            kills: 0,
            deaths: 0,
        }
    }
}
impl Learning {
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
                opponent.motion.velocity = opponent.motion.velocity.lerp(measured, 0.55);
            } else if elapsed > 0.8 {
                opponent.motion.velocity = V2::ZERO;
                opponent.motion.confidence *= 0.7;
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
