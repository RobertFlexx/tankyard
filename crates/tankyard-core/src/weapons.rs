use crate::physics::{circle_rect, sweep_rect};
use crate::*;
use tankyard_content::{Ability, Barrel, ClassDef, FirePattern, ProjectileKind};

fn deployable_slot(kind: ProjectileKind) -> Option<usize> {
    match kind {
        ProjectileKind::Drone | ProjectileKind::Minion => Some(0),
        ProjectileKind::Trap => Some(1),
        ProjectileKind::Mine => Some(2),
        ProjectileKind::Barrier => Some(3),
        ProjectileKind::Turret => Some(4),
        _ => None,
    }
}

fn cadence_phase(seed: u64, cycle: u32) -> f32 {
    let sample = |epoch: u32| {
        let mut value = seed ^ u64::from(epoch).wrapping_mul(0x9e3779b97f4a7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((value ^ (value >> 31)) >> 40) as f32 / 16_777_216.0 - 0.5
    };
    let epoch = cycle / 8;
    let fraction = (cycle % 8) as f32 / 8.0;
    let blend = fraction * fraction * (3.0 - 2.0 * fraction);
    (sample(epoch) * (1.0 - blend) + sample(epoch.wrapping_add(1)) * blend) * 0.7
}

pub(crate) fn reload_rate(tank: &Tank, class: &ClassDef) -> f32 {
    stat_scale(tank.stats[6]) * class.multipliers[6] * if tank.boost > 0. { 1.45 } else { 1. }
}

pub(crate) fn reload_interval(
    tank: &Tank,
    class: &ClassDef,
    config: &WorldConfig,
    index: usize,
    previous: f32,
    dt: f32,
) -> f32 {
    let barrel = &class.barrels[index];
    let in_burst =
        barrel.pattern == FirePattern::Burst && !tank.barrel_shots[index].is_multiple_of(3);
    let period = (if in_burst { 0.085 } else { barrel.cooldown }) / reload_rate(tank, class);
    let mut interval = period;
    if !in_burst
        && matches!(
            barrel.pattern,
            FirePattern::Alternating | FirePattern::Burst
        )
        && class.barrels.iter().enumerate().any(|(other, mount)| {
            other != index
                && mount.pattern == barrel.pattern
                && mount.projectile == barrel.projectile
        })
    {
        let cycle = if barrel.pattern == FirePattern::Burst {
            tank.barrel_shots[index] / 3
        } else {
            tank.barrel_shots[index]
        };
        let seed = config.seed
            ^ tank.id.wrapping_mul(0x517cc1b727220a95)
            ^ (index as u64).wrapping_mul(0x6eed0e9da4d94a4f);
        // Phase moves gradually. Its bounded differences preserve the long-run rate.
        interval +=
            period * (cadence_phase(seed, cycle) - cadence_phase(seed, cycle.wrapping_sub(1)));
    }
    // Carry a partial tick from an active reload. Idle guns cannot bank shots.
    let remainder = if previous > 0.0 {
        (previous - dt).min(0.0)
    } else {
        0.0
    };
    (interval + remainder).max(0.0)
}

impl World {
    // A single internal constructor keeps all physical projectile properties explicit.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn simple_projectile(
        &mut self,
        owner: EntityId,
        team: u8,
        pos: V2,
        vel: V2,
        kind: ProjectileKind,
        damage: f32,
        radius: f32,
        lifetime: f32,
        penetration: f32,
    ) -> Projectile {
        Projectile {
            id: self.id(),
            owner,
            team,
            pos,
            previous: pos,
            vel,
            cruise_speed: vel.length(),
            radius,
            damage,
            penetration,
            lifetime,
            kind,
            ricochets: 0,
            age: 0.,
            hit: Vec::new(),
            gun: (kind == ProjectileKind::Minion).then_some(MinionGun {
                aim: vel.angle(),
                cooldown: 0.35,
                shots: 0,
            }),
        }
    }
    pub(crate) fn fire_weapons(&mut self) {
        let mut shots = Vec::new();
        let mut abilities = Vec::new();
        let mut events = Vec::new();
        for index in 0..self.tanks.len() {
            let tank = &self.tanks[index];
            if !tank.alive {
                continue;
            }
            let Some(class) = self.content.class(&tank.class) else {
                continue;
            };
            let mut auto_aims = [None; 16];
            for (i, b) in class.barrels.iter().enumerate() {
                if b.pattern == FirePattern::Auto
                    && (tank.cooldowns.get(i).copied().unwrap_or(0.) <= DT
                        || self
                            .tick
                            .wrapping_add(tank.id)
                            .wrapping_add(i as u64)
                            .is_multiple_of(6))
                {
                    auto_aims[i] = self.mount_aim(tank, b);
                }
            }
            let mut owned = [0_usize; 5];
            let deploying = class.barrels.iter().enumerate().any(|(i, barrel)| {
                deployable_slot(barrel.projectile).is_some()
                    && tank.cooldowns.get(i).copied().unwrap_or(barrel.delay) <= DT
                    && (barrel.projectile == ProjectileKind::Minion
                        || tank.input.fire
                        || (barrel.pattern == FirePattern::Auto && auto_aims[i].is_some())
                        || (barrel.pattern == FirePattern::Charge && tank.charge > 0.15))
            });
            if deploying {
                for projectile in self
                    .projectiles
                    .iter()
                    .filter(|p| p.owner == tank.id && p.lifetime > 0.0 && p.penetration > 0.0)
                {
                    if let Some(slot) = deployable_slot(projectile.kind) {
                        owned[slot] += 1;
                    }
                }
            }
            let tank = &mut self.tanks[index];
            let Some(class) = self.content.class(&tank.class) else {
                continue;
            };
            if tank.input.ability && tank.ability_cooldown <= 0. && class.ability != Ability::None {
                tank.ability_cooldown = class.ability_cooldown;
                abilities.push((index, class.ability));
            }
            if tank.input.fire {
                tank.protection = 0.;
            }
            let reload = reload_rate(tank, class);
            if tank.cooldowns.len() != class.barrels.len()
                || tank.barrel_shots.len() != class.barrels.len()
                || tank.barrel_aims.len() != class.barrels.len()
            {
                tank.cooldowns = class.barrels.iter().map(|b| b.delay / reload).collect();
                tank.barrel_shots = vec![0; class.barrels.len()];
                tank.barrel_aims = class.barrels.iter().map(|b| tank.aim + b.angle).collect();
            }
            let previous_charge = tank.charge;
            if tank.input.fire {
                tank.charge = (tank.charge + DT).min(1.5);
            }
            let charged = tank.charge >= 1. || (!tank.input.fire && previous_charge > 0.15);
            let mut ready_launchers = [0_usize; 5];
            if deploying {
                for (i, barrel) in class.barrels.iter().enumerate() {
                    let firing = if barrel.pattern == FirePattern::Charge {
                        charged
                    } else {
                        barrel.projectile == ProjectileKind::Minion
                            || tank.input.fire
                            || (barrel.pattern == FirePattern::Auto && auto_aims[i].is_some())
                    };
                    if firing
                        && tank.cooldowns[i] <= DT
                        && let Some(slot) = deployable_slot(barrel.projectile)
                    {
                        ready_launchers[slot] += 1;
                    }
                }
            }
            for (barrel_index, barrel) in class.barrels.iter().enumerate() {
                let previous_cooldown = tank.cooldowns[barrel_index];
                tank.cooldowns[barrel_index] = (previous_cooldown - DT).max(0.0);
                let automatic = barrel.pattern == FirePattern::Auto;
                let auto_aim = auto_aims[barrel_index];
                if automatic && let Some(aim) = auto_aim {
                    tank.barrel_aims[barrel_index] = aim;
                }
                let fire = if barrel.pattern == FirePattern::Charge {
                    charged
                } else {
                    barrel.projectile == ProjectileKind::Minion
                        || tank.input.fire
                        || (automatic && auto_aim.is_some())
                };
                if !fire || tank.cooldowns[barrel_index] > 0. {
                    continue;
                }
                let slot = deployable_slot(barrel.projectile);
                let deployable = slot.is_some();
                let room = self
                    .config
                    .max_projectiles
                    .saturating_sub(self.projectiles.len() + shots.len());
                let mut count = usize::from(barrel.count.max(1)).min(room);
                if let Some(slot) = slot {
                    // One damn pool per owner. Factory minions and triangle drones
                    // both spend the same slots, including pellets waiting to spawn.
                    let remaining = [usize::from(class.drone_limit), 24, 24, 8, 6][slot]
                        .saturating_sub(owned[slot]);
                    // Save room for the other ready ports so one greedy barrel
                    // doesn't hog the whole damn squad on this tick.
                    count = count.min(remaining.div_ceil(ready_launchers[slot].max(1)));
                    ready_launchers[slot] = ready_launchers[slot].saturating_sub(1);
                    if count == 0 {
                        tank.cooldowns[barrel_index] = 0.2;
                        continue;
                    }
                    owned[slot] += count;
                }
                if count == 0 {
                    continue;
                }
                let mut aim = tank.aim + barrel.angle;
                if automatic && let Some(target) = auto_aim {
                    aim = target;
                }
                if barrel.pattern == FirePattern::Spin {
                    aim += self.tick as f32 * DT * 1.7;
                }
                tank.barrel_aims[barrel_index] = aim;
                tank.protection = 0.;
                let local = V2::new(barrel.offset[0], barrel.offset[1]).rotate(tank.aim);
                let mut origin =
                    tank.pos + local + V2::from_angle(aim) * (barrel.length + tank.radius * 0.45);
                let charge = if barrel.pattern == FirePattern::Charge {
                    0.5 + tank.charge * 1.7
                } else {
                    1.
                };
                // A barrel beside a thin wall cannot spawn a bullet through it.
                let mount = tank.pos + local;
                let muzzle = origin;
                let mut contact: f32 = 1.0;
                for &wall in &self.walls {
                    if let Some((time, _)) =
                        sweep_rect(mount, muzzle, barrel.radius * charge.sqrt(), wall)
                    {
                        contact = contact.min(time);
                    }
                }
                origin = mount.lerp(muzzle, contact);
                for pellet in 0..count {
                    let spread = if count == 1 {
                        0.
                    } else {
                        (pellet as f32 / (count - 1) as f32 - 0.5) * barrel.spread
                    };
                    let speed = barrel.speed * stat_scale(tank.stats[3]) * class.multipliers[3];
                    let damage = barrel.damage
                        * stat_scale(tank.stats[5])
                        * class.multipliers[5]
                        * charge
                        * self.config.rules.damage_multiplier;
                    let penetration =
                        barrel.penetration * stat_scale(tank.stats[4]) * class.multipliers[4];
                    let velocity = V2::from_angle(aim + spread) * speed
                        + if deployable {
                            V2::ZERO
                        } else {
                            tank.vel * 0.16
                        };
                    shots.push((
                        tank.id,
                        tank.team,
                        origin,
                        velocity,
                        barrel.projectile,
                        damage,
                        barrel.radius * charge.sqrt(),
                        barrel.lifetime,
                        penetration,
                        barrel.ricochets,
                    ));
                }
                tank.vel -= V2::from_angle(aim) * barrel.recoil;
                tank.barrel_shots[barrel_index] = tank.barrel_shots[barrel_index].wrapping_add(1);
                tank.shot_count = tank.shot_count.wrapping_add(1);
                tank.cooldowns[barrel_index] = reload_interval(
                    tank,
                    class,
                    &self.config,
                    barrel_index,
                    previous_cooldown,
                    DT,
                );
                events.push((origin, tank.id, tank.team, barrel.projectile as u32));
            }
            if charged || !tank.input.fire {
                tank.charge = 0.;
            }
        }
        let room = self
            .config
            .max_projectiles
            .saturating_sub(self.projectiles.len());
        for (owner, team, pos, vel, kind, damage, radius, lifetime, penetration, ricochets) in
            shots.into_iter().take(room)
        {
            let mut shot = self.simple_projectile(
                owner,
                team,
                pos,
                vel,
                kind,
                damage,
                radius,
                lifetime,
                penetration,
            );
            shot.ricochets = ricochets;
            self.projectiles.push(shot);
        }
        for (pos, id, team, value) in events {
            self.event(EventKind::Shot, pos, id, value, team);
        }
        for (index, ability) in abilities {
            self.activate_ability(index, ability);
        }
    }
    fn mount_aim(&self, tank: &Tank, barrel: &Barrel) -> Option<f32> {
        let mount = tank.pos + V2::new(barrel.offset[0], barrel.offset[1]).rotate(tank.aim);
        let team = tank.team;
        let owner = tank.id;
        let forward = V2::from_angle(tank.aim + barrel.angle);
        let minimum_alignment = barrel.target_arc.map(|arc| (arc * 0.5).cos());
        let mut best = f32::MAX;
        let mut result = None;
        let mut candidate = |pos: V2, priority: f32| {
            let delta = pos - mount;
            let distance_squared = delta.length_squared();
            if distance_squared > 900.0 * 900.0 {
                return;
            }
            let distance = distance_squared.sqrt();
            let alignment = forward.dot(delta) / distance.max(0.00001);
            if minimum_alignment.is_some_and(|minimum| alignment < minimum) {
                return;
            }
            let score = distance + 180. * (1. - alignment) + priority;
            if score < best
                && self
                    .walls
                    .iter()
                    .all(|&wall| sweep_rect(mount, pos, barrel.radius, wall).is_none())
            {
                best = score;
                result = Some(delta.angle());
            }
        };
        for tank in &self.tanks {
            if tank.alive
                && tank.id != owner
                && tank.protection <= 0.
                && self.hostile(team, tank.team)
            {
                candidate(tank.pos, 0.);
            }
        }
        for boss in &self.bosses {
            if boss.health > 0. && boss.id != owner && self.hostile(team, boss.team) {
                candidate(boss.pos, 40.);
            }
        }
        for target in &self.targets {
            if target.health > 0. {
                candidate(target.pos, 300.);
            }
        }
        result
    }
    fn activate_ability(&mut self, index: usize, ability: Ability) {
        let (pos, id, team, aim) = {
            let t = &self.tanks[index];
            (t.pos, t.id, t.team, t.aim)
        };
        crate::physics::apply_personal_ability(
            &mut self.tanks[index],
            ability,
            &self.config,
            &self.walls,
        );
        match ability {
            Ability::None
            | Ability::Dash
            | Ability::Shield
            | Ability::Cloak
            | Ability::Overdrive
            | Ability::Blink => {}
            Ability::Heal => {
                let heal = self.tanks[index].max_health * 0.27;
                for t in &mut self.tanks {
                    if t.alive
                        && (t.id == id || team > 0 && t.team == team)
                        && t.pos.distance(pos) < 330.
                    {
                        t.health = (t.health + heal).min(t.max_health);
                    }
                }
            }
            Ability::Nova => {
                self.tanks[index].protection = 0.;
                self.rebuild_grid_for_ability();
                self.explode(pos, 250., 65., id, team);
            }
            Ability::Recall => {
                for p in &mut self.projectiles {
                    if p.owner == id && p.kind.is_drone() {
                        p.pos = pos + V2::from_angle(p.id as f32) * 65.;
                        p.vel = V2::from_angle(aim) * 300.;
                        p.hit.clear();
                        p.penetration += 1.;
                    }
                }
            }
            Ability::Fortify => {
                let p = self.simple_projectile(
                    id,
                    team,
                    pos,
                    V2::ZERO,
                    ProjectileKind::Barrier,
                    20.,
                    48.,
                    6.,
                    12.,
                );
                if self.projectiles.len() < self.config.max_projectiles {
                    self.projectiles.push(p);
                }
            }
            Ability::Magnet => {
                self.tanks[index].protection = 0.;
                for t in &mut self.targets {
                    let d = pos - t.pos;
                    if d.length() < 500. {
                        t.vel += d.normalized() * 160.;
                    }
                }
                for p in &mut self.projectiles {
                    if p.owner != id && (team == 0 || p.team != team) && p.pos.distance(pos) < 240.
                    {
                        p.vel = (p.pos - pos).normalized() * p.vel.length();
                    }
                }
            }
        }
        self.event(EventKind::Ability, pos, id, ability as u32, team);
    }
    fn rebuild_grid_for_ability(&mut self) {
        self.grid.clear();
        for (i, t) in self.tanks.iter().enumerate() {
            if t.alive {
                self.grid.insert(t.pos, crate::physics::Body::Tank(i));
            }
        }
        for (i, t) in self.targets.iter().enumerate() {
            if t.health > 0. {
                self.grid.insert(t.pos, crate::physics::Body::Target(i));
            }
        }
        for (i, b) in self.bosses.iter().enumerate() {
            if b.health > 0. {
                self.grid.insert(b.pos, crate::physics::Body::Boss(i));
            }
        }
    }
    pub fn spawn_boss(&mut self, kind: BossKind) -> EntityId {
        let id = self.id();
        let center = V2::new(self.config.width * 0.5, self.config.height * 0.5);
        let health = 1800. + f32::from(self.match_state.boss_wave.max(1)) * 350.;
        let mut pos = center;
        for _ in 0..32 {
            if self
                .walls
                .iter()
                .all(|&w| circle_rect(pos, 125., w).is_none())
            {
                break;
            }
            pos = V2::new(
                200. + self.random() * (self.config.width - 400.),
                200. + self.random() * (self.config.height - 400.),
            );
        }
        self.bosses.push(Boss {
            id,
            pos,
            vel: V2::ZERO,
            radius: 90.,
            health,
            max_health: health,
            kind,
            phase: 1,
            timer: 2.,
            aim: 0.,
            team: 0,
            mothership: false,
        });
        self.event(EventKind::BossSpawn, pos, id, kind as u32, 0);
        id
    }
    pub(crate) fn update_bosses(&mut self) {
        let mut volleys = Vec::new();
        for i in 0..self.bosses.len() {
            let b = &self.bosses[i];
            if b.health <= 0. {
                continue;
            }
            let target = self
                .tanks
                .iter()
                .filter(|t| t.alive && self.hostile(b.team, t.team) && t.protection <= 0.)
                .min_by(|a, c| a.pos.distance(b.pos).total_cmp(&c.pos.distance(b.pos)))
                .map(|t| (t.pos, t.vel));
            let Some((target, velocity)) = target else {
                continue;
            };
            let b = &mut self.bosses[i];
            b.phase = if b.health < b.max_health * 0.3 {
                3
            } else if b.health < b.max_health * 0.65 {
                2
            } else {
                1
            };
            let to = target - b.pos;
            b.aim = (to + velocity * 0.35).angle();
            let speed = if b.mothership {
                28.
            } else {
                match b.kind {
                    BossKind::Tempest => 125.,
                    BossKind::Eclipse => 95.,
                    BossKind::Foundry | BossKind::Bastion => 32.,
                    _ => 55.,
                }
            };
            let desired = if to.length() > 450. {
                to.normalized() * speed
            } else {
                V2::new(-to.y, to.x).normalized() * speed * 0.7
            };
            b.vel = b.vel.lerp(desired, DT * 2.);
            let next = b.pos + b.vel * DT;
            if self
                .walls
                .iter()
                .all(|&w| circle_rect(next, b.radius, w).is_none())
            {
                b.pos = next;
            }
            b.pos.x = b.pos.x.clamp(b.radius, self.config.width - b.radius);
            b.pos.y = b.pos.y.clamp(b.radius, self.config.height - b.radius);
            b.timer -= DT;
            if b.timer > 0. {
                continue;
            }
            let mut emit = |angle: f32,
                            kind: ProjectileKind,
                            speed: f32,
                            damage: f32,
                            radius: f32,
                            life: f32,
                            pen: f32,
                            bounces: u8| {
                volleys.push((
                    b.id,
                    b.team,
                    b.pos + V2::from_angle(angle) * (b.radius + 10.),
                    V2::from_angle(angle) * speed,
                    kind,
                    damage,
                    radius,
                    life,
                    pen,
                    bounces,
                ));
            };
            match b.kind {
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
                            5.,
                            2.,
                            2,
                        );
                    }
                    b.timer = 1.3;
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
                    for n in 0..(3 + b.phase) {
                        emit(
                            b.aim + (n as f32 - 1.) * 0.45,
                            ProjectileKind::Homing,
                            285.,
                            14.,
                            7.,
                            4.,
                            1.,
                            0,
                        );
                    }
                    b.vel += to.normalized() * 120.;
                    b.timer = 0.9;
                }
                BossKind::Bastion => {
                    for n in 0..8 {
                        emit(
                            n as f32 * std::f32::consts::TAU / 8. + self.tick as f32 * 0.005,
                            ProjectileKind::Mine,
                            220.,
                            32.,
                            13.,
                            8.,
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
                    b.vel += V2::from_angle(b.aim) * 180.;
                }
            }
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
