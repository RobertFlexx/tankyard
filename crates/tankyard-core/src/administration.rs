//! Trusted, process-local tools. Network authorization belongs to the host.
//! These fields are absent from snapshots and cannot be changed by player input.
use crate::{EntityId, MAX_LEVEL, World};

#[derive(Clone, Copy, Default)]
pub struct Modifiers {
    pub god: bool,
    pub unlimited_ability: bool,
}

impl World {
    pub fn admin_class(&mut self, player: EntityId, class: &str) -> bool {
        let Some(def) = self.content.class(class) else {
            return false;
        };
        let Some(tank) = self.tanks.iter().find(|t| t.id == player) else {
            return false;
        };
        let level = tank.level.max(def.min_level);
        let increase_level = level > tank.level;
        self.config.modded = true;
        if increase_level {
            self.set_level_unchecked(player, level);
        }
        self.set_class(player, class);
        true
    }

    pub fn admin_full_build(&mut self, player: EntityId) {
        self.config.modded = true;
        self.set_level_unchecked(player, MAX_LEVEL);
        if let Some(tank) = self.tanks.iter_mut().find(|t| t.id == player) {
            tank.stats = [7; 8];
            tank.points = 0;
        }
        self.refresh_stats(player, true);
    }

    /// Revocation removes active powers immediately; the round stays modified.
    pub fn reset_administration(&mut self) {
        self.admin_modifiers.clear();
        self.bots_frozen = false;
        for tank in &mut self.tanks {
            tank.speed_scale = 1.;
            tank.damage_scale = 1.;
            tank.reload_scale = 1.;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn host_modifiers_apply_authoritatively_and_revocation_removes_them() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let player = world.add_player("Creator", false);
        world.tanks[0].game_creator = true;
        assert!(world.admin_class(player, "riposte"));
        world.admin_modifiers.insert(
            player,
            Modifiers {
                god: true,
                unlimited_ability: true,
            },
        );
        world.tanks[0].speed_scale = 3.;
        let health = world.tanks[0].health;
        world.damage_tank(0, 500., player + 1, V2::ZERO);
        assert_eq!(world.tanks[0].health, health);
        world.tanks[0].ability_cooldown = 4.;
        world.tanks[0].input.ability = true;
        world.step();
        assert!(world.tanks[0].shield > 0.);
        world.bots_frozen = true;
        world.reset_administration();
        assert!(world.admin_modifiers.is_empty());
        assert!(!world.bots_frozen);
        assert_eq!(world.tanks[0].speed_scale, 1.);
        assert!(world.config.modded);
        world.tanks[0].shield = 0.;
        world.damage_tank(0, 10., player + 1, V2::ZERO);
        assert!(world.tanks[0].health < health);
    }

    #[test]
    fn last_creator_departure_clears_persistent_controls() {
        let mut world = World::new(WorldConfig::default(), tankyard_content::Content::builtin());
        let player = world.add_player("Creator", false);
        world.tanks[0].game_creator = true;
        world.admin_modifiers.insert(
            player,
            Modifiers {
                god: true,
                unlimited_ability: false,
            },
        );
        world.bots_frozen = true;
        world.remove_player(player);
        assert!(world.admin_modifiers.is_empty());
        assert!(!world.bots_frozen);
    }
}
