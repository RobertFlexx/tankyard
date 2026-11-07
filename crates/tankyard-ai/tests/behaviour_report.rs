//! A behaviour report for tuning: how often bots stand idle, scrape walls or
//! get stuck, and what the controller costs per tick. Run it with
//! `cargo test --release -p tankyard-ai --test behaviour_report -- --ignored --nocapture`.
use tankyard_ai::{BotController, Difficulty};
use tankyard_content::Content;
use tankyard_core::{Mode, V2, World, WorldConfig};

pub struct Report {
    pub idle_states: std::collections::BTreeMap<String, u32>,
    pub stuck_states: std::collections::BTreeMap<String, u32>,
    pub idle: f32,
    pub walls: f32,
    pub stuck: f32,
    pub kills: u32,
    pub micros_per_tick: f32,
}

fn touching(world: &World, pos: V2, radius: f32) -> bool {
    let bounds = world.config.bounds();
    let border = (pos.x - bounds.x)
        .min(pos.y - bounds.y)
        .min(bounds.x + bounds.w - pos.x)
        .min(bounds.y + bounds.h - pos.y);
    border <= radius + 4.
        || world.walls.iter().any(|wall| {
            let nearest = V2::new(
                pos.x.clamp(wall.x, wall.x + wall.w),
                pos.y.clamp(wall.y, wall.y + wall.h),
            );
            nearest.distance(pos) <= radius + 4.
        })
}

pub fn run(mode: Mode, map: &str, bots: usize, ticks: usize) -> Report {
    let mut world = World::new(
        WorldConfig {
            mode,
            map: map.into(),
            width: 3600.,
            height: 2600.,
            seed: 7,
            ..Default::default()
        },
        Content::builtin(),
    );
    for n in 0..bots {
        world.add_player(&format!("Bot {n}"), true);
    }
    let mut controller = BotController::new(Difficulty::Normal);
    let (mut alive, mut idle, mut walls, mut stuck) = (0u32, 0u32, 0u32, 0u32);
    let mut history: std::collections::HashMap<u64, std::collections::VecDeque<V2>> =
        Default::default();
    let mut spent = std::time::Duration::ZERO;
    let mut idle_states = std::collections::BTreeMap::new();
    let mut stuck_states = std::collections::BTreeMap::new();
    for _ in 0..ticks {
        let start = std::time::Instant::now();
        controller.update(&mut world);
        spent += start.elapsed();
        world.step();
        for tank in world.tanks.iter().filter(|t| t.bot && t.alive) {
            alive += 1;
            let state = format!("{:?}", controller.state(tank.id));
            if tank.vel.length() < 30. {
                idle += 1;
                *idle_states.entry(state.clone()).or_default() += 1;
            }
            if touching(&world, tank.pos, tank.radius) {
                walls += 1;
            }
            let trail = history.entry(tank.id).or_default();
            trail.push_back(tank.pos);
            if trail.len() > 120 {
                let old = trail.pop_front().unwrap();
                if old.distance(tank.pos) < 50. && tank.input.movement.length() > 0.5 {
                    stuck += 1;
                    *stuck_states.entry(state).or_default() += 1;
                }
            }
        }
        for tank in world.tanks.iter().filter(|t| !t.alive) {
            history.remove(&tank.id);
        }
    }
    let alive = alive.max(1) as f32;
    Report {
        idle_states,
        stuck_states,
        idle: idle as f32 / alive,
        walls: walls as f32 / alive,
        stuck: stuck as f32 / alive,
        kills: world.tanks.iter().map(|t| t.kills).sum(),
        micros_per_tick: spent.as_secs_f32() * 1e6 / ticks as f32,
    }
}

#[test]
#[ignore = "tuning report; run with --ignored --nocapture"]
fn behaviour_report() {
    for (name, mode, map) in [
        ("ffa open", Mode::Ffa, "open"),
        ("ffa fortress", Mode::Ffa, "fortress"),
        ("maze", Mode::Maze, "open"),
        ("2 teams competitive", Mode::TwoTeams, "competitive"),
    ] {
        let r = run(mode, map, 30, 7200);
        println!(
            "{name:<22} idle {:>5.1}%  walls {:>5.1}%  stuck {:>5.1}%  kills {:>3}  ai {:>6.0}us/tick",
            r.idle * 100.,
            r.walls * 100.,
            r.stuck * 100.,
            r.kills,
            r.micros_per_tick
        );
        println!(
            "    idle by state {:?}\n    stuck by state {:?}",
            r.idle_states, r.stuck_states
        );
    }
}

#[test]
fn bots_stay_busy_and_off_the_walls() {
    for (mode, map) in [(Mode::Ffa, "fortress"), (Mode::Maze, "open")] {
        let report = run(mode, map, 12, 2400);
        assert!(report.idle < 0.07, "{map}: idle {:.3}", report.idle);
        assert!(report.walls < 0.04, "{map}: walls {:.3}", report.walls);
        assert!(report.stuck < 0.08, "{map}: stuck {:.3}", report.stuck);
    }
}
