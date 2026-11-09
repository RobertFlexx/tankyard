//! Reproducible, headless validation and measurement tools.
mod allocations;
mod drones;
#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;
use clap::{Parser, Subcommand};
use std::{path::PathBuf, time::Instant};
use tankyard_ai::{BotController, Difficulty};
use tankyard_content::{Content, FirePattern, ProjectileKind};
use tankyard_core::*;
#[derive(Parser)]
#[command(about = "Tank Yard content, simulation and balance tools", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Validate enabled runtime packs without starting a server or window.
    ValidateMods {
        #[arg(long = "mod", required = true)]
        mods: Vec<PathBuf>,
    },
    /// Review packs (folders or .zip archives) for system access, native
    /// code and hidden payloads. Exits with status 2 if any pack is dangerous
    /// and 1 if any needs review; prints every finding.
    ScanMod {
        #[arg(required = true)]
        packs: Vec<PathBuf>,
        /// Also list advisory notes.
        #[arg(long)]
        verbose: bool,
    },
    /// Validate the complete upgrade graph and all mechanical profiles.
    Validate {
        #[arg(long)]
        content: Option<PathBuf>,
    },
    /// Measure actual fixed-step simulation and snapshot serialization.
    Bench {
        #[arg(long, default_value_t = 64)]
        players: usize,
        #[arg(long, default_value_t = 24)]
        bots: usize,
        #[arg(long, default_value_t = 1000)]
        targets: usize,
        #[arg(long, default_value_t = 3000)]
        projectiles: usize,
        #[arg(long, default_value_t = 1200)]
        ticks: usize,
        #[arg(long, default_value_t = 120)]
        warmup: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value = "open")]
        map: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Stress full drone swarms through cursor changes and recall, without a window.
    DroneBench {
        #[arg(long, default_value_t = 64)]
        swarms: usize,
        #[arg(long, default_value_t = 1200)]
        ticks: usize,
        #[arg(long, default_value_t = 120)]
        warmup: usize,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Measure what enabled packs cost per tick in a real bot match.
    ModBench {
        #[arg(long = "mod", required = true)]
        mods: Vec<PathBuf>,
        #[arg(long, default_value_t = 40)]
        bots: usize,
        #[arg(long, default_value_t = 3600)]
        ticks: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value = "ffa")]
        mode: String,
    },
    /// Nominal weapon throughput; combat outcomes still require playtesting.
    Balance,
    /// Run a bot arena and report real class usage, kills and scores.
    Tournament {
        #[arg(long, default_value_t = 24)]
        bots: usize,
        #[arg(long, default_value_t = 36000)]
        ticks: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
}
#[derive(serde::Serialize)]
struct Distribution {
    average_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}
fn distribution(values: &mut [f64]) -> Distribution {
    values.sort_by(f64::total_cmp);
    let sample = |p: f64| {
        values[((values.len() as f64 * p).ceil() as usize)
            .saturating_sub(1)
            .min(values.len() - 1)]
    };
    Distribution {
        average_ms: values.iter().sum::<f64>() / values.len() as f64,
        p95_ms: sample(0.95),
        p99_ms: sample(0.99),
        max_ms: *values.last().unwrap(),
    }
}
#[derive(serde::Serialize)]
struct Report {
    scenario: String,
    seed: u64,
    players: usize,
    bots: usize,
    targets: usize,
    requested_projectiles: usize,
    average_projectiles: f64,
    peak_projectiles: usize,
    ticks: usize,
    warmup_ticks: usize,
    simulation: Distribution,
    ai: Distribution,
    snapshot: Distribution,
    average_tick_allocation_calls: f64,
    average_snapshot_allocation_calls: f64,
    average_snapshot_bytes: f64,
    estimated_snapshot_payload_bytes_per_second: f64,
    rss_kib: Option<u64>,
    cpu: String,
    os: String,
    rustc: String,
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::ScanMod { packs, verbose } => {
            use tankyard_mods::scan::{Severity, Verdict};
            let mut worst = Verdict::Clean;
            for path in &packs {
                let pack = tankyard_mods::PackFiles::read(path)?;
                let report = tankyard_mods::scan::scan_pack(&pack);
                let verdict = report.verdict();
                println!("{}: {verdict:?} ({} files)", path.display(), pack.len());
                for finding in report
                    .findings
                    .iter()
                    .filter(|f| verbose || f.severity != Severity::Info)
                {
                    let place = if finding.line > 0 {
                        format!("{}:{}", finding.file, finding.line)
                    } else {
                        finding.file.clone()
                    };
                    println!("  {:?} {place}: {}", finding.severity, finding.message);
                }
                worst = match (worst, verdict) {
                    (Verdict::Dangerous, _) | (_, Verdict::Dangerous) => Verdict::Dangerous,
                    (Verdict::Review, _) | (_, Verdict::Review) => Verdict::Review,
                    _ => Verdict::Clean,
                };
            }
            match worst {
                Verdict::Clean => {}
                Verdict::Review => std::process::exit(1),
                Verdict::Dangerous => std::process::exit(2),
            }
        }
        Command::ValidateMods { mods } => {
            let loaded = tankyard_lua::load(&mods)?;
            println!(
                "Validated {} mods, {} tanks, {} maps, {} hooks, {} textures",
                loaded.bundle.mods.len(),
                loaded.bundle.content.classes.len(),
                loaded.bundle.maps.len(),
                loaded
                    .behaviors
                    .iter()
                    .map(|m| m.hooks.len())
                    .sum::<usize>(),
                loaded.bundle.textures.len()
            );
        }
        Command::Validate { content } => {
            let data = if let Some(file) = content {
                Content::from_json(&std::fs::read_to_string(file)?)?
            } else {
                Content::builtin()
            };
            data.validate()?;
            println!(
                "Validated {} mechanically unique classes; all tiers and upgrade edges reachable.",
                data.classes.len()
            );
            for tier in 0..=4 {
                println!(
                    "  Tier {tier}: {} classes",
                    data.classes.iter().filter(|c| c.tier == tier).count()
                );
            }
        }
        Command::Bench {
            players,
            bots,
            targets,
            projectiles,
            ticks,
            warmup,
            seed,
            map,
            output,
        } => {
            if players + bots == 0
                || players + bots > 256
                || ticks == 0
                || ticks > 1_000_000
                || targets > 10000
                || projectiles > 20000
                || warmup > 100000
            {
                return Err("benchmark requires 1..256 participants, 1..1000000 ticks, <=10000 targets and <=20000 requested projectiles".into());
            }
            let mut world = World::new(
                WorldConfig {
                    width: 8000.,
                    height: 8000.,
                    target_count: targets,
                    max_projectiles: 30000,
                    seed,
                    map: map.clone(),
                    spawn_protection: 0.,
                    ..Default::default()
                },
                Content::builtin(),
            );
            let classes: Vec<_> = world
                .content
                .classes
                .iter()
                .filter(|c| c.tier == 3)
                .map(|c| c.id.clone())
                .collect();
            world.config.mode = Mode::Sandbox;
            for i in 0..players + bots {
                let id = world.add_player(&format!("Stress{i}"), i >= players);
                world.sandbox_class(id, &classes[i % classes.len()]);
                world.sandbox_invulnerability(id, true);
            }
            world.config.mode = Mode::Ffa;
            let mut snapshot_buffer = world.snapshot();
            let mut snapshot_encoder = tankyard_protocol::SnapshotEncoder::default();
            let mut ai = BotController::new(Difficulty::Advanced);
            ai.adaptive = false;
            let mut rng = seed;
            let mut tick_times = Vec::with_capacity(ticks);
            let mut ai_times = Vec::with_capacity(ticks);
            let mut snapshot_times = Vec::with_capacity(ticks / 3 + 1);
            let mut sizes = 0usize;
            let mut sample_count = 0usize;
            let mut sum_projectiles = 0usize;
            let mut peak = 0;
            let mut tick_allocations = 0u64;
            let mut snapshot_allocations = 0u64;
            for step in 0..warmup + ticks {
                fill_projectiles(&mut world, projectiles, &mut rng);
                for tank in world.tanks.iter_mut().filter(|t| !t.bot) {
                    tank.input = PlayerInput {
                        sequence: step as u32 + 1,
                        movement: V2::from_angle(step as f32 * 0.007 + tank.id as f32),
                        aim: step as f32 * 0.014 + tank.id as f32,
                        fire: true,
                        ability: step % 360 == 0,
                        ..Default::default()
                    };
                }
                let allocations_before = allocations::count();
                let start = Instant::now();
                ai.update(&mut world);
                let ai_time = start.elapsed().as_secs_f64() * 1000.;
                world.step();
                let tick_time = start.elapsed().as_secs_f64() * 1000.;
                if step >= warmup {
                    tick_allocations += allocations::count() - allocations_before;
                    tick_times.push(tick_time);
                    ai_times.push(ai_time);
                    sum_projectiles += world.projectiles.len();
                    peak = peak.max(world.projectiles.len());
                    if step % 3 == 0 {
                        let allocations_before = allocations::count();
                        let start = Instant::now();
                        world.snapshot_for_into(&mut snapshot_buffer, world.tanks[0].pos, 1400.);
                        let bytes = snapshot_encoder.encode(&snapshot_buffer)?;
                        snapshot_allocations += allocations::count() - allocations_before;
                        snapshot_times.push(start.elapsed().as_secs_f64() * 1000.);
                        sizes += bytes.len();
                        sample_count += 1;
                    }
                }
            }
            if snapshot_times.is_empty() {
                let allocations_before = allocations::count();
                let start = Instant::now();
                let bytes =
                    snapshot_encoder.encode(&world.snapshot_for(world.tanks[0].pos, 1400.))?;
                snapshot_allocations += allocations::count() - allocations_before;
                snapshot_times.push(start.elapsed().as_secs_f64() * 1000.);
                sizes += bytes.len();
                sample_count += 1;
            }
            let average_bytes = sizes as f64 / sample_count as f64;
            let report = Report {
                scenario: format!(
                    "{map}: synthetic moving inputs, diverse level 45 classes, uniformly distributed replenished bullets, invulnerable participants"
                ),
                seed,
                players,
                bots,
                targets: world.targets.len(),
                requested_projectiles: projectiles,
                average_projectiles: sum_projectiles as f64 / ticks as f64,
                peak_projectiles: peak,
                ticks,
                warmup_ticks: warmup,
                simulation: distribution(&mut tick_times),
                ai: distribution(&mut ai_times),
                snapshot: distribution(&mut snapshot_times),
                average_tick_allocation_calls: tick_allocations as f64 / ticks as f64,
                average_snapshot_allocation_calls: snapshot_allocations as f64
                    / sample_count as f64,
                average_snapshot_bytes: average_bytes,
                estimated_snapshot_payload_bytes_per_second: average_bytes * 20.,
                rss_kib: memory_kib(),
                cpu: cpu_name(),
                os: std::env::consts::OS.into(),
                rustc: std::process::Command::new("rustc")
                    .arg("--version")
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().into())
                    .unwrap_or_default(),
            };
            let json = serde_json::to_string_pretty(&report)?;
            println!("{json}");
            println!(
                "Snapshot bandwidth is one viewer's compressed payload at 20 Hz; UDP/Renet overhead is excluded. System allocator calls are counted; this does not measure rendering or internet latency."
            );
            if let Some(path) = output {
                std::fs::write(path, json)?;
            }
        }
        Command::DroneBench {
            swarms,
            ticks,
            warmup,
            output,
        } => {
            drones::bench(swarms, ticks, warmup, output.as_deref())?;
        }
        Command::Balance => {
            println!(
                "class,ability,direct_dps,max_direct_dps,deployable_volley_damage,range,base_health,move_speed,body_damage"
            );
            for class in Content::builtin().classes {
                let dps: f32 = class
                    .barrels
                    .iter()
                    .filter(|b| {
                        !matches!(
                            b.projectile,
                            ProjectileKind::Drone
                                | ProjectileKind::Minion
                                | ProjectileKind::Trap
                                | ProjectileKind::Mine
                                | ProjectileKind::Barrier
                                | ProjectileKind::Turret
                        )
                    })
                    .map(|b| {
                        let cycle = match b.pattern {
                            FirePattern::Charge => 1.0_f32.max(b.cooldown / class.multipliers[6]),
                            FirePattern::Burst => (b.cooldown + 0.17) / 3.,
                            _ => b.cooldown,
                        };
                        let factor = if b.pattern == FirePattern::Charge {
                            2.2
                        } else {
                            1.
                        };
                        b.damage * b.count as f32 * factor / cycle
                            * class.multipliers[5]
                            * if b.pattern == FirePattern::Charge {
                                1.0
                            } else {
                                class.multipliers[6]
                            }
                    })
                    .sum();
                let range = class
                    .barrels
                    .iter()
                    .map(|b| b.speed * b.lifetime)
                    .fold(0., f32::max);
                let deployed_damage: f32 = class
                    .barrels
                    .iter()
                    .filter(|b| {
                        matches!(
                            b.projectile,
                            ProjectileKind::Drone
                                | ProjectileKind::Minion
                                | ProjectileKind::Trap
                                | ProjectileKind::Mine
                                | ProjectileKind::Barrier
                                | ProjectileKind::Turret
                        )
                    })
                    .map(|b| b.damage * f32::from(b.count) * class.multipliers[5])
                    .sum();
                println!(
                    "{},{:?},{:.2},{:.2},{:.2},{:.0},{:.0},{:.0},{:.0}",
                    class.id,
                    class.ability,
                    dps,
                    dps * stat_scale(7).powi(2),
                    deployed_damage,
                    range * class.multipliers[3],
                    100. * class.multipliers[0],
                    210. * class.multipliers[7],
                    30. * class.multipliers[2]
                );
            }
            println!(
                "Direct DPS excludes accuracy, armor, penetration, explosions and deployed units. Max DPS is the damage/reload build ceiling; charged weapons also have a one-second charge time."
            );
        }
        Command::ModBench {
            mods,
            bots,
            ticks,
            seed,
            mode,
        } => {
            if !(1..=128).contains(&bots) || ticks == 0 || ticks > 1_000_000 {
                return Err("mod-bench requires 1..128 bots and 1..1000000 ticks".into());
            }
            let mode = match mode.as_str() {
                "ffa" => Mode::Ffa,
                "teams" | "two-teams" => Mode::TwoTeams,
                "four-teams" => Mode::FourTeams,
                other => {
                    return Err(
                        format!("unknown mode {other}; use ffa, teams or four-teams").into(),
                    );
                }
            };
            let start = Instant::now();
            let loaded = tankyard_lua::load(&mods)?;
            let load_ms = start.elapsed().as_secs_f64() * 1000.;
            let mut session = tankyard_server::LocalSession::with_mods(
                WorldConfig {
                    mode,
                    seed,
                    ..Default::default()
                },
                bots,
                loaded,
            )?;
            let mut mod_times = Vec::with_capacity(ticks);
            let mut tick_times = Vec::with_capacity(ticks);
            let mut mod_allocations = 0u64;
            for _ in 0..ticks {
                let tick_start = Instant::now();
                session.bots.update(&mut session.world);
                let mut spent = 0.;
                let mut allocations_spent = 0;
                let mods = &mut session.mods;
                session.world.step_with(|world| {
                    let before = allocations::count();
                    let start = Instant::now();
                    mods.before_step(world);
                    spent += start.elapsed().as_secs_f64() * 1000.;
                    allocations_spent += allocations::count() - before;
                });
                let before = allocations::count();
                let start = Instant::now();
                session.mods.after_step(&mut session.world);
                spent += start.elapsed().as_secs_f64() * 1000.;
                allocations_spent += allocations::count() - before;
                mod_allocations += allocations_spent;
                mod_times.push(spent);
                tick_times.push(tick_start.elapsed().as_secs_f64() * 1000.);
            }
            let mods_distribution = distribution(&mut mod_times);
            let tick_distribution = distribution(&mut tick_times);
            println!(
                "{} packs, {bots} bots, {ticks} ticks, load {load_ms:.1} ms",
                session.bundle.mods.len()
            );
            for (name, d) in [("mods", &mods_distribution), ("tick", &tick_distribution)] {
                println!(
                    "  {name:<5} avg {:.4} ms  p95 {:.4} ms  p99 {:.4} ms  max {:.4} ms",
                    d.average_ms, d.p95_ms, d.p99_ms, d.max_ms
                );
            }
            println!(
                "  mods use {:.1}% of the tick, {:.0} allocations per tick",
                100. * mods_distribution.average_ms / tick_distribution.average_ms.max(1e-9),
                mod_allocations as f64 / ticks as f64
            );
            if !session.mods.errors.is_empty() {
                return Err(session.mods.errors.join("; ").into());
            }
        }
        Command::Tournament { bots, ticks, seed } => {
            if !(2..=128).contains(&bots) || ticks > 1_000_000 {
                return Err("tournament requires2..128bots and <=1000000ticks".into());
            }
            let mut world = World::new(
                WorldConfig {
                    seed,
                    ..Default::default()
                },
                Content::builtin(),
            );
            for i in 0..bots {
                world.add_player(&format!("Bot{i}"), true);
            }
            let mut ai = BotController::new(Difficulty::Expert);
            ai.adaptive = false;
            for _ in 0..ticks {
                ai.update(&mut world);
                world.step();
            }
            world.tanks.sort_by_key(|t| std::cmp::Reverse(t.score));
            println!("seed={seed},ticks={ticks}; name,class,level,score,kills,deaths");
            for tank in world.tanks {
                println!(
                    "{},{},{},{},{},{}",
                    tank.name, tank.class, tank.level, tank.score, tank.kills, tank.deaths
                );
            }
        }
    }
    Ok(())
}
fn fill_projectiles(world: &mut World, wanted: usize, rng: &mut u64) {
    let count = wanted.saturating_sub(world.projectiles.len());
    let owner = world.tanks[0].id;
    for _ in 0..count {
        let pos = V2::new(
            50. + random(rng) * (world.config.width - 100.),
            50. + random(rng) * (world.config.height - 100.),
        );
        if !world.position_clear(pos, 5.) {
            continue;
        }
        let velocity = V2::from_angle(random(rng) * std::f32::consts::TAU) * 600.;
        world.projectiles.push(Projectile {
            id: rng.wrapping_add(1_000_000_000),
            owner,
            team: 0,
            pos,
            previous: pos,
            vel: velocity,
            cruise_speed: velocity.length(),
            radius: 4.,
            damage: 1.,
            penetration: 1.,
            lifetime: 3.,
            kind: ProjectileKind::Bullet,
            ricochets: 0,
            age: 0.,
            hit: Vec::new(),
            gun: None,
            homing_target: 0,
            frozen: None,
            stasis_used: false,
        });
    }
}
fn random(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*seed >> 40) as f32 / (1u32 << 24) as f32
}
fn memory_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}
fn cpu_name() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| std::env::consts::ARCH.into())
}
