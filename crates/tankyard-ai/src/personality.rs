//! Stable traits that make each bot play like a particular person rather than
//! a copy of one procedure. Traits shape choices (how far to push, when to
//! back off, where to roam, whom to trust); they never change stats, damage,
//! perception or reaction limits, which stay identical for every bot.
use crate::noise;
use tankyard_core::EntityId;

#[derive(Clone, Copy, Debug)]
pub struct Personality {
    /// Pushes fights, hunts players over shapes, retreats later.
    pub aggression: f32,
    /// Holds range and position; impatient bots reposition and chase more.
    pub patience: f32,
    /// Seeks out places where action was seen instead of wandering.
    pub curiosity: f32,
    /// Sticks with allies and warms to peaceful players faster.
    pub sociability: f32,
}

impl Personality {
    pub fn of(id: EntityId) -> Self {
        // Mid-weighted traits: extremes are rare, as with people.
        let trait_of = |salt: u64| {
            let a = noise(id.wrapping_mul(0x9e37_79b9) ^ salt);
            let b = noise(id.wrapping_mul(0x85eb_ca6b) ^ salt.rotate_left(17));
            (a + b) * 0.5
        };
        Self {
            aggression: trait_of(0xa66),
            patience: trait_of(0x9a7),
            curiosity: trait_of(0xc41),
            sociability: trait_of(0x50c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personalities_are_stable_bounded_and_varied() {
        let a = Personality::of(7);
        let b = Personality::of(7);
        assert_eq!(a.aggression, b.aggression);
        let traits: Vec<f32> = (1..200).map(|id| Personality::of(id).aggression).collect();
        assert!(traits.iter().all(|t| (0.0..=1.0).contains(t)));
        let low = traits.iter().filter(|t| **t < 0.35).count();
        let high = traits.iter().filter(|t| **t > 0.65).count();
        assert!(low > 10 && high > 10, "bots need a spread of temperaments");
    }
}
