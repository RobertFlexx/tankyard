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
            || shot.owner == tank.id
            || !self.hostile(tank.team, shot.team)
        {
            return false;
        }
        if shot.kind.is_drone() {
            return self.parry_drone(projectile, defender, impact, ability.unwrap());
        }
        if !matches!(
            shot.kind,
            ProjectileKind::Bullet
                | ProjectileKind::Shell
                | ProjectileKind::Piercing
                | ProjectileKind::Scatter
                | ProjectileKind::Homing
                | ProjectileKind::Explosive
                | ProjectileKind::Fragment
        ) {
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
            Ability::VolleyParry => (3.2, 60., 240., 0.15),
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

    fn parry_drone(
        &mut self,
        projectile: usize,
        defender: usize,
        impact: V2,
        ability: Ability,
    ) -> bool {
        let tank = &self.tanks[defender];
        let (owner, team, pos) = (tank.id, tank.team, tank.pos);
        let incoming = &self.projectiles[projectile];
        if incoming.owner == 0 {
            return false;
        }
        let target = incoming.owner;
        let direction = self.homing_position(target, team, owner).map_or_else(
            || incoming.vel.normalized() * -1.0,
            |p| (p - impact).normalized(),
        );
        let (multiplier, damage_cap, health_cap, radius_cap, speed, lifetime, healing) =
            match ability {
                Ability::CounterParry => (3.0, 260., 9., 42., 560., 7., 0.12),
                Ability::VolleyParry => (3.2, 320., 12., 48., 620., 8., 0.15),
                _ => (2.6, 200., 6., 36., 500., 6., 0.08),
            };
        let damage = (incoming.damage * multiplier).clamp(45., damage_cap);
        let health = incoming.penetration.clamp(2., health_cap);
        let radius = (incoming.radius * 1.8).clamp(20., radius_cap);
        // Re-parrying a returned swarm must not renew its remaining life.
        let lifetime = if incoming.is_return_drone() {
            incoming.lifetime.min(lifetime)
        } else {
            lifetime
        };
        let merged = self.projectiles.iter().position(|p| {
            p.id != self.projectiles[projectile].id
                && p.owner == owner
                && p.is_return_drone()
                && p.lifetime > 0.
                && p.penetration > 0.
        });
        if let Some(index) = merged {
            let drone = &mut self.projectiles[index];
            drone.radius = (drone.radius * drone.radius + radius * radius)
                .sqrt()
                .min(radius_cap);
            drone.damage = (drone.damage + damage).min(damage_cap);
            drone.penetration = (drone.penetration + health).min(health_cap);
            drone.cruise_speed = speed;
            drone.lifetime = drone.lifetime.min(lifetime);
            self.projectiles[projectile].lifetime = 0.;
            self.projectiles[projectile].penetration = 0.;
        } else {
            let drone = &mut self.projectiles[projectile];
            drone.owner = owner;
            drone.team = team;
            drone.kind = ProjectileKind::Drone;
            drone.homing_target = target;
            drone.radius = radius;
            drone.damage = damage;
            drone.penetration = health;
            drone.cruise_speed = speed;
            drone.vel = direction * speed;
            drone.pos = impact + direction * (radius + 1.);
            drone.previous = drone.pos;
            drone.lifetime = lifetime;
            drone.age = 0.;
            drone.ricochets = 0;
            drone.gun = None;
            drone.hit.clear();
        }
        let tank = &mut self.tanks[defender];
        tank.health = (tank.health + tank.max_health * healing).min(tank.max_health);
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
    fn parried_swarm_becomes_one_drone_and_switches_between_cursor_and_attacker() {
        let (mut world, shooter, defender) = arena();
        world.tanks[1].shield = 0.1;
        for kind in [
            ProjectileKind::Drone,
            ProjectileKind::Minion,
            ProjectileKind::Drone,
        ] {
            shoot(&mut world, shooter, kind);
            let incoming = world.projectiles.len() - 1;
            assert!(world.parry_projectile(incoming, 1, V2::new(770., 500.)));
        }
        let returns: Vec<_> = world
            .projectiles
            .iter()
            .filter(|p| p.lifetime > 0. && p.penetration > 0. && p.owner == defender)
            .collect();
        assert_eq!(returns.len(), 1);
        let drone = returns[0];
        assert!(drone.is_return_drone());
        assert_eq!(drone.homing_target, shooter);
        assert!(drone.radius > 30.);
        assert!(drone.gun.is_none());
        let lifetime = drone.lifetime;
        // A nearer bystander must not distract the automatic return.
        world.add_player("Bystander", false);
        world.tanks[2].pos = V2::new(790., 300.);
        world.projectiles[0].vel = V2::ZERO;
        world.steer_projectile(0);
        assert!(world.projectiles[0].vel.x < 0.);
        world.tanks[1].input.fire = true;
        world.tanks[1].input.steer_return = true;
        world.tanks[1].aim = std::f32::consts::FRAC_PI_2;
        world.tanks[1].input.aim_distance = 500.;
        world.projectiles[0].vel = V2::ZERO;
        world.steer_projectile(0);
        assert!(world.projectiles[0].vel.y > 0.);
        // Releasing the mouse must return the drone even with auto-fire on.
        world.tanks[1].input.steer_return = false;
        world.projectiles[0].vel = V2::ZERO;
        world.steer_projectile(0);
        assert!(world.projectiles[0].vel.x < 0.);
        assert_eq!(world.projectiles[0].lifetime, lifetime);
        world.tanks[1].alive = false;
        world.steer_projectile(0);
        assert_eq!(world.projectiles[0].lifetime, 0.);
    }

    #[test]
    fn each_parry_upgrade_has_a_stronger_merged_drone() {
        let mut previous = (0., 0., 0., 0., 0.);
        for class in ["riposte", "counterguard", "reprisal"] {
            let (mut world, shooter, _) = arena();
            world.tanks[1].class = class.into();
            world.tanks[1].shield = 0.1;
            for _ in 0..8 {
                shoot(&mut world, shooter, ProjectileKind::Drone);
                let index = world.projectiles.len() - 1;
                assert!(world.parry_projectile(index, 1, V2::new(770., 500.)));
            }
            let drone = &world.projectiles[0];
            assert!(drone.damage > previous.0);
            assert!(drone.penetration > previous.1);
            assert!(drone.radius > previous.2);
            assert!(drone.cruise_speed > previous.3);
            assert!(drone.lifetime > previous.4);
            previous = (
                drone.damage,
                drone.penetration,
                drone.radius,
                drone.cruise_speed,
                drone.lifetime,
            );
        }
    }

    #[test]
    fn a_real_incoming_swarm_merges_before_the_captured_body_can_destroy_it() {
        let (mut world, shooter, defender) = arena();
        world.tanks[1].input.ability = true;
        for index in 0..8 {
            shoot(&mut world, shooter, ProjectileKind::Drone);
            let drone = world.projectiles.last_mut().unwrap();
            drone.pos = V2::new(738. - index as f32, 500.);
            drone.previous = drone.pos;
            drone.vel = V2::new(400., 0.);
            drone.cruise_speed = 400.;
            drone.damage = 10.;
            drone.penetration = 2.;
        }
        let health = world.tanks[1].health;
        for _ in 0..10 {
            world.step();
        }
        assert_eq!(world.tanks[1].health, health);
        let captured: Vec<_> = world
            .projectiles
            .iter()
            .filter(|p| p.owner == defender && p.is_return_drone())
            .collect();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].radius >= 35.);
        assert_eq!(captured[0].damage, 200.);
        assert!(world.projectiles.iter().all(|p| p.owner != shooter));
    }

    #[test]
    fn drone_collision_parries_without_damaging_defender_and_cannot_refresh_returns() {
        let (mut world, shooter, defender) = arena();
        let health = world.tanks[1].health;
        shoot(&mut world, shooter, ProjectileKind::Drone);
        world.tanks[1].input.ability = true;
        world.step();
        assert_eq!(world.tanks[1].health, health);
        assert_eq!(world.projectiles.len(), 1);
        assert_eq!(world.projectiles[0].owner, defender);
        world.tanks[0].class = "reprisal".into();
        world.tanks[0].shield = 0.1;
        world.projectiles[0].lifetime = 0.4;
        assert!(world.parry_projectile(0, 0, V2::new(520., 500.)));
        assert!(world.projectiles[0].lifetime <= 0.4);
        assert_eq!(world.projectiles[0].homing_target, defender);
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
