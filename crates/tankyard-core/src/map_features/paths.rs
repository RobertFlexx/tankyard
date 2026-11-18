//! Keyframed paths and lighting changes. A path plays as a chain of ordinary
//! Move and Turn motions, one per point, so clients draw it from the same
//! motions they already receive and a moving path costs no extra traffic.
use super::*;
use tankyard_content::{MapPath, PathRepeat};

/// A group travelling along a path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PathPlay {
    pub group: u8,
    /// Index into the map's paths.
    pub path: usize,
    /// The point it is heading for or resting at.
    pub point: usize,
    /// For There and back: whether it is heading toward the last point.
    pub forward: bool,
    /// When it sets off for the next point.
    pub due: f32,
}

impl PathPlay {
    /// Whether another point follows the one it is heading for.
    fn continues(&self, path: &MapPath) -> bool {
        match path.repeat {
            PathRepeat::Once => self.point + 1 < path.points.len(),
            PathRepeat::Loop | PathRepeat::Bounce => path.points.len() > 1,
        }
    }

    /// Move on to the next point.
    fn advance(&mut self, path: &MapPath) {
        let last = path.points.len().saturating_sub(1);
        match path.repeat {
            PathRepeat::Once | PathRepeat::Loop => {
                self.point = (self.point + 1) % path.points.len().max(1);
            }
            PathRepeat::Bounce => {
                if self.forward && self.point >= last {
                    self.forward = false;
                } else if !self.forward && self.point == 0 {
                    self.forward = true;
                }
                self.point = if self.forward {
                    (self.point + 1).min(last)
                } else {
                    self.point.saturating_sub(1)
                };
            }
        }
    }
}

impl MapLogic {
    /// Play path `number` (counting from 1) on `group`, from wherever the
    /// group is now.
    pub(super) fn play_path(&mut self, map: &MapDef, group: u8, number: i32, now: f32) {
        if !(1..=MAP_GROUPS).contains(&group) {
            return;
        }
        let Some(index) = usize::try_from(number - 1)
            .ok()
            .filter(|&i| map.paths.get(i).is_some_and(|p| !p.points.is_empty()))
        else {
            return;
        };
        self.paths.retain(|play| play.group != group);
        let mut play = PathPlay {
            group,
            path: index,
            point: 0,
            forward: true,
            due: now,
        };
        self.head_for_point(map, &mut play, now);
        if play.continues(&map.paths[index]) {
            self.paths.push(play);
        }
    }

    /// Start the motions toward `play`'s current point.
    fn head_for_point(&mut self, map: &MapDef, play: &mut PathPlay, now: f32) {
        let point = map.paths[play.path].points[play.point];
        let seconds = point.seconds.clamp(0., tankyard_content::TWEEN_SECONDS);
        let from = self.tween_value(play.group, TweenChannel::Move, now);
        self.start_tween(Tween {
            group: play.group,
            channel: TweenChannel::Move,
            from,
            to: [point.at[0], point.at[1], 0., 0.],
            start: now,
            length: seconds,
            easing: point.easing,
        });
        let turn = self.tween_value(play.group, TweenChannel::Turn, now);
        if (turn[0] - point.turn).abs() > 1e-3 {
            self.start_tween(Tween {
                group: play.group,
                channel: TweenChannel::Turn,
                from: turn,
                to: [point.turn, 0., 0., 0.],
                start: now,
                length: seconds,
                easing: point.easing,
            });
        }
        play.due = now + seconds + point.pause.clamp(0., tankyard_content::TWEEN_SECONDS);
    }

    /// Send on the groups whose time at a point is up. Each group moves on
    /// at most one point a step, so a path of instant points can't stall a
    /// step however long it is.
    pub(super) fn advance_paths(&mut self, map: &MapDef, now: f32) {
        if !self.paths.iter().any(|play| play.due <= now) {
            return;
        }
        let mut plays = std::mem::take(&mut self.paths);
        plays.retain_mut(|play| {
            let Some(path) = map.paths.get(play.path) else {
                return false;
            };
            if play.due > now {
                return true;
            }
            play.advance(path);
            self.head_for_point(map, play, now);
            play.continues(path)
        });
        // A trigger acting during the loop may have started paths of its own.
        plays.append(&mut self.paths);
        self.paths = plays;
    }

    /// Change the lighting to `darkness` tinted `color` over `seconds`, and
    /// back again `back` seconds after it gets there when `back` is above 0.
    pub(super) fn light(
        &mut self,
        map: &MapDef,
        color: [u8; 3],
        darkness: f32,
        (seconds, back): (f32, f32),
        easing: Easing,
        now: f32,
    ) {
        let from = match self
            .tweens
            .iter()
            .find(|t| t.channel == TweenChannel::Ambient)
        {
            Some(tween) => tween.value(now),
            None => GroupPoses::default().lighting(&map.ambient),
        };
        let [r, g, b] = color.map(|c| f32::from(c) / 255.);
        let darkness = if darkness.is_finite() {
            darkness.clamp(0., tankyard_content::DARKNESS_LIMIT)
        } else {
            0.
        };
        // Lighting that starts from none takes its color at once, so a fade
        // into the dark never passes through another hue.
        let from = if from[3] <= 0. { [r, g, b, 0.] } else { from };
        let tween = Tween {
            group: 0,
            channel: TweenChannel::Ambient,
            from,
            to: [r, g, b, darkness],
            start: now,
            length: if seconds.is_finite() {
                seconds.clamp(0., tankyard_content::TWEEN_SECONDS)
            } else {
                0.
            },
            easing,
        };
        // Going back returns to the lighting before the first of several
        // changes in a row, so repeated flashes never leave it brighter.
        let ambient = |t: &Tween| t.group == 0 && t.channel == TweenChannel::Ambient;
        let home = self
            .returns
            .iter()
            .find(|(_, t)| ambient(t))
            .map_or(from, |(_, t)| t.to);
        self.returns.retain(|(_, t)| !ambient(t));
        if back > 0. && back.is_finite() && home != tween.to {
            self.returns.push((
                now + tween.length + back.min(tankyard_content::TWEEN_SECONDS),
                Tween {
                    from: tween.to,
                    to: home,
                    ..tween
                },
            ));
        }
        self.start_tween(tween);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::{MapBlock, MapTrigger, PathPoint, TriggerAction, TriggerWhen};

    fn world_with(edit: impl FnOnce(&mut MapDef)) -> World {
        let mut maps = tankyard_content::builtin_maps();
        edit(&mut maps[0]);
        tankyard_content::validate_maps(&maps).unwrap();
        World::with_maps_for_tests(
            WorldConfig {
                target_count: 0,
                ..WorldConfig::default()
            },
            maps,
        )
    }
    fn trigger(when: TriggerWhen, action: TriggerAction) -> MapTrigger {
        MapTrigger {
            when,
            action,
            rect: [0.4, 0.4, 0.2, 0.2],
            seconds: 0.,
            ..Default::default()
        }
    }
    fn block(rect: [f32; 4], group: u8) -> MapBlock {
        MapBlock {
            rect,
            style: Default::default(),
            color: None,
            group,
        }
    }
    /// A point reached in half a second, with a 0.3 second rest.
    fn point(x: f32, y: f32) -> PathPoint {
        PathPoint {
            at: [x, y],
            seconds: 0.5,
            pause: 0.3,
            ..PathPoint::default()
        }
    }
    fn run(world: &mut World, seconds: f32) {
        for _ in 0..(seconds * 60.).round() as usize {
            world.step();
        }
    }
    /// Group 5 travels path 1 from the start; counter 1 counts its arrivals.
    fn travelling(repeat: PathRepeat) -> World {
        world_with(|map| {
            map.blocks.push(block([0.2, 0.2, 0.03, 0.03], 5));
            map.paths.push(MapPath {
                name: "Patrol".into(),
                points: vec![point(0.1, 0.), point(0.1, 0.1), point(0., 0.1)],
                repeat,
            });
            map.triggers.push(MapTrigger {
                group: 5,
                amount: 1,
                ..trigger(TriggerWhen::Start, TriggerAction::Path)
            });
            map.triggers.push(MapTrigger {
                source: 5,
                group: 1,
                amount: 1,
                ..trigger(TriggerWhen::Arrived, TriggerAction::Count)
            });
        })
    }
    fn offset(world: &World) -> [f32; 2] {
        world.group_poses().offset(5)
    }
    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn paths_visit_each_point_and_arrive_only_at_the_end() {
        // Legs end at 0.5, 1.3 and 2.1 seconds, each followed by a rest.
        let mut world = travelling(PathRepeat::Once);
        run(&mut world, 0.65);
        assert!(near(offset(&world), [0.1, 0.]), "{:?}", offset(&world));
        run(&mut world, 0.8);
        assert!(near(offset(&world), [0.1, 0.1]), "{:?}", offset(&world));
        assert_eq!(world.match_state.counters[0], 0, "no arrival mid-path");
        run(&mut world, 0.85);
        assert!(near(offset(&world), [0., 0.1]));
        assert_eq!(world.match_state.counters[0], 1, "one arrival, at the end");
        run(&mut world, 2.);
        assert!(near(offset(&world), [0., 0.1]), "and it stays");
        assert_eq!(world.match_state.counters[0], 1);
    }

    #[test]
    fn looping_and_bouncing_paths_carry_on() {
        // The last point is left at 2.4 seconds; the next leg ends at 2.9.
        let mut world = travelling(PathRepeat::Loop);
        run(&mut world, 3.05);
        assert!(near(offset(&world), [0.1, 0.]), "{:?}", offset(&world));
        assert_eq!(world.match_state.counters[0], 0, "a loop never arrives");

        // Out to the last point, then back through the middle one.
        let mut world = travelling(PathRepeat::Bounce);
        run(&mut world, 3.05);
        assert!(near(offset(&world), [0.1, 0.1]), "{:?}", offset(&world));
        run(&mut world, 0.8);
        assert!(near(offset(&world), [0.1, 0.]), "{:?}", offset(&world));
    }

    #[test]
    fn a_move_or_stop_takes_a_group_off_its_path() {
        let mut world = travelling(PathRepeat::Loop);
        run(&mut world, 0.3);
        world.map_input(LogicInput::Stop(5));
        run(&mut world, 0.1);
        let held = offset(&world);
        run(&mut world, 2.);
        assert!(near(offset(&world), held), "stopped where it was");
    }

    #[test]
    fn touch_and_release_follow_blocks_on_and_off_an_area() {
        let mut world = world_with(|map| {
            map.blocks.push(block([0.2, 0.2, 0.03, 0.03], 6));
            // Slide over the plate, then back 1.5 s later.
            map.triggers.push(MapTrigger {
                group: 6,
                travel: [0.1, 0.],
                tween: 0.5,
                duration: 1.5,
                ..trigger(TriggerWhen::Start, TriggerAction::Place)
            });
            let plate = [0.3, 0.2, 0.05, 0.05];
            map.triggers.push(MapTrigger {
                rect: plate,
                source: 6,
                group: 1,
                amount: 1,
                ..trigger(TriggerWhen::Touch, TriggerAction::Count)
            });
            map.triggers.push(MapTrigger {
                rect: plate,
                source: 6,
                group: 2,
                amount: 1,
                ..trigger(TriggerWhen::Release, TriggerAction::Count)
            });
        });
        run(&mut world, 0.1);
        assert_eq!(world.match_state.counters[..2], [0, 0], "not there yet");
        run(&mut world, 0.6);
        assert_eq!(world.match_state.counters[..2], [1, 0], "touched once");
        run(&mut world, 1.0);
        assert_eq!(world.match_state.counters[..2], [1, 0], "resting on it");
        run(&mut world, 1.0);
        assert_eq!(world.match_state.counters[..2], [1, 1], "released");
    }

    #[test]
    fn fling_blast_and_zoom_act_on_tanks_inside() {
        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                amount: 900,
                angle: 270.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Fling)
            });
            map.triggers.push(MapTrigger {
                amount: 60,
                tween: 3.,
                ..trigger(TriggerWhen::Enter, TriggerAction::Zoom)
            });
        });
        let id = world.add_player("Pilot", false);
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        world.tanks[0].pos = center;
        let mut zoom = None;
        for _ in 0..2 {
            world.step();
            zoom = zoom.or(world
                .events
                .iter()
                .find(|e| matches!(e.kind, EventKind::MapZoom) && e.subject == id)
                .map(|e| crate::zoom_of(e.value)));
        }
        let tank = &world.tanks[0];
        assert!(tank.vel.y < -400., "flung upward: {:?}", tank.vel);
        let zoom = zoom.expect("zoomed");
        assert_eq!((zoom.scale, zoom.seconds), (0.6, Some(3.)));
        assert_eq!(zoom.ease, crate::ZoomView::EASE, "the usual change");

        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                amount: 700,
                seconds: 1.,
                ..trigger(TriggerWhen::Every, TriggerAction::Blast)
            });
        });
        world.add_player("Near", false);
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        world.tanks[0].pos = center + V2::new(80., 0.);
        world.tanks[0].vel = V2::ZERO;
        run(&mut world, 1.05);
        assert!(
            world.tanks[0].vel.x > 300.,
            "thrown out: {:?}",
            world.tanks[0].vel
        );
    }

    #[test]
    fn lighting_fades_in_as_one_tween_players_receive() {
        let mut world = world_with(|map| {
            map.triggers.push(MapTrigger {
                amount: 80,
                color: [10, 20, 60],
                tween: 2.,
                ..trigger(TriggerWhen::Start, TriggerAction::Light)
            });
        });
        run(&mut world, 1.);
        let snapshot = world.snapshot();
        let ambient = tankyard_content::MapAmbient::default();
        let halfway = snapshot_poses(&snapshot, snapshot.match_state.elapsed).lighting(&ambient);
        assert!(halfway[3] > 0.2 && halfway[3] < 0.7, "{halfway:?}");
        assert!(
            (halfway[2] - 60. / 255.).abs() < 0.01,
            "the color is set at once"
        );
        run(&mut world, 1.5);
        let snapshot = world.snapshot();
        let dark = snapshot_poses(&snapshot, snapshot.match_state.elapsed).lighting(&ambient);
        assert!((dark[3] - 0.8).abs() < 1e-3, "{dark:?}");
    }

    #[test]
    fn lightning_flashes_and_goes_back_to_the_dark() {
        let mut world = world_with(|map| {
            map.ambient.darkness = 0.8;
            map.triggers.push(MapTrigger {
                amount: 0,
                seconds: 1.,
                duration: 0.2,
                ..trigger(TriggerWhen::Every, TriggerAction::Light)
            });
        });
        let darkness = |world: &World| {
            let snapshot = world.snapshot();
            snapshot_poses(&snapshot, snapshot.match_state.elapsed).lighting(&world.map.ambient)[3]
        };
        assert!((darkness(&world) - 0.8).abs() < 1e-4);
        run(&mut world, 1.05);
        assert!(darkness(&world) < 1e-4, "the flash");
        run(&mut world, 0.25);
        assert!((darkness(&world) - 0.8).abs() < 1e-4, "dark again");
        // Flash after flash, it always returns to the same dark.
        run(&mut world, 3.);
        assert!((darkness(&world) - 0.8).abs() < 1e-4);
    }
}
