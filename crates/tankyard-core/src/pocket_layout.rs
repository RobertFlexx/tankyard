//! One aligned room plan for collision, navigation and drawing. Internal wall
//! joins are removed from the outline, so a T junction has no seam through it.
use crate::{
    Rect, V2, WorldConfig,
    physics::{self, WallIndex},
};
use std::cell::RefCell;

pub const ROOM_SPAN: f32 = 400.;
pub const WALL_WIDTH: f32 = 48.;
const NAV_RADIUS: f32 = 26.;
const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// How a ceiling fixture behaves. Most hum steadily; a few fail or are dead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightState {
    Steady,
    Failing,
    Dead,
}

#[derive(Clone, Copy, Debug)]
pub struct PocketLight {
    pub pos: V2,
    pub state: LightState,
}

#[derive(Clone, Copy, Debug)]
pub struct PocketEdge {
    pub a: V2,
    pub b: V2,
    pub normal: V2,
}

pub struct PocketLayout {
    pub bounds: Rect,
    pub walls: Vec<Rect>,
    pub edges: Vec<PocketEdge>,
    pub room_size: V2,
    pub lights: Vec<PocketLight>,
    seed: u64,
    index: RefCell<WallIndex>,
    nodes: Vec<V2>,
    links: Vec<u8>,
    columns: usize,
    rows: usize,
    step: V2,
    flow: RefCell<Flow>,
}

struct Flow {
    goal: usize,
    distance: Vec<u16>,
    queue: Vec<usize>,
    cursor: usize,
}

fn key(seed: u64, x: usize, y: usize, axis: u64) -> u64 {
    let v = seed
        ^ (x as u64).wrapping_mul(0x517cc1b727220a95)
        ^ (y as u64).wrapping_mul(0x6eed0e9da4d94a4f)
        ^ axis;
    (v ^ (v >> 27)).wrapping_mul(0x9e3779b97f4a7c15)
}

/// Where a doorway sits along its wall: centred, or shifted a fixed step
/// either way. The step leaves a stub at least a wall thick at the corner, so
/// shifted doorways still line up with the room grid.
fn doorway(seed: u64, span: f32, gap: f32) -> f32 {
    let step = (span * 0.5 - gap * 0.5 - WALL_WIDTH * 1.5)
        .min(span * 0.14)
        .max(0.);
    match (seed >> 8) % 4 {
        1 => -step,
        2 => step,
        _ => 0.,
    }
}

pub fn bounds(config: &WorldConfig) -> Rect {
    // Population changes in the arena must not move private rooms underfoot.
    Rect {
        x: 0.,
        y: 0.,
        w: config.width,
        h: config.height,
    }
}

impl PocketLayout {
    pub(crate) fn new(config: &WorldConfig) -> Self {
        let bounds = bounds(config);
        let columns = (bounds.w / ROOM_SPAN).round().clamp(1., 64.) as usize;
        let rows = (bounds.h / ROOM_SPAN).round().clamp(1., 64.) as usize;
        let room_size = V2::new(bounds.w / columns as f32, bounds.h / rows as f32);
        let half = WALL_WIDTH * 0.5;
        let mut walls = Vec::with_capacity(columns * rows * 4 + 4);
        for y in 0..rows {
            for x in 1..columns {
                let seed = key(config.seed, x, y, 0);
                if seed.is_multiple_of(5) {
                    continue;
                }
                let gap = 144_f32.min(room_size.y * 0.48);
                let center = (y as f32 + 0.5) * room_size.y + doorway(seed, room_size.y, gap);
                let top = y as f32 * room_size.y - half;
                let bottom = (y + 1) as f32 * room_size.y + half;
                walls.push(Rect {
                    x: x as f32 * room_size.x - half,
                    y: top,
                    w: WALL_WIDTH,
                    h: center - gap * 0.5 - top,
                });
                walls.push(Rect {
                    x: x as f32 * room_size.x - half,
                    y: center + gap * 0.5,
                    w: WALL_WIDTH,
                    h: bottom - center - gap * 0.5,
                });
            }
        }
        for y in 1..rows {
            for x in 0..columns {
                let seed = key(config.seed, x, y, 0xa0761d6478bd642f);
                if seed.is_multiple_of(5) {
                    continue;
                }
                let gap = 144_f32.min(room_size.x * 0.48);
                let center = (x as f32 + 0.5) * room_size.x + doorway(seed, room_size.x, gap);
                let left = x as f32 * room_size.x - half;
                let right = (x + 1) as f32 * room_size.x + half;
                walls.push(Rect {
                    x: left,
                    y: y as f32 * room_size.y - half,
                    w: center - gap * 0.5 - left,
                    h: WALL_WIDTH,
                });
                walls.push(Rect {
                    x: center + gap * 0.5,
                    y: y as f32 * room_size.y - half,
                    w: right - center - gap * 0.5,
                    h: WALL_WIDTH,
                });
            }
        }
        walls.extend([
            Rect {
                x: -WALL_WIDTH,
                y: -WALL_WIDTH,
                w: bounds.w + 2. * WALL_WIDTH,
                h: WALL_WIDTH,
            },
            Rect {
                x: -WALL_WIDTH,
                y: bounds.h,
                w: bounds.w + 2. * WALL_WIDTH,
                h: WALL_WIDTH,
            },
            Rect {
                x: -WALL_WIDTH,
                y: 0.,
                w: WALL_WIDTH,
                h: bounds.h,
            },
            Rect {
                x: bounds.w,
                y: 0.,
                w: WALL_WIDTH,
                h: bounds.h,
            },
        ]);
        merge_runs(&mut walls);
        let edges = outline(&walls);
        let mut lights = Vec::with_capacity(columns * rows);
        for y in 0..rows {
            for x in 0..columns {
                let seed = key(config.seed, x, y, 0xe7037ed1a0b428db);
                lights.push(PocketLight {
                    pos: V2::new(
                        (x as f32 + 0.5) * room_size.x,
                        (y as f32 + 0.5) * room_size.y,
                    ),
                    state: match seed % 16 {
                        0 => LightState::Dead,
                        1 | 2 => LightState::Failing,
                        _ => LightState::Steady,
                    },
                });
            }
        }
        let mut index = WallIndex::new(bounds.w, bounds.h);
        index.sync(&walls);
        // Nodes fit every doorway with room to spare for a double's hull.
        let nav_columns = (bounds.w / 56.).ceil() as usize;
        let nav_rows = (bounds.h / 56.).ceil() as usize;
        let step = V2::new(bounds.w / nav_columns as f32, bounds.h / nav_rows as f32);
        let count = nav_columns * nav_rows;
        let mut nodes = Vec::with_capacity(count);
        let mut open = vec![false; count];
        for y in 0..nav_rows {
            for x in 0..nav_columns {
                let p = V2::new((x as f32 + 0.5) * step.x, (y as f32 + 0.5) * step.y);
                open[y * nav_columns + x] = bounds.contains_circle(p, NAV_RADIUS)
                    && index
                        .near(p, NAV_RADIUS)
                        .iter()
                        .all(|&wall| physics::circle_rect(p, NAV_RADIUS, wall).is_none());
                nodes.push(p);
            }
        }
        let mut links = vec![0_u8; count];
        for i in 0..count {
            if !open[i] {
                continue;
            }
            let x = (i % nav_columns) as i32;
            let y = (i / nav_columns) as i32;
            for (direction, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= nav_columns as i32 || ny >= nav_rows as i32 {
                    continue;
                }
                let n = ny as usize * nav_columns + nx as usize;
                if open[n]
                    && index
                        .query(nodes[i], nodes[n], NAV_RADIUS)
                        .iter()
                        .all(|&wall| {
                            physics::sweep_rect(nodes[i], nodes[n], NAV_RADIUS, walls[wall])
                                .is_none()
                        })
                {
                    links[i] |= 1 << direction;
                }
            }
        }
        Self {
            bounds,
            seed: config.seed,
            walls,
            edges,
            room_size,
            lights,
            index: RefCell::new(index),
            nodes,
            links,
            columns: nav_columns,
            rows: nav_rows,
            step,
            flow: RefCell::new(Flow {
                goal: usize::MAX,
                distance: vec![u16::MAX; count],
                queue: Vec::with_capacity(count),
                cursor: 0,
            }),
        }
    }

    pub(crate) fn matches(&self, config: &WorldConfig) -> bool {
        self.seed == config.seed && self.bounds.w == config.width && self.bounds.h == config.height
    }

    pub fn clear(&self, from: V2, to: V2, radius: f32) -> bool {
        self.bounds.contains_circle(to, radius)
            && self
                .index
                .borrow_mut()
                .query(from, to, radius)
                .iter()
                .all(|&i| physics::sweep_rect(from, to, radius, self.walls[i]).is_none())
    }

    fn node(&self, pos: V2, radius: f32) -> Option<usize> {
        let x = (pos.x / self.step.x).floor() as i32;
        let y = (pos.y / self.step.y).floor() as i32;
        let mut best: Option<(f32, usize)> = None;
        for dy in -2..=2 {
            for dx in -2..=2 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= self.columns as i32 || ny >= self.rows as i32 {
                    continue;
                }
                let i = ny as usize * self.columns + nx as usize;
                let d = (self.nodes[i] - pos).length_squared();
                if self.links[i] != 0
                    && best.is_none_or(|b| d < b.0)
                    && self.clear(pos, self.nodes[i], radius)
                {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    pub(crate) fn route(&self, from: V2, target: V2, radius: f32) -> Option<V2> {
        if self.clear(from, target, radius) {
            return Some(target);
        }
        let goal = self.node(target, radius)?;
        let current = self.node(from, radius)?;
        let mut flow = self.flow.borrow_mut();
        if flow.goal != goal {
            flow.goal = goal;
            flow.distance.fill(u16::MAX);
            flow.queue.clear();
            flow.distance[goal] = 0;
            flow.queue.push(goal);
            flow.cursor = 0;
        }
        // Nearby pursuers need only a small part of a large community arena.
        // Continue the same search for the next double instead of flooding the
        // whole map whenever its visitor enters a new navigation cell.
        while flow.distance[current] == u16::MAX && flow.cursor < flow.queue.len() {
            let i = flow.queue[flow.cursor];
            flow.cursor += 1;
            for (direction, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
                if self.links[i] & (1 << direction) == 0 {
                    continue;
                }
                let n = (i as isize + dy as isize * self.columns as isize + dx as isize) as usize;
                if flow.distance[n] == u16::MAX {
                    flow.distance[n] = flow.distance[i].saturating_add(1);
                    flow.queue.push(n);
                }
            }
        }
        if flow.distance[current] == u16::MAX {
            return None;
        }
        let mut next = current;
        for (direction, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
            if self.links[current] & (1 << direction) == 0 {
                continue;
            }
            let n = (current as isize + dy as isize * self.columns as isize + dx as isize) as usize;
            if flow.distance[n] < flow.distance[next] && self.clear(from, self.nodes[n], radius) {
                next = n;
            }
        }
        Some(self.nodes[next])
    }

    pub(crate) fn spawn(&self, player: V2, wanted: V2, used: &[V2]) -> Option<V2> {
        self.nodes
            .iter()
            .zip(&self.links)
            .filter(|(p, links)| {
                **links != 0
                    && p.distance(player) >= 340.
                    && p.distance(player) <= 760.
                    && used.iter().all(|other| p.distance(*other) > 180.)
            })
            .min_by(|(a, _), (b, _)| a.distance(wanted).total_cmp(&b.distance(wanted)))
            .map(|(&pos, _)| pos)
    }

    pub(crate) fn slide(&self, mut pos: V2, mut movement: V2, radius: f32) -> V2 {
        for _ in 0..3 {
            let end = pos + movement;
            let mut hit: Option<(f32, V2)> = None;
            let mut index = self.index.borrow_mut();
            for &wall in index.query(pos, end, radius) {
                if let Some(collision) = physics::sweep_rect(pos, end, radius, self.walls[wall])
                    && movement.dot(collision.1) < -1e-6
                    && hit.is_none_or(|old| collision.0 < old.0)
                {
                    hit = Some(collision);
                }
            }
            let Some((at, normal)) = hit else {
                pos = end;
                break;
            };
            pos += movement * (at - 0.001).max(0.);
            movement *= 1. - at;
            movement -= normal * movement.dot(normal).min(0.);
            if movement.length_squared() < 0.0001 {
                break;
            }
        }
        self.bounds.clamp_circle(pos, radius)
    }
}

// Merge collinear overlap before finding the union's exterior.
fn merge_runs(walls: &mut Vec<Rect>) {
    for vertical in [true, false] {
        walls.sort_by(|a, b| {
            let (line_a, line_b, start_a, start_b) = if vertical {
                (a.x, b.x, a.y, b.y)
            } else {
                (a.y, b.y, a.x, b.x)
            };
            line_a.total_cmp(&line_b).then(start_a.total_cmp(&start_b))
        });
        let mut out: Vec<Rect> = Vec::with_capacity(walls.len());
        for wall in walls.drain(..) {
            if let Some(last) = out.last_mut() {
                let joins = if vertical {
                    (last.x - wall.x).abs() < 0.01
                        && (last.w - wall.w).abs() < 0.01
                        && wall.y <= last.y + last.h + 0.01
                } else {
                    (last.y - wall.y).abs() < 0.01
                        && (last.h - wall.h).abs() < 0.01
                        && wall.x <= last.x + last.w + 0.01
                };
                if joins {
                    if vertical {
                        last.h = (wall.y + wall.h).max(last.y + last.h) - last.y;
                    } else {
                        last.w = (wall.x + wall.w).max(last.x + last.w) - last.x;
                    }
                    continue;
                }
            }
            out.push(wall);
        }
        *walls = out;
    }
}

fn outline(walls: &[Rect]) -> Vec<PocketEdge> {
    let mut xs: Vec<f32> = walls.iter().flat_map(|r| [r.x, r.x + r.w]).collect();
    let mut ys: Vec<f32> = walls.iter().flat_map(|r| [r.y, r.y + r.h]).collect();
    for values in [&mut xs, &mut ys] {
        values.sort_by(f32::total_cmp);
        values.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    }
    let width = xs.len() - 1;
    let height = ys.len() - 1;
    let mut solid = vec![false; width * height];
    // The nearest grid line, so a merged end that rounds a little differently
    // from the coordinate it came from still lands on its own line.
    let at = |values: &[f32], value: f32| {
        let after = values.partition_point(|v| *v < value);
        match (after.checked_sub(1), values.get(after)) {
            (Some(before), Some(next)) if value - values[before] < next - value => before,
            (Some(before), None) => before,
            _ => after,
        }
    };
    for r in walls {
        for y in at(&ys, r.y)..at(&ys, r.y + r.h) {
            for x in at(&xs, r.x)..at(&xs, r.x + r.w) {
                solid[y * width + x] = true;
            }
        }
    }
    let mut edges: Vec<PocketEdge> = Vec::new();
    // Each row/column scan coalesces exposed runs, never drawing an inner seam.
    for y in 0..=height {
        for normal in [-1., 1.] {
            let mut start = None;
            for x in 0..=width {
                let above = y > 0 && x < width && solid[(y - 1) * width + x];
                let below = y < height && x < width && solid[y * width + x];
                let exposed = if normal < 0. {
                    below && !above
                } else {
                    above && !below
                };
                if exposed && start.is_none() {
                    start = Some(x);
                }
                if !exposed && let Some(from) = start.take() {
                    edges.push(PocketEdge {
                        a: V2::new(xs[from], ys[y]),
                        b: V2::new(xs[x], ys[y]),
                        normal: V2::new(0., normal),
                    });
                }
            }
        }
    }
    for x in 0..=width {
        for normal in [-1., 1.] {
            let mut start = None;
            for y in 0..=height {
                let left = x > 0 && y < height && solid[y * width + x - 1];
                let right = x < width && y < height && solid[y * width + x];
                let exposed = if normal < 0. {
                    right && !left
                } else {
                    left && !right
                };
                if exposed && start.is_none() {
                    start = Some(y);
                }
                if !exposed && let Some(from) = start.take() {
                    edges.push(PocketEdge {
                        a: V2::new(xs[x], ys[from]),
                        b: V2::new(xs[x], ys[y]),
                        normal: V2::new(normal, 0.),
                    });
                }
            }
        }
    }
    edges
}

thread_local! { static LAYOUT: RefCell<Option<PocketLayout>> = const { RefCell::new(None) }; }
pub fn with_layout<R>(config: &WorldConfig, draw: impl FnOnce(&PocketLayout) -> R) -> R {
    LAYOUT.with(|slot| {
        if slot
            .borrow()
            .as_ref()
            .is_none_or(|old| !old.matches(config))
        {
            *slot.borrow_mut() = Some(PocketLayout::new(config));
        }
        let borrowed = slot.borrow();
        draw(borrowed.as_ref().unwrap())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooms_join_cleanly_and_every_doorway_connects_at_hull_width() {
        for (width, height) in [
            (900., 700.),
            (3600., 2600.),
            (16000., 700.),
            (16000., 16000.),
        ] {
            let layout = PocketLayout::new(&WorldConfig {
                width,
                height,
                ..Default::default()
            });
            let inside = |p: V2| {
                layout.walls.iter().any(|wall| {
                    p.x > wall.x && p.x < wall.x + wall.w && p.y > wall.y && p.y < wall.y + wall.h
                })
            };
            for edge in &layout.edges {
                let mid = (edge.a + edge.b) * 0.5;
                assert!(
                    inside(mid - edge.normal * 0.1),
                    "missing wall behind {edge:?}"
                );
                assert!(
                    !inside(mid + edge.normal * 0.1),
                    "interior seam at {edge:?}"
                );
            }
            // Every room centre is open floor, and every traversable node
            // reaches the same maze, including narrow arenas and the largest
            // supported community server.
            let start = layout.links.iter().position(|&links| links != 0).unwrap();
            let mut seen = vec![false; layout.nodes.len()];
            let mut queue = vec![start];
            seen[start] = true;
            let mut at = 0;
            while at < queue.len() {
                let i = queue[at];
                at += 1;
                for (direction, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
                    if layout.links[i] & (1 << direction) == 0 {
                        continue;
                    }
                    let n =
                        (i as isize + dy as isize * layout.columns as isize + dx as isize) as usize;
                    if !seen[n] {
                        seen[n] = true;
                        queue.push(n);
                    }
                }
            }
            assert!(
                layout
                    .links
                    .iter()
                    .enumerate()
                    .all(|(i, &links)| links == 0 || seen[i])
            );
            for light in &layout.lights {
                assert!(layout.clear(light.pos, light.pos, NAV_RADIUS));
                let node = layout
                    .node(light.pos, NAV_RADIUS)
                    .expect("a room with no floor");
                assert!(seen[node], "a sealed room at {light:?}");
            }
            // Shifted doorways keep a stub of wall at each corner.
            assert!(
                layout
                    .walls
                    .iter()
                    .all(|w| w.w >= WALL_WIDTH - 0.01 && w.h >= WALL_WIDTH - 0.01)
            );
        }
    }

    #[test]
    fn doorways_vary_and_a_few_fixtures_fail() {
        let layout = PocketLayout::new(&WorldConfig::default());
        let states = |state| layout.lights.iter().filter(|l| l.state == state).count();
        assert!(states(LightState::Steady) > layout.lights.len() / 2);
        assert!(states(LightState::Failing) + states(LightState::Dead) > 0);
        let large = PocketLayout::new(&WorldConfig {
            width: 8000.,
            height: 8000.,
            ..Default::default()
        });
        // Interior walls end on the room grid; their doorways do not all
        // sit at the middle of the wall.
        let lengths: std::collections::BTreeSet<i32> = large
            .walls
            .iter()
            .map(|w| w.w.max(w.h).round() as i32)
            .collect();
        assert!(lengths.len() > 3, "every doorway is centred: {lengths:?}");
    }

    #[test]
    fn pursuit_routes_around_a_wall_to_a_visitor_hugging_its_other_side() {
        let layout = PocketLayout::new(&WorldConfig {
            width: 1200.,
            height: 1200.,
            seed: 1,
            ..Default::default()
        });
        let mut from = V2::new(200., 80.);
        let target = V2::new(443.1, 80.);
        assert!(!layout.clear(from, target, 19.));
        for _ in 0..600 {
            let goal = layout
                .route(from, target, 19.)
                .expect("route around the doorway");
            let movement = (goal - from).clamp_length(180. * crate::DT);
            let next = layout.slide(from, movement, 19.);
            assert!(
                layout.clear(next, next, 18.99),
                "a double entered a wall at {next:?}"
            );
            from = next;
            if from.distance(target) < 2. {
                break;
            }
        }
        assert!(from.distance(target) < 2., "stuck at {from:?}");
    }

    #[test]
    fn room_positions_stay_fixed_when_the_shared_arena_resizes() {
        let mut config = WorldConfig::default();
        let layout = PocketLayout::new(&config);
        config.arena_scale = 0.5;
        assert!(layout.matches(&config));
        assert_eq!(bounds(&config).w, config.width);
        assert_eq!(layout.walls, PocketLayout::new(&config).walls);
    }
}
