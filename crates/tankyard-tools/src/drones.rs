use crate::{Distribution, allocations, distribution};
use std::{path::Path, time::Instant};
use tankyard_content::{Content, ProjectileKind};
use tankyard_core::{Mode, PlayerInput, Projectile, V2, World, WorldConfig};

#[derive(serde::Serialize)]
struct Report {
    scenario: &'static str,
    version: &'static str,
    swarms: usize,
    drones: usize,
    ticks: usize,
    warmup_ticks: usize,
    simulation: Distribution,
    average_tick_allocation_calls: f64,
    maximum_outline_overlap: f32,
    final_outline_overlap: f32,
}

pub fn bench(
    swarms: usize,
    ticks: usize,
    warmup: usize,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !(1..=128).contains(&swarms) || !(1..=1_000_000).contains(&ticks) || warmup > 100_000 {
        return Err(
            "drone benchmark requires 1..128 swarms, 1..1000000 ticks and <=100000 warmup ticks"
                .into(),
        );
    }
    let mut world = World::new(
        WorldConfig {
            mode: Mode::Sandbox,
            width: 8000.0,
            height: 16000.0,
            target_count: 0,
            time_limit: 86400.0,
            ..Default::default()
        },
        Content::builtin(),
    );
    for index in 0..swarms {
        let owner = world.add_player(&format!("Swarm {}", index + 1), false);
        world.sandbox_class(owner, "flock");
        let tank = world.tanks.last_mut().unwrap();
        tank.pos = V2::new(
            500.0 + (index % 8) as f32 * 900.0,
            500.0 + (index / 8) as f32 * 900.0,
        );
        let pos = tank.pos + V2::new(250.0, 0.0);
        for drone in 0..16 {
            world.projectiles.push(Projectile {
                id: 1_000_000 + index as u64 * 16 + drone,
                owner,
                team: 0,
                pos,
                previous: pos,
                vel: V2::ZERO,
                cruise_speed: 300.0,
                radius: 7.5,
                damage: 10.0,
                penetration: 100.0,
                lifetime: 86400.0,
                kind: ProjectileKind::Drone,
                ricochets: 0,
                age: 0.0,
                hit: Vec::new(),
                gun: None,
            });
        }
    }
    let mut times = Vec::with_capacity(ticks);
    let mut allocation_calls = 0;
    let mut maximum_overlap = 0.0_f32;
    for tick in 0..warmup + ticks {
        let phase = (tick / 300) % 4;
        for tank in &mut world.tanks {
            tank.input = PlayerInput {
                sequence: tick as u32 + 1,
                fire: true,
                alt_fire: phase == 3,
                aim: phase as f32 * std::f32::consts::FRAC_PI_2,
                aim_distance: 350.0,
                ..Default::default()
            };
        }
        let before = allocations::count();
        let start = Instant::now();
        world.step();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let calls = allocations::count() - before;
        if world.projectiles.len() != swarms * 16 {
            return Err(format!(
                "expected {} drones, found {} at tick {tick}",
                swarms * 16,
                world.projectiles.len()
            )
            .into());
        }
        if tick >= warmup {
            times.push(elapsed);
            allocation_calls += calls;
            maximum_overlap = maximum_overlap.max(outline_overlap(&world.projectiles));
        }
    }
    let report = Report {
        scenario: "independent full 16-drone swarms; cursor changes every five seconds, then recall; no targets, AI, walls, rendering or network",
        version: env!("CARGO_PKG_VERSION"),
        swarms,
        drones: swarms * 16,
        ticks,
        warmup_ticks: warmup,
        simulation: distribution(&mut times),
        average_tick_allocation_calls: allocation_calls as f64 / ticks as f64,
        maximum_outline_overlap: maximum_overlap,
        final_outline_overlap: outline_overlap(&world.projectiles),
    };
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        std::fs::write(path, &json)?;
    }
    println!("{json}");
    Ok(())
}

fn outline_overlap(projectiles: &[Projectile]) -> f32 {
    let mut overlap = 0.0_f32;
    for swarm in projectiles.as_chunks::<16>().0 {
        for (i, a) in swarm.iter().enumerate() {
            for b in &swarm[i + 1..] {
                overlap = overlap.max(a.radius + b.radius + 4.0 - a.pos.distance(b.pos));
            }
        }
    }
    overlap
}
