//! A trusted event form. Clients cannot request it through gameplay controls.
use crate::*;
use serde::{Deserialize, Serialize};
use tankyard_content::ProjectileKind;
pub const NAME: &str = "Axiom, the Worldshaper";
pub const HEALTH: f32 = 120_000.;
pub const RADIUS: f32 = 180.;
pub const ATTACK_COUNT: usize = 12;
pub const MAX_COOLDOWN: f32 = 30.;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Attack {
    Starfall,
    Sunburst,
    Constellation,
    Repulse,
    MeteorShower,
    CometLance,
    NovaMines,
    StarLattice,
    SolarTurrets,
    GravityWell,
    RiftDash,
    Reforge,
}
impl Attack {
    pub const ALL: [Self; ATTACK_COUNT] = [
        Self::Starfall,
        Self::Sunburst,
        Self::Constellation,
        Self::Repulse,
        Self::MeteorShower,
        Self::CometLance,
        Self::NovaMines,
        Self::StarLattice,
        Self::SolarTurrets,
        Self::GravityWell,
        Self::RiftDash,
        Self::Reforge,
    ];
    pub const GROUPS: [[Self; 4]; 3] = [
        [
            Self::Starfall,
            Self::Sunburst,
            Self::CometLance,
            Self::MeteorShower,
        ],
        [
            Self::Constellation,
            Self::Repulse,
            Self::GravityWell,
            Self::RiftDash,
        ],
        [
            Self::NovaMines,
            Self::StarLattice,
            Self::SolarTurrets,
            Self::Reforge,
        ],
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Starfall => "Starfall",
            Self::Sunburst => "Sunburst",
            Self::Constellation => "Constellation",
            Self::Repulse => "Repulse",
            Self::MeteorShower => "Meteor Shower",
            Self::CometLance => "Comet Lance",
            Self::NovaMines => "Nova Mines",
            Self::StarLattice => "Star Lattice",
            Self::SolarTurrets => "Solar Turrets",
            Self::GravityWell => "Gravity Well",
            Self::RiftDash => "Rift Dash",
            Self::Reforge => "Reforge",
        }
    }
    pub fn cooldown(self) -> f32 {
        [
            2.5,
            7.,
            10.,
            12.,
            9.,
            6.,
            14.,
            18.,
            20.,
            16.,
            8.,
            MAX_COOLDOWN,
        ][self.index()]
    }
    pub fn description(self) -> &'static str {
        [
            "A focused spread toward the cursor",
            "A ring of twenty-four solar shells",
            "Twelve slow tracking stars",
            "Clear nearby hostile shots; emit a wave",
            "Seven explosive meteors toward the cursor",
            "A massive, fast piercing lance",
            "Twelve lingering explosive mines",
            "Eight defensive satellite barriers",
            "Six sentries that acquire nearby enemies",
            "Bend hostile shots inward; release stars",
            "Surge toward the cursor, stopping at walls",
            "Restore eight percent of maximum health",
        ][self.index()]
    }
}
impl World {
    pub fn creator_form(&mut self, player: EntityId, enabled: bool) -> Result<(), String> {
        let index = self
            .tanks
            .iter()
            .position(|t| t.id == player && t.game_creator && !t.bot && (t.alive || !enabled))
            .ok_or("Living creator required.")?;
        if self.tanks[index].creator_boss == enabled {
            return Ok(());
        }
        let pos = if enabled {
            physics::clear_circle_position(
                self.tanks[index].pos,
                RADIUS,
                self.config.bounds(),
                &self.walls,
            )
            .ok_or("This map has no clear space for Axiom.")?
        } else {
            self.tanks[index].pos
        };
        if !enabled {
            self.projectiles.retain(|p| p.owner != player);
        }
        // Form changes must not carry a class's pocket, cloak or temporary
        // combat effects into Axiom (or back out). Placement above is checked
        // first, so a failed transformation leaves the current life intact.
        let tank = &mut self.tanks[index];
        tank.pocket = 0.;
        tank.pocket_echoes.clear();
        tank.protection = 0.;
        tank.spawn_immunity = false;
        tank.shield = 0.;
        tank.boost = 0.;
        tank.stasis = 0.;
        tank.charge = 0.;
        tank.knockback = 0.;
        tank.terrain = 1.;
        tank.push = V2::ZERO;
        self.tanks[index].creator_boss = enabled;
        self.tanks[index].creator_attacks = [0.; ATTACK_COUNT];
        self.tanks[index].pos = pos;
        self.tanks[index].vel = V2::ZERO;
        self.refresh_stats(player, true);
        self.config.modded = true;
        Ok(())
    }
    pub fn creator_attack(&mut self, player: EntityId, attack: Attack) -> Result<(), String> {
        let index = self
            .tanks
            .iter()
            .position(|t| t.id == player && t.game_creator && t.creator_boss && t.alive)
            .ok_or("Become Axiom first.")?;
        if self.tanks[index].creator_attacks[attack.index()] > 0. {
            return Err("Attack is recharging.".into());
        }
        let (count, speed, damage, radius, lifetime, kind) = match attack {
            Attack::Starfall => (9, 760., 36., 9., 2.5, ProjectileKind::Piercing),
            Attack::Sunburst => (24, 350., 28., 13., 3.2, ProjectileKind::Shell),
            Attack::Constellation => (12, 230., 24., 11., 4., ProjectileKind::Homing),
            Attack::Repulse => (32, 460., 20., 10., 1.2, ProjectileKind::Bullet),
            Attack::MeteorShower => (7, 310., 70., 16., 3.5, ProjectileKind::Explosive),
            Attack::CometLance => (1, 1050., 180., 25., 2.5, ProjectileKind::Piercing),
            Attack::NovaMines => (12, 120., 45., 14., 10., ProjectileKind::Mine),
            Attack::StarLattice => (8, 0., 12., 30., 8., ProjectileKind::Barrier),
            Attack::SolarTurrets => (6, 0., 30., 20., 12., ProjectileKind::Turret),
            Attack::GravityWell => (12, 180., 18., 12., 4., ProjectileKind::Homing),
            Attack::RiftDash | Attack::Reforge => (0, 0., 0., 0., 0., ProjectileKind::Bullet),
        };
        let tank = &self.tanks[index];
        let (origin, aim, team, radius_tank) = (tank.pos, tank.aim, tank.team, tank.radius);
        if attack == Attack::Reforge && tank.health >= tank.max_health {
            return Err("Axiom is already at full health.".into());
        }
        let mut shots = Vec::with_capacity(count);
        for i in 0..count {
            let angle = if matches!(attack, Attack::Starfall | Attack::MeteorShower) {
                aim + (i as f32 - (count - 1) as f32 * 0.5) * 0.065
            } else {
                aim + i as f32 * std::f32::consts::TAU / count as f32
            };
            let direction = V2::from_angle(angle);
            let offset = match attack {
                Attack::NovaMines => 100.,
                Attack::StarLattice => 60.,
                Attack::SolarTurrets => 180.,
                _ => 8.,
            };
            let pos = origin + direction * (radius_tank + radius + offset);
            if self.position_clear(pos, radius) {
                shots.push((pos, direction));
            }
        }
        let retained = if attack == Attack::Repulse {
            self.projectiles
                .iter()
                .filter(|p| {
                    p.owner == player
                        || (team > 0 && p.team == team)
                        || p.pos.distance(origin) > 360.
                })
                .count()
        } else {
            self.projectiles.len()
        };
        let slots = self.config.max_projectiles.saturating_sub(retained);
        if attack == Attack::Repulse {
            shots.truncate(slots);
        } else if shots.len() > slots {
            return Err("Projectile limit reached. Try again shortly.".into());
        }
        if count > 0 && shots.is_empty() && attack != Attack::Repulse {
            return Err("No clear space to launch that attack.".into());
        }
        if matches!(attack, Attack::Repulse) {
            self.projectiles.retain(|p| {
                p.owner == player || (team > 0 && p.team == team) || p.pos.distance(origin) > 360.
            });
        }
        if attack == Attack::GravityWell {
            for shot in &mut self.projectiles {
                let distance = shot.pos.distance(origin);
                if shot.owner != player
                    && (team == 0 || shot.team != team)
                    && !shot.kind.is_drone()
                    && shot.frozen.is_none()
                    && (radius_tank..900.).contains(&distance)
                {
                    let inward = (origin - shot.pos).normalized() * shot.vel.length();
                    shot.vel = shot.vel.lerp(inward, 0.7);
                }
            }
        }
        if attack == Attack::RiftDash {
            let end = self
                .config
                .bounds()
                .clamp_circle(origin + V2::from_angle(aim) * 700., radius_tank);
            let fraction = self
                .walls
                .iter()
                .filter_map(|wall| {
                    physics::sweep_rect(origin, end, radius_tank, *wall).map(|hit| hit.0)
                })
                .fold(1., f32::min);
            let destination = origin + (end - origin) * (fraction - 0.002).max(0.);
            if origin.distance(destination) < 1. || !self.position_clear(destination, radius_tank) {
                return Err("No room to Rift Dash in that direction.".into());
            }
            self.tanks[index].pos = destination;
            self.tanks[index].vel = V2::ZERO;
        }
        if attack == Attack::Reforge {
            let tank = &mut self.tanks[index];
            tank.health = (tank.health + tank.max_health * 0.08).min(tank.max_health);
        }
        for (pos, direction) in shots {
            let shot = self.simple_projectile(
                player,
                team,
                pos,
                direction * speed,
                kind,
                damage,
                radius,
                lifetime,
                match attack {
                    Attack::CometLance => 20.,
                    Attack::StarLattice => 30.,
                    Attack::SolarTurrets => 12.,
                    _ => 4.,
                },
            );
            self.projectiles.push(shot);
        }
        self.tanks[index].creator_attacks[attack.index()] = attack.cooldown();
        self.tanks[index].protection = 0.;
        self.tanks[index].spawn_immunity = false;
        self.event(
            EventKind::BossAttack,
            origin,
            player,
            attack.index() as u32,
            team,
        );
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_form_is_authorized_bounded_and_restores_normal_stats() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let id = world.add_player("Creator", false);
        assert!(world.creator_form(id, true).is_err());
        world.tanks[0].game_creator = true;
        world.creator_form(id, true).unwrap();
        world.tanks[0].pos = world.config.bounds().center();
        assert_eq!(world.tanks[0].health, HEALTH);
        assert_eq!(world.tanks[0].radius, RADIUS);
        for attack in Attack::ALL {
            if attack == Attack::Reforge {
                world.tanks[0].health -= 1.;
            }
            world.creator_attack(id, attack).unwrap();
            let count = world.projectiles.len();
            assert!(world.creator_attack(id, attack).is_err());
            assert_eq!(world.projectiles.len(), count);
        }
        assert!(world.projectiles.len() <= 123);
        world.refresh_stats(id, false);
        assert_eq!(world.tanks[0].max_health, HEALTH);
        world.reset_administration();
        assert!(!world.tanks[0].creator_boss);
        assert!(world.tanks[0].max_health < 1000.);
        assert!(world.creator_attack(id, Attack::Sunburst).is_err());
    }
    fn event_world() -> (World, EntityId) {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let id = world.add_player("Creator", false);
        world.tanks[0].game_creator = true;
        world.creator_form(id, true).unwrap();
        world.tanks[0].pos = world.config.bounds().center();
        (world, id)
    }
    #[test]
    fn form_changes_clear_class_effects_and_disable_the_underlying_ability() {
        let (mut world, id) = event_world();
        world.creator_form(id, false).unwrap();
        let tank = &mut world.tanks[0];
        tank.class = "bacteria".into();
        tank.pocket = 8.;
        tank.protection = 2.;
        tank.spawn_immunity = true;
        tank.shield = 3.;
        tank.boost = 4.;
        tank.stasis = 5.;
        tank.charge = 1.;
        tank.knockback = 0.3;
        tank.terrain = 2.;
        tank.push = V2::new(100., 30.);
        world.creator_form(id, true).unwrap();
        let tank = &world.tanks[0];
        assert_eq!(tank.pocket, 0.);
        assert!(tank.pocket_echoes.is_empty());
        assert_eq!(tank.protection, 0.);
        assert!(!tank.spawn_immunity);
        assert_eq!(
            [
                tank.shield,
                tank.boost,
                tank.stasis,
                tank.charge,
                tank.knockback
            ],
            [0.; 5]
        );
        assert_eq!(tank.terrain, 1.);
        assert_eq!(tank.push, V2::ZERO);
        world.tanks[0].input.ability = true;
        world.step();
        assert_eq!(world.tanks[0].pocket, 0.);
        let mut predicted = world.tanks[0].clone();
        crate::physics::predict_tank(
            &mut predicted,
            &world.content,
            &world.config,
            &world.walls,
            world.tick,
            DT,
        );
        assert_eq!(predicted.pocket, 0.);
        world.creator_form(id, false).unwrap();
        world.tanks[0].ability_cooldown = 0.;
        world.step();
        assert!(world.tanks[0].pocket > 9.);
    }
    #[test]
    fn failed_form_placement_preserves_the_current_life() {
        let (mut world, id) = event_world();
        world.creator_form(id, false).unwrap();
        world.tanks[0].pocket = 4.;
        world.tanks[0].boost = 2.;
        let before = world.tanks[0].pos;
        world.walls = vec![world.config.bounds()];
        assert!(world.creator_form(id, true).is_err());
        assert!(!world.tanks[0].creator_boss);
        assert_eq!(world.tanks[0].pos, before);
        assert_eq!(world.tanks[0].pocket, 4.);
        assert_eq!(world.tanks[0].boost, 2.);
    }
    #[test]
    fn blocked_attacks_and_full_health_reforge_do_not_spend_cooldowns() {
        let (mut world, id) = event_world();
        world.events.clear();
        assert!(world.creator_attack(id, Attack::Reforge).is_err());
        world.walls = vec![world.config.bounds()];
        assert!(world.creator_attack(id, Attack::CometLance).is_err());
        assert!(world.projectiles.is_empty());
        assert!(world.events.is_empty());
        assert_eq!(world.tanks[0].creator_attacks, [0.; ATTACK_COUNT]);
    }
    #[test]
    fn repulse_clears_hostile_shots_even_when_the_projectile_budget_is_full() {
        let (mut world, id) = event_world();
        world.config.max_projectiles = 4;
        world.tanks[0].team = 1;
        let origin = world.tanks[0].pos;
        for (owner, team, distance) in [
            (id, 1, 250.),
            (999, 1, 250.),
            (998, 2, 500.),
            (997, 2, 250.),
        ] {
            let shot = world.simple_projectile(
                owner,
                team,
                origin + V2::new(distance, 0.),
                V2::ZERO,
                ProjectileKind::Bullet,
                1.,
                5.,
                2.,
                1.,
            );
            world.projectiles.push(shot);
        }
        world.creator_attack(id, Attack::Repulse).unwrap();
        assert_eq!(world.projectiles.len(), 4);
        assert!(world.projectiles.iter().any(|p| p.owner == 999));
        assert!(world.projectiles.iter().any(|p| p.owner == 998));
        assert!(!world.projectiles.iter().any(|p| p.owner == 997));
        assert_eq!(
            world.projectiles.iter().filter(|p| p.owner == id).count(),
            2
        );
        assert!(world.tanks[0].creator_attacks[Attack::Repulse.index()] > 0.);
    }
    #[test]
    fn rift_dash_sweeps_the_colossal_body_against_walls_and_borders() {
        let (mut world, id) = event_world();
        let origin = world.tanks[0].pos;
        world.tanks[0].aim = 0.;
        world.walls.push(Rect {
            x: origin.x + 450.,
            y: origin.y - 500.,
            w: 60.,
            h: 1000.,
        });
        world.creator_attack(id, Attack::RiftDash).unwrap();
        let destination = world.tanks[0].pos;
        assert!(destination.x > origin.x + 200.);
        assert!(destination.x + RADIUS < origin.x + 450.);
        assert!(world.position_clear(destination, RADIUS));
        world.walls.clear();
        let bounds = world.config.bounds();
        world.tanks[0].pos.x = bounds.x + bounds.w - RADIUS - 100.;
        world.tanks[0].creator_attacks[Attack::RiftDash.index()] = 0.;
        world.creator_attack(id, Attack::RiftDash).unwrap();
        assert!(bounds.contains_circle(world.tanks[0].pos, RADIUS));
    }
    #[test]
    fn solar_sentries_fire_and_reforge_is_limited_by_health_and_cooldown() {
        let (mut world, id) = event_world();
        let enemy = world.add_player("Opponent", false);
        let origin = world.tanks[0].pos;
        world.tanks[1].pos = origin + V2::new(650., 0.);
        world.tanks[1].protection = 0.;
        world.creator_attack(id, Attack::SolarTurrets).unwrap();
        for _ in 0..65 {
            world.step();
        }
        assert!(
            world
                .projectiles
                .iter()
                .any(|p| p.owner == id && p.kind == ProjectileKind::Bullet)
        );
        assert!(world.tanks.iter().any(|t| t.id == enemy));
        world.tanks[0].health = HEALTH * 0.5;
        world.creator_attack(id, Attack::Reforge).unwrap();
        assert!((world.tanks[0].health - HEALTH * 0.58).abs() < 1.);
        assert!(world.creator_attack(id, Attack::Reforge).is_err());
        world.tanks[0].creator_attacks[Attack::Reforge.index()] = 0.;
        world.tanks[0].health = HEALTH - 1.;
        world.creator_attack(id, Attack::Reforge).unwrap();
        assert_eq!(world.tanks[0].health, HEALTH);
    }
    #[test]
    fn gravity_bends_only_hostile_rounds_without_extending_lifetime() {
        let (mut world, id) = event_world();
        world.tanks[0].team = 1;
        let origin = world.tanks[0].pos;
        for (owner, team) in [(id, 1), (999, 1), (998, 2)] {
            let shot = world.simple_projectile(
                owner,
                team,
                origin + V2::new(500., 0.),
                V2::new(0., 400.),
                ProjectileKind::Bullet,
                10.,
                5.,
                2.,
                1.,
            );
            world.projectiles.push(shot);
        }
        world.creator_attack(id, Attack::GravityWell).unwrap();
        assert_eq!(world.projectiles[0].vel, V2::new(0., 400.));
        assert_eq!(world.projectiles[1].vel, V2::new(0., 400.));
        assert!(world.projectiles[2].vel.x < 0.);
        assert_eq!(world.projectiles[2].lifetime, 2.);
        assert_eq!(world.projectiles.len(), 15);
    }
}
