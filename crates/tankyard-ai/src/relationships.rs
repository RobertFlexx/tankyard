//! Match-local recognition changes tactics, never perception or damage.
use tankyard_core::EntityId;

const MEMORY_LIMIT: usize = 12;

#[derive(Clone, Copy, Default)]
pub struct Disposition {
    pub fear: f32,
    pub animosity: f32,
    pub friendship: f32,
}

struct Relationship {
    id: EntityId,
    disposition: Disposition,
    tick: u64,
}

#[derive(Default)]
pub struct Relationships {
    people: Vec<Relationship>,
}

impl Relationships {
    pub fn get(&self, id: EntityId, tick: u64) -> Disposition {
        self.people
            .iter()
            .find(|person| person.id == id)
            .map_or(Disposition::default(), |person| {
                // Grudges cool down without requiring a per-tick memory scan.
                let decay = (-(tick.saturating_sub(person.tick) as f32) / 7200.).exp();
                Disposition {
                    fear: person.disposition.fear * decay,
                    animosity: person.disposition.animosity * decay,
                    friendship: person.disposition.friendship * decay,
                }
            })
    }

    pub fn hurt_by(&mut self, id: EntityId, damage: f32, killed: bool, tick: u64) {
        if id == 0 || !damage.is_finite() || (damage <= 0. && !killed) {
            return;
        }
        let disposition = self.get(id, tick);
        let person = self.remember(id, tick);
        person.disposition = Disposition {
            fear: (disposition.fear + damage.clamp(0., 1.) * 0.75 + if killed { 0.3 } else { 0. })
                .min(0.9),
            animosity: (disposition.animosity
                + damage.clamp(0., 1.) * 0.35
                + if killed { 0.25 } else { 0. })
            .min(0.85),
            friendship: disposition.friendship * 0.25,
        };
    }

    pub fn defeated(&mut self, id: EntityId, tick: u64) {
        if id == 0 {
            return;
        }
        let disposition = self.get(id, tick);
        let person = self.remember(id, tick);
        person.disposition = Disposition {
            fear: disposition.fear * 0.6,
            animosity: disposition.animosity * 0.7,
            friendship: disposition.friendship * 0.7,
        };
    }

    pub fn cooperate(&mut self, id: EntityId, strength: f32, tick: u64) {
        if id == 0 || !strength.is_finite() || strength <= 0. {
            return;
        }
        let previous_tick = self
            .people
            .iter()
            .find(|p| p.id == id)
            .map_or(tick.saturating_sub(12), |p| p.tick);
        let elapsed = tick.saturating_sub(previous_tick).min(30) as f32 / 60.;
        if elapsed <= 0. {
            return;
        }
        let disposition = self.get(id, tick);
        let person = self.remember(id, tick);
        person.disposition = Disposition {
            friendship: (disposition.friendship + strength.clamp(0., 1.) * elapsed * 0.025)
                .min(0.9),
            animosity: (disposition.animosity - elapsed * 0.008).max(0.),
            fear: disposition.fear * (1. - elapsed * 0.015),
        };
    }

    fn remember(&mut self, id: EntityId, tick: u64) -> &mut Relationship {
        let index = if let Some(index) = self.people.iter().position(|person| person.id == id) {
            index
        } else {
            if self.people.len() == MEMORY_LIMIT {
                let oldest = self
                    .people
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, person)| person.tick)
                    .unwrap()
                    .0;
                self.people.swap_remove(oldest);
            }
            self.people.push(Relationship {
                id,
                tick,
                disposition: Disposition::default(),
            });
            self.people.len() - 1
        };
        self.people[index].tick = tick;
        &mut self.people[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognition_is_personal_and_grudges_cool_down_or_ease_after_a_win() {
        let mut memory = Relationships::default();
        memory.hurt_by(41, 0.4, false, 100);
        memory.hurt_by(41, 0., true, 110);
        let remembered = memory.get(41, 110);
        assert!(remembered.fear > 0.55 && remembered.animosity > 0.35);
        assert_eq!(memory.get(42, 110).fear, 0.);
        assert!(memory.get(41, 7310).fear < remembered.fear * 0.4);
        memory.defeated(41, 120);
        assert!(memory.get(41, 120).fear < remembered.fear * 0.61);
        assert!(memory.get(41, 120).animosity < remembered.animosity);
    }

    #[test]
    fn grudges_cannot_grow_unbounded_or_keep_everyone_in_a_busy_server() {
        let mut memory = Relationships::default();
        for tick in 0..100 {
            memory.hurt_by(1, 10., true, tick);
        }
        assert!(memory.get(1, 100).fear <= 0.9);
        assert!(memory.get(1, 100).animosity <= 0.85);
        memory.hurt_by(2, f32::NAN, false, 100);
        assert_eq!(memory.get(2, 100).fear, 0.);
        for id in 2..200 {
            memory.hurt_by(id, 0.1, false, id + 100);
        }
        assert_eq!(memory.people.len(), MEMORY_LIMIT);
        assert_eq!(memory.get(1, 400).fear, 0.);
    }
    #[test]
    fn cooperation_builds_friendship_and_betrayal_changes_that_relationship() {
        let mut memory = Relationships::default();
        for tick in (0..3600).step_by(12) {
            memory.cooperate(41, 1., tick);
        }
        let friendship = memory.get(41, 3600).friendship;
        assert!(friendship > 0.8);
        memory.hurt_by(41, 0.5, false, 3601);
        let betrayed = memory.get(41, 3601);
        assert!(betrayed.friendship < friendship * 0.26);
        assert!(betrayed.animosity > 0.15 && betrayed.fear > 0.3);
        memory.cooperate(42, f32::NAN, 3602);
        assert_eq!(memory.get(42, 3602).friendship, 0.);
    }
}
