//! Bodies: groups of blocks that tanks push by driving into them, such as
//! crates and push doors. The server moves them; snapshots carry where each
//! one is and how fast it slides, so clients draw them between snapshots.
use super::*;
use crate::physics::circle_rect;
use tankyard_content::{BodyPose, MapBody};

/// How a body has moved from where the map places it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyState {
    /// In world units.
    pub offset: V2,
    /// In world units a second.
    pub velocity: V2,
}

/// Fastest a body slides, in world units a second.
const BODY_SPEED: f32 = 2400.;
/// How quickly a pushed body takes up its pushers' pace.
const PUSH_RESPONSE: f32 = 10.;
/// How quickly a let-go body takes up its spring's pace.
const SPRING_RESPONSE: f32 = 8.;
/// World units a tank must be within to push.
const CONTACT: f32 = 2.;

/// `v` with only the parts `body`'s axis allows.
fn along_axis(body: &MapBody, v: V2) -> V2 {
    let [x, y] = body.axis.project([v.x, v.y]);
    V2::new(x, y)
}

impl World {
    /// The arena's size in world units, which arena fractions are of.
    fn arena_size(&self) -> V2 {
        let scale = self.config.arena_scale;
        V2::new(self.config.width * scale, self.config.height * scale)
    }

    /// Where map logic and pushes have put each group at round time `now`.
    pub(crate) fn composed_poses(&self, now: f32) -> GroupPoses {
        let mut poses = GroupPoses::at(&self.match_state.tweens, now);
        let size = self.arena_size();
        for (body, state) in self.map.bodies.iter().zip(&self.bodies) {
            poses.nudge(
                body.group,
                [state.offset.x / size.x, state.offset.y / size.y],
            );
        }
        poses
    }

    /// Pushed bodies as arena-fraction offsets by group, for map logic.
    pub(crate) fn body_offsets(&self, out: &mut Vec<(u8, [f32; 2])>) {
        out.clear();
        let size = self.arena_size();
        for (body, state) in self.map.bodies.iter().zip(&self.bodies) {
            if state.offset != V2::ZERO {
                out.push((
                    body.group,
                    [state.offset.x / size.x, state.offset.y / size.y],
                ));
            }
        }
    }

    /// Give body `group` a push of `velocity`, in world units a second, as
    /// a Shove body trigger or a script does. Returns whether it is a body.
    pub fn shove_body(&mut self, group: u8, velocity: V2) -> bool {
        let Some(index) = self.map.bodies.iter().position(|b| b.group == group) else {
            return false;
        };
        if !velocity.finite() {
            return false;
        }
        self.bodies
            .resize(self.map.bodies.len(), BodyState::default());
        let body = &self.map.bodies[index];
        let state = &mut self.bodies[index];
        let push = along_axis(body, velocity) / body.weight.max(0.25);
        state.velocity = (state.velocity + push).clamp_length(BODY_SPEED);
        true
    }

    /// Where body `group` sits and how fast it slides, if it is one.
    pub fn body_state(&self, group: u8) -> Option<BodyState> {
        let index = self.map.bodies.iter().position(|b| b.group == group)?;
        Some(self.bodies.get(index).copied().unwrap_or_default())
    }

    /// A shot of `damage` travelling at `velocity` struck wall `wall`; a
    /// body that shots push takes some of its momentum.
    pub(crate) fn shot_hit_wall(&mut self, wall: usize, velocity: V2, damage: f32) {
        let Some(&group) = self.wall_groups.get(wall) else {
            return;
        };
        if group == 0 || !self.map.body(group).is_some_and(|body| body.shots) {
            return;
        }
        let push = velocity.normalized() * (damage.clamp(0., 400.).sqrt() * 18.);
        if push.finite() && self.body_hits.len() < 256 {
            self.body_hits.push((group, push));
        }
    }

    /// Slide every body by what pushes it this step: tanks driving into it,
    /// shots, its spring and friction. A body stops at the end of its reach,
    /// at other cover and at a tank it would crush against cover.
    pub(crate) fn update_bodies(&mut self) {
        if self.map.bodies.is_empty() {
            self.body_hits.clear();
            if !self.bodies.is_empty() || !self.match_state.bodies.is_empty() {
                self.bodies.clear();
                self.match_state.bodies.clear();
            }
            return;
        }
        self.bodies
            .resize(self.map.bodies.len(), BodyState::default());
        let hidden = self.map_hidden();
        let size = self.arena_size();
        let hits = std::mem::take(&mut self.body_hits);
        let mut moved_mask = 0;
        let mut old = std::mem::take(&mut self.motion_scratch);
        old.clone_from(&self.walls);
        for index in 0..self.map.bodies.len() {
            let body = self.map.bodies[index].clone();
            let mut state = self.bodies[index];
            if MapDef::group_bit(body.group) & hidden != 0 {
                // A hidden body neither moves nor keeps sliding.
                state.velocity = V2::ZERO;
                self.bodies[index] = state;
                continue;
            }
            let (drive, pushers) = self.pushing(&body, state.velocity);
            let weight = body.weight.max(0.25);
            if pushers > 0 {
                // A light body never outruns the tanks pushing it.
                let target = along_axis(&body, drive / weight.max(pushers as f32));
                state.velocity = state
                    .velocity
                    .lerp(target, 1. - (-PUSH_RESPONSE * DT).exp());
            } else {
                state.velocity *= (-body.friction.max(0.5) * DT).exp();
                if body.spring > 0. {
                    let home = V2::ZERO - state.offset;
                    let distance = home.length();
                    // Glide home at the spring's pace, easing in at the end.
                    let pace = (body.spring * size.x).min(distance * 6.);
                    let back = if distance > 0.01 {
                        home / distance * pace
                    } else {
                        V2::ZERO
                    };
                    state.velocity = state
                        .velocity
                        .lerp(back, 1. - (-SPRING_RESPONSE * DT).exp());
                    if distance <= 0.01 && state.velocity.length() < 1. {
                        state.offset = V2::ZERO;
                        state.velocity = V2::ZERO;
                    }
                }
            }
            for &(group, push) in &hits {
                if group == body.group {
                    state.velocity += push / weight;
                }
            }
            state.velocity = along_axis(&body, state.velocity).clamp_length(BODY_SPEED);
            if state.velocity.length_squared() < 0.25 && pushers == 0 {
                state.velocity = V2::ZERO;
            }
            let step = state.velocity * DT;
            let mut shift = V2::ZERO;
            for vertical in [false, true] {
                let along = if vertical { step.y } else { step.x };
                if along == 0. {
                    continue;
                }
                let (low, high, at) = if vertical {
                    (
                        -body.reach[1] * size.y,
                        body.reach[3] * size.y,
                        state.offset.y,
                    )
                } else {
                    (
                        -body.reach[0] * size.x,
                        body.reach[2] * size.x,
                        state.offset.x,
                    )
                };
                let wanted = (at + along).clamp(low, high);
                let limited = wanted != at + along;
                let delta = wanted - at;
                let try_shift = if vertical {
                    V2::new(shift.x, delta)
                } else {
                    V2::new(delta, shift.y)
                };
                let blocked =
                    delta.abs() < 1e-4 || self.body_blocked(body.group, try_shift, vertical);
                if blocked || limited {
                    let bounce = -body.bounce.clamp(0., 0.9);
                    if vertical {
                        state.velocity.y *= bounce;
                    } else {
                        state.velocity.x *= bounce;
                    }
                }
                if !blocked {
                    shift = try_shift;
                }
            }
            if shift != V2::ZERO {
                state.offset += shift;
                moved_mask |= MapDef::group_bit(body.group);
                // Move this body's walls now, so the next body sees it.
                for (wall, &group) in self.walls.iter_mut().zip(&self.wall_groups) {
                    if group == body.group {
                        wall.x += shift.x;
                        wall.y += shift.y;
                    }
                }
            }
            self.bodies[index] = state;
        }
        if moved_mask != 0 {
            self.poses = self.composed_poses(self.match_state.elapsed);
            self.scale_map_walls();
            self.push_from_moving_walls(&old, moved_mask);
        }
        self.motion_scratch = old;
        self.publish_bodies(size);
    }

    /// Tanks driving into `body`, which moves at `velocity`: the push they add
    /// up to, in world units a second, and how many push.
    fn pushing(&mut self, body: &MapBody, velocity: V2) -> (V2, u32) {
        let mut drive = V2::ZERO;
        let mut pushers = 0;
        for tank in self.tanks.iter_mut().filter(|t| t.tangible()) {
            let intent = tank.input.movement.clamp_length(1.);
            if intent.length_squared() < 0.04 {
                continue;
            }
            let speed = crate::physics::movement_speed(tank, &self.content, &self.config);
            let contact = self
                .walls
                .iter()
                .zip(&self.wall_groups)
                .filter(|&(_, &group)| group == body.group)
                .filter_map(|(&wall, _)| circle_rect(tank.pos, tank.radius + CONTACT, wall))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let Some((normal, _)) = contact else {
                continue;
            };
            let into = -intent.dot(normal);
            if into <= 0.2 {
                continue;
            }
            // Collision removes normal speed against static walls. Retain the
            // contacted body's departing speed so contact survives next tick,
            // instead of accepting pushes from tanks across an empty gap.
            crate::physics::keep_pushing_velocity(tank, normal, velocity, speed);
            drive -= normal * (into * speed);
            pushers += 1;
        }
        (drive, pushers)
    }

    /// Whether `group`'s walls, moved a further `shift`, would leave the
    /// arena, overlap other cover, or crush a tank or boss against cover.
    fn body_blocked(&self, group: u8, shift: V2, vertical: bool) -> bool {
        let bounds = self.config.bounds();
        let moved = |wall: Rect| Rect {
            x: wall.x + shift.x,
            y: wall.y + shift.y,
            ..wall
        };
        let others = || {
            self.walls
                .iter()
                .zip(&self.wall_groups)
                .filter(move |&(_, &g)| g != group)
                .map(|(&wall, _)| wall)
        };
        let clear_of_cover = |pos: V2, radius: f32| {
            bounds.contains_circle(pos, radius)
                && others().all(|wall| circle_rect(pos, radius - 0.5, wall).is_none())
        };
        for (&wall, _) in self
            .walls
            .iter()
            .zip(&self.wall_groups)
            .filter(|&(_, &g)| g == group)
        {
            let to = moved(wall);
            if to.x < bounds.x - 0.01
                || to.y < bounds.y - 0.01
                || to.x + to.w > bounds.x + bounds.w + 0.01
                || to.y + to.h > bounds.y + bounds.h + 0.01
            {
                return true;
            }
            const SLACK: f32 = 0.05;
            if others().any(|other| {
                to.x + SLACK < other.x + other.w
                    && other.x + SLACK < to.x + to.w
                    && to.y + SLACK < other.y + other.h
                    && other.y + SLACK < to.y + to.h
            }) {
                return true;
            }
            // What the face would sweep into must have somewhere to go.
            let from = Rect {
                x: wall.x + if vertical { shift.x } else { 0. },
                y: wall.y + if vertical { 0. } else { shift.y },
                ..wall
            };
            for tank in self.tanks.iter().filter(|t| t.tangible()) {
                if let Some(pushed) = swept_by(tank.pos, tank.radius, from, to, vertical) {
                    let pos = if vertical {
                        V2::new(tank.pos.x, pushed)
                    } else {
                        V2::new(pushed, tank.pos.y)
                    };
                    if !clear_of_cover(pos, tank.radius) {
                        return true;
                    }
                }
            }
            if self
                .bosses
                .iter()
                .any(|boss| swept_by(boss.pos, boss.radius, from, to, vertical).is_some())
            {
                return true;
            }
        }
        false
    }

    /// Tell clients where bodies are: only those away from home or moving.
    fn publish_bodies(&mut self, size: V2) {
        self.match_state.bodies.clear();
        for (body, state) in self.map.bodies.iter().zip(&self.bodies) {
            if state.offset == V2::ZERO && state.velocity == V2::ZERO {
                continue;
            }
            self.match_state.bodies.push(BodyPose {
                group: body.group,
                offset: [state.offset.x / size.x, state.offset.y / size.y],
                velocity: [state.velocity.x / size.x, state.velocity.y / size.y],
            });
        }
    }
}

/// Where map logic and pushes have put each group, as a client draws it
/// from a snapshot at round time `now`. Bodies slide on at their last known
/// pace for at most a tenth of a second past the snapshot; the server stops
/// them at the end of their reach, so the next snapshot is never far off.
pub fn snapshot_poses(snapshot: &Snapshot, now: f32) -> GroupPoses {
    let mut poses = GroupPoses::at(&snapshot.match_state.tweens, now);
    let ahead = (now - snapshot.match_state.elapsed).clamp(0., 0.1);
    for pose in &snapshot.match_state.bodies {
        poses.nudge(
            pose.group,
            [
                pose.offset[0] + pose.velocity[0] * ahead,
                pose.offset[1] + pose.velocity[1] * ahead,
            ],
        );
    }
    poses
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::{BodyAxis, MapBlock};

    /// The open arena with block `rect` in group 3, made a body, and
    /// `extra` blocks that stay put.
    fn crate_world(body: MapBody, rect: [f32; 4], extra: &[[f32; 4]]) -> World {
        let mut maps = tankyard_content::builtin_maps();
        let map = &mut maps[0];
        let block = |rect, group| MapBlock {
            rect,
            style: Default::default(),
            color: None,
            group,
        };
        map.blocks.push(block(rect, 3));
        map.blocks.extend(extra.iter().map(|&r| block(r, 0)));
        map.bodies.push(MapBody { group: 3, ..body });
        tankyard_content::validate_maps(&maps).unwrap();
        let mut world = World::with_maps_for_tests(
            WorldConfig {
                target_count: 0,
                ..WorldConfig::default()
            },
            maps,
        );
        world.add_player("Pusher", false);
        // Joining players grow the arena, so place tanks once it settles.
        for _ in 0..240 {
            world.update_arena_size();
        }
        world.scale_map_walls();
        world
    }

    fn body_wall(world: &World) -> Rect {
        let index = world.wall_groups.iter().position(|&g| g == 3).unwrap();
        world.walls[index]
    }

    fn fixed_wall(world: &World) -> Rect {
        let index = world.wall_groups.iter().position(|&g| g == 0).unwrap();
        world.walls[index]
    }

    /// Put the tank against the body's left face, driving `movement`.
    fn lean_on(world: &mut World, movement: V2) {
        let wall = body_wall(world);
        let tank = &mut world.tanks[0];
        tank.pos = V2::new(wall.x - tank.radius - 1., wall.y + wall.h * 0.5);
        tank.vel = V2::ZERO;
        tank.input.movement = movement;
    }

    fn run(world: &mut World, ticks: usize) {
        for _ in 0..ticks {
            world.step();
        }
    }

    #[test]
    fn tanks_push_bodies_and_springs_bring_them_home() {
        let mut world = crate_world(
            MapBody {
                spring: 0.3,
                ..MapBody::default()
            },
            [0.45, 0.45, 0.04, 0.04],
            &[],
        );
        let start = body_wall(&world);
        lean_on(&mut world, V2::new(1., 0.));
        run(&mut world, 60);
        let pushed = world.body_state(3).unwrap();
        assert!(pushed.offset.x > 40., "pushed {:?}", pushed.offset);
        assert!(pushed.offset.y.abs() < 1., "straight along the push");
        assert!(body_wall(&world).x > start.x + 40., "its cover moves");
        assert!(world.tanks[0].pos.x > start.x - 60., "the tank came along");
        // Players see where it went.
        let snapshot = world.snapshot();
        let pose = snapshot.match_state.bodies[0];
        assert_eq!(pose.group, 3);
        assert!(pose.valid() && pose.offset[0] > 0.);
        assert!(snapshot_poses(&snapshot, snapshot.match_state.elapsed).offset(3)[0] > 0.);
        // Let go: the spring takes it home.
        world.tanks[0].input.movement = V2::ZERO;
        world.tanks[0].pos.x -= 400.;
        run(&mut world, 600);
        let home = world.body_state(3).unwrap();
        assert!(home.offset.length() < 1., "home again: {:?}", home.offset);
        // The arena may have resized meanwhile; its cover sits where the
        // layout puts it.
        let settled = body_wall(&world);
        world.poses = GroupPoses::default();
        world.scale_map_walls();
        let layout = body_wall(&world);
        assert!(
            (settled.x - layout.x).abs() < 1.,
            "{settled:?} vs {layout:?}"
        );
    }

    #[test]
    fn bodies_keep_to_their_axis_reach_and_other_cover() {
        let mut world = crate_world(
            MapBody {
                axis: BodyAxis::Across,
                reach: [0., 0., 0.03, 0.],
                ..MapBody::default()
            },
            [0.45, 0.45, 0.04, 0.04],
            &[],
        );
        let start = body_wall(&world);
        lean_on(&mut world, V2::new(1., 0.));
        run(&mut world, 240);
        let size = world.arena_size();
        let state = world.body_state(3).unwrap();
        assert!(state.offset.y == 0.);
        assert!(
            state.offset.x <= 0.03 * size.x + 0.01 && state.offset.x > 0.03 * size.x - 2.,
            "it stops at its reach: {} of {}",
            state.offset.x,
            0.03 * size.x
        );
        assert!(body_wall(&world).x > start.x);
        world.tanks[0].input.movement = V2::ZERO;
        world.shove_body(3, V2::new(-300., 400.));
        assert_eq!(
            world.body_state(3).unwrap().velocity.y,
            0.,
            "it only slides across"
        );

        // Other cover in the way stops it before it overlaps.
        let mut world = crate_world(
            MapBody {
                reach: [0.2; 4],
                ..MapBody::default()
            },
            [0.15, 0.2, 0.04, 0.04],
            &[[0.21, 0.19, 0.04, 0.06]],
        );
        lean_on(&mut world, V2::new(1., 0.));
        run(&mut world, 240);
        let (moved, fixed) = (body_wall(&world), fixed_wall(&world));
        assert!(
            moved.x + moved.w <= fixed.x + 0.1,
            "{moved:?} against {fixed:?}"
        );
        assert!(moved.x + moved.w > fixed.x - 3., "but it gets there");
    }

    #[test]
    fn shoves_and_shots_move_bodies_by_their_weight() {
        let mut world = crate_world(
            MapBody {
                weight: 2.,
                shots: true,
                reach: [0.2; 4],
                ..MapBody::default()
            },
            [0.45, 0.45, 0.04, 0.04],
            &[],
        );
        world.tanks[0].pos = V2::new(200., 200.);
        assert!(world.shove_body(3, V2::new(0., 600.)));
        assert!(
            !world.shove_body(4, V2::new(0., 600.)),
            "group 4 isn't a body"
        );
        assert!(!world.shove_body(3, V2::new(f32::NAN, 0.)));
        let state = world.body_state(3).unwrap();
        assert!((state.velocity.y - 300.).abs() < 1e-3, "half for weight 2");
        run(&mut world, 60);
        let shoved = world.body_state(3).unwrap();
        assert!(shoved.offset.y > 20. && shoved.velocity.length() < 1.);
        // A heavy shot gives it a nudge.
        let index = world.wall_groups.iter().position(|&g| g == 3).unwrap();
        world.shot_hit_wall(index, V2::new(-900., 0.), 100.);
        world.step();
        assert!(world.body_state(3).unwrap().velocity.x < -10.);
    }

    #[test]
    fn tanks_cannot_push_a_body_before_making_contact() {
        let mut world = crate_world(MapBody::default(), [0.4, 0.45, 0.04, 0.04], &[]);
        lean_on(&mut world, V2::new(1., 0.));
        world.tanks[0].pos.x -= 10.;
        let body = world.map.bodies[0].clone();
        let (drive, pushers) = world.pushing(&body, V2::new(150., 0.));
        assert_eq!(
            pushers, 0,
            "a moving crate cannot be pushed across empty space"
        );
        assert_eq!(drive, V2::ZERO);
    }

    #[test]
    fn pushed_bodies_keep_pace_with_the_tank_by_their_weight() {
        for weight in [1., 1.5, 3.] {
            let mut world = crate_world(
                MapBody {
                    weight,
                    friction: 5.,
                    reach: [0.4; 4],
                    ..MapBody::default()
                },
                [0.2, 0.45, 0.04, 0.04],
                &[],
            );
            lean_on(&mut world, V2::new(1., 0.));
            // Up to speed first, then measure a second of steady pushing.
            run(&mut world, 30);
            let before = world.body_state(3).unwrap().offset.x;
            run(&mut world, 60);
            let pace = world.body_state(3).unwrap().offset.x - before;
            let tank = crate::physics::tank_speed(&world.tanks[0], &world.content)
                * world.config.rules.movement_multiplier;
            let expected = tank / weight.max(1.);
            assert!(
                pace > expected * 0.85 && pace < expected * 1.05,
                "weight {weight}: {pace:.0} a second, expected about {expected:.0}"
            );
        }
    }

    #[test]
    fn cooperating_pushers_do_not_make_a_light_body_outrun_them() {
        for weight in [1., 3.] {
            let mut world = crate_world(
                MapBody {
                    weight,
                    friction: 5.,
                    reach: [0.4; 4],
                    ..MapBody::default()
                },
                [0.2, 0.4, 0.04, 0.15],
                &[],
            );
            world.add_player("Helper", false);
            for _ in 0..240 {
                world.update_arena_size();
            }
            world.scale_map_walls();
            let wall = body_wall(&world);
            for (i, tank) in world.tanks.iter_mut().enumerate() {
                tank.pos = V2::new(
                    wall.x - tank.radius - 1.,
                    wall.y + wall.h * 0.5 + if i == 0 { -45. } else { 45. },
                );
                tank.vel = V2::ZERO;
                tank.input.movement = V2::new(1., 0.);
            }
            run(&mut world, 30);
            let before = world.body_state(3).unwrap().offset.x;
            run(&mut world, 60);
            let pace = world.body_state(3).unwrap().offset.x - before;
            let speed =
                crate::physics::movement_speed(&world.tanks[0], &world.content, &world.config);
            let expected = speed * (2. / weight).min(1.);
            assert!(
                pace > expected * 0.85 && pace < expected * 1.05,
                "weight {weight}: pace {pace}, expected {expected}"
            );
        }
    }

    #[test]
    fn bodies_never_crush_a_tank_against_cover() {
        let mut world = crate_world(
            MapBody {
                reach: [0.3; 4],
                ..MapBody::default()
            },
            [0.15, 0.2, 0.04, 0.04],
            &[[0.25, 0.15, 0.04, 0.14]],
        );
        let (body, fixed) = (body_wall(&world), fixed_wall(&world));
        let r = world.tanks[0].radius;
        world.tanks[0].pos = V2::new(fixed.x - r - 0.5, body.y + body.h * 0.5);
        for _ in 0..120 {
            world.shove_body(3, V2::new(1500., 0.));
            world.step();
        }
        // The arena may resize meanwhile, so measure everything now.
        let (moved, fixed, tank) = (body_wall(&world), fixed_wall(&world), &world.tanks[0]);
        assert!(
            world.body_state(3).unwrap().offset.x > 50.,
            "it slid toward the tank"
        );
        assert!(
            moved.x + moved.w <= tank.pos.x - tank.radius + 1.,
            "and stopped at the tank: {moved:?}, tank at {:?}",
            tank.pos
        );
        assert!(
            tank.pos.x + tank.radius <= fixed.x + 1.,
            "the tank isn't pushed into cover: tank at {:?} r {}, cover {fixed:?}",
            tank.pos,
            tank.radius
        );
    }
}
