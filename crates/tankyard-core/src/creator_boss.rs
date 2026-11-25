//! A trusted event form. Clients cannot request it through gameplay controls.
use crate::*;
use serde::{Deserialize, Serialize};
use tankyard_content::ProjectileKind;
pub const NAME: &str = "Axiom, the Worldshaper";
pub const HEALTH: f32 = 120_000.;
pub const RADIUS: f32 = 180.;
pub const ATTACK_COUNT: usize = 12;
pub const MAX_COOLDOWN: f32 = 30.;
/// Axiom's cannons, counted around the hull from the one on its aim.
pub const CANNONS: usize = 6;
/// How far from its center a cannon's mouth is, in hull radii.
pub const MUZZLE: f32 = 1.42;
/// How far from Axiom's center Repulse clears hostile shots.
pub const REPULSE_REACH: f32 = 360.;
/// How far from Axiom's center Gravity Well bends them.
pub const GRAVITY_REACH: f32 = 900.;
/// The forward cannons' turns while fire is held, as steps from the aim.
const GUN_ORDER: [i8; 4] = [0, 1, 0, -1];
const GUN_INTERVAL: f32 = 0.13;
const GUN_SPEED: f32 = 860.;
const GUN_DAMAGE: f32 = 22.;
const GUN_RADIUS: f32 = 11.;
const GUN_LIFETIME: f32 = 1.6;
const GUN_PENETRATION: f32 = 3.;

/// Where the mouth of the cannon `step` places around from the aim is.
pub fn muzzle(center: V2, aim: f32, step: i8, radius: f32) -> V2 {
    let angle = aim + f32::from(step) * std::f32::consts::TAU / CANNONS as f32;
    center + V2::from_angle(angle) * radius * MUZZLE
}
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
    /// The attacks in the order the panel and the number row lay them out.
    pub fn bar() -> &'static [Self] {
        Self::GROUPS.as_flattened()
    }
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
    /// Shots, speed, damage, radius, lifetime and kind.
    fn stats(self) -> (usize, f32, f32, f32, f32, ProjectileKind) {
        match self {
            Self::Starfall => (9, 760., 36., 9., 2.5, ProjectileKind::Piercing),
            Self::Sunburst => (24, 350., 28., 13., 3.2, ProjectileKind::Shell),
            Self::Constellation => (12, 230., 24., 11., 4., ProjectileKind::Homing),
            Self::Repulse => (32, 460., 20., 10., 1.2, ProjectileKind::Bullet),
            Self::MeteorShower => (7, 310., 70., 16., 3.5, ProjectileKind::Explosive),
            Self::CometLance => (1, 1050., 180., 25., 2.5, ProjectileKind::Piercing),
            Self::NovaMines => (12, 120., 45., 14., 10., ProjectileKind::Mine),
            Self::StarLattice => (8, 0., 12., 30., 8., ProjectileKind::Barrier),
            Self::SolarTurrets => (6, 0., 30., 20., 12., ProjectileKind::Turret),
            Self::GravityWell => (12, 180., 18., 12., 4., ProjectileKind::Homing),
            Self::RiftDash | Self::Reforge => (0, 0., 0., 0., 0., ProjectileKind::Bullet),
        }
    }
    /// How far from the center of a hull of radius `hull` this attack's shots
    /// appear.
    pub fn spawn_reach(self, hull: f32) -> f32 {
        let offset = match self {
            Self::NovaMines => 100.,
            Self::StarLattice => 60.,
            Self::SolarTurrets => 180.,
            _ => 8.,
        };
        hull + self.stats().3 + offset
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
        tank.grip = 1.;
        tank.rift_shots = 0;
        tank.rift_armed = 0.;
        tank.creator_reload = 0.;
        tank.creator_turn = 0;
        self.tanks[index].creator_boss = enabled;
        self.tanks[index].creator_attacks = [0.; ATTACK_COUNT];
        self.tanks[index].pos = pos;
        self.tanks[index].vel = V2::ZERO;
        self.refresh_stats(player, true);
        self.config.modded = true;
        Ok(())
    }
    /// One tick of Axiom's own guns. While fire is held its forward cannons
    /// take turns, each angled so the stream meets at the cursor.
    pub(crate) fn creator_guns(&mut self, index: usize) {
        let room = self.projectiles.len() < self.config.max_projectiles;
        let rules = self.config.rules.damage_multiplier;
        let tuning = self.owner_weapons(self.tanks[index].id);
        let tank = &mut self.tanks[index];
        tank.charge = 0.;
        tank.creator_reload = (tank.creator_reload - DT).max(0.);
        if !tank.input.fire || (tank.spawn_immunity && tank.protection > 0.) {
            return;
        }
        tank.protection = 0.;
        tank.spawn_immunity = false;
        if tank.creator_reload > 0. || !room {
            return;
        }
        let step = GUN_ORDER[usize::from(tank.creator_turn) % GUN_ORDER.len()];
        tank.creator_turn = (tank.creator_turn + 1) % GUN_ORDER.len() as u8;
        tank.creator_reload = GUN_INTERVAL / tank.reload_scale.clamp(0.25, 6.);
        let (id, team, center, aim, drift, radius) = (
            tank.id,
            tank.team,
            tank.pos,
            tank.aim,
            tank.vel,
            tank.radius,
        );
        let reach = tank
            .input
            .aim_distance
            .max(radius * 1.6)
            .min(GUN_SPEED * tuning.shot_speed * GUN_LIFETIME);
        let damage = GUN_DAMAGE * rules * (tank.damage_scale * tuning.damage).clamp(0.25, 8.);
        let mouth = muzzle(center, aim, step, radius);
        // A cannon pressed against cover holds fire rather than shoot through.
        if !self.position_clear(mouth, GUN_RADIUS)
            || self
                .walls
                .iter()
                .any(|&wall| physics::sweep_rect(center, mouth, GUN_RADIUS, wall).is_some())
        {
            return;
        }
        let heading = (center + V2::from_angle(aim) * reach - mouth).normalized();
        let shot = self.simple_projectile(
            id,
            team,
            mouth,
            heading * (GUN_SPEED * tuning.shot_speed) + drift * 0.16,
            ProjectileKind::Shell,
            damage,
            GUN_RADIUS,
            GUN_LIFETIME,
            GUN_PENETRATION,
        );
        self.projectiles.push(shot);
        let tank = &mut self.tanks[index];
        tank.shot_count = tank.shot_count.wrapping_add(1);
        self.event(
            EventKind::Shot,
            mouth,
            id,
            ProjectileKind::Shell as u32,
            team,
        );
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
        let (count, speed, damage, radius, lifetime, kind) = attack.stats();
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
            let pos = origin + direction * attack.spawn_reach(radius_tank);
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
                        || p.pos.distance(origin) > REPULSE_REACH
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
                p.owner == player
                    || (team > 0 && p.team == team)
                    || p.pos.distance(origin) > REPULSE_REACH
            });
        }
        if attack == Attack::GravityWell {
            for shot in &mut self.projectiles {
                let distance = shot.pos.distance(origin);
                if shot.owner != player
                    && (team == 0 || shot.team != team)
                    && !shot.kind.is_drone()
                    && shot.frozen.is_none()
                    && (radius_tank..GRAVITY_REACH).contains(&distance)
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
    fn the_number_row_covers_every_attack_once() {
        let bar = Attack::bar();
        assert_eq!(bar.len(), ATTACK_COUNT);
        for attack in Attack::ALL {
            assert_eq!(
                bar.iter().filter(|a| **a == attack).count(),
                1,
                "{attack:?}"
            );
        }
    }
    #[test]
    fn holding_fire_runs_axioms_cannons_and_not_the_tank_underneath() {
        let (mut world, id) = event_world();
        world.tanks[0].aim = 0.;
        world.tanks[0].input.aim = 0.;
        world.tanks[0].input.aim_distance = 900.;
        world.tanks[0].input.fire = true;
        let center = world.tanks[0].pos;
        let mut mouths = Vec::new();
        for _ in 0..40 {
            let known: Vec<_> = world.projectiles.iter().map(|p| p.id).collect();
            world.step();
            for shot in world.projectiles.iter().filter(|p| !known.contains(&p.id)) {
                assert_eq!(shot.owner, id);
                assert_eq!(shot.kind, ProjectileKind::Shell);
                assert!(shot.damage >= GUN_DAMAGE * 0.99, "{}", shot.damage);
                let reach = shot.pos.distance(center);
                assert!((reach - RADIUS * MUZZLE).abs() < 24., "{reach}");
                // Every stream leans in toward the cursor, ahead of the hull.
                assert!(shot.vel.x > 0.);
                mouths.push(shot.pos.y - center.y);
            }
        }
        // 40 ticks at 60 Hz, one shot each 0.13 s.
        assert!((4..=6).contains(&mouths.len()), "{}", mouths.len());
        assert!(mouths.iter().any(|y| *y > 100.) && mouths.iter().any(|y| *y < -100.));
        assert!(mouths.iter().any(|y| y.abs() < 20.));
        assert!(world.tanks[0].vel.length() < 1., "firing doesn't shove it");
    }
    #[test]
    fn axioms_cannons_respect_cover_the_shot_budget_and_their_reload() {
        let (mut world, _) = event_world();
        world.tanks[0].aim = 0.;
        world.tanks[0].input.fire = true;
        world.config.max_projectiles = 0;
        for _ in 0..30 {
            world.step();
        }
        assert!(world.projectiles.is_empty(), "no room, no shot");
        world.config.max_projectiles = 256;
        // Walls are rebuilt from the map each tick, so this drives the guns alone.
        let center = world.tanks[0].pos;
        world.walls = vec![Rect {
            x: center.x + RADIUS + 4.,
            y: center.y - 600.,
            w: 20.,
            h: 1200.,
        }];
        world.tanks[0].creator_turn = 0;
        world.tanks[0].creator_reload = 0.;
        world.creator_guns(0);
        assert!(world.projectiles.is_empty(), "the wall stops the muzzle");
        world.walls.clear();
        world.tanks[0].creator_turn = 0;
        world.tanks[0].creator_reload = 0.;
        world.tanks[0].reload_scale = 2.;
        world.creator_guns(0);
        assert_eq!(world.projectiles.len(), 1);
        assert!(world.tanks[0].creator_reload <= GUN_INTERVAL / 2. + 0.001);
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
