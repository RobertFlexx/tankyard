//! Seeded cover stays inside the arena and scales from its original geometry.
use crate::{physics, *};
use tankyard_content::{MapDef, MapProcedure};

impl World {
    pub(crate) fn map_object_scale(&self) -> f32 {
        (self.config.width / 3600.)
            .min(self.config.height / 2600.)
            .clamp(0.35, 1.)
            * self.config.arena_scale
    }
    pub(crate) fn initialize_map(&mut self) {
        self.walls.extend(self.map.walls.iter().map(|wall| Rect {
            x: wall[0] * self.config.width,
            y: wall[1] * self.config.height,
            w: wall[2] * self.config.width,
            h: wall[3] * self.config.height,
        }));
        match self.map.procedural {
            MapProcedure::Maze => self.maze_map(),
            MapProcedure::Dense => {
                self.config.target_count = self.config.target_count.clamp(350, 10000);
                self.garden_map();
            }
            MapProcedure::None => {}
        }
        self.map_walls.clone_from(&self.walls);
        self.scale_map_walls();
    }

    pub(crate) fn scale_map_walls(&mut self) {
        let bounds = self.config.bounds();
        let scale = self.config.arena_scale;
        for (wall, original) in self.walls.iter_mut().zip(&self.map_walls) {
            *wall = Rect {
                x: bounds.x + original.x * scale,
                y: bounds.y + original.y * scale,
                w: original.w * scale,
                h: original.h * scale,
            };
        }
    }

    /// Apply a validated map from the session's catalog. Identities, upgrades,
    /// scores and authority remain intact; transient shots cannot hide in cover.
    pub fn change_map(&mut self, map: MapDef) -> Result<(), String> {
        let old_config = self.config.clone();
        let old_rng = self.rng;
        let old_next_id = self.next_id;
        let old_walls = std::mem::take(&mut self.walls);
        let old_map_walls = std::mem::take(&mut self.map_walls);
        let old_objectives = std::mem::take(&mut self.objectives);
        self.config.map.clone_from(&map.id);
        let old_map = std::mem::replace(&mut self.map, map);
        self.initialize_map();
        let bounds = self.config.bounds();
        let tanks = self
            .tanks
            .iter()
            .map(|tank| {
                if tank.alive {
                    physics::clear_circle_position(tank.pos, tank.radius, bounds, &self.walls)
                } else {
                    Some(tank.pos)
                }
            })
            .collect::<Option<Vec<_>>>();
        let bosses = self
            .bosses
            .iter()
            .map(|boss| physics::clear_circle_position(boss.pos, boss.radius, bounds, &self.walls))
            .collect::<Option<Vec<_>>>();
        let mut objectives_clear = true;
        if matches!(self.config.mode, Mode::Domination | Mode::CaptureCore) {
            self.initialize_mode();
            for objective in &mut self.objectives {
                if self.config.mode == Mode::Domination {
                    objective.pos = self.config.arena_point(objective.pos);
                }
                if let Some(clear) = physics::clear_circle_position(
                    objective.pos,
                    objective.radius,
                    self.config.bounds(),
                    &self.walls,
                ) {
                    objective.pos = clear;
                } else {
                    objectives_clear = false;
                }
                objective.home = objective.pos;
            }
        }
        let (Some(tanks), Some(bosses), true) = (tanks, bosses, objectives_clear) else {
            self.config = old_config;
            self.rng = old_rng;
            self.next_id = old_next_id;
            self.map = old_map;
            self.walls = old_walls;
            self.map_walls = old_map_walls;
            self.objectives = old_objectives;
            return Err(
                "This map cannot safely fit the current tanks, bosses or objectives.".into(),
            );
        };
        for (tank, pos) in self.tanks.iter_mut().zip(tanks) {
            tank.pos = pos;
            tank.vel = V2::ZERO;
        }
        for (boss, pos) in self.bosses.iter_mut().zip(bosses) {
            boss.pos = pos;
            boss.goal = pos;
        }
        self.projectiles.clear();
        self.targets.clear();
        for _ in 0..self.target_capacity() {
            self.spawn_target();
        }
        Ok(())
    }

    fn garden_map(&mut self) {
        let odd_cells = |length: f32| ((length / 640.).floor() as usize).clamp(3, 11) | 1;
        let cols = odd_cells(self.config.width);
        let rows = odd_cells(self.config.height);
        let cw = self.config.width * 0.8 / cols as f32;
        let ch = self.config.height * 0.8 / rows as f32;
        let unit = cw.min(ch);
        for y in 0..rows {
            for x in 0..cols {
                // A clear central promenade connects all four approaches.
                if x == cols / 2 || y == rows / 2 {
                    continue;
                }
                let mut w = unit * (0.38 + self.random() * 0.04);
                let mut h = unit * (0.24 + self.random() * 0.04);
                if (x + y) % 2 == 1 {
                    std::mem::swap(&mut w, &mut h);
                }
                let cx = self.config.width * 0.1 + (x as f32 + 0.5) * cw;
                let cy = self.config.height * 0.1 + (y as f32 + 0.5) * ch;
                self.walls.push(Rect {
                    x: cx - w / 2.,
                    y: cy - h / 2.,
                    w,
                    h,
                });
            }
        }
    }

    fn maze_map(&mut self) {
        let cols = (self.config.width / 360.).floor().max(3.) as usize;
        let rows = (self.config.height / 360.).floor().max(3.) as usize;
        let margin = (self.config.width.min(self.config.height) * 0.04).clamp(24., 64.);
        let cw = (self.config.width - margin * 2.) / cols as f32;
        let ch = (self.config.height - margin * 2.) / rows as f32;
        let half = (cw.min(ch) * 0.05).clamp(6., 24.);
        let mut visited = vec![false; cols * rows];
        let mut passage = vec![[false; 2]; cols * rows];
        let mut stack = Vec::with_capacity(cols * rows);
        let mut neighbors = Vec::with_capacity(4);
        stack.push(0usize);
        visited[0] = true;
        while let Some(&cell) = stack.last() {
            let x = cell % cols;
            let y = cell / cols;
            neighbors.clear();
            if x + 1 < cols && !visited[cell + 1] {
                neighbors.push((cell + 1, cell, 0));
            }
            if y + 1 < rows && !visited[cell + cols] {
                neighbors.push((cell + cols, cell, 1));
            }
            if x > 0 && !visited[cell - 1] {
                neighbors.push((cell - 1, cell - 1, 0));
            }
            if y > 0 && !visited[cell - cols] {
                neighbors.push((cell - cols, cell - cols, 1));
            }
            if neighbors.is_empty() {
                stack.pop();
                continue;
            }
            let index = (self.random() * neighbors.len() as f32) as usize;
            let (next, edge, axis) = neighbors[index.min(neighbors.len() - 1)];
            passage[edge][axis] = true;
            visited[next] = true;
            stack.push(next);
        }
        // Keep the spanning tree and open loops for routes around campers.
        for cell in &mut passage {
            for open in cell {
                *open |= self.random() < 0.14;
            }
        }
        let vertical_start = self.walls.len();
        for x in 0..cols - 1 {
            let mut y = 0;
            while y < rows {
                if passage[y * cols + x][0] {
                    y += 1;
                    continue;
                }
                let start = y;
                while y < rows && !passage[y * cols + x][0] {
                    y += 1;
                }
                let top = (margin + start as f32 * ch - half).max(margin);
                let bottom = (margin + y as f32 * ch + half).min(self.config.height - margin);
                self.walls.push(Rect {
                    x: margin + (x + 1) as f32 * cw - half,
                    y: top,
                    w: half * 2.,
                    h: bottom - top,
                });
            }
        }
        let vertical_end = self.walls.len();
        let mut cuts = Vec::new();
        for y in 0..rows - 1 {
            let mut x = 0;
            while x < cols {
                if passage[y * cols + x][1] {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < cols && !passage[y * cols + x][1] {
                    x += 1;
                }
                let left = (margin + start as f32 * cw - half).max(margin);
                let right = (margin + x as f32 * cw + half).min(self.config.width - margin);
                let top = margin + (y + 1) as f32 * ch - half;
                // Split at vertical joins, producing one solid union without
                // overlapping rectangles or doubled decorative end caps.
                cuts.clear();
                for wall in &self.walls[vertical_start..vertical_end] {
                    if wall.y < top + half * 2.
                        && wall.y + wall.h > top
                        && wall.x < right
                        && wall.x + wall.w > left
                    {
                        cuts.push((wall.x.max(left), (wall.x + wall.w).min(right)));
                    }
                }
                cuts.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                let mut at = left;
                for &(a, b) in &cuts {
                    if a - at > 0.01 {
                        self.walls.push(Rect {
                            x: at,
                            y: top,
                            w: a - at,
                            h: half * 2.,
                        });
                    }
                    at = at.max(b);
                }
                if right - at > 0.01 {
                    self.walls.push(Rect {
                        x: at,
                        y: top,
                        w: right - at,
                        h: half * 2.,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;

    #[test]
    fn maze_passages_remain_connected_for_tank_hulls_at_small_and_large_sizes() {
        for (width, height) in [(900., 700.), (3600., 2600.), (16000., 16000.)] {
            let world = World::new(
                WorldConfig {
                    map: "maze".into(),
                    width,
                    height,
                    target_count: 0,
                    ..Default::default()
                },
                Content::builtin(),
            );
            let cols = (width / 360.).floor().max(3.) as usize;
            let rows = (height / 360.).floor().max(3.) as usize;
            let margin = (width.min(height) * 0.04).clamp(24., 64.);
            let at = |x: usize, y: usize| {
                V2::new(
                    margin + (x as f32 + 0.5) * (width - 2. * margin) / cols as f32,
                    margin + (y as f32 + 0.5) * (height - 2. * margin) / rows as f32,
                )
            };
            let mut seen = vec![false; cols * rows];
            let mut queue = std::collections::VecDeque::from([(0_usize, 0_usize)]);
            seen[0] = true;
            while let Some((x, y)) = queue.pop_front() {
                for (nx, ny) in [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ] {
                    if nx >= cols || ny >= rows || seen[ny * cols + nx] {
                        continue;
                    }
                    if world
                        .walls
                        .iter()
                        .all(|&wall| physics::sweep_rect(at(x, y), at(nx, ny), 30., wall).is_none())
                    {
                        seen[ny * cols + nx] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
            assert!(seen.into_iter().all(|seen| seen), "{width}x{height}");
        }
    }

    #[test]
    fn an_unsafe_map_switch_restores_geometry_rng_and_entities() {
        let mut world = World::new(
            WorldConfig {
                width: 900.,
                height: 700.,
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Pilot", false);
        world.spawn_boss(BossKind::Bastion);
        world.bosses[0].pos = V2::new(450., 350.);
        world.bosses[0].radius = 340.;
        let rng = world.rng;
        let id = world.next_id;
        let pos = world.tanks[0].pos;
        let map = tankyard_content::builtin_maps()
            .into_iter()
            .find(|map| map.id == "maze")
            .unwrap();
        assert!(world.change_map(map).is_err());
        assert_eq!(world.config.map, "open");
        assert_eq!(world.map.id, "open");
        assert!(world.walls.is_empty() && world.map_walls.is_empty());
        assert_eq!(world.rng, rng);
        assert_eq!(world.next_id, id);
        assert_eq!(world.tanks[0].pos, pos);
        assert_eq!(world.bosses.len(), 1);
    }

    #[test]
    fn procedural_cover_is_contained_disjoint_and_restores_after_scaling() {
        for map in ["maze", "dense"] {
            for (width, height) in [
                (900., 700.),
                (1600., 900.),
                (3600., 2600.),
                (16000., 700.),
                (900., 16000.),
                (16000., 16000.),
            ] {
                for seed in [1, 42, 937] {
                    let mut world = World::new(
                        WorldConfig {
                            map: map.into(),
                            width,
                            height,
                            seed,
                            target_count: 0,
                            ..Default::default()
                        },
                        Content::builtin(),
                    );
                    let original = world.walls.clone();
                    assert!(world.walls.len() <= 4096);
                    for scale in [0.65, 0.82, 1.] {
                        world.config.arena_scale = scale;
                        world.scale_map_walls();
                        let bounds = world.config.bounds();
                        for (i, a) in world.walls.iter().enumerate() {
                            assert!(
                                a.x >= bounds.x - 0.01
                                    && a.y >= bounds.y - 0.01
                                    && a.x + a.w <= bounds.x + bounds.w + 0.01
                                    && a.y + a.h <= bounds.y + bounds.h + 0.01,
                                "{map} {width}x{height} {scale}: {a:?}"
                            );
                            for b in &world.walls[i + 1..] {
                                let overlap = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                                let vertical = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                                assert!(
                                    overlap < 0.01 || vertical < 0.01,
                                    "{map}: overlapping cover {a:?} {b:?}"
                                );
                            }
                        }
                    }
                    assert_eq!(
                        world.walls, original,
                        "scaling must not accumulate coordinate drift"
                    );
                }
            }
        }
    }
}
