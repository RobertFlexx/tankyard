//! Progression shared by local games, bots, and online arenas.
use tankyard_content::MAX_LEVEL;

/// Cumulative XP at the beginning of each level (1..=55).
/// The ramp slows complete builds without making early levels a long grind.
const LEVEL_XP: [u32; 55] = [
    0, 5, 16, 35, 62, 97, 141, 196, 265, 347, 444, 558, 690, 845, 1020, 1235, 1452, 1712, 2006,
    2350, 2708, 3123, 3574, 4118, 4682, 5322, 6032, 6817, 7685, 8644, 9702, 10865, 12108, 13433,
    14846, 16353, 17962, 19677, 21506, 23455, 25533, 27748, 30108, 32625, 35304, 38144, 41154,
    44345, 47727, 51312, 55112, 59140, 63410, 67936, 72734,
];

pub fn xp_for_level(level: u8) -> u32 {
    LEVEL_XP[(level.clamp(1, MAX_LEVEL) - 1) as usize]
}

pub fn level_for_xp(xp: u32) -> u8 {
    // 55 entries: binary search avoids repeated reverse scans during awards.
    LEVEL_XP.partition_point(|&threshold| threshold <= xp) as u8
}

/// Three points per four levels, spread across the entire match.
/// 33 points at level 45; one every two levels afterwards, ending at 38.
pub fn points_for_level(level: u8) -> u8 {
    let level = level.clamp(1, MAX_LEVEL);
    if level <= 45 {
        ((u16::from(level) - 1) * 3 / 4) as u8
    } else {
        33 + (level - 45) / 2
    }
}

/// Respawn bands preserve some progress. Points can be reassigned after death.
pub fn respawn_level(level: u8) -> u8 {
    match level.clamp(1, MAX_LEVEL) {
        1..=2 => 1,
        3..=11 => level - 1,
        12..=13 => 11,
        14..=15 => 12,
        16..=18 => 13,
        19..=20 => 14,
        21..=23 => 15,
        24..=26 => 16,
        27..=30 => 17,
        31..=33 => 18,
        34..=37 => 19,
        38..=41 => 20,
        42..=44 => 21,
        45 => 22,
        46..=48 => 23,
        49..=51 => 24,
        52..=54 => 25,
        _ => 26,
    }
}

pub fn stat_scale(points: u8) -> f32 {
    let points = f32::from(points.min(7));
    1.0 + points * 0.105 + points * points * 0.003
}

/// Trap launch distance uses the weapon-speed slot without extending field life.
pub fn trap_range_scale(points: u8) -> f32 {
    let points = f32::from(points.min(7));
    1. + points * 0.13 + points * points * 0.005
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progression_is_monotonic_and_capped() {
        let mut previous = 0;
        for level in 1..=MAX_LEVEL {
            let xp = xp_for_level(level);
            assert!(level == 1 || xp > previous);
            assert_eq!(level_for_xp(xp), level);
            if level > 1 {
                assert_eq!(level_for_xp(xp - 1), level - 1);
            }
            previous = xp;
        }
        assert_eq!(level_for_xp(u32::MAX), MAX_LEVEL);
        assert_eq!(xp_for_level(15), 1020);
        assert_eq!(xp_for_level(30), 8644);
        assert_eq!(xp_for_level(45), 35304);
    }

    #[test]
    fn skills_are_spread_across_all_levels_and_finish_at_thirty_three() {
        for (level, points) in [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 1),
            (5, 3),
            (15, 10),
            (28, 20),
            (30, 21),
            (44, 32),
            (45, 33),
            (46, 33),
            (47, 34),
            (55, 38),
            (255, 38),
        ] {
            assert_eq!(points_for_level(level), points);
        }
        for level in 2..=MAX_LEVEL {
            let gain = points_for_level(level) - points_for_level(level - 1);
            assert!(gain <= 1);
            // Every four levels must contribute three points, including late game.
            if (5..=45).contains(&level) {
                assert_eq!(points_for_level(level) - points_for_level(level - 4), 3);
            }
        }
    }

    #[test]
    fn death_reduces_progression_without_returning_everyone_to_level_one() {
        assert_eq!(respawn_level(1), 1);
        assert_eq!(respawn_level(15), 12);
        assert_eq!(respawn_level(30), 17);
        assert_eq!(respawn_level(45), 22);
        for level in 1..=MAX_LEVEL {
            assert!(respawn_level(level) >= 1);
            assert!(respawn_level(level) <= level);
        }
    }

    #[test]
    fn scaling_increases_without_unbounded_growth() {
        for point in 0..7 {
            assert!(stat_scale(point + 1) > stat_scale(point));
        }
        assert!(stat_scale(7) < 1.9);
        assert!(stat_scale(7).powi(2) < 3.6); // Damage and reload together.
        assert_eq!(stat_scale(255), stat_scale(7));
    }
}
