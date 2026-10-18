use crate::{EntityId, Mode, Tank};

pub fn can_spectate(viewer: &Tank, target: &Tank, mode: Mode) -> bool {
    !viewer.alive
        && viewer.id != target.id
        && target.alive
        && target.health > 0.
        && (mode.teams() == 0 || target.team == viewer.team)
}

/// Both camera selection and server interest use the same fallback when a
/// watched player dies or disconnects. Team matches only reveal teammates.
pub fn spectator_target(
    tanks: &[Tank],
    viewer: EntityId,
    requested: Option<EntityId>,
    mode: Mode,
) -> Option<&Tank> {
    let viewer = tanks.iter().find(|tank| tank.id == viewer)?;
    let eligible = |tank: &&Tank| can_spectate(viewer, tank, mode);
    tanks.iter().filter(eligible).min_by_key(|tank| {
        (
            Some(tank.id) != requested,
            tank.id != viewer.last_attacker,
            std::cmp::Reverse(tank.score),
            tank.id,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{World, WorldConfig};
    use tankyard_content::Content;

    #[test]
    fn dead_players_follow_valid_targets_and_living_players_cannot_spectate() {
        let mut world = World::new(WorldConfig::default(), Content::builtin());
        let viewer = world.add_player("Viewer", false);
        let attacker = world.add_player("Attacker", false);
        let other = world.add_player("Other", false);
        assert!(spectator_target(&world.tanks, viewer, Some(other), Mode::Ffa).is_none());
        world.tanks[0].alive = false;
        world.tanks[0].last_attacker = attacker;
        assert_eq!(
            spectator_target(&world.tanks, viewer, None, Mode::Ffa)
                .unwrap()
                .id,
            attacker
        );
        assert_eq!(
            spectator_target(&world.tanks, viewer, Some(other), Mode::Ffa)
                .unwrap()
                .id,
            other
        );
        world.tanks[2].alive = false;
        assert_eq!(
            spectator_target(&world.tanks, viewer, Some(other), Mode::Ffa)
                .unwrap()
                .id,
            attacker
        );
        world.tanks[0].team = 1;
        world.tanks[1].team = 2;
        assert!(spectator_target(&world.tanks, viewer, None, Mode::CoopPve).is_none());
        world.tanks[1].team = 1;
        assert_eq!(
            spectator_target(&world.tanks, viewer, None, Mode::CoopPve)
                .unwrap()
                .id,
            attacker
        );
    }
}
