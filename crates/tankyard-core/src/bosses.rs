use crate::physics::{circle_rect, sweep_rect};
use crate::world::drone_phase;
use crate::*;
use tankyard_content::ProjectileKind;

fn boss_clear_steering(pos: V2, desired: V2, radius: f32, bounds: Rect, walls: &[Rect]) -> V2 {
    let mut best = V2::ZERO;
    let mut best_score = f32::NEG_INFINITY;
    for angle in [0., 0.35, -0.35, 0.7, -0.7, 1.2, -1.2, 1.8, -1.8, 2.6, -2.6] {
        let velocity = desired.rotate(angle);
        let next = pos + velocity * 0.75;
        if !bounds.contains_circle(next, radius + 8.)
            || walls
                .iter()
                .any(|&w| sweep_rect(pos, next, radius + 4., w).is_some())
        {
            continue;
        }
        let border = (next.x - bounds.x)
            .min(next.y - bounds.y)
            .min(bounds.x + bounds.w - next.x)
            .min(bounds.y + bounds.h - next.y);
        let score =
            velocity.normalized().dot(desired.normalized()) + (border / 300.).min(1.) * 0.25;
        if score > best_score {
            best = velocity;
            best_score = score;
        }
    }
    best
}

impl World {
    pub(crate) fn boss_drone_goal(&self, boss: &Boss, drone: EntityId, pos: V2, team: u8) -> V2 {
        let (acquisition, leash) = match boss.kind {
            BossKind::Summoner => (820., 1050.),
            BossKind::Guardian => (900., 1200.),
            BossKind::FallenOverlord => (1000., 1350.),
            _ => (760., 1000.),
        };
        let guard = boss.pos
            + V2::from_angle(drone_phase(drone) + self.tick as f32 * DT * 0.25)
                * (boss.radius + 55.);
        if pos.distance(boss.pos) > leash {
            return guard;
        }
        // Acquire from the drone, but never chain pursuit across the map. The
        // owner leash and protected/new-player rules match the boss's awareness.
        self.tanks
            .iter()
            .filter(|tank| {
                tank.alive
                    && tank.health > 0.
                    && tank.protection <= 0.
                    && !self.config.base_protected(tank.team, tank.pos, tank.radius)
                    && self.hostile(team, tank.team)
                    && (tank.level >= 15
                        || tank.id == boss.last_attacker
                        || tank.pos.distance(boss.pos) < 420.)
                    && tank.pos.distance(pos) <= acquisition
                    && tank.pos.distance(boss.pos) <= leash - 100.
                    && self
                        .walls
                        .iter()
                        .all(|&wall| sweep_rect(pos, tank.pos, 2., wall).is_none())
            })
            .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
            .map_or(guard, |tank| tank.pos)
    }

    pub(crate) fn random_boss_event(&mut self) {
        if self.tick < self.boss_roll_at {
            return;
        }
        self.boss_roll_at = self.tick.saturating_add(60);
        if !matches!(
            self.config.mode,
            Mode::Ffa | Mode::TwoTeams | Mode::FourTeams | Mode::Maze | Mode::RandomEvents
        ) {
            return;
        }
        if !self.bosses.is_empty() {
            self.boss_roll_at = self.tick.saturating_add(90 * 60);
            return;
        }
        if !self.tanks.iter().any(|t| t.alive) {
            return;
        }
        // One aggregate 0.25% roll per second, independent of simulation FPS.
        if self.random() < 0.0025 {
            let index = (self.random() * BossKind::ALL.len() as f32) as usize;
            self.spawn_boss(BossKind::ALL[index.min(BossKind::ALL.len() - 1)]);
        }
    }

    /// Returns zero if the map cannot accommodate the boss safely.
    pub fn spawn_boss(&mut self, kind: BossKind) -> EntityId {
        if self.bosses.len() >= 16 {
            return 0;
        }
        let bounds = self.config.bounds();
        let radius = kind.radius();
        let mut best = None;
        let mut safety = f32::NEG_INFINITY;
        for attempt in 0..128 {
            let pos = if attempt == 0 {
                bounds.center()
            } else {
                V2::new(
                    bounds.x + radius + 30. + self.random() * (bounds.w - 2. * radius - 60.),
                    bounds.y + radius + 30. + self.random() * (bounds.h - 2. * radius - 60.),
                )
            };
            if !self.position_clear(pos, radius + 25.)
                || self
                    .bosses
                    .iter()
                    .any(|b| b.pos.distance(pos) < b.radius + radius + 40.)
            {
                continue;
            }
            let nearest = self
                .tanks
                .iter()
                .filter(|t| t.alive)
                .map(|t| t.pos.distance(pos) - t.radius - radius)
                .fold(1500., f32::min);
            if nearest > safety {
                safety = nearest;
                best = Some(pos);
            }
            if safety > 700. {
                break;
            }
        }
        let Some(pos) = best else {
            return 0;
        };
        // Never materialize a damaging hull on top of a participant.
        if safety < 100. {
            return 0;
        }
        let health = (4400. + f32::from(self.match_state.boss_wave.max(1)) * 700.)
            * match kind {
                BossKind::Bastion | BossKind::Defender => 1.35,
                BossKind::Guardian | BossKind::Summoner => 1.15,
                BossKind::Tempest | BossKind::FallenBooster => 0.9,
                _ => 1.,
            };
        let health = health * (1. + self.tanks.len().saturating_sub(4).min(16) as f32 * 0.045);
        let id = self.id();
        self.bosses.push(Boss {
            id,
            pos,
            vel: V2::ZERO,
            radius,
            health,
            max_health: health,
            kind,
            phase: 1,
            timer: 2.,
            aim: 0.,
            team: 0,
            mothership: false,
            target: 0,
            goal: pos,
            tactic: BossTactic::Patrol,
            tactic_time: 2.,
            last_attacker: 0,
            arrival: 2.,
            attack_timer: 7.,
            attack: 0,
            attack_windup: 0.,
        });
        self.event(EventKind::BossSpawn, pos, id, kind as u32, 0);
        id
    }
    pub(crate) fn update_bosses(&mut self) {
        let mut volleys = Vec::new();
        let mut warnings = Vec::new();
        for i in 0..self.bosses.len() {
            let b = &self.bosses[i];
            if b.health <= 0. {
                continue;
            }
            let owned = self
                .projectiles
                .iter()
                .filter(|p| {
                    p.owner == b.id && p.kind.is_drone() && p.lifetime > 0. && p.penetration > 0.
                })
                .count();
            let target = self
                .tanks
                .iter()
                .filter(|t| {
                    t.alive
                        && t.health > 0.
                        && self.hostile(b.team, t.team)
                        && t.protection <= 0.
                        && !self.config.base_protected(t.team, t.pos, t.radius)
                        && t.pos.distance(b.pos) < 1600.
                        && (t.level >= 15
                            || b.kind.drone_limit() == 0
                            || t.id == b.last_attacker
                            || t.pos.distance(b.pos) < 420.)
                        && self
                            .walls
                            .iter()
                            .all(|&w| sweep_rect(b.pos, t.pos, 5., w).is_none())
                })
                .min_by(|a, c| {
                    let cost = |t: &Tank| {
                        t.pos.distance(b.pos)
                            * if t.id == b.target { 0.75 } else { 1. }
                            * if t.id == b.last_attacker { 0.65 } else { 1. }
                            * (0.85 + t.health / t.max_health.max(1.) * 0.15)
                    };
                    cost(a).total_cmp(&cost(c))
                })
                .map(|t| (t.id, t.pos, t.vel));
            let bounds = self.config.bounds();
            let b = &mut self.bosses[i];
            b.arrival = (b.arrival - DT).max(0.);
            b.phase = if b.health < b.max_health * 0.3 {
                3
            } else if b.health < b.max_health * 0.65 {
                2
            } else {
                1
            };
            if let Some((id, position, _)) = target {
                b.target = id;
                b.goal = position;
            } else if b.pos.distance(b.goal) < 130.
                || !bounds.contains_circle(b.goal, b.radius + 60.)
            {
                b.target = 0;
                let angle = self.tick as f32 * 0.003 + b.id as f32 * 1.618;
                b.goal = bounds.clamp_circle(
                    bounds.center() + V2::from_angle(angle) * (bounds.w.min(bounds.h) * 0.3),
                    b.radius + 80.,
                );
            }
            let to = b.goal - b.pos;
            let velocity = target.map_or(V2::ZERO, |(_, _, v)| v);
            let lead = (to.length() / 420.).clamp(0., 0.7);
            if b.tactic != BossTactic::Windup && b.tactic != BossTactic::Charge {
                let aim = (to + velocity * lead).angle();
                let difference = (aim - b.aim + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                b.aim += difference.clamp(-2.8 * DT, 2.8 * DT);
            }
            b.tactic_time = (b.tactic_time - DT).max(0.);
            let speed = if b.mothership {
                28.
            } else {
                match b.kind {
                    BossKind::Tempest => 125.,
                    BossKind::Eclipse => 95.,
                    BossKind::FallenBooster => 180.,
                    BossKind::Guardian => 90.,
                    BossKind::Summoner => 45.,
                    BossKind::FallenOverlord => 65.,
                    BossKind::Foundry | BossKind::Bastion | BossKind::Defender => 32.,
                    _ => 55.,
                }
            };
            let preferred = match b.kind {
                BossKind::FallenBooster => 0.,
                BossKind::Guardian => 330.,
                BossKind::Summoner | BossKind::FallenOverlord | BossKind::Foundry => 540.,
                BossKind::Bastion | BossKind::Defender => 650.,
                _ => 450.,
            };
            let depleted = b.kind.drone_limit() > 0 && owned < b.kind.drone_limit() / 3;
            let mut desired = if target.is_none() {
                b.tactic = BossTactic::Patrol;
                to.normalized() * speed * 0.45
            } else if b.tactic == BossTactic::Windup && b.tactic_time > 0. {
                V2::ZERO
            } else if b.tactic == BossTactic::Windup {
                b.tactic = BossTactic::Charge;
                b.tactic_time = 0.65;
                V2::from_angle(b.aim) * speed * 2.2
            } else if b.tactic == BossTactic::Charge && b.tactic_time > 0. {
                V2::from_angle(b.aim) * speed * 2.2
            } else if to.length() < preferred * if depleted || b.phase == 3 { 1.15 } else { 0.7 } {
                b.tactic = BossTactic::Retreat;
                to.normalized() * -speed
            } else if to.length() > preferred + 130. {
                b.tactic = BossTactic::Advance;
                to.normalized() * speed
            } else {
                b.tactic = BossTactic::Orbit;
                V2::new(-to.y, to.x).normalized()
                    * speed
                    * if b.id.is_multiple_of(2) { 0.8 } else { -0.8 }
            };
            // Read nearby incoming rounds, never inputs, and sidestep collisions.
            let mut evasion = V2::ZERO;
            for shot in self.projectiles.iter().filter(|p| {
                p.owner != b.id
                    && p.lifetime > 0.
                    && p.penetration > 0.
                    && (p.team == 0 || b.team == 0 || p.team != b.team)
                    && p.pos.distance(b.pos) < 450.
            }) {
                let relative = shot.vel - b.vel;
                let delta = b.pos - shot.pos;
                let time = delta.dot(relative) / relative.length_squared().max(1.);
                if (0.05..0.65).contains(&time)
                    && (delta - relative * time).length() < b.radius + shot.radius + 18.
                {
                    let side = V2::new(-relative.y, relative.x).normalized();
                    evasion += side * if delta.dot(side) < 0. { -1. } else { 1. };
                }
            }
            if b.tactic != BossTactic::Charge {
                desired += evasion.clamp_length(1.) * speed * 0.7;
            }
            desired = boss_clear_steering(b.pos, desired, b.radius, bounds, &self.walls);
            if b.arrival > 0. {
                desired = V2::ZERO;
            }
            b.vel = b.vel.lerp(desired, 1. - (-DT * 3.5).exp());
            let next = b.pos + b.vel * DT;
            if self
                .walls
                .iter()
                .all(|&w| circle_rect(next, b.radius, w).is_none())
            {
                b.pos = bounds.clamp_circle(next, b.radius);
            } else {
                b.vel *= 0.6;
            }
            b.timer -= DT;
            let primary_due = b.timer <= 0. && b.arrival <= 0.;
            let previous_windup = b.attack_windup;
            b.attack_windup = (b.attack_windup - DT).max(0.);
            let secondary = previous_windup > 0. && b.attack_windup <= 0. && target.is_some();
            if b.arrival <= 0. && target.is_some() && previous_windup <= 0. {
                b.attack_timer -= DT;
                if b.attack_timer <= 0. {
                    b.attack = (b.attack + 1) % 3;
                    b.attack_windup = 0.9;
                    b.attack_timer = 9.5 - f32::from(b.phase) * 1.5;
                    warnings.push((b.pos, b.id, b.attack as u32, b.team));
                }
            }
            // Let the warning and secondary pattern read clearly. A primary
            // volley cannot land on the same frame as the special attack.
            let primary = primary_due && b.attack_windup <= 0. && !secondary;
            if secondary {
                b.timer = b.timer.max(0.4);
            }
            if target.is_none() || (!primary && !secondary) {
                continue;
            }
            let mut spawned_drones = owned;
            let mut emit = |angle: f32,
                            kind: ProjectileKind,
                            speed: f32,
                            damage: f32,
                            radius: f32,
                            life: f32,
                            pen: f32,
                            bounces: u8| {
                if kind.is_drone() {
                    if spawned_drones >= b.kind.drone_limit() {
                        return;
                    }
                    spawned_drones += 1;
                }
                let origin = b.pos + V2::from_angle(angle) * (b.radius + radius + 3.);
                if !bounds.contains_circle(origin, radius)
                    || self
                        .walls
                        .iter()
                        .any(|&w| sweep_rect(b.pos, origin, radius, w).is_some())
                {
                    return;
                }
                volleys.push((
                    b.id,
                    b.team,
                    origin,
                    V2::from_angle(angle) * speed,
                    kind,
                    damage,
                    radius,
                    life,
                    pen,
                    bounces,
                ));
            };
            if secondary {
                // Distinct, telegraphed complements to each boss's normal arsenal.
                // Every ring leaves a moving gap and every projectile uses normal
                // wall collisions, parries, lifetime and resource limits.
                let turn = f32::from(b.attack) * 0.33;
                match b.kind {
                    BossKind::Guardian | BossKind::Summoner | BossKind::FallenOverlord => {
                        for n in 0..12 {
                            let a = b.aim + turn + (n as f32 - 5.5) * 0.24;
                            let kind = if b.kind == BossKind::FallenOverlord {
                                ProjectileKind::Homing
                            } else {
                                ProjectileKind::Bullet
                            };
                            emit(a, kind, 340., 18., 8., 2.8, 1.3, 0);
                        }
                    }
                    BossKind::Defender | BossKind::Bastion => {
                        for n in 1..12 {
                            emit(
                                b.aim + turn + n as f32 * std::f32::consts::TAU / 12.,
                                ProjectileKind::Trap,
                                320.,
                                27.,
                                12.,
                                5.,
                                3.5,
                                0,
                            );
                        }
                        emit(b.aim, ProjectileKind::Shell, 460., 38., 14., 2.5, 2., 0);
                    }
                    BossKind::FallenBooster | BossKind::Tempest => {
                        for n in 0..5 {
                            emit(
                                b.aim + (n as f32 - 2.) * 0.22,
                                ProjectileKind::Homing,
                                350.,
                                18.,
                                8.,
                                3.,
                                1.2,
                                0,
                            );
                        }
                        if b.tactic != BossTactic::Charge {
                            b.tactic = BossTactic::Windup;
                            b.tactic_time = 0.7;
                        }
                    }
                    BossKind::Sentinel => {
                        for n in 0..7 {
                            emit(
                                b.aim + (n as f32 - 3.) * 0.13,
                                ProjectileKind::Piercing,
                                560.,
                                28.,
                                8.,
                                2.5,
                                2.,
                                0,
                            );
                        }
                    }
                    BossKind::Prism | BossKind::Eclipse => {
                        for n in 1..16 {
                            emit(
                                b.aim + turn + n as f32 * std::f32::consts::TAU / 16.,
                                if b.attack == 1 {
                                    ProjectileKind::Homing
                                } else {
                                    ProjectileKind::Piercing
                                },
                                300. + n as f32 * 8.,
                                17.,
                                7.,
                                3.,
                                1.5,
                                1,
                            );
                        }
                    }
                    BossKind::Foundry => {
                        for n in 0..5 {
                            emit(
                                b.aim + (n as f32 - 2.) * 0.35,
                                ProjectileKind::Explosive,
                                310.,
                                32.,
                                12.,
                                3.,
                                1.5,
                                0,
                            );
                        }
                        for n in 0..6 {
                            emit(
                                b.aim + n as f32 * std::f32::consts::TAU / 6.,
                                ProjectileKind::Mine,
                                240.,
                                25.,
                                11.,
                                5.,
                                2.5,
                                0,
                            );
                        }
                    }
                }
            }
            if !primary {
                continue;
            }
            match b.kind {
                BossKind::Guardian => {
                    for n in 0..4 {
                        emit(
                            b.aim + std::f32::consts::PI + (n as f32 - 1.5) * 0.18,
                            ProjectileKind::Drone,
                            255.,
                            16.,
                            15.,
                            10.,
                            3.5,
                            0,
                        );
                    }
                    b.timer = 0.65;
                }
                BossKind::Summoner => {
                    for n in 0..4 {
                        emit(
                            b.aim + n as f32 * std::f32::consts::FRAC_PI_2,
                            ProjectileKind::Drone,
                            220.,
                            18.,
                            19.,
                            11.,
                            4.,
                            0,
                        );
                    }
                    b.timer = 0.72;
                }
                BossKind::FallenOverlord => {
                    for n in 0..4 {
                        emit(
                            b.aim + n as f32 * std::f32::consts::FRAC_PI_2,
                            ProjectileKind::Drone,
                            290.,
                            24.,
                            17.,
                            10.,
                            5.,
                            0,
                        );
                    }
                    b.timer = 0.95;
                }
                BossKind::Defender => {
                    for n in 0..3 {
                        let angle =
                            self.tick as f32 * DT * 0.35 + n as f32 * std::f32::consts::TAU / 3.;
                        emit(angle, ProjectileKind::Trap, 180., 26., 18., 7., 3., 0);
                        emit(
                            b.aim + (n as f32 - 1.) * 0.1,
                            ProjectileKind::Bullet,
                            410.,
                            17.,
                            8.,
                            2.5,
                            1.5,
                            0,
                        );
                    }
                    b.timer = 0.85;
                }
                BossKind::FallenBooster => {
                    emit(b.aim, ProjectileKind::Bullet, 460., 19., 10., 2., 1.8, 0);
                    for angle in [-0.45_f32, -0.2, 0.2, 0.45] {
                        emit(
                            b.aim + std::f32::consts::PI + angle,
                            ProjectileKind::Bullet,
                            330.,
                            12.,
                            7.,
                            1.6,
                            1.,
                            0,
                        );
                    }
                    b.timer = 0.65;
                }
                BossKind::Sentinel => {
                    for n in 0..(3 + b.phase as usize * 2) {
                        let count = 3 + b.phase as usize * 2;
                        emit(
                            b.aim + (n as f32 - (count - 1) as f32 / 2.) * 0.16,
                            ProjectileKind::Shell,
                            330.,
                            24.,
                            12.,
                            3.5,
                            2.,
                            0,
                        );
                    }
                    b.timer = 1.15;
                }
                BossKind::Prism => {
                    for n in 0..(6 + b.phase as usize * 2) {
                        emit(
                            self.tick as f32 * DT * 0.6
                                + n as f32 * std::f32::consts::TAU
                                    / (6 + b.phase as usize * 2) as f32,
                            ProjectileKind::Piercing,
                            410.,
                            16.,
                            7.,
                            4.2,
                            2.,
                            1,
                        );
                    }
                    b.timer = 1.45;
                }
                BossKind::Foundry => {
                    for n in 0..(2 + b.phase) {
                        emit(
                            b.aim + (n as f32 - 1.) * 0.7,
                            ProjectileKind::Drone,
                            200.,
                            17.,
                            12.,
                            7.,
                            4.,
                            0,
                        );
                    }
                    if b.phase > 1 {
                        emit(b.aim, ProjectileKind::Turret, 80., 19., 17., 6., 4., 0);
                    }
                    b.timer = 1.8;
                }
                BossKind::Tempest => {
                    let count = 3 + b.phase;
                    for n in 0..count {
                        emit(
                            b.aim + (n as f32 - (f32::from(count) - 1.) * 0.5) * 0.45,
                            ProjectileKind::Homing,
                            285.,
                            14.,
                            7.,
                            4.,
                            1.,
                            0,
                        );
                    }
                    if b.tactic != BossTactic::Windup && b.tactic != BossTactic::Charge {
                        b.tactic = BossTactic::Windup;
                        b.tactic_time = 0.7;
                    }
                    b.timer = 1.05;
                }
                BossKind::Bastion => {
                    for n in 0..8 {
                        emit(
                            n as f32 * std::f32::consts::TAU / 8. + self.tick as f32 * 0.005,
                            ProjectileKind::Mine,
                            220.,
                            27.,
                            13.,
                            6.5,
                            2.,
                            0,
                        );
                    }
                    emit(b.aim, ProjectileKind::Explosive, 300., 40., 15., 3., 1., 0);
                    b.timer = 2.;
                }
                BossKind::Eclipse => {
                    for n in 0..(10 + b.phase as usize * 4) {
                        emit(
                            b.aim
                                + n as f32 * std::f32::consts::TAU
                                    / (10 + b.phase as usize * 4) as f32,
                            ProjectileKind::Fragment,
                            370.,
                            20.,
                            9.,
                            2.5,
                            1.,
                            0,
                        );
                    }
                    b.timer = 2.2;
                    if b.tactic != BossTactic::Windup && b.tactic != BossTactic::Charge {
                        b.tactic = BossTactic::Windup;
                        b.tactic_time = 0.9;
                    }
                }
            }
        }
        for (pos, id, attack, team) in warnings {
            self.event(EventKind::BossAttack, pos, id, attack, team);
        }
        let room = self
            .config
            .max_projectiles
            .saturating_sub(self.projectiles.len());
        for (owner, team, pos, vel, kind, damage, radius, life, pen, bounces) in
            volleys.into_iter().take(room)
        {
            let mut p =
                self.simple_projectile(owner, team, pos, vel, kind, damage, radius, life, pen);
            p.ricochets = bounces;
            self.projectiles.push(p);
            self.event(EventKind::Shot, pos, owner, kind as u32, team);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;

    fn arena() -> World {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                dynamic_arena: false,
                spawn_protection: 0.,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Pilot", false);
        world.tanks[0].level = 45;
        world
    }

    #[test]
    fn summoner_drones_have_shorter_acquisition_and_return_to_their_owner() {
        let mut world = arena();
        assert_ne!(world.spawn_boss(BossKind::Summoner), 0);
        world.bosses[0].pos = V2::new(1600., 1300.);
        let boss = &world.bosses[0];
        world.tanks[0].pos = boss.pos + V2::new(810., 0.);
        assert_eq!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.tanks[0].pos = boss.pos + V2::new(850., 0.);
        assert!(
            world
                .boss_drone_goal(boss, 100, boss.pos, 0)
                .distance(boss.pos)
                < 200.
        );
        // A drone near the player cannot extend its boss's pursuit across the map.
        world.tanks[0].pos = boss.pos + V2::new(1100., 0.);
        assert!(
            world
                .boss_drone_goal(boss, 100, boss.pos + V2::new(900., 0.), 0)
                .distance(boss.pos)
                < 200.
        );
        world.tanks[0].pos = boss.pos + V2::new(700., 0.);
        assert!(
            world
                .boss_drone_goal(boss, 100, boss.pos + V2::new(1100., 0.), 0)
                .distance(boss.pos)
                < 200.
        );
    }

    #[test]
    fn boss_drones_observe_cover_protection_and_new_player_grace() {
        let mut world = arena();
        world.spawn_boss(BossKind::Summoner);
        world.bosses[0].pos = V2::new(1600., 1300.);
        world.tanks[0].pos = V2::new(2200., 1300.);
        world.tanks[0].level = 1;
        let boss = &world.bosses[0];
        assert_ne!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.tanks[0].level = 45;
        world.tanks[0].protection = 5.;
        assert_ne!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.tanks[0].protection = 0.;
        world.walls.push(Rect {
            x: 1900.,
            y: 1000.,
            w: 50.,
            h: 600.,
        });
        assert_ne!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
        world.walls.clear();
        assert_eq!(
            world.boss_drone_goal(boss, 100, boss.pos, 0),
            world.tanks[0].pos
        );
    }

    #[test]
    fn secondary_warnings_pause_primary_fire_and_leave_a_recovery_gap() {
        for kind in BossKind::ALL {
            let mut world = arena();
            world.tanks[0].pos = V2::new(2450., 1300.);
            world.spawn_boss(kind);
            world.bosses[0].pos = V2::new(1600., 1300.);
            world.bosses[0].arrival = 0.;
            world.bosses[0].timer = 0.;
            world.bosses[0].attack_timer = 0.;
            world.update_bosses();
            assert!(
                world.projectiles.is_empty(),
                "{kind:?} hid its windup behind a primary volley"
            );
            for _ in 0..55 {
                world.update_bosses();
            }
            assert!(
                !world.projectiles.is_empty(),
                "{kind:?} did not release its secondary"
            );
            let shots = world.projectiles.len();
            for _ in 0..20 {
                world.update_bosses();
            }
            assert_eq!(
                world.projectiles.len(),
                shots,
                "{kind:?} overlapped its recovery gap"
            );
        }
    }

    #[test]
    fn every_boss_has_more_health_and_a_telegraphed_secondary_attack() {
        for kind in BossKind::ALL {
            let mut world = arena();
            world.tanks[0].pos = V2::new(2450., 1300.);
            world.spawn_boss(kind);
            let boss = &mut world.bosses[0];
            assert!(boss.max_health >= 4500.);
            boss.pos = V2::new(1600., 1300.);
            boss.arrival = 0.;
            boss.timer = 1000.;
            boss.attack_timer = 0.;
            world.update_bosses();
            assert!(world.bosses[0].attack_windup > 0.8);
            assert!(
                world.projectiles.is_empty(),
                "{kind:?} attacked before its warning"
            );
            assert!(
                world
                    .events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::BossAttack))
            );
            for _ in 0..60 {
                world.update_bosses();
            }
            assert!(
                !world.projectiles.is_empty(),
                "{kind:?} did not release its attack"
            );
            assert!(world.projectiles.len() <= 24);
            assert!(
                world
                    .projectiles
                    .iter()
                    .all(|p| p.vel.finite() && p.owner == world.bosses[0].id)
            );
        }
    }

    #[test]
    fn random_events_roll_once_per_second_with_grace_and_a_single_boss_cooldown() {
        let mut world = arena();
        let rng = world.rng;
        for tick in 1..3600 {
            world.tick = tick;
            world.random_boss_event();
        }
        assert!(world.bosses.is_empty());
        assert_eq!(world.rng, rng);
        let mut arrivals = 0;
        for second in 60..10060 {
            world.tick = second * 60;
            world.random_boss_event();
            arrivals += world.bosses.len();
            world.bosses.clear();
        }
        assert!(
            (10..=45).contains(&arrivals),
            "0.25% aggregate roll produced {arrivals} arrivals in 10000 seconds"
        );
        assert_ne!(world.spawn_boss(BossKind::Summoner), 0);
        world.tick += 60;
        world.random_boss_event();
        assert_eq!(world.bosses.len(), 1);
        assert_eq!(world.boss_roll_at, world.tick + 90 * 60);
    }

    #[test]
    fn every_drone_boss_has_a_bounded_swarm_that_pursues_players() {
        for kind in BossKind::ALL
            .into_iter()
            .filter(|kind| kind.drone_limit() > 0)
        {
            let mut world = arena();
            world.config.mode = Mode::Sandbox;
            let player = world.tanks[0].id;
            world.sandbox_invulnerability(player, true);
            world.tanks[0].pos = V2::new(2450., 1300.);
            let id = world.spawn_boss(kind);
            assert_ne!(id, 0);
            world.bosses[0].pos = V2::new(1600., 1300.);
            for _ in 0..1200 {
                world.step();
                assert!(
                    world
                        .projectiles
                        .iter()
                        .filter(|p| p.owner == id && p.kind.is_drone())
                        .count()
                        <= kind.drone_limit()
                );
            }
            assert!(
                world
                    .projectiles
                    .iter()
                    .any(|p| p.owner == id && p.kind.is_drone())
            );
            assert_eq!(world.bosses[0].target, player);
        }
    }

    #[test]
    fn boss_movement_and_spawn_respect_borders_walls_and_protected_players() {
        let mut world = arena();
        world.config.arena_scale = 0.7;
        world.walls.push(Rect {
            x: 1750.,
            y: 600.,
            w: 100.,
            h: 1400.,
        });
        let id = world.spawn_boss(BossKind::FallenBooster);
        assert_ne!(id, 0);
        let boss = &world.bosses[0];
        assert!(world.position_clear(boss.pos, boss.radius));
        world.tanks[0].protection = 30.;
        for _ in 0..120 {
            world.update_bosses();
        }
        assert!(
            !world
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Shot))
        );
        assert!(
            world
                .config
                .bounds()
                .contains_circle(world.bosses[0].pos, world.bosses[0].radius)
        );
    }

    #[test]
    fn arrival_grace_separates_hulls_without_contact_damage_then_combat_resumes() {
        let mut world = arena();
        world.config.mode = Mode::Sandbox;
        world.spawn_boss(BossKind::Sentinel);
        world.tanks[0].pos = world.bosses[0].pos + V2::new(30., 0.);
        let health = world.bosses[0].health;
        world.step();
        assert_eq!(world.bosses[0].health, health);
        assert_eq!(world.tanks[0].health, world.tanks[0].max_health);
        assert!(
            world.tanks[0].pos.distance(world.bosses[0].pos)
                >= world.tanks[0].radius + world.bosses[0].radius - 0.1
        );
        world.bosses[0].arrival = 0.;
        world.tanks[0].pos = world.bosses[0].pos + V2::new(30., 0.);
        world.step();
        assert!(world.tanks[0].health < world.tanks[0].max_health);
        assert!(world.bosses[0].health < health);
    }
}
