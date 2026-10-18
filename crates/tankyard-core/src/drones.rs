//! Friendly drones press together without sharing the same space.
use crate::physics::{Body, SpatialGrid, circle_rect, sweep_circle, sweep_rect};
use crate::{DT, Projectile, Rect, V2, WorldConfig};
#[cfg(test)]
use tankyard_content::ProjectileKind;

// Include the drawn outline so touching drones remain distinct at any heading.
const OUTLINE: f32 = 2.0;
const CONTACT_PASSES: usize = 12;

pub(crate) struct DroneContacts {
    grid: SpatialGrid,
    indices: Vec<usize>,
    nearby: Vec<Body>,
    walls: DroneWalls,
}

impl DroneContacts {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            grid: SpatialGrid::new(width, height),
            indices: Vec::with_capacity(128),
            nearby: Vec::with_capacity(32),
            walls: DroneWalls::new(width, height),
        }
    }

    pub fn prepare_walls(&mut self, walls: &[Rect]) {
        self.walls.sync(walls);
    }

    pub fn move_drone(&mut self, drone: &mut Projectile, config: &WorldConfig) {
        self.walls.move_drone(drone, config);
    }

    pub fn resolve(&mut self, projectiles: &mut [Projectile], config: &WorldConfig) {
        self.indices.clear();
        self.indices
            .extend(projectiles.iter().enumerate().filter_map(|(i, p)| {
                (p.kind.is_drone() && p.lifetime > 0.0 && p.penetration > 0.0).then_some(i)
            }));
        if self.indices.len() < 2 {
            return;
        }
        // IDs keep contact order stable when unrelated projectiles disappear.
        self.indices.sort_unstable_by_key(|&i| projectiles[i].id);
        let radius = self
            .indices
            .iter()
            .map(|&i| projectiles[i].radius + OUTLINE)
            .fold(0.0, f32::max);
        let travel = self
            .indices
            .iter()
            .map(|&i| projectiles[i].pos.distance(projectiles[i].previous))
            .fold(0.0, f32::max);
        for pass in 0..CONTACT_PASSES {
            self.grid.clear();
            for &i in &self.indices {
                self.grid.insert(projectiles[i].pos, Body::Deployable(i));
            }
            let mut deepest = 0.0_f32;
            for &i in &self.indices {
                let p = &projectiles[i];
                let start = if pass == 0 { p.previous } else { p.pos };
                self.grid.query(
                    start,
                    p.pos,
                    p.radius + OUTLINE + radius + if pass == 0 { travel } else { 0.0 },
                    &mut self.nearby,
                );
                for body in &self.nearby {
                    let Body::Deployable(j) = *body else { continue };
                    let other = &projectiles[j];
                    let p = &projectiles[i];
                    if other.id <= p.id
                        || !(p.owner == other.owner
                            || (!config.friendly_fire && p.team != 0 && p.team == other.team))
                    {
                        continue;
                    }
                    let Some((normal, depth)) = contact(p, other, pass == 0) else {
                        continue;
                    };
                    deepest = deepest.max(depth);
                    let (a, b) = if i < j {
                        let (left, right) = projectiles.split_at_mut(j);
                        (&mut left[i], &mut right[0])
                    } else {
                        let (left, right) = projectiles.split_at_mut(i);
                        (&mut right[0], &mut left[j])
                    };
                    let closing = (b.vel - a.vel).dot(normal).min(0.0) * 0.5;
                    a.vel += normal * closing;
                    b.vel -= normal * closing;
                    let correction = normal * ((depth + 0.01) * 0.5);
                    self.walls.slide(a, correction * -1.0, config);
                    self.walls.slide(b, correction, config);
                }
            }
            if deepest < 0.02 {
                break;
            }
        }
    }
}

fn contact(a: &Projectile, b: &Projectile, swept: bool) -> Option<(V2, f32)> {
    let radius = a.radius + b.radius + OUTLINE * 2.0;
    let end = b.pos - a.pos;
    if swept {
        let start = b.previous - a.previous;
        // Keep fast drones on their original sides of a contact. Only a fresh
        // movement sweep uses previous positions; packing passes use current ones.
        if start.length_squared() >= radius * radius
            && let Some(time) = sweep_circle(start, end, V2::ZERO, radius)
        {
            let normal = start.lerp(end, time).normalized();
            let depth = radius - end.dot(normal);
            if depth > 0.0 {
                return Some((normal, depth));
            }
        }
    }
    let squared = end.length_squared();
    if squared >= radius * radius {
        return None;
    }
    let distance = squared.sqrt();
    let normal = if distance > 0.0001 {
        end / distance
    } else {
        // Newly launched drones can coincide exactly. A pair-specific direction
        // separates them without consuming the arena's random sequence.
        let seed = a.id.wrapping_mul(0x517cc1b727220a95) ^ b.id.wrapping_mul(0x6eed0e9da4d94a4f);
        V2::from_angle((seed >> 40) as f32 / 16_777_216.0 * std::f32::consts::TAU)
    };
    Some((normal, radius - distance))
}

struct DroneWalls {
    grid: SpatialGrid<usize>,
    walls: Vec<Rect>,
    nearby: Vec<usize>,
}

impl DroneWalls {
    fn new(width: f32, height: f32) -> Self {
        Self {
            grid: SpatialGrid::new(width, height),
            walls: Vec::new(),
            nearby: Vec::with_capacity(16),
        }
    }

    fn sync(&mut self, walls: &[Rect]) {
        if self.walls == walls {
            return;
        }
        self.walls.clear();
        self.walls.extend_from_slice(walls);
        self.grid.clear();
        for (index, &wall) in walls.iter().enumerate() {
            self.grid.insert_rect(wall, index);
        }
    }

    fn query(&mut self, start: V2, end: V2, radius: f32) {
        if self.walls.is_empty() {
            self.nearby.clear();
            return;
        }
        self.grid.query(start, end, radius, &mut self.nearby);
        // A long wall occupies several cells. Test it once, in map order.
        self.nearby.sort_unstable();
        self.nearby.dedup();
    }

    fn move_drone(&mut self, p: &mut Projectile, config: &WorldConfig) {
        p.pos = p.previous;
        // Recall and map changes can place a drone against a wall before movement.
        for _ in 0..3 {
            self.query(p.pos, p.pos, p.radius);
            let mut moved = false;
            for &index in &self.nearby {
                if let Some((normal, depth)) = circle_rect(p.pos, p.radius, self.walls[index]) {
                    p.pos += normal * (depth + 0.01);
                    p.vel -= normal * p.vel.dot(normal).min(0.0);
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        self.slide(p, p.vel * DT, config);
    }

    fn slide(&mut self, p: &mut Projectile, mut movement: V2, config: &WorldConfig) {
        for _ in 0..4 {
            if movement.length_squared() < 1e-8 {
                break;
            }
            let mut first: Option<(f32, V2)> = None;
            self.query(p.pos, p.pos + movement, p.radius);
            for &index in &self.nearby {
                if let Some((time, normal)) =
                    sweep_rect(p.pos, p.pos + movement, p.radius, self.walls[index])
                    && movement.dot(normal) < -1e-6
                    && first.is_none_or(|hit| time < hit.0)
                {
                    first = Some((time, normal));
                }
            }
            if let Some((time, normal)) = first {
                p.pos += movement * (time - 0.0001).max(0.0);
                movement *= 1.0 - time;
                movement -= normal * movement.dot(normal).min(0.0);
                p.vel -= normal * p.vel.dot(normal).min(0.0);
            } else {
                p.pos += movement;
                break;
            }
        }
        let bounds = config.bounds();
        if p.pos.x <= bounds.x + p.radius {
            p.vel.x = p.vel.x.max(0.0);
        }
        if p.pos.x >= bounds.x + bounds.w - p.radius {
            p.vel.x = p.vel.x.min(0.0);
        }
        if p.pos.y <= bounds.y + p.radius {
            p.vel.y = p.vel.y.max(0.0);
        }
        if p.pos.y >= bounds.y + bounds.h - p.radius {
            p.vel.y = p.vel.y.min(0.0);
        }
        p.pos = bounds.clamp_circle(p.pos, p.radius);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityId, Mode, PlayerInput, Rect, World};
    use tankyard_content::Content;

    fn swarm(position: V2, goal: V2) -> (World, EntityId) {
        let mut world = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                spawn_protection: 0.0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let owner = world.add_player("Pilot", false);
        world.tanks[0].pos = V2::new(1000.0, 1000.0);
        // Keep a manual drone command without introducing extra gunfire.
        world.tanks[0].cooldowns = vec![1000.0];
        world.tanks[0].barrel_shots = vec![0];
        world.tanks[0].barrel_aims = vec![0.0];
        command(&mut world, owner, goal);
        for _ in 0..16 {
            let drone = world.simple_projectile(
                owner,
                0,
                position,
                V2::new(300.0, 0.0),
                ProjectileKind::Drone,
                10.0,
                7.5,
                60.0,
                20.0,
            );
            world.projectiles.push(drone);
        }
        (world, owner)
    }

    fn command(world: &mut World, owner: EntityId, goal: V2) {
        let direction = goal - world.tanks[0].pos;
        world.set_input(
            owner,
            PlayerInput {
                sequence: world.tick as u32 + 1,
                aim: direction.angle(),
                aim_distance: direction.length(),
                fire: true,
                ..Default::default()
            },
        );
    }

    fn assert_separated(world: &World, tolerance: f32) {
        assert_eq!(world.projectiles.len(), 16);
        for (i, a) in world.projectiles.iter().enumerate() {
            assert!(a.pos.finite() && a.vel.finite());
            assert!(a.pos.x >= a.radius && a.pos.x <= world.config.width - a.radius);
            assert!(a.pos.y >= a.radius && a.pos.y <= world.config.height - a.radius);
            for wall in &world.walls {
                assert!(
                    circle_rect(a.pos, a.radius - 0.01, *wall).is_none(),
                    "drone {} entered a wall",
                    a.id
                );
            }
            for b in &world.projectiles[i + 1..] {
                let distance = a.pos.distance(b.pos);
                let required = a.radius + b.radius + OUTLINE * 2.0;
                assert!(
                    distance >= required - tolerance,
                    "drones {} and {} overlap by {} at tick {}",
                    a.id,
                    b.id,
                    required - distance,
                    world.tick
                );
            }
        }
    }

    fn center(world: &World) -> V2 {
        world
            .projectiles
            .iter()
            .fold(V2::ZERO, |sum, p| sum + p.pos)
            / world.projectiles.len() as f32
    }

    #[test]
    fn a_full_swarm_packs_at_the_cursor_then_travels_and_recalls() {
        let goal = V2::new(1400.0, 1000.0);
        let (mut world, owner) = swarm(goal, goal);
        for tick in 0..300 {
            world.step();
            assert_separated(&world, if tick < 10 { 2.0 } else { 0.3 });
        }
        assert!(center(&world).distance(goal) < 5.0);
        assert!(
            world
                .projectiles
                .iter()
                .all(|p| p.pos.distance(goal) < 60.0)
        );
        let start = center(&world);
        command(&mut world, owner, V2::new(2400.0, 1000.0));
        for _ in 0..60 {
            world.step();
            assert_separated(&world, 0.4);
        }
        assert!(center(&world).x > start.x + 210.0);
        assert!(world.projectiles.iter().all(|p| p.vel.length() > 280.0));
        world.tanks[0].input.alt_fire = true;
        for _ in 0..300 {
            world.step();
            assert_separated(&world, 0.4);
        }
        assert!(
            world
                .projectiles
                .iter()
                .all(|p| p.pos.distance(world.tanks[0].pos) < 140.0)
        );
        assert!(
            world
                .projectiles
                .iter()
                .all(|p| p.penetration == 20.0 && p.hit.is_empty())
        );
    }

    #[test]
    fn crowded_drones_slide_along_walls_and_can_leave_them() {
        let (mut world, owner) = swarm(V2::new(1370.0, 1000.0), V2::new(1600.0, 1000.0));
        world.walls = vec![Rect {
            x: 1400.0,
            y: 850.0,
            w: 16.0,
            h: 300.0,
        }];
        for tick in 0..240 {
            world.step();
            assert_separated(&world, if tick < 10 { 3.0 } else { 0.5 });
            assert!(
                world
                    .projectiles
                    .iter()
                    .all(|p| p.pos.x < 1400.0 - p.radius + 0.01)
            );
        }
        command(&mut world, owner, V2::new(1500.0, 650.0));
        for _ in 0..240 {
            world.step();
            assert_separated(&world, 0.5);
        }
        assert!(
            world
                .projectiles
                .iter()
                .all(|p| p.pos.y < 750.0 && p.pos.x > 1450.0)
        );
    }

    #[test]
    fn a_swarm_pressed_into_an_arena_corner_stays_solid_and_finite() {
        let (mut world, _) = swarm(V2::new(35.0, 35.0), V2::ZERO);
        for tick in 0..360 {
            world.step();
            assert_separated(&world, if tick < 20 { 3.0 } else { 0.6 });
        }
        assert!(world.projectiles.iter().all(|p| p.pos.length() < 120.0));
    }

    #[test]
    fn fast_friendly_drones_cannot_cross_through_each_other() {
        let (mut world, _) = swarm(V2::ZERO, V2::ZERO);
        world.projectiles.truncate(2);
        let a = &mut world.projectiles[0];
        a.previous = V2::new(480.0, 500.0);
        a.pos = V2::new(530.0, 500.0);
        a.vel = V2::new(3000.0, 40.0);
        let b = &mut world.projectiles[1];
        b.previous = V2::new(520.0, 500.0);
        b.pos = V2::new(470.0, 500.0);
        b.vel = V2::new(-3000.0, 40.0);
        let mut contacts = DroneContacts::new(world.config.width, world.config.height);
        contacts.resolve(&mut world.projectiles, &world.config);
        let [a, b] = world.projectiles.as_slice() else {
            unreachable!()
        };
        assert!(a.pos.x < b.pos.x);
        assert!(b.pos.x - a.pos.x >= a.radius + b.radius + OUTLINE * 2.0);
        assert!(a.vel.x.abs() < 0.001 && b.vel.x.abs() < 0.001);
        assert_eq!((a.vel.y, b.vel.y), (40.0, 40.0));
    }

    #[test]
    fn contact_order_is_independent_of_projectile_storage_order() {
        let goal = V2::new(1400.0, 1000.0);
        let (mut a, _) = swarm(goal, goal);
        let (mut b, _) = swarm(goal, goal);
        b.projectiles.reverse();
        for _ in 0..180 {
            a.step();
            b.step();
        }
        for p in &a.projectiles {
            let other = b.projectiles.iter().find(|q| q.id == p.id).unwrap();
            assert_eq!(p.pos, other.pos);
            assert_eq!(p.vel, other.vel);
        }
    }

    #[test]
    fn teammates_pack_without_damage_while_hostile_contacts_keep_combat_rules() {
        for (same_owner, team, friendly_fire, separate) in [
            (true, 0, false, true),
            (false, 1, false, true),
            (false, 0, false, false),
            (false, 1, true, false),
        ] {
            let (mut world, _) = swarm(V2::new(1000.0, 500.0), V2::new(1000.0, 500.0));
            world.projectiles.truncate(2);
            world.config.friendly_fire = friendly_fire;
            for p in &mut world.projectiles {
                p.team = team;
            }
            if !same_owner {
                world.projectiles[1].owner += 100;
            }
            let mut contacts = DroneContacts::new(world.config.width, world.config.height);
            contacts.resolve(&mut world.projectiles, &world.config);
            let distance = world.projectiles[0].pos.distance(world.projectiles[1].pos);
            assert_eq!(distance > 18.0, separate);
            assert!(
                world
                    .projectiles
                    .iter()
                    .all(|p| p.penetration == 20.0 && p.hit.is_empty())
            );
        }
    }

    #[test]
    fn drones_with_different_sizes_pack_without_merging() {
        let goal = V2::new(1400.0, 1000.0);
        let (mut world, _) = swarm(goal, goal);
        for (i, p) in world.projectiles.iter_mut().enumerate() {
            p.radius = if i % 2 == 0 { 5.0 } else { 12.0 };
        }
        for tick in 0..240 {
            world.step();
            assert_separated(&world, if tick < 10 { 3.0 } else { 0.5 });
        }
    }

    #[test]
    fn allied_swarms_can_share_a_cursor_without_merging_their_bodies() {
        let goal = V2::new(1400.0, 1000.0);
        for owners in [2, 4, 8] {
            let (mut world, _) = swarm(goal, goal);
            world.tanks[0].team = 1;
            for p in &mut world.projectiles {
                p.team = 1;
            }
            for index in 1..owners {
                let owner = world.add_player("Teammate", false);
                assert!(world.sandbox_class(owner, "flock"));
                let tank = world.tanks.last_mut().unwrap();
                tank.team = 1;
                tank.pos = V2::new(1000.0, 400.0 + index as f32 * 120.0);
                let direction = goal - tank.pos;
                tank.input = PlayerInput {
                    fire: true,
                    aim: direction.angle(),
                    aim_distance: direction.length(),
                    ..Default::default()
                };
                for _ in 0..16 {
                    let p = world.simple_projectile(
                        owner,
                        1,
                        goal,
                        V2::ZERO,
                        ProjectileKind::Drone,
                        10.0,
                        7.5,
                        60.0,
                        20.0,
                    );
                    world.projectiles.push(p);
                }
            }
            for tick in 0..360 {
                world.step();
                assert_eq!(world.projectiles.len(), owners * 16);
                if tick < 60 {
                    continue;
                }
                for (i, a) in world.projectiles.iter().enumerate() {
                    assert!(a.pos.finite() && a.vel.finite());
                    for b in &world.projectiles[i + 1..] {
                        assert!(
                            a.pos.distance(b.pos) >= a.radius + b.radius - 0.1,
                            "{} allied drones overlap at tick {}",
                            owners * 16,
                            world.tick
                        );
                    }
                }
            }
            assert!(
                world
                    .projectiles
                    .iter()
                    .all(|p| p.pos.distance(goal) < 150.0)
            );
        }
    }

    #[test]
    fn enemy_drones_still_exchange_damage_and_friendly_drones_do_not() {
        for (team, friendly_fire, hostile) in [(1, false, false), (0, false, true), (1, true, true)]
        {
            let (mut world, _) = swarm(V2::new(1400.0, 500.0), V2::new(1400.0, 500.0));
            world.projectiles.truncate(2);
            let other = world.add_player("Other pilot", false);
            world.tanks[1].pos = V2::new(2000.0, 1000.0);
            world.tanks[1].team = team;
            world.tanks[0].team = team;
            world.config.friendly_fire = friendly_fire;
            world.projectiles[1].owner = other;
            for p in &mut world.projectiles {
                p.team = team;
            }
            world.step();
            assert_eq!(world.projectiles.len(), 2);
            for p in &world.projectiles {
                assert_eq!(p.penetration < 20.0, hostile);
            }
            if !hostile {
                assert!(world.projectiles[0].pos.distance(world.projectiles[1].pos) >= 19.0);
            }
        }
    }

    #[test]
    fn indexed_walls_stop_fast_drones_and_follow_map_edits() {
        let (mut world, _) = swarm(V2::new(800.0, 1000.0), V2::new(1800.0, 1000.0));
        let mut drone = world.projectiles.remove(0);
        let mut walls = DroneWalls::new(world.config.width, world.config.height);
        for x in [1000.0, 1500.0] {
            let wall = Rect {
                x,
                y: 0.0,
                w: 4.0,
                h: 2400.0,
            };
            walls.sync(&[wall]);
            drone.previous = V2::new(800.0, 1000.0);
            drone.vel = V2::new(60000.0, 90.0);
            walls.move_drone(&mut drone, &world.config);
            assert!((drone.pos.x - (x - drone.radius)).abs() < 0.2);
            assert!(drone.pos.y > 1001.0);
            assert!(circle_rect(drone.pos, drone.radius - 0.01, wall).is_none());
        }
        walls.sync(&[]);
        drone.previous = V2::new(800.0, 1000.0);
        drone.vel = V2::new(60000.0, 90.0);
        walls.move_drone(&mut drone, &world.config);
        assert!(
            drone.pos.x > 1799.0,
            "the removed wall left a stale collision"
        );
    }

    #[test]
    fn a_drone_whose_owner_is_dead_expires_before_dealing_damage() {
        let (mut world, _) = swarm(V2::new(1400.0, 1000.0), V2::new(1400.0, 1000.0));
        world.tanks[0].alive = false;
        let enemy = world.add_player("Enemy", false);
        let victim = world.tanks.iter_mut().find(|t| t.id == enemy).unwrap();
        victim.pos = V2::new(1400.0, 1000.0);
        victim.protection = 0.0;
        world.step();
        assert!(world.projectiles.is_empty());
        assert_eq!(world.tanks[1].health, world.tanks[1].max_health);
    }
}
