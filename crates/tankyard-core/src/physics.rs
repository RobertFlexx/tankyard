//! Lightweight collision and prediction shared by clients and authoritative servers.
use crate::{Rect, Tank, V2, WorldConfig, stat_scale};
use tankyard_content::Content;

/// Earliest intersection of a moving circle with a stationary circle.
pub fn sweep_circle(start: V2, end: V2, center: V2, radius: f32) -> Option<f32> {
    let m = start - center;
    let d = end - start;
    let c = m.length_squared() - radius * radius;
    if c <= 0. {
        return Some(0.);
    }
    let a = d.length_squared();
    if a < 1e-10 {
        return None;
    }
    let b = m.dot(d);
    if b >= 0. {
        return None;
    }
    let discriminant = b * b - a * c;
    if discriminant < 0. {
        return None;
    }
    let t = (-b - discriminant.sqrt()) / a;
    (0. ..=1.).contains(&t).then_some(t)
}
pub fn circle_rect(pos: V2, r: f32, rect: Rect) -> Option<(V2, f32)> {
    let nearest = V2::new(
        pos.x.clamp(rect.x, rect.x + rect.w),
        pos.y.clamp(rect.y, rect.y + rect.h),
    );
    let d = pos - nearest;
    let squared = d.length_squared();
    if squared >= r * r {
        return None;
    }
    let l = squared.sqrt();
    if l > 0.00001 {
        return Some((d / l, r - l));
    }
    let candidates = [
        (V2::new(-1., 0.), pos.x - rect.x),
        (V2::new(1., 0.), rect.x + rect.w - pos.x),
        (V2::new(0., -1.), pos.y - rect.y),
        (V2::new(0., 1.), rect.y + rect.h - pos.y),
    ];
    let (n, dist) = candidates.into_iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
    Some((n, r + dist))
}
/// Swept circle against an axis aligned rectangle. Rounded corners are checked
/// exactly rather than treating the expanded rectangle corners as solid squares.
pub fn sweep_rect(start: V2, end: V2, r: f32, rect: Rect) -> Option<(f32, V2)> {
    // Most walls are nowhere near this tick's motion. Reject the swept bounds
    // before testing faces and rounded corners, including for AI sight lines.
    if start.x.max(end.x) < rect.x - r
        || start.x.min(end.x) > rect.x + rect.w + r
        || start.y.max(end.y) < rect.y - r
        || start.y.min(end.y) > rect.y + rect.h + r
    {
        return None;
    }
    if let Some((n, _)) = circle_rect(start, r, rect) {
        return Some((0., n));
    }
    let d = end - start;
    let mut best: Option<(f32, V2)> = None;
    let mut candidate = |t: f32, n: V2| {
        if (0. ..=1.).contains(&t) && best.is_none_or(|b| t < b.0) {
            best = Some((t, n));
        }
    };
    if d.x.abs() > 1e-8 {
        for (x, n) in [
            (rect.x - r, V2::new(-1., 0.)),
            (rect.x + rect.w + r, V2::new(1., 0.)),
        ] {
            let t = (x - start.x) / d.x;
            let y = start.y + d.y * t;
            if y >= rect.y && y <= rect.y + rect.h {
                candidate(t, n);
            }
        }
    }
    if d.y.abs() > 1e-8 {
        for (y, n) in [
            (rect.y - r, V2::new(0., -1.)),
            (rect.y + rect.h + r, V2::new(0., 1.)),
        ] {
            let t = (y - start.y) / d.y;
            let x = start.x + d.x * t;
            if x >= rect.x && x <= rect.x + rect.w {
                candidate(t, n);
            }
        }
    }
    for c in [
        V2::new(rect.x, rect.y),
        V2::new(rect.x + rect.w, rect.y),
        V2::new(rect.x, rect.y + rect.h),
        V2::new(rect.x + rect.w, rect.y + rect.h),
    ] {
        if let Some(t) = sweep_circle(start, end, c, r) {
            candidate(t, (start + d * t - c).normalized());
        }
    }
    best
}
pub fn polygon_vertices(center: V2, r: f32, sides: u8, angle: f32) -> Vec<V2> {
    (0..sides)
        .map(|i| {
            center + V2::from_angle(angle + i as f32 * std::f32::consts::TAU / sides as f32) * r
        })
        .collect()
}
/// Fill caller-owned storage for collision geometry without allocating.
pub fn polygon_vertices_into(center: V2, r: f32, angle: f32, vertices: &mut [V2]) {
    let step = std::f32::consts::TAU / vertices.len().max(1) as f32;
    for (index, vertex) in vertices.iter_mut().enumerate() {
        *vertex = center + V2::from_angle(angle + index as f32 * step) * r;
    }
}
pub fn circle_polygon(p: V2, r: f32, vertices: &[V2]) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    let mut inside = true;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let edge = b - a;
        let rel = p - a;
        if edge.x * rel.y - edge.y * rel.x < 0. {
            inside = false;
        }
        let t = (rel.dot(edge) / edge.length_squared().max(1e-8)).clamp(0., 1.);
        if p.distance(a + edge * t) < r {
            return true;
        }
    }
    inside
}
pub fn sweep_polygon(start: V2, end: V2, r: f32, vertices: &[V2]) -> Option<f32> {
    if circle_polygon(start, r, vertices) {
        return Some(0.);
    }
    let d = end - start;
    let mut best = None;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let edge = b - a;
        let normal = V2::new(edge.y, -edge.x).normalized();
        let denom = d.dot(normal);
        if denom < -1e-8 {
            let t = (r - (start - a).dot(normal)) / denom;
            let at = start + d * t - normal * r;
            let u = (at - a).dot(edge) / edge.length_squared().max(1e-8);
            if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u) && best.is_none_or(|v| t < v) {
                best = Some(t);
            }
        }
        if let Some(t) = sweep_circle(start, end, a, r)
            && best.is_none_or(|v| t < v)
        {
            best = Some(t);
        }
    }
    best
}
pub fn tank_speed(tank: &Tank, content: &Content) -> f32 {
    let mult = content.class(&tank.class).map_or(1., |c| c.multipliers[7]);
    210. * mult * stat_scale(tank.stats[7]) * tank.speed_scale.clamp(0.25, 6.)
}
/// Uses the same acceleration/drag and wall constraints for client prediction.
pub fn move_tank(
    tank: &mut Tank,
    content: &Content,
    config: &WorldConfig,
    walls: &[Rect],
    dt: f32,
) {
    if !tank.alive {
        return;
    }
    let speed = tank_speed(tank, content)
        * config.rules.movement_multiplier
        * (if tank.boost > 0. { 1.55 } else { 1. })
        * (if tank.carrying { 0.75 } else { 1. });
    let desired = tank.input.movement.clamp_length(1.) * speed;
    // Acceleration and coast are exponential, so prediction and the server
    // agree at every frame rate. Keep enough inertia for recoil and dodging.
    let rate = if tank.knockback > 0. {
        1.6
    } else if desired.length_squared() > 0.1 {
        config.rules.acceleration
    } else {
        config.rules.coast
    };
    tank.vel = tank
        .vel
        .lerp(desired, 1. - (-rate * dt).exp())
        .clamp_length(if tank.knockback > 0. {
            1200.
        } else {
            speed * 3.
        });
    // Resolve initial penetrations first (spawns, map edits, remote correction).
    // Keeping the simulation finite prevents one bad collision from affecting
    // all clients and makes prediction use identical authoritative rules.
    for _ in 0..3 {
        let mut resolved = false;
        for wall in walls {
            if let Some((normal, depth)) = circle_rect(tank.pos, tank.radius, *wall) {
                tank.pos += normal * (depth + 0.01);
                tank.vel -= normal * tank.vel.dot(normal).min(0.);
                resolved = true;
            }
        }
        if !resolved {
            break;
        }
    }
    // Continuous swept contact followed by tangential sliding. Unlike a
    // single impact-and-stop, a diagonal approach continues along the wall.
    let mut remaining = tank.vel * dt.clamp(0.0, 0.1);
    for _ in 0..4 {
        if remaining.length_squared() < 1e-8 {
            break;
        }
        let start = tank.pos;
        let end = start + remaining;
        let mut first: Option<(f32, V2)> = None;
        for &wall in walls {
            if let Some((time, normal)) = sweep_rect(start, end, tank.radius, wall) {
                // An exact tangent is not a collision if we are departing.
                if remaining.dot(normal) >= -1e-6 {
                    continue;
                }
                if first.is_none_or(|hit| time < hit.0) {
                    first = Some((time, normal));
                }
            }
        }
        if let Some((time, normal)) = first {
            let travel = (time - 0.0001).max(0.0);
            tank.pos += remaining * travel;
            let leftover = remaining * (1.0 - time);
            remaining = leftover - normal * leftover.dot(normal).min(0.);
            tank.vel -= normal * tank.vel.dot(normal).min(0.);
        } else {
            tank.pos = end;
            break;
        }
    }
    constrain_tank(tank, config, &[]);
    tank.aim = tank.input.aim;
    if !tank.pos.finite() || !tank.vel.finite() {
        tank.pos = V2::new(config.width * 0.5, config.height * 0.5);
        tank.vel = V2::ZERO;
    }
}

/// Contacts can push a hull into a wall or beyond the arena after movement.
/// Apply the same positional constraints without adding another movement step.
pub fn constrain_tank(tank: &mut Tank, config: &WorldConfig, walls: &[Rect]) {
    for _ in 0..3 {
        let mut resolved = false;
        for &wall in walls {
            if let Some((normal, depth)) = circle_rect(tank.pos, tank.radius, wall) {
                tank.pos += normal * (depth + 0.01);
                tank.vel -= normal * tank.vel.dot(normal).min(0.0);
                resolved = true;
            }
        }
        if !resolved {
            break;
        }
    }
    let bounds = config.bounds();
    let clamped = bounds.clamp_circle(tank.pos, tank.radius);
    if tank.pos.x < clamped.x {
        tank.vel.x = tank.vel.x.max(0.);
    }
    if tank.pos.x > clamped.x {
        tank.vel.x = tank.vel.x.min(0.);
    }
    if tank.pos.y < clamped.y {
        tank.vel.y = tank.vel.y.max(0.);
    }
    if tank.pos.y > clamped.y {
        tank.vel.y = tank.vel.y.min(0.);
    }
    tank.pos = clamped;
    if let Some(clear) = clear_circle_position(tank.pos, tank.radius, bounds, walls) {
        tank.pos = clear;
    }
}

/// Resolve a moving border pinching a circle against a fixed map wall.
pub fn clear_circle_position(pos: V2, radius: f32, bounds: Rect, walls: &[Rect]) -> Option<V2> {
    let pos = bounds.clamp_circle(pos, radius);
    let valid = |p: V2| {
        bounds.contains_circle(p, radius)
            && walls
                .iter()
                .all(|&wall| circle_rect(p, radius, wall).is_none())
    };
    if valid(pos) {
        return Some(pos);
    }
    let mut best: Option<V2> = None;
    // Resolve the actual contact first. A long maze may have thousands of
    // earlier, distant walls; using only those can teleport a hull across it.
    for contacts_only in [true, false] {
        for wall in walls
            .iter()
            .filter(|&&wall| !contacts_only || circle_rect(pos, radius, wall).is_some())
            .take(256)
        {
            let left = wall.x - radius - 0.1;
            let right = wall.x + wall.w + radius + 0.1;
            let top = wall.y - radius - 0.1;
            let bottom = wall.y + wall.h + radius + 0.1;
            for p in [
                V2::new(left, pos.y),
                V2::new(right, pos.y),
                V2::new(pos.x, top),
                V2::new(pos.x, bottom),
                V2::new(left, top),
                V2::new(left, bottom),
                V2::new(right, top),
                V2::new(right, bottom),
            ] {
                let p = bounds.clamp_circle(p, radius);
                if valid(p) && best.is_none_or(|b| p.distance(pos) < b.distance(pos)) {
                    best = Some(p);
                }
            }
        }
        if best.is_some() {
            break;
        }
    }
    best
}

/// Predict movement, personal ability effects, and manual cannon recoil.
/// Automatic targeting, world damage, and deployable limits come from the server.
pub fn predict_tank(
    tank: &mut Tank,
    content: &Content,
    config: &WorldConfig,
    walls: &[Rect],
    tick: u64,
    dt: f32,
) {
    use tankyard_content::{Ability, FirePattern, ProjectileKind};
    if !tank.alive {
        return;
    }
    tank.boost = (tank.boost - dt).max(0.0);
    tank.knockback = (tank.knockback - dt).max(0.);
    tank.stasis = (tank.stasis - dt).max(0.);
    tank.ability_cooldown = (tank.ability_cooldown - dt).max(0.0);
    tank.shield = (tank.shield - dt).max(0.0);
    update_protection(tank, dt);
    move_tank(tank, content, config, walls, dt);
    if tank.spawn_immunity && tank.protection > 0. {
        return;
    }
    let Some(class) = content.class(&tank.class) else {
        return;
    };
    let activate =
        tank.input.ability && tank.ability_cooldown <= 0.0 && class.ability != Ability::None;
    if activate {
        tank.ability_cooldown = class.ability_cooldown;
    }
    if tank.cooldowns.len() != class.barrels.len()
        || tank.barrel_shots.len() != class.barrels.len()
        || tank.barrel_aims.len() != class.barrels.len()
    {
        let reload = crate::weapons::reload_rate(tank, class);
        tank.cooldowns = class
            .barrels
            .iter()
            .map(|barrel| barrel.delay / reload)
            .collect();
        tank.barrel_shots = vec![0; class.barrels.len()];
        tank.barrel_aims = class
            .barrels
            .iter()
            .map(|barrel| tank.aim + barrel.angle)
            .collect();
    }
    tank.barrel_shots.resize(class.barrels.len(), 0);
    let previous_charge = tank.charge;
    if tank.input.fire {
        tank.charge = (tank.charge + dt).min(1.5);
    }
    let charged = tank.charge >= 1.0 || (!tank.input.fire && previous_charge > 0.);
    let mut ready_defenses = [0_usize; 4];
    for (index, barrel) in class.barrels.iter().enumerate() {
        if barrel.pattern != FirePattern::Auto
            && tank.cooldowns[index] <= dt
            && (if barrel.pattern == FirePattern::Charge {
                charged
            } else {
                tank.input.fire
            })
            && let Some(slot) = crate::weapons::defense_slot(barrel.projectile)
        {
            ready_defenses[slot] += 1;
        }
    }
    for (index, barrel) in class.barrels.iter().enumerate() {
        let previous_cooldown = tank.cooldowns[index];
        tank.cooldowns[index] = (previous_cooldown - dt).max(0.0);
        if barrel.pattern == FirePattern::Auto
            || matches!(
                barrel.projectile,
                ProjectileKind::Drone | ProjectileKind::Minion
            )
        {
            continue;
        }
        let firing = if barrel.pattern == FirePattern::Charge {
            charged
        } else {
            tank.input.fire
        };
        if !firing || tank.cooldowns[index] > 0.0 {
            continue;
        }
        if let Some(slot) = crate::weapons::defense_slot(barrel.projectile) {
            let room = [24_u8, 24, 8, 6][slot].saturating_sub(tank.deployed_defenses[slot]);
            let count = usize::from(barrel.count.max(1))
                .min(usize::from(room).div_ceil(ready_defenses[slot].max(1)));
            ready_defenses[slot] = ready_defenses[slot].saturating_sub(1);
            if count == 0 {
                tank.cooldowns[index] = 0.2;
                continue;
            }
            tank.deployed_defenses[slot] += count as u8;
        }
        let angle = tank.aim
            + barrel.angle
            + if barrel.pattern == FirePattern::Spin {
                tick as f32 * crate::DT * 1.7
            } else {
                0.0
            };
        tank.vel -= V2::from_angle(angle) * barrel.recoil;
        tank.barrel_shots[index] = tank.barrel_shots[index].wrapping_add(1);
        tank.barrel_aims[index] = angle;
        tank.cooldowns[index] =
            crate::weapons::reload_interval(tank, class, config, index, previous_cooldown, dt);
    }
    if charged || !tank.input.fire {
        tank.charge = 0.0;
    }
    if activate {
        apply_personal_ability(tank, class.ability, config, walls);
    }
    constrain_tank(tank, config, walls);
}

/// Shared actor effects; world damage, healing and spawned entities stay authoritative.
pub(crate) fn apply_personal_ability(
    tank: &mut Tank,
    ability: tankyard_content::Ability,
    config: &WorldConfig,
    walls: &[Rect],
) {
    use tankyard_content::Ability;
    if ability != Ability::None {
        tank.spawn_immunity = false;
        tank.protection = 0.0;
    }
    match ability {
        Ability::Dash => {
            tank.vel += V2::from_angle(tank.aim) * 680.;
            tank.boost = 0.6;
        }
        Ability::Shield => tank.shield = 3.5,
        Ability::Stasis => tank.stasis = crate::STASIS_SECONDS,
        Ability::Parry | Ability::CounterParry | Ability::VolleyParry => {
            tank.shield = ability.parry_window().unwrap_or(0.)
        }
        Ability::Recall => tank.shield = 1.,
        Ability::Cloak => {
            tank.protection = 2.;
            tank.boost = 1.5;
        }
        Ability::Overdrive => {
            tank.boost = 4.;
            tank.cooldowns.fill(0.);
        }
        Ability::Blink => {
            let desired = tank.pos + V2::from_angle(tank.aim) * 260.;
            if config.bounds().contains_circle(desired, 30.)
                && walls
                    .iter()
                    .all(|&wall| sweep_rect(tank.pos, desired, tank.radius, wall).is_none())
            {
                tank.pos = desired;
                tank.shield = 0.5;
            }
        }
        Ability::Fortify => {
            tank.shield = 6.;
            tank.vel *= 0.3;
        }
        Ability::None | Ability::Heal | Ability::Nova | Ability::Magnet => {}
    }
}

pub(crate) fn update_protection(tank: &mut Tank, dt: f32) {
    let moving = tank.input.movement.length_squared() > 0.0;
    if (tank.spawn_immunity && moving)
        || (!tank.spawn_immunity && tank.input.fire)
        || tank.protection <= dt + 0.001
    {
        tank.protection = 0.0;
    } else {
        tank.protection -= dt;
    }
    tank.spawn_immunity &= tank.protection > 0.0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Body {
    Tank(usize),
    Target(usize),
    Boss(usize),
    Deployable(usize),
}
pub(crate) struct SpatialGrid<T = Body> {
    cols: usize,
    rows: usize,
    cell: f32,
    buckets: Vec<Vec<T>>,
    occupied: Vec<usize>,
}
impl<T: Copy> SpatialGrid<T> {
    pub fn new(width: f32, height: f32) -> Self {
        let cell = 160.;
        let cols = (width / cell).ceil() as usize;
        let rows = (height / cell).ceil() as usize;
        Self {
            cols,
            rows,
            cell,
            buckets: (0..cols * rows).map(|_| Vec::new()).collect(),
            occupied: Vec::with_capacity(128),
        }
    }
    pub fn clear(&mut self) {
        for index in self.occupied.drain(..) {
            self.buckets[index].clear();
        }
    }
    pub fn insert(&mut self, pos: V2, body: T) {
        let x = (pos.x / self.cell).max(0.) as usize;
        let y = (pos.y / self.cell).max(0.) as usize;
        let index = y.min(self.rows - 1) * self.cols + x.min(self.cols - 1);
        if self.buckets[index].is_empty() {
            self.occupied.push(index);
        }
        self.buckets[index].push(body);
    }
    pub fn insert_rect(&mut self, rect: Rect, body: T) {
        let minx = ((rect.x / self.cell).max(0.0) as usize).min(self.cols - 1);
        let miny = ((rect.y / self.cell).max(0.0) as usize).min(self.rows - 1);
        let maxx = (((rect.x + rect.w) / self.cell).max(0.0) as usize).min(self.cols - 1);
        let maxy = (((rect.y + rect.h) / self.cell).max(0.0) as usize).min(self.rows - 1);
        for y in miny..=maxy {
            for x in minx..=maxx {
                let index = y * self.cols + x;
                if self.buckets[index].is_empty() {
                    self.occupied.push(index);
                }
                self.buckets[index].push(body);
            }
        }
    }
    pub fn query(&self, a: V2, b: V2, pad: f32, out: &mut Vec<T>) {
        out.clear();
        let minx = ((a.x.min(b.x) - pad) / self.cell).max(0.) as usize;
        let miny = ((a.y.min(b.y) - pad) / self.cell).max(0.) as usize;
        let maxx = ((a.x.max(b.x) + pad) / self.cell).max(0.) as usize;
        let maxy = ((a.y.max(b.y) + pad) / self.cell).max(0.) as usize;
        for y in miny.min(self.rows - 1)..=maxy.min(self.rows - 1) {
            for x in minx.min(self.cols - 1)..=maxx.min(self.cols - 1) {
                out.extend_from_slice(&self.buckets[y * self.cols + x]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolving_contact_uses_a_nearby_wall_even_after_hundreds_of_distant_walls() {
        let mut walls = vec![
            Rect {
                x: 1500.,
                y: 1500.,
                w: 20.,
                h: 20.
            };
            300
        ];
        walls.push(Rect {
            x: 500.,
            y: 500.,
            w: 100.,
            h: 100.,
        });
        let start = V2::new(550., 550.);
        let clear = clear_circle_position(
            start,
            20.,
            Rect {
                x: 0.,
                y: 0.,
                w: 2000.,
                h: 2000.,
            },
            &walls,
        )
        .unwrap();
        assert!(clear.distance(start) < 71.);
        assert!(
            walls
                .iter()
                .all(|&wall| circle_rect(clear, 20., wall).is_none())
        );
    }
    #[test]
    fn aiming_auto_fire_and_abilities_cannot_spend_or_cancel_stationary_spawn_immunity() {
        let mut world = crate::World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let player = world.add_player("Stationary", false);
        world.tanks[0].class = "riposte".into();
        for tick in 1..120 {
            world.tanks[0].input = crate::PlayerInput {
                fire: true,
                ability: true,
                aim: tick as f32 * 0.7,
                ..Default::default()
            };
            world.step();
            world.damage_tank(0, 999., player + 1, V2::new(100., 0.));
            assert_eq!(world.tanks[0].health, world.tanks[0].max_health);
            assert!(world.tanks[0].spawn_immunity && world.tanks[0].protection > 12.9);
            assert_eq!(world.tanks[0].ability_cooldown, 0.);
            assert!(world.projectiles.is_empty());
        }
        world.tanks[0].input.movement = V2::new(1., 0.);
        world.step();
        assert!(!world.tanks[0].spawn_immunity);
        assert!(!world.projectiles.is_empty());
    }
    #[test]
    fn spawn_immunity_lasts_fifteen_idle_seconds_and_prediction_agrees() {
        let content = Content::builtin();
        let config = WorldConfig {
            target_count: 0,
            ..Default::default()
        };
        let mut world = crate::World::new(config, content);
        world.add_player("Idle", false);
        let mut predicted = world.tanks[0].clone();
        assert_eq!(predicted.protection, 15.0);
        for tick in 1..=900 {
            predicted.aim = tick as f32 * 0.01;
            predict_tank(
                &mut predicted,
                &world.content,
                &world.config,
                &world.walls,
                tick,
                crate::DT,
            );
            world.step();
            let actual = &world.tanks[0];
            assert_eq!(predicted.protection, actual.protection);
            assert_eq!(predicted.spawn_immunity, actual.spawn_immunity);
            if tick < 900 {
                assert!(actual.protection > 0.0);
            }
        }
        assert_eq!(world.tanks[0].protection, 0.0);
        assert!(!world.tanks[0].spawn_immunity);
    }

    #[test]
    fn movement_ends_spawn_immunity_immediately_but_keeps_cloak() {
        for input in [
            crate::PlayerInput {
                movement: V2::new(1.0, 0.0),
                ..Default::default()
            },
            crate::PlayerInput {
                movement: V2::new(0.001, 0.0),
                ..Default::default()
            },
        ] {
            let mut world = crate::World::new(
                WorldConfig {
                    target_count: 0,
                    ..Default::default()
                },
                Content::builtin(),
            );
            world.add_player("Moving", false);
            world.tanks[0].input = input;
            let mut predicted = world.tanks[0].clone();
            predict_tank(
                &mut predicted,
                &world.content,
                &world.config,
                &world.walls,
                1,
                crate::DT,
            );
            world.step();
            assert_eq!(world.tanks[0].protection, 0.0);
            assert!(!world.tanks[0].spawn_immunity);
            assert_eq!(predicted.protection, 0.0);
        }
        let mut world = crate::World::new(WorldConfig::default(), Content::builtin());
        world.add_player("Cloak", false);
        let tank = &mut world.tanks[0];
        apply_personal_ability(
            tank,
            tankyard_content::Ability::Cloak,
            &world.config,
            &world.walls,
        );
        tank.input.movement = V2::new(1.0, 0.0);
        update_protection(tank, crate::DT);
        assert!(!tank.spawn_immunity);
        assert!(tank.protection > 1.9);
    }

    #[test]
    fn clearing_the_spatial_grid_removes_only_previous_entries() {
        let mut grid = SpatialGrid::new(4000.0, 4000.0);
        let mut found = Vec::new();
        for pos in [
            V2::new(10.0, 20.0),
            V2::new(3900.0, 3900.0),
            V2::new(10.0, 20.0),
        ] {
            grid.insert(pos, Body::Deployable(1));
            grid.insert(pos, Body::Deployable(2));
            grid.query(V2::ZERO, V2::new(4000.0, 4000.0), 0.0, &mut found);
            assert_eq!(found.len(), 2);
            assert_eq!(grid.occupied.len(), 1);
            grid.clear();
            grid.query(V2::ZERO, V2::new(4000.0, 4000.0), 0.0, &mut found);
            assert!(found.is_empty());
        }
    }
    #[test]
    fn fast_projectile_does_not_tunnel() {
        let t = sweep_circle(V2::ZERO, V2::new(1000., 0.), V2::new(500., 0.), 5.).unwrap();
        assert!((t - 0.495).abs() < 0.00001);
    }
    #[test]
    fn wall_sweep_and_overlap() {
        let rect = Rect {
            x: 50.,
            y: 0.,
            w: 10.,
            h: 100.,
        };
        let (t, n) = sweep_rect(V2::new(0., 50.), V2::new(100., 50.), 5., rect).unwrap();
        assert!((t - 0.45).abs() < 0.001);
        assert_eq!(n, V2::new(-1., 0.));
        assert!(circle_rect(V2::new(55., 50.), 5., rect).unwrap().1 > 0.);
    }
    #[test]
    fn wall_bounds_rejection_preserves_long_sweeps_and_corner_contacts() {
        let rect = Rect {
            x: 100.,
            y: 100.,
            w: 30.,
            h: 40.,
        };
        assert!(sweep_rect(V2::new(-1000., 120.), V2::new(1000., 120.), 5., rect).is_some());
        assert!(sweep_rect(V2::new(120., -1000.), V2::new(120., 1000.), 5., rect).is_some());
        let (t, normal) = sweep_rect(V2::new(80., 80.), V2::new(110., 110.), 5., rect).unwrap();
        assert!(t > 0.5 && t < 0.6);
        assert!((normal.length() - 1.).abs() < 0.001);
        assert!(sweep_rect(V2::new(20., 20.), V2::new(50., 50.), 5., rect).is_none());
        assert!(sweep_rect(V2::new(98., 98.), V2::new(98., 98.), 5., rect).is_some());
    }
    #[test]
    fn rounded_rect_corner_does_not_false_hit() {
        assert!(
            sweep_rect(
                V2::new(0., 0.),
                V2::new(45., 45.),
                5.,
                Rect {
                    x: 50.,
                    y: 50.,
                    w: 10.,
                    h: 10.
                }
            )
            .is_none()
        );
    }
    #[test]
    fn polygon_sweep() {
        let polygon = polygon_vertices(V2::new(50., 50.), 10., 4, 0.);
        assert!(sweep_polygon(V2::new(0., 50.), V2::new(100., 50.), 2., &polygon).is_some());
        assert!(!circle_polygon(V2::ZERO, 1., &polygon));
    }
    #[test]
    fn sliding_motion_keeps_parallel_velocity() {
        let content = Content::builtin();
        let config = WorldConfig::default();
        let mut world = crate::World::new(config.clone(), content.clone());
        let player = world.add_player("slide", false);
        let tank = world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == player)
            .unwrap();
        tank.pos = V2::new(65.0, 120.0);
        tank.vel = V2::new(650.0, 120.0);
        tank.input.movement = V2::new(1.0, 1.0);
        let wall = Rect {
            x: 100.0,
            y: 50.0,
            w: 20.0,
            h: 200.0,
        };
        let starting_y = tank.pos.y;
        move_tank(tank, &content, &config, &[wall], 0.1);
        assert!(tank.pos.x < 100.0 - tank.radius + 0.1);
        assert!(tank.pos.y > starting_y, "tank should glide along the wall");
    }
    #[test]
    fn wall_tangent_can_move_away_from_surface() {
        let content = Content::builtin();
        let config = WorldConfig::default();
        let mut world = crate::World::new(config.clone(), content.clone());
        let player = world.add_player("escape", false);
        let tank = world
            .tanks
            .iter_mut()
            .find(|tank| tank.id == player)
            .unwrap();
        tank.pos = V2::new(78., 120.);
        tank.vel = V2::new(-100., 0.);
        tank.input.movement = V2::new(-1., 0.);
        let wall = Rect {
            x: 100.,
            y: 50.,
            w: 20.,
            h: 200.,
        };
        move_tank(tank, &content, &config, &[wall], 0.1);
        assert!(
            tank.pos.x < 78.,
            "a tank at a wall should be able to retreat"
        );
    }
}
