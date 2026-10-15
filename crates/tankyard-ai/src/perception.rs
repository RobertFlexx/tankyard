//! Reusable spatial buckets for bounded local bot perception.
use std::collections::HashMap;
use tankyard_core::{EntityId, V2, World};

#[derive(Clone, Copy)]
pub(crate) enum Seen {
    Tank(usize),
    Target(usize),
    Boss(usize),
    Projectile(usize),
}

#[derive(Default)]
pub(crate) struct Perception {
    cols: usize,
    rows: usize,
    buckets: Vec<Vec<Seen>>,
    targets: HashMap<EntityId, u8>,
}

impl Perception {
    const CELL: f32 = 260.;

    pub fn rebuild(&mut self, world: &World) {
        let cols = (world.config.width / Self::CELL).ceil().max(1.) as usize;
        let rows = (world.config.height / Self::CELL).ceil().max(1.) as usize;
        if cols != self.cols || rows != self.rows {
            self.cols = cols;
            self.rows = rows;
            self.buckets = (0..cols * rows).map(|_| Vec::new()).collect();
        }
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.targets.clear();
        for (index, tank) in world.tanks.iter().enumerate() {
            if tank.alive {
                self.insert(tank.pos, Seen::Tank(index));
                if tank.protection <= 0.0 && tank.health > 0.0 {
                    self.targets.insert(tank.id, tank.team);
                }
            }
        }
        for (index, target) in world.targets.iter().enumerate() {
            if target.health > 0. {
                self.insert(target.pos, Seen::Target(index));
                self.targets.insert(target.id, 0);
            }
        }
        for (index, boss) in world.bosses.iter().enumerate() {
            if boss.health > 0. {
                self.insert(boss.pos, Seen::Boss(index));
                self.targets.insert(boss.id, boss.team);
            }
        }
        for (index, projectile) in world.projectiles.iter().enumerate() {
            if projectile.lifetime > 0. {
                self.insert(projectile.pos, Seen::Projectile(index));
            }
        }
    }

    pub fn target_valid(&self, id: EntityId, team: u8) -> bool {
        self.targets
            .get(&id)
            .is_some_and(|&other| team == 0 || other == 0 || team != other)
    }

    fn insert(&mut self, position: V2, value: Seen) {
        let x = (position.x / Self::CELL).max(0.) as usize;
        let y = (position.y / Self::CELL).max(0.) as usize;
        let index = y.min(self.rows - 1) * self.cols + x.min(self.cols - 1);
        self.buckets[index].push(value);
    }

    pub fn query(&self, position: V2, radius: f32, result: &mut Vec<Seen>) {
        result.clear();
        if self.cols == 0 {
            return;
        }
        let min_x = ((position.x - radius) / Self::CELL).max(0.) as usize;
        let min_y = ((position.y - radius) / Self::CELL).max(0.) as usize;
        let max_x = ((position.x + radius) / Self::CELL).max(0.) as usize;
        let max_y = ((position.y + radius) / Self::CELL).max(0.) as usize;
        for y in min_y.min(self.rows - 1)..=max_y.min(self.rows - 1) {
            for x in min_x.min(self.cols - 1)..=max_x.min(self.cols - 1) {
                for &seen in &self.buckets[y * self.cols + x] {
                    // A bounded scan protects decision time in pathological
                    // projectile piles. The broad phase still scales linearly.
                    if result.len() >= 8192 {
                        return;
                    }
                    result.push(seen);
                }
            }
        }
    }
}
