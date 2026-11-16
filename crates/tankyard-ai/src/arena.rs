//! Head-to-head matches between two bot controllers, for measuring whether a
//! change makes bots better and for training them. Both sides play the same
//! arena at once, their bots interleaved, so in team modes every team has
//! some of each.
use crate::BotController;
use tankyard_content::Content;
use tankyard_core::{Mode, World, WorldConfig};

/// How one side did.
#[derive(Clone, Copy, Debug, Default)]
pub struct Side {
    pub bots: u32,
    /// Points earned, including points later lost to deaths.
    pub points: f64,
    pub kills: u32,
    pub deaths: u32,
    /// Bot-ticks spent alive.
    pub alive: u64,
    /// Bot-ticks spent alive against a wall or the arena's edge.
    pub scraping: u64,
    /// Bot-ticks spent alive barely moving.
    pub idle: u64,
}

impl Side {
    /// Points a bot earns per minute alive.
    pub fn points_per_minute(&self) -> f64 {
        self.points / (self.alive as f64 / 3600.).max(1. / 60.)
    }

    /// One number to compare sides by: points per bot, each kill worth a
    /// little extra and each death costing some. Seconds spent scraping
    /// walls or standing still cost a little too: bots that play well but
    /// look stuck don't feel clever.
    pub fn fitness(&self) -> f64 {
        let bots = f64::from(self.bots.max(1));
        (self.points + f64::from(self.kills) * 400.
            - f64::from(self.deaths) * 600.
            - self.scraping as f64 / 60. * 150.
            - self.idle as f64 / 60. * 60.)
            / bots
    }

    /// Shares of time alive spent scraping walls and standing still.
    pub fn scraping_share(&self) -> f64 {
        self.scraping as f64 / self.alive.max(1) as f64
    }
    pub fn idle_share(&self) -> f64 {
        self.idle as f64 / self.alive.max(1) as f64
    }

    pub fn add(&mut self, other: &Side) {
        self.bots += other.bots;
        self.points += other.points;
        self.kills += other.kills;
        self.deaths += other.deaths;
        self.alive += other.alive;
        self.scraping += other.scraping;
        self.idle += other.idle;
    }
}

/// Whether a tank at `pos` touches a wall or the arena's edge.
fn touching(world: &World, pos: tankyard_core::V2, radius: f32) -> bool {
    let bounds = world.config.bounds();
    let reach = radius + 4.;
    let border = (pos.x - bounds.x)
        .min(pos.y - bounds.y)
        .min(bounds.x + bounds.w - pos.x)
        .min(bounds.y + bounds.h - pos.y);
    border <= reach
        || world.walls.iter().any(|wall| {
            let dx = (wall.x - pos.x).max(pos.x - (wall.x + wall.w)).max(0.);
            let dy = (wall.y - pos.y).max(pos.y - (wall.y + wall.h)).max(0.);
            dx * dx + dy * dy <= reach * reach
        })
}

/// One match to play.
#[derive(Clone, Debug)]
pub struct Match {
    pub mode: Mode,
    pub map: String,
    /// Bots on each side.
    pub bots: usize,
    pub ticks: u64,
    pub seed: u64,
}

/// Plays `game` with side `a` driving even-numbered bots and side `b` the
/// odd ones, and reports how each side did.
pub fn play(game: &Match, a: &mut BotController, b: &mut BotController) -> (Side, Side) {
    let mut world = World::new(
        WorldConfig {
            mode: game.mode,
            map: game.map.clone(),
            seed: game.seed,
            ..Default::default()
        },
        Content::builtin(),
    );
    let mut sides = [Side::default(), Side::default()];
    for n in 0..game.bots * 2 {
        world.add_player(&format!("Bot {n}"), true);
        sides[n % 2].bots += 1;
    }
    let side_of: std::collections::HashMap<_, _> = world
        .tanks
        .iter()
        .enumerate()
        .map(|(n, tank)| (tank.id, n % 2))
        .collect();
    let mut last_score: std::collections::HashMap<_, _> =
        world.tanks.iter().map(|t| (t.id, t.score)).collect();
    for _ in 0..game.ticks {
        if world.match_state.finished {
            break;
        }
        a.update_some(&mut world, |tank| side_of.get(&tank.id) == Some(&0));
        b.update_some(&mut world, |tank| side_of.get(&tank.id) == Some(&1));
        world.step();
        for tank in &world.tanks {
            let Some(&side) = side_of.get(&tank.id) else {
                continue;
            };
            let last = last_score.entry(tank.id).or_insert(tank.score);
            // Scores drop on death; only gains count.
            sides[side].points += f64::from(tank.score.saturating_sub(*last));
            *last = tank.score;
            if tank.alive {
                sides[side].alive += 1;
                if touching(&world, tank.pos, tank.radius) {
                    sides[side].scraping += 1;
                }
                if tank.vel.length_squared() < 30. * 30. {
                    sides[side].idle += 1;
                }
            }
        }
    }
    for tank in &world.tanks {
        if let Some(&side) = side_of.get(&tank.id) {
            sides[side].kills += tank.kills;
            sides[side].deaths += tank.deaths;
        }
    }
    (sides[0], sides[1])
}
