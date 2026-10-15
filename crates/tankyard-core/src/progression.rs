//! The normal level curve and upgrade timing follow the familiar 45-level
//! geometric-arena progression. Kept in the shared simulation so bots, local
//! games and community servers all apply exactly the same rules.

/// Cumulative XP at the beginning of each level (1..=45).
/// The early thresholds deliberately make the first upgrades arrive quickly.
const LEVEL_XP: [u32; 45] = [
    0, 4, 13, 28, 50, 78, 113, 157, 211, 275, 350, 437, 538, 655, 787, 948, 1109, 1301, 1516, 1767,
    2026, 2325, 2647, 3035, 3433, 3883, 4379, 4925, 5525, 6184, 6907, 7698, 8537, 9426, 10368,
    11367, 12426, 13549, 14739, 16000, 17337, 18754, 20256, 21849, 23536,
];

pub fn xp_for_level(level: u8) -> u32 {
    LEVEL_XP[(level.clamp(1, 45) - 1) as usize]
}

pub fn level_for_xp(xp: u32) -> u8 {
    // 45 entries: binary search avoids repeated reverse scans during awards.
    LEVEL_XP.partition_point(|&threshold| threshold <= xp) as u8
}

/// 27 points by level 28; one at 30 and one every three levels thereafter.
/// Level 45 awards the 33rd point.
pub fn points_for_level(level: u8) -> u8 {
    let base = level.saturating_sub(1).min(27);
    if level < 30 {
        base
    } else {
        base + 1 + (level.min(45) - 30) / 3
    }
}

/// Respawn bands preserve some progress. Points can be reassigned after death.
pub fn respawn_level(level: u8) -> u8 {
    match level.clamp(1, 45) {
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
        _ => 22,
    }
}

pub fn stat_scale(points: u8) -> f32 {
    let points = f32::from(points.min(7));
    1.0 + points * 0.105 + points * points * 0.003
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progression_is_monotonic_and_capped() {
        let mut previous = 0;
        for level in 1..=45 {
            let xp = xp_for_level(level);
            assert!(level == 1 || xp > previous);
            assert_eq!(level_for_xp(xp), level);
            if level > 1 {
                assert_eq!(level_for_xp(xp - 1), level - 1);
            }
            previous = xp;
        }
        assert_eq!(level_for_xp(u32::MAX), 45);
        assert_eq!(xp_for_level(15), 787);
        assert_eq!(xp_for_level(30), 6184);
        assert_eq!(xp_for_level(45), 23536);
    }

    #[test]
    fn skill_points_arrive_early_and_finish_at_thirty_three() {
        assert_eq!(points_for_level(1), 0);
        for level in 2..=28 {
            assert_eq!(points_for_level(level), level - 1);
        }
        assert_eq!(points_for_level(29), 27);
        for (level, points) in [(30, 28), (32, 28), (33, 29), (42, 32), (45, 33)] {
            assert_eq!(points_for_level(level), points);
        }
        for level in 2..=45 {
            assert!(points_for_level(level) >= points_for_level(level - 1));
            assert!(points_for_level(level) - points_for_level(level - 1) <= 1);
        }
    }

    #[test]
    fn death_reduces_progression_without_returning_everyone_to_level_one() {
        assert_eq!(respawn_level(1), 1);
        assert_eq!(respawn_level(15), 12);
        assert_eq!(respawn_level(30), 17);
        assert_eq!(respawn_level(45), 22);
        for level in 1..=45 {
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
