//! Trusted, process-local tools. Network authorization belongs to the host.
//! These fields are absent from snapshots and cannot be changed by player input.
use crate::{EntityId, MAX_LEVEL, World};

#[derive(Clone, Copy, Default)]
pub struct Modifiers {
    pub god: bool,
    pub unlimited_ability: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WeaponTuning {
    pub damage: f32,
    pub shot_speed: f32,
}
impl Default for WeaponTuning {
    fn default() -> Self {
        Self {
            damage: 1.,
            shot_speed: 1.,
        }
    }
}
impl WeaponTuning {
    pub const DAMAGE: [f32; 2] = [0.25, 8.];
    pub const SHOT_SPEED: [f32; 2] = [0.25, 3.];
    pub fn valid(self) -> bool {
        (Self::DAMAGE[0]..=Self::DAMAGE[1]).contains(&self.damage)
            && (Self::SHOT_SPEED[0]..=Self::SHOT_SPEED[1]).contains(&self.shot_speed)
    }
}

pub(crate) struct OwnerTankOverride {
    weapons: WeaponTuning,
    /// The ordinary class and the staff-selected replacement, for revocation.
    class_change: Option<(String, String)>,
}

impl World {
    pub fn owner_weapons(&self, player: EntityId) -> WeaponTuning {
        self.owner_tanks
            .get(&player)
            .map_or_else(WeaponTuning::default, |tank| tank.weapons)
    }

    pub fn tune_owner_tank(
        &mut self,
        player: EntityId,
        class: &str,
        weapons: WeaponTuning,
    ) -> bool {
        if !weapons.valid() || self.content.class(class).is_none() {
            return false;
        }
        let Some(tank) = self.tanks.iter().find(|t| t.id == player && !t.bot) else {
            return false;
        };
        let change_class = tank.class != class;
        if !change_class && self.owner_weapons(player) == weapons {
            return true;
        }
        let settings = self
            .owner_tanks
            .entry(player)
            .or_insert_with(|| OwnerTankOverride {
                weapons: WeaponTuning::default(),
                class_change: None,
            });
        settings.weapons = weapons;
        if change_class {
            let original = match settings.class_change.take() {
                Some((original, granted)) if tank.class == granted => original,
                _ => tank.class.clone(),
            };
            settings.class_change = (original != class).then(|| (original, class.to_owned()));
            // An event loadout borrows the hull. It does not award XP or levels.
            self.set_class(player, class);
        }
        self.config.modded = true;
        true
    }

    pub fn clear_owner_tank(&mut self, player: EntityId) {
        if let Some(settings) = self.owner_tanks.remove(&player) {
            self.projectiles.retain(|shot| shot.owner != player);
            if let Some((original, granted)) = settings.class_change
                && self
                    .tanks
                    .iter()
                    .any(|t| t.id == player && t.class == granted)
                && self.content.class(&original).is_some()
            {
                self.set_class(player, &original);
            }
        }
    }

    /// Host-owned team assignment also finds a legal spawn and drops held objectives.
    pub fn assign_team(&mut self, player: EntityId, team: u8) -> bool {
        if team == 0 || team > self.config.mode.teams() {
            return false;
        }
        let Some(index) = self.tanks.iter().position(|t| t.id == player) else {
            return false;
        };
        let previous = self.tanks[index].pos;
        let pos = self.spawn_position(team, self.tanks[index].radius, player);
        for objective in &mut self.objectives {
            if objective.carrier == Some(player) {
                objective.carrier = None;
                objective.pos = previous;
                objective.return_timer = 15.;
            }
        }
        self.tanks[index].team = team;
        self.tanks[index].pos = pos;
        self.tanks[index].vel = crate::V2::ZERO;
        self.tanks[index].carrying = false;
        true
    }
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
        let forms: Vec<_> = self
            .tanks
            .iter()
            .filter(|t| t.creator_boss)
            .map(|t| t.id)
            .collect();
        self.projectiles.retain(|p| !forms.contains(&p.owner));
        for id in forms {
            if let Some(tank) = self.tanks.iter_mut().find(|t| t.id == id) {
                tank.creator_boss = false;
                tank.creator_attacks = [0.; crate::creator_boss::ATTACK_COUNT];
            }
            self.refresh_stats(id, true);
        }
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
    fn reassignment_drops_a_held_core_before_moving_to_the_new_team() {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::CaptureCore,
                target_count: 0,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let id = world.add_player("Friend", false);
        let before = world.tanks[0].pos;
        world.objectives[0].carrier = Some(id);
        world.tanks[0].carrying = true;
        assert!(!world.assign_team(id, 3));
        assert_eq!(world.objectives[0].carrier, Some(id));
        assert!(world.assign_team(id, 2));
        assert_eq!(world.objectives[0].carrier, None);
        assert_eq!(world.objectives[0].pos, before);
        assert!(!world.tanks[0].carrying);
        assert_eq!(world.tanks[0].team, 2);
        assert!(world.position_clear(world.tanks[0].pos, world.tanks[0].radius));
    }

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

    /// The first round `player` fires straight ahead from a standstill:
    /// its damage and speed.
    fn first_shot(world: &mut World, player: EntityId) -> (f32, f32) {
        world.projectiles.retain(|shot| shot.owner != player);
        let tank = world.tanks.iter_mut().find(|t| t.id == player).unwrap();
        tank.cooldowns.clear();
        tank.vel = V2::ZERO;
        let sequence = tank.last_input.wrapping_add(1);
        world.set_input(
            player,
            PlayerInput {
                sequence,
                fire: true,
                ..Default::default()
            },
        );
        for _ in 0..120 {
            world.step();
            if let Some(shot) = world.projectiles.iter().find(|shot| shot.owner == player) {
                return (shot.damage, shot.vel.length());
            }
        }
        panic!("no shot");
    }

    #[test]
    fn the_owners_tuning_scales_their_own_shots_and_revocation_returns_the_hull() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let owner = world.add_player("Owner", false);
        let other = world.add_player("Other", false);
        let bot = world.add_player("Bot", true);
        let original = world.tanks[0].class.clone();
        let (damage, speed) = first_shot(&mut world, owner);
        let (other_damage, other_speed) = first_shot(&mut world, other);
        let tuned = WeaponTuning {
            damage: 2.5,
            shot_speed: 1.5,
        };
        // Out of range, unknown hulls and bots are refused, changing nothing.
        for (player, class, weapons) in [
            (
                owner,
                original.as_str(),
                WeaponTuning {
                    damage: 8.5,
                    ..tuned
                },
            ),
            (
                owner,
                original.as_str(),
                WeaponTuning {
                    shot_speed: f32::NAN,
                    ..tuned
                },
            ),
            (owner, "no-such-hull", tuned),
            (bot, original.as_str(), tuned),
        ] {
            assert!(!world.tune_owner_tank(player, class, weapons));
        }
        assert!(!world.config.modded);
        assert!(world.tune_owner_tank(owner, &original, tuned));
        assert!(world.config.modded);
        let (tuned_damage, tuned_speed) = first_shot(&mut world, owner);
        assert!(
            (tuned_damage / damage - 2.5).abs() < 1e-3,
            "{tuned_damage} from {damage}"
        );
        assert!(
            (tuned_speed / speed - 1.5).abs() < 1e-3,
            "{tuned_speed} from {speed}"
        );
        // Nobody else's guns change.
        assert_eq!(first_shot(&mut world, other), (other_damage, other_speed));

        let hull = world
            .content
            .classes
            .iter()
            .find(|class| class.tier == 4 && class.id != original)
            .unwrap()
            .id
            .clone();
        assert!(world.tune_owner_tank(owner, &hull, tuned));
        assert_eq!(world.tanks[0].class, hull);
        assert_eq!(world.owner_weapons(owner), tuned);
        // Choosing the original hull again needs nothing to hand back.
        assert!(world.tune_owner_tank(owner, &original, WeaponTuning::default()));
        assert_eq!(world.tanks[0].class, original);
        assert!(world.tune_owner_tank(owner, &hull, tuned));
        world.clear_owner_tank(owner);
        assert_eq!(world.tanks[0].class, original);
        assert_eq!(world.owner_weapons(owner), WeaponTuning::default());
        assert!(world.projectiles.iter().all(|shot| shot.owner != owner));
        assert_eq!(first_shot(&mut world, owner), (damage, speed));
        // A departing owner leaves nothing behind.
        assert!(world.tune_owner_tank(owner, &hull, tuned));
        world.remove_player(owner);
        assert_eq!(world.owner_weapons(owner), WeaponTuning::default());
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
