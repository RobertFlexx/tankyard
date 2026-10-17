use crate::*;
use tankyard_content::{Ability, ProjectileKind};

impl World {
    pub(crate) fn parry_projectile(
        &mut self,
        projectile: usize,
        defender: usize,
        impact: V2,
    ) -> bool {
        let tank = &self.tanks[defender];
        let shot = &self.projectiles[projectile];
        let ability = self.content.class(&tank.class).map(|class| class.ability);
        if tank.shield <= 0.0
            || !ability.is_some_and(Ability::is_parry)
            || shot.lifetime <= 0.
            || shot.penetration <= 0.
            || !matches!(
                shot.kind,
                ProjectileKind::Bullet
                    | ProjectileKind::Shell
                    | ProjectileKind::Piercing
                    | ProjectileKind::Scatter
                    | ProjectileKind::Homing
                    | ProjectileKind::Explosive
                    | ProjectileKind::Fragment
            )
        {
            return false;
        }
        let (owner, team, pos) = (tank.id, tank.team, tank.pos);
        let ability = ability.unwrap_or(Ability::Parry);
        let target = shot.owner;
        let target_pos = self.homing_position(target, team, owner);
        let direction = target_pos.map_or_else(
            || {
                if shot.vel.length_squared() > 0.001 {
                    shot.vel.normalized() * -1.0
                } else {
                    V2::from_angle(tank.aim)
                }
            },
            |target| (target - impact).normalized(),
        );
        let shot = &mut self.projectiles[projectile];
        let remaining = if shot.kind == ProjectileKind::Homing {
            shot.lifetime
        } else {
            crate::homing::MAX_FLIGHT_SECONDS
        };
        shot.owner = owner;
        shot.team = team;
        shot.kind = ProjectileKind::Homing;
        shot.homing_target = target;
        shot.radius = shot.radius.clamp(3.0, 12.0);
        shot.cruise_speed = shot.cruise_speed.clamp(280.0, 900.0);
        shot.vel = direction * shot.cruise_speed;
        shot.pos = impact + direction * (shot.radius + 1.0);
        shot.previous = shot.pos;
        shot.penetration = shot.penetration.clamp(1.0, 3.0);
        let (multiplier, minimum, maximum, healing) = match ability {
            Ability::CounterParry => (3., 55., 220., 0.12),
            Ability::VolleyParry => (3.2, 60., 240., 0.12),
            _ => (2.6, 45., 200., 0.08),
        };
        // Only a successful reflection earns this damage and healing. The
        // player's ordinary cannon and a missed timing window get no bonus.
        shot.damage = (shot.damage * multiplier).clamp(minimum, maximum);
        shot.lifetime = crate::homing::flight_lifetime(remaining, shot.cruise_speed);
        shot.age = 0.0;
        shot.ricochets = 0;
        shot.gun = None;
        shot.hit.clear();
        let (echo_origin, echo_speed, echo_radius, echo_damage) = (
            shot.pos,
            shot.cruise_speed * 0.92,
            (shot.radius * 0.75).clamp(3., 8.),
            (shot.damage * 0.45).min(80.),
        );
        let tank = &mut self.tanks[defender];
        tank.health = (tank.health + tank.max_health * healing).min(tank.max_health);
        if ability == Ability::VolleyParry {
            let room = self
                .config
                .max_projectiles
                .saturating_sub(self.projectiles.len())
                .min(2);
            for offset in [-0.22, 0.22].into_iter().take(room) {
                let velocity = direction.rotate(offset) * echo_speed;
                let mut echo = self.simple_projectile(
                    owner,
                    team,
                    echo_origin,
                    velocity,
                    ProjectileKind::Homing,
                    echo_damage,
                    echo_radius,
                    1.1,
                    2.,
                );
                echo.homing_target = target;
                self.projectiles.push(echo);
            }
        }
        self.event(EventKind::Parry, pos, owner, 0, team);
        true
    }

    pub(crate) fn homing_position(
        &self,
        target: EntityId,
        team: u8,
        owner: EntityId,
    ) -> Option<V2> {
        if target == 0 || target == owner {
            return None;
        }
        self.tanks
            .iter()
            .find(|tank| {
                tank.id == target
                    && tank.alive
                    && tank.health > 0.0
                    && tank.protection <= 0.0
                    && self.hostile(team, tank.team)
            })
            .map(|tank| tank.pos)
            .or_else(|| {
                self.bosses
                    .iter()
                    .find(|boss| {
                        boss.id == target && boss.health > 0.0 && self.hostile(team, boss.team)
                    })
                    .map(|boss| boss.pos)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;

    fn arena() -> (World, EntityId, EntityId) {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Ffa,
                target_count: 0,
                spawn_protection: 0.0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let shooter = world.add_player("Shooter", false);
        let defender = world.add_player("Defender", false);
        world.set_level_unchecked(defender, 30);
        assert!(world.evolve(defender, "hammer"));
        assert!(world.evolve(defender, "riposte"));
        world.tanks[0].pos = V2::new(500.0, 500.0);
        world.tanks[1].pos = V2::new(800.0, 500.0);
        (world, shooter, defender)
    }

    fn shoot(world: &mut World, owner: EntityId, kind: ProjectileKind) -> EntityId {
        let shot = world.simple_projectile(
            owner,
            0,
            V2::new(750.0, 500.0),
            V2::new(6000.0, 0.0),
            kind,
            200.0,
            5.0,
            1.0,
            1.0,
        );
        let id = shot.id;
        world.projectiles.push(shot);
        id
    }

    #[test]
    fn parrying_an_already_homing_round_does_not_refresh_its_lifetime() {
        let (mut world, shooter, _) = arena();
        shoot(&mut world, shooter, ProjectileKind::Bullet);
        world.tanks[1].shield = 0.4;
        assert!(world.parry_projectile(0, 1, V2::new(770.0, 500.0)));
        assert!(world.projectiles[0].lifetime <= crate::homing::MAX_FLIGHT_SECONDS);
        assert!(
            world.projectiles[0].lifetime * world.projectiles[0].cruise_speed
                <= crate::homing::MAX_FLIGHT_DISTANCE + 0.001
        );
        world.projectiles[0].lifetime = 0.12;
        world.tanks[0].class = "riposte".into();
        world.tanks[0].shield = 0.4;
        assert!(world.parry_projectile(0, 0, V2::new(520.0, 500.0)));
        assert_eq!(world.projectiles[0].lifetime, 0.12);
    }

    #[test]
    fn parry_returns_the_same_bullet_and_credits_the_defender_for_the_kill() {
        let (mut world, shooter, defender) = arena();
        let bullet = shoot(&mut world, shooter, ProjectileKind::Explosive);
        world.set_input(
            defender,
            PlayerInput {
                sequence: 1,
                ability: true,
                ..Default::default()
            },
        );
        let health = world.tanks[1].health;
        world.step();
        assert_eq!(world.tanks[1].health, health);
        assert_eq!(
            world.projectiles.len(),
            1,
            "reflection must not multiply shots"
        );
        let reflected = &world.projectiles[0];
        assert_eq!(reflected.id, bullet);
        assert_eq!(reflected.owner, defender);
        assert_eq!(reflected.kind, ProjectileKind::Homing);
        assert_eq!(reflected.homing_target, shooter);
        assert!(reflected.vel.x < 0.0);
        assert!(
            world
                .events
                .iter()
                .any(|event| matches!(event.kind, EventKind::Parry) && event.subject == defender)
        );
        world.tanks[1].input.ability = false;
        for _ in 0..100 {
            world.step();
            if !world.tanks[0].alive {
                break;
            }
        }
        assert!(!world.tanks[0].alive);
        assert_eq!(world.tanks[1].kills, 1);
        assert!(world.tanks[1].score > 0);
    }

    #[test]
    fn parry_requires_its_window_and_cooldown_and_ignores_deployables() {
        let (mut world, shooter, defender) = arena();
        shoot(&mut world, shooter, ProjectileKind::Bullet);
        assert!(!world.parry_projectile(0, 1, V2::new(770.0, 500.0)));
        world.tanks[1].input.ability = true;
        world.step();
        world.projectiles.clear();
        world.tanks[1].input.ability = false;
        for _ in 0..36 {
            world.step();
        }
        assert_eq!(world.tanks[1].shield, 0.0);
        assert!(world.tanks[1].ability_cooldown > 3.0);
        world.set_input(
            defender,
            PlayerInput {
                sequence: 2,
                ability: true,
                ..Default::default()
            },
        );
        world.step();
        assert_eq!(
            world.tanks[1].shield, 0.0,
            "cooldown must prevent reopening the window"
        );
        for kind in [
            ProjectileKind::Drone,
            ProjectileKind::Minion,
            ProjectileKind::Mine,
            ProjectileKind::Barrier,
            ProjectileKind::Turret,
        ] {
            world.projectiles.clear();
            shoot(&mut world, shooter, kind);
            world.tanks[1].shield = 0.4;
            assert!(!world.parry_projectile(0, 1, V2::new(770.0, 500.0)));
        }
        world.tanks[1].class = "bulwark".into();
        world.projectiles.clear();
        shoot(&mut world, shooter, ProjectileKind::Bullet);
        assert!(!world.parry_projectile(0, 1, V2::new(770.0, 500.0)));
    }

    #[test]
    fn reflected_shots_track_the_original_shooter_before_nearer_targets() {
        let (mut world, shooter, defender) = arena();
        world.tanks[0].pos = V2::new(500.0, 900.0);
        let other = world.add_player("Closer", false);
        // Keep the fallback inside the forward cone after the first step.
        world.tanks[2].pos = V2::new(700.0, 450.0);
        shoot(&mut world, shooter, ProjectileKind::Bullet);
        world.tanks[1].shield = 0.4;
        assert!(world.parry_projectile(0, 1, V2::new(770.0, 500.0)));
        world.projectiles[0].vel = V2::new(-400.0, 0.0);
        world.step();
        assert!(
            world.projectiles[0].vel.y > 0.0,
            "home toward the shooter below, not the nearer tank above"
        );
        world.remove_player(shooter);
        world.projectiles[0].vel = V2::new(-400.0, 0.0);
        world.step();
        assert!(
            world.projectiles[0].vel.y < 0.0,
            "retarget when the shooter leaves"
        );
        assert_eq!(world.homing_position(defender, 0, defender), None);
        assert_eq!(
            world.homing_position(other, 1, defender),
            Some(world.tanks[1].pos)
        );
    }
}
