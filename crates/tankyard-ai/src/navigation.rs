//! Clearance-aware, bounded A* for rectangular arena walls.
use std::{cmp::Reverse, collections::BinaryHeap};
#[cfg(test)]
use tankyard_core::physics::circle_rect;
use tankyard_core::{Rect, V2, physics::sweep_rect};

pub(crate) fn clear_line(start: V2, end: V2, clearance: f32, walls: &[Rect]) -> bool {
    walls
        .iter()
        .all(|&wall| sweep_rect(start, end, clearance, wall).is_none())
}

/// Clearance is exact up to this far from cover, which is wider than any
/// tank (a class's body scale stops at twice the usual size); a cell farther
/// from everything counts as this clear. Each wall then reaches only the
/// cells this close to it, so a crate sliding along refreshes a few dozen
/// cells rather than the whole grid.
const REACH: f32 = 320.;
/// Past this many walls moving at once, as when a large group glides,
/// rebuilding the whole grid costs no more than following each one.
const MOST_FOLLOWED: usize = 48;

/// First and last column, then first and last row, of a block of cells.
type Cells = (usize, usize, usize, usize);

/// Add `cells` to the blocks a refresh settles, merged with any it
/// overlaps so no cell is settled twice.
fn unsettle(blocks: &mut Vec<Cells>, mut cells: Cells) {
    let mut index = 0;
    while index < blocks.len() {
        let (x0, y0, x1, y1) = blocks[index];
        if x0 <= cells.2 && cells.0 <= x1 && y0 <= cells.3 && cells.1 <= y1 {
            cells = (
                x0.min(cells.0),
                y0.min(cells.1),
                x1.max(cells.2),
                y1.max(cells.3),
            );
            blocks.swap_remove(index);
            index = 0;
        } else {
            index += 1;
        }
    }
    blocks.push(cells);
}

pub(crate) struct Navigation {
    cols: usize,
    rows: usize,
    cell: f32,
    width: f32,
    height: f32,
    /// Each cell's squared distance to the nearest cover or the arena's
    /// edge, up to [`REACH`].
    clearance_squared: Vec<f32>,
    costs: Vec<u32>,
    parents: Vec<usize>,
    open: BinaryHeap<Reverse<(u32, usize)>>,
    /// The walls the grid was built for, and which of them touch each cell,
    /// so a short line only tests the walls beside it.
    walls: Vec<Rect>,
    wall_cells: Vec<Vec<u32>>,
    /// `seen[wall] == stamp` once a line has tested that wall.
    seen: Vec<u32>,
    stamp: u32,
    /// Whether this route's walls are the indexed ones.
    indexed: bool,
    /// Blocks of cells a refresh settles, kept to save allocating.
    unsettled: Vec<Cells>,
}

impl Default for Navigation {
    fn default() -> Self {
        Self {
            cols: 0,
            rows: 0,
            cell: 80.,
            width: 0.,
            height: 0.,
            clearance_squared: vec![],
            costs: vec![],
            parents: vec![],
            open: BinaryHeap::new(),
            walls: vec![],
            wall_cells: vec![],
            seen: vec![],
            stamp: 0,
            indexed: false,
            unsettled: vec![],
        }
    }
}

impl Navigation {
    /// Fit the grid to an arena `width` by `height` with `walls`. When only
    /// a few walls moved since the last refresh, as when a crate slides or a
    /// door glides, only the cells near them change.
    pub fn refresh(&mut self, width: f32, height: f32, walls: &[Rect]) {
        let cols = (width / self.cell).ceil().clamp(1., 256.) as usize;
        let rows = (height / self.cell).ceil().clamp(1., 256.) as usize;
        let same_grid = self.cols == cols
            && self.rows == rows
            && self.width == width
            && self.height == height
            && self.walls.len() == walls.len();
        if same_grid {
            let moved = self.walls.iter().zip(walls).filter(|(a, b)| a != b).count();
            if moved == 0 {
                return;
            }
            if moved <= MOST_FOLLOWED {
                self.follow(walls);
                return;
            }
        }
        self.width = width;
        self.height = height;
        self.cols = cols;
        self.rows = rows;
        let cells = cols * rows;
        self.clearance_squared.resize(cells, 0.);
        self.costs.resize(cells, u32::MAX);
        self.parents.resize(cells, usize::MAX);
        self.walls.clear();
        self.walls.extend_from_slice(walls);
        self.wall_cells.resize_with(cells, Vec::new);
        for cell in &mut self.wall_cells {
            cell.clear();
        }
        for index in 0..walls.len() {
            self.index_wall(index, true);
        }
        self.seen.clear();
        self.seen.resize(walls.len(), 0);
        self.stamp = 0;
        self.settle((0, 0, cols - 1, rows - 1));
    }

    /// Catch up with walls that moved: index each where it is now, then
    /// settle the cells near where it was and where it is.
    fn follow(&mut self, walls: &[Rect]) {
        let mut unsettled = std::mem::take(&mut self.unsettled);
        unsettled.clear();
        for (index, &wall) in walls.iter().enumerate() {
            let before = self.walls[index];
            if before == wall {
                continue;
            }
            unsettle(&mut unsettled, self.near(before));
            self.index_wall(index, false);
            self.walls[index] = wall;
            self.index_wall(index, true);
            unsettle(&mut unsettled, self.near(wall));
        }
        for &cells in &unsettled {
            self.settle(cells);
        }
        self.unsettled = unsettled;
    }

    /// The cells within [`REACH`] of `wall`.
    fn near(&self, wall: Rect) -> Cells {
        self.cell_span(
            V2::new(wall.x, wall.y),
            V2::new(wall.x + wall.w, wall.y + wall.h),
            REACH,
        )
    }

    /// Add wall `index` to the cells it covers, or take it from them.
    fn index_wall(&mut self, index: usize, add: bool) {
        let wall = self.walls[index];
        let (x0, y0, x1, y1) = self.cell_span(
            V2::new(wall.x, wall.y),
            V2::new(wall.x + wall.w, wall.y + wall.h),
            0.,
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                let cell = &mut self.wall_cells[y * self.cols + x];
                if add {
                    cell.push(index as u32);
                } else {
                    cell.retain(|&other| other as usize != index);
                }
            }
        }
    }

    /// Work out afresh how clear each cell in `cells` is: its center's
    /// distance to the nearest wall or the arena's edge, up to [`REACH`].
    fn settle(&mut self, (x0, y0, x1, y1): Cells) {
        let most = REACH * REACH;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let index = y * self.cols + x;
                let p = self.center(index);
                let border =
                    p.x.min(p.y)
                        .min(self.width - p.x)
                        .min(self.height - p.y)
                        .max(0.);
                self.clearance_squared[index] = (border * border).min(most);
            }
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.stamp = 1;
        }
        // A wall within reach of these cells covers a cell within reach of
        // them too, so the walls indexed around them are all that matter.
        let pad = (REACH / self.cell).ceil() as usize;
        for wy in y0.saturating_sub(pad)..=(y1 + pad).min(self.rows - 1) {
            for wx in x0.saturating_sub(pad)..=(x1 + pad).min(self.cols - 1) {
                let around = wy * self.cols + wx;
                for k in 0..self.wall_cells[around].len() {
                    let index = self.wall_cells[around][k] as usize;
                    if self.seen[index] == self.stamp {
                        continue;
                    }
                    self.seen[index] = self.stamp;
                    let wall = self.walls[index];
                    let (cx0, cy0, cx1, cy1) = self.near(wall);
                    for y in cy0.max(y0)..=cy1.min(y1) {
                        for x in cx0.max(x0)..=cx1.min(x1) {
                            let cell = y * self.cols + x;
                            let p = self.center(cell);
                            let nearest = V2::new(
                                p.x.clamp(wall.x, wall.x + wall.w),
                                p.y.clamp(wall.y, wall.y + wall.h),
                            );
                            let distance = (p - nearest).length_squared();
                            if distance < self.clearance_squared[cell] {
                                self.clearance_squared[cell] = distance;
                            }
                        }
                    }
                }
            }
        }
    }

    /// A grid of `world`'s arena and cover as they stand.
    #[cfg(test)]
    pub fn of(world: &tankyard_core::World) -> Self {
        let mut navigation = Self::default();
        navigation.refresh(world.config.width, world.config.height, &world.walls);
        navigation
    }

    /// Visit, once each, the walls of the last refresh that are indexed in
    /// the cells the box from `a` to `b`, grown by `pad`, covers, until
    /// `visit` returns false. Returns whether it never did.
    fn visit_near(&self, a: V2, b: V2, pad: f32, mut visit: impl FnMut(Rect) -> bool) -> bool {
        if self.cols == 0 {
            return true;
        }
        let (x0, y0, x1, y1) = self.cell_span(a, b, pad + 1.);
        for y in y0..=y1 {
            for x in x0..=x1 {
                for &index in &self.wall_cells[y * self.cols + x] {
                    let wall = self.walls[index as usize];
                    // A wall over several of these cells counts in the first.
                    let corner = V2::new(wall.x, wall.y);
                    let (wx, wy, _, _) = self.cell_span(corner, corner, 0.);
                    if wx.max(x0) == x && wy.max(y0) == y && !visit(wall) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// How far from `start` toward `end`, 0 to 1, a circle of `radius` first
    /// meets the walls of the last refresh, if it does. With `any`, the
    /// first meeting found will do.
    fn contact(&self, start: V2, end: V2, radius: f32, any: bool) -> Option<f32> {
        // Stretches about three cells long keep the cells searched close to
        // the line, however it slants.
        let stretches = ((start.distance(end) / (self.cell * 3.)).ceil() as usize).clamp(1, 64);
        let mut first: Option<f32> = None;
        let mut from = start;
        for stretch in 1..=stretches {
            let reached = stretch as f32 / stretches as f32;
            let to = start.lerp(end, reached);
            let finished = !self.visit_near(from, to, radius, |wall| {
                match sweep_rect(start, end, radius, wall) {
                    Some((time, _)) => {
                        first = Some(first.map_or(time, |best| best.min(time)));
                        !any
                    }
                    None => true,
                }
            });
            // Nothing in a later stretch can be met sooner.
            if finished || first.is_some_and(|time| time <= reached) {
                break;
            }
            from = to;
        }
        first
    }

    /// Whether a circle of `clearance` gets from `start` to `end` without
    /// touching the walls of the last refresh: [`clear_line`] for those
    /// walls, testing only the ones beside the line.
    pub fn clear_path(&self, start: V2, end: V2, clearance: f32) -> bool {
        self.contact(start, end, clearance, true).is_none()
    }

    /// How far from `start` toward `end`, 0 to 1, a circle of `radius`
    /// gets before touching the walls of the last refresh.
    pub fn first_contact(&self, start: V2, end: V2, radius: f32) -> f32 {
        self.contact(start, end, radius, false).unwrap_or(1.)
    }

    /// Visit, once each, the walls of the last refresh that a circle of
    /// `radius` around `pos` may touch.
    pub fn walls_around(&self, pos: V2, radius: f32, mut visit: impl FnMut(Rect)) {
        self.visit_near(pos, pos, radius, |wall| {
            visit(wall);
            true
        });
    }

    /// The columns and rows a box from `a` to `b`, grown by `pad`, covers.
    fn cell_span(&self, a: V2, b: V2, pad: f32) -> (usize, usize, usize, usize) {
        let col = |x: f32| ((x / self.cell).max(0.) as usize).min(self.cols - 1);
        let row = |y: f32| ((y / self.cell).max(0.) as usize).min(self.rows - 1);
        (
            col(a.x.min(b.x) - pad),
            row(a.y.min(b.y) - pad),
            col(a.x.max(b.x) + pad),
            row(a.y.max(b.y) + pad),
        )
    }

    /// [`clear_line`], testing only the walls in the cells the line passes.
    fn clear(&mut self, start: V2, end: V2, clearance: f32, walls: &[Rect]) -> bool {
        if !self.indexed {
            return clear_line(start, end, clearance, walls);
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.stamp = 1;
        }
        let (x0, y0, x1, y1) = self.cell_span(start, end, clearance + 1.);
        for y in y0..=y1 {
            for x in x0..=x1 {
                for &index in &self.wall_cells[y * self.cols + x] {
                    let index = index as usize;
                    if self.seen[index] == self.stamp {
                        continue;
                    }
                    self.seen[index] = self.stamp;
                    if sweep_rect(start, end, clearance, walls[index]).is_some() {
                        return false;
                    }
                }
            }
        }
        true
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
        &mut self,
        p: V2,
        clearance: f32,
        walls: &[Rect],
        require_connection: bool,
    ) -> Option<usize> {
        let cell = self.cell_of(p);
        let reachable = |navigation: &mut Self, index| {
            navigation.clearance_squared[index] > clearance * clearance
                && (!require_connection
                    || navigation.clear(p, navigation.center(index), clearance, walls))
        };
        if reachable(self, cell) {
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
                    if reachable(self, index) {
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
        // The grid knows clearance only up to its reach.
        let clearance = clearance.min(REACH - 1.);
        self.indexed = self.cols > 0 && self.walls == walls;
        if self.clear(start, goal, clearance, walls) {
            return vec![goal];
        }
        let goal_clear = self.clear(goal, goal, clearance, walls);
        let (Some(from), Some(to)) = (
            self.nearest_free(start, clearance, walls, true),
            // Exploration and combat spacing can place a goal inside a wall.
            // In that case approach a free cell beside it instead of stalling.
            self.nearest_free(goal, clearance, walls, goal_clear),
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
                    && self.clear(self.center(index), self.center(next), clearance, walls)
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
        if self.clear(*points.last().unwrap_or(&start), goal, clearance, walls) {
            points.push(goal);
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid's clearance, worked out the slow way from every wall.
    fn brute_force(nav: &Navigation, walls: &[Rect]) -> Vec<f32> {
        (0..nav.cols * nav.rows)
            .map(|index| {
                let p = nav.center(index);
                let border =
                    p.x.min(p.y)
                        .min(nav.width - p.x)
                        .min(nav.height - p.y)
                        .max(0.);
                walls
                    .iter()
                    .fold(border * border, |best, wall| {
                        let nearest = V2::new(
                            p.x.clamp(wall.x, wall.x + wall.w),
                            p.y.clamp(wall.y, wall.y + wall.h),
                        );
                        best.min((p - nearest).length_squared())
                    })
                    .min(REACH * REACH)
            })
            .collect()
    }

    fn sorted_cells(nav: &Navigation) -> Vec<Vec<u32>> {
        nav.wall_cells
            .iter()
            .map(|cell| {
                let mut cell = cell.clone();
                cell.sort_unstable();
                cell
            })
            .collect()
    }

    #[test]
    fn grid_queries_agree_with_testing_every_wall() {
        let walls: Vec<Rect> = (0..140)
            .map(|n| {
                let n = n as f32;
                Rect {
                    x: (n * 211.) % 3800.,
                    y: (n * 97.) % 2400.,
                    w: 8. + (n * 29.) % 700.,
                    h: 8. + (n * 17.) % 90.,
                }
            })
            .collect();
        let mut nav = Navigation::default();
        nav.refresh(4000., 2600., &walls);
        for n in 0..400 {
            let n = n as f32;
            let start = V2::new((n * 131.) % 4000., (n * 71.) % 2600.);
            let end = V2::new((n * 389.) % 4000., (n * 223.) % 2600.);
            let radius = 2. + n % 60.;
            assert_eq!(
                nav.clear_path(start, end, radius),
                clear_line(start, end, radius, &walls),
                "{start:?} {end:?} {radius}"
            );
            let first = walls.iter().fold(1_f32, |best, &wall| {
                sweep_rect(start, end, radius, wall).map_or(best, |(time, _)| best.min(time))
            });
            assert_eq!(nav.first_contact(start, end, radius), first);
            let mut found = Vec::new();
            nav.walls_around(start, radius * 3., |wall| found.push(wall));
            let near: Vec<_> = walls
                .iter()
                .filter(|&&wall| circle_rect(start, radius * 3., wall).is_some())
                .collect();
            assert!(near.iter().all(|wall| found.contains(wall)), "{start:?}");
            for wall in &found {
                assert_eq!(found.iter().filter(|other| *other == wall).count(), 1);
            }
        }
    }

    #[test]
    fn moving_walls_refresh_only_what_they_change() {
        let mut walls: Vec<Rect> = (0..90)
            .map(|n| {
                let n = n as f32;
                Rect {
                    x: (n * 137.) % 2800.,
                    y: (n * 251.) % 1800.,
                    w: 20. + (n * 7.) % 300.,
                    h: 20. + (n * 13.) % 260.,
                }
            })
            .collect();
        let mut nav = Navigation::default();
        nav.refresh(3000., 2000., &walls);
        assert_eq!(nav.clearance_squared, brute_force(&nav, &walls));
        // A crate sliding a little each tick, a door gliding, a block that
        // jumps across the arena, and a crowd moving at once.
        for step in 0..40 {
            walls[3].x += 3.5;
            walls[7].y -= 2.25;
            if step % 10 == 0 {
                walls[11].x = (walls[11].x + 1700.) % 2600.;
            }
            if step == 25 {
                for wall in walls.iter_mut().skip(20) {
                    wall.y += 40.;
                }
            }
            nav.refresh(3000., 2000., &walls);
            let mut fresh = Navigation::default();
            fresh.refresh(3000., 2000., &walls);
            assert_eq!(
                nav.clearance_squared, fresh.clearance_squared,
                "step {step}"
            );
            assert_eq!(sorted_cells(&nav), sorted_cells(&fresh), "step {step}");
            assert_eq!(nav.walls, walls);
        }
        assert_eq!(nav.clearance_squared, brute_force(&nav, &walls));
    }

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

    #[test]
    fn indexed_lines_agree_with_testing_every_wall() {
        let mut walls = Vec::new();
        for n in 0..120 {
            let f = n as f32;
            walls.push(Rect {
                x: (f * 137.) % 3400. - 40.,
                y: (f * 251.) % 2500. - 30.,
                w: 10. + (f * 7.) % 300.,
                h: 10. + (f * 13.) % 260.,
            });
        }
        let mut nav = Navigation::default();
        nav.refresh(3600., 2600., &walls);
        nav.indexed = true;
        for n in 0..6000 {
            let f = n as f32;
            let start = V2::new((f * 61.) % 3700. - 50., (f * 97.) % 2700. - 50.);
            let end = start + V2::from_angle(f * 0.37) * ((f * 31.) % 900.);
            let clearance = 2. + (f * 0.7) % 60.;
            assert_eq!(
                nav.clear(start, end, clearance, &walls),
                clear_line(start, end, clearance, &walls),
                "line {n}"
            );
        }
    }
}
