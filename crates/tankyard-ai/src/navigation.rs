//! Clearance-aware, bounded A* for rectangular arena walls.
use std::{cmp::Reverse, collections::BinaryHeap};
use tankyard_core::{Rect, V2, physics::sweep_rect};

pub(crate) fn clear_line(start: V2, end: V2, clearance: f32, walls: &[Rect]) -> bool {
    walls
        .iter()
        .all(|&wall| sweep_rect(start, end, clearance, wall).is_none())
}

pub(crate) struct Navigation {
    cols: usize,
    rows: usize,
    cell: f32,
    width: f32,
    height: f32,
    signature: u64,
    clearance_squared: Vec<f32>,
    costs: Vec<u32>,
    parents: Vec<usize>,
    open: BinaryHeap<Reverse<(u32, usize)>>,
}

impl Default for Navigation {
    fn default() -> Self {
        Self {
            cols: 0,
            rows: 0,
            cell: 80.,
            width: 0.,
            height: 0.,
            signature: 0,
            clearance_squared: vec![],
            costs: vec![],
            parents: vec![],
            open: BinaryHeap::new(),
        }
    }
}

impl Navigation {
    pub fn refresh(&mut self, width: f32, height: f32, walls: &[Rect]) {
        let mut signature = u64::from(width.to_bits()) ^ (u64::from(height.to_bits()) << 32);
        for wall in walls {
            for value in [wall.x, wall.y, wall.w, wall.h] {
                signature = signature.rotate_left(7) ^ u64::from(value.to_bits());
            }
        }
        if self.cols > 0 && self.signature == signature {
            return;
        }
        self.signature = signature;
        self.width = width;
        self.height = height;
        self.cols = (width / self.cell).ceil().clamp(1., 256.) as usize;
        self.rows = (height / self.cell).ceil().clamp(1., 256.) as usize;
        self.clearance_squared.resize(self.cols * self.rows, 0.);
        self.costs.resize(self.cols * self.rows, u32::MAX);
        self.parents.resize(self.cols * self.rows, usize::MAX);
        for index in 0..self.clearance_squared.len() {
            let p = self.center(index);
            let border = p.x.min(p.y).min(width - p.x).min(height - p.y).max(0.);
            self.clearance_squared[index] = walls.iter().fold(border * border, |best, wall| {
                let nearest = V2::new(
                    p.x.clamp(wall.x, wall.x + wall.w),
                    p.y.clamp(wall.y, wall.y + wall.h),
                );
                best.min((p - nearest).length_squared())
            });
        }
    }

    pub fn cell_of(&self, p: V2) -> usize {
        let x = (p.x / self.cell).max(0.) as usize;
        let y = (p.y / self.cell).max(0.) as usize;
        y.min(self.rows.saturating_sub(1)) * self.cols + x.min(self.cols.saturating_sub(1))
    }

    fn center(&self, index: usize) -> V2 {
        V2::new(
            ((index % self.cols) as f32 + 0.5) * self.cell,
            ((index / self.cols) as f32 + 0.5) * self.cell,
        )
    }

    fn nearest_free(
        &self,
        p: V2,
        clearance: f32,
        walls: &[Rect],
        require_connection: bool,
    ) -> Option<usize> {
        let cell = self.cell_of(p);
        let reachable = |index| {
            self.clearance_squared[index] > clearance * clearance
                && (!require_connection || clear_line(p, self.center(index), clearance, walls))
        };
        if reachable(cell) {
            return Some(cell);
        }
        let x = (cell % self.cols) as isize;
        let y = (cell / self.cols) as isize;
        for ring in 1_isize..=12 {
            let mut best: Option<(f32, usize)> = None;
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= self.cols as isize || ny >= self.rows as isize {
                        continue;
                    }
                    let index = ny as usize * self.cols + nx as usize;
                    if reachable(index) {
                        let distance = (self.center(index) - p).length_squared();
                        if best.is_none_or(|(previous, _)| distance < previous) {
                            best = Some((distance, index));
                        }
                    }
                }
            }
            if let Some((_, index)) = best {
                return Some(index);
            }
        }
        None
    }

    fn heuristic(&self, a: usize, b: usize) -> u32 {
        let dx = (a % self.cols).abs_diff(b % self.cols) as u32;
        let dy = (a / self.cols).abs_diff(b / self.cols) as u32;
        10 * dx.max(dy) + 4 * dx.min(dy)
    }

    /// Returns an empty route if the bounded search cannot reach the goal.
    /// No unbounded pathfinding or exponential recursive searches are used.
    pub fn route(&mut self, start: V2, goal: V2, walls: &[Rect], clearance: f32) -> Vec<V2> {
        if clear_line(start, goal, clearance, walls) {
            return vec![goal];
        }
        let (Some(from), Some(to)) = (
            self.nearest_free(start, clearance, walls, true),
            // Exploration and combat spacing can place a goal inside a wall.
            // In that case approach a free cell beside it instead of stalling.
            self.nearest_free(
                goal,
                clearance,
                walls,
                clear_line(goal, goal, clearance, walls),
            ),
        ) else {
            return vec![];
        };
        self.costs.fill(u32::MAX);
        self.parents.fill(usize::MAX);
        self.open.clear();
        self.costs[from] = 0;
        self.open.push(Reverse((self.heuristic(from, to), from)));
        let mut expansions = 0;
        let mut found = false;
        while let Some(Reverse((estimate, index))) = self.open.pop() {
            if estimate != self.costs[index].saturating_add(self.heuristic(index, to)) {
                continue;
            }
            if index == to {
                found = true;
                break;
            }
            expansions += 1;
            if expansions > 4096 {
                break;
            }
            let (x, y) = ((index % self.cols) as isize, (index / self.cols) as isize);
            for (dx, dy) in [
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (-1, 1),
                (1, -1),
                (1, 1),
            ] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= self.cols as isize || ny >= self.rows as isize {
                    continue;
                }
                let next = ny as usize * self.cols + nx as usize;
                if self.clearance_squared[next] <= clearance * clearance {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    // Prevent cutting a corner between two inflated walls.
                    let side_x = y as usize * self.cols + nx as usize;
                    let side_y = ny as usize * self.cols + x as usize;
                    if self.clearance_squared[side_x] <= clearance * clearance
                        || self.clearance_squared[side_y] <= clearance * clearance
                    {
                        continue;
                    }
                }
                let cost = self.costs[index] + if dx != 0 && dy != 0 { 14 } else { 10 };
                if cost < self.costs[next]
                    && clear_line(self.center(index), self.center(next), clearance, walls)
                {
                    self.costs[next] = cost;
                    self.parents[next] = index;
                    self.open
                        .push(Reverse((cost + self.heuristic(next, to), next)));
                }
            }
        }
        if !found {
            return vec![];
        }
        let mut points = Vec::with_capacity(32);
        let mut cursor = to;
        while cursor != from {
            points.push(self.center(cursor));
            cursor = self.parents[cursor];
            if cursor == usize::MAX || points.len() > 4096 {
                return vec![];
            }
        }
        points.push(self.center(from));
        points.reverse();
        if clear_line(*points.last().unwrap_or(&start), goal, clearance, walls) {
            points.push(goal);
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_goes_around_a_wall_with_hull_clearance() {
        let walls = [Rect {
            x: 440.,
            y: 180.,
            w: 100.,
            h: 360.,
        }];
        let mut nav = Navigation::default();
        nav.refresh(1200., 800., &walls);
        let start = V2::new(220., 350.);
        let goal = V2::new(900., 350.);
        let route = nav.route(start, goal, &walls, 30.);
        assert!(!route.is_empty());
        let mut previous = start;
        for &point in &route {
            assert!(clear_line(previous, point, 30., &walls));
            previous = point;
        }
        assert!(previous.distance(goal) < 1.);
    }

    #[test]
    fn no_route_through_a_sealed_wall() {
        let walls = [Rect {
            x: 440.,
            y: 0.,
            w: 100.,
            h: 800.,
        }];
        let mut nav = Navigation::default();
        nav.refresh(1200., 800., &walls);
        assert!(
            nav.route(V2::new(220., 350.), V2::new(900., 350.), &walls, 30.)
                .is_empty()
        );
    }

    #[test]
    fn thin_walls_between_grid_centers_cannot_be_crossed() {
        let walls = [Rect {
            x: 318.,
            y: 0.,
            w: 4.,
            h: 800.,
        }];
        let mut nav = Navigation::default();
        nav.refresh(1200., 800., &walls);
        assert!(
            nav.route(V2::new(200., 350.), V2::new(900., 350.), &walls, 20.)
                .is_empty()
        );
    }

    #[test]
    fn a_goal_inside_a_wall_projects_to_reachable_space() {
        let walls = [Rect {
            x: 440.,
            y: 180.,
            w: 100.,
            h: 360.,
        }];
        let mut nav = Navigation::default();
        nav.refresh(1200., 800., &walls);
        let start = V2::new(220., 350.);
        let goal = V2::new(490., 350.);
        let route = nav.route(start, goal, &walls, 30.);
        assert!(!route.is_empty());
        let mut previous = start;
        for point in route {
            assert!(clear_line(previous, point, 30., &walls));
            previous = point;
        }
        assert!(previous.distance(goal) < 180.);
        assert!(tankyard_core::physics::circle_rect(previous, 30., walls[0]).is_none());
    }

    #[test]
    fn route_clearance_matches_the_current_hull() {
        let walls = [
            Rect {
                x: 438.,
                y: 0.,
                w: 84.,
                h: 250.,
            },
            Rect {
                x: 438.,
                y: 310.,
                w: 84.,
                h: 490.,
            },
        ];
        let mut nav = Navigation::default();
        nav.refresh(1200., 800., &walls);
        let start = V2::new(200., 400.);
        let goal = V2::new(900., 400.);
        let route = nav.route(start, goal, &walls, 20.);
        assert!(!route.is_empty());
        let mut previous = start;
        for point in route {
            assert!(clear_line(previous, point, 20., &walls));
            previous = point;
        }
        assert!(previous.distance(goal) < 1.);
        assert!(nav.route(start, goal, &walls, 34.).is_empty());
    }
}
