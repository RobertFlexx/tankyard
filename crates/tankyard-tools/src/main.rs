//! Reproducible, headless validation and measurement tools.
mod allocations;
mod drones;
mod train;
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
        /// The map to play, by ID, such as one a pack adds. It plays at its
        /// own suggested size, as in a workshop playtest.
        #[arg(long, default_value = "open")]
        map: String,
        /// Keep every body on the map sliding back and forth, as players
        /// pushing crates do.
        #[arg(long)]
        shove: bool,
    },
    /// Nominal weapon throughput; combat outcomes still require playtesting.
    Balance,
    /// Train the bots' driving network by self-play and write the best
    /// weights as Rust source.
    TrainDriver {
        #[arg(long, default_value_t = 120)]
        generations: u32,
        /// Pairs of weight changes tried each generation.
        #[arg(long, default_value_t = 12)]
        pairs: usize,
        /// Size of each change.
        #[arg(long, default_value_t = 0.08)]
        sigma: f32,
        /// Step size.
        #[arg(long, default_value_t = 0.02)]
        rate: f32,
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Bots on each side of every match.
        #[arg(long, default_value_t = 6)]
        bots: usize,
        #[arg(long, default_value_t = 4800)]
        ticks: u64,
        #[arg(long, default_value_t = 8)]
        threads: usize,
        /// Where to write the best weights, e.g. crates/tankyard-ai/src/driver_weights.rs.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Start from the weights bots ship with instead of the initial ones.
        #[arg(long)]
        resume: bool,
    },
    /// Pit expert bots at the top level against each boss in turn and
    /// report how long it lasts and what it costs them.
    BossFight {
        #[arg(long, default_value_t = 4)]
        bots: usize,
        /// The longest a fight runs, in seconds.
        #[arg(long, default_value_t = 180.)]
        seconds: f32,
        /// Fights per boss, each with its own seed.
        #[arg(long, default_value_t = 4)]
        rounds: u64,
        /// Bosses by id, such as siege_engine; every boss if none.
        #[arg(long, value_delimiter = ',')]
        bosses: Vec<String>,
    },
    /// Duel expert bots of every class in a tier against each other, both
    /// at the level the tier opens, and report each class's wins.
    Duels {
        #[arg(long, default_value_t = 4)]
        tier: u8,
        /// Duels per pair of classes, each with its own seed.
        #[arg(long, default_value_t = 6)]
        rounds: u64,
        /// The longest a duel runs, in seconds; then it is a draw.
        #[arg(long, default_value_t = 60.)]
        seconds: f32,
        /// Only these classes, by id.
        #[arg(long, value_delimiter = ',')]
        classes: Vec<String>,
    },
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
            let plugins = tankyard_lua::editor::Plugins::load(&mods);
            if let Some(error) = plugins.errors.first() {
                return Err(error.clone().into());
            }
            println!(
                "Validated {} mods, {} tanks, {} maps, {} hooks, {} textures, {} workshop tools",
                loaded.bundle.mods.len(),
                loaded.bundle.content.classes.len(),
                loaded.bundle.maps.len(),
                loaded
                    .behaviors
                    .iter()
                    .map(|m| m.hooks.len())
                    .sum::<usize>(),
                loaded.bundle.textures.len(),
                plugins.plugins.iter().map(|p| p.tools.len()).sum::<usize>()
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
            map,
            shove,
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
            let Some((width, height)) = loaded
                .bundle
                .maps
                .iter()
                .find(|m| m.id == map)
                .map(|m| (m.width, m.height))
            else {
                let known: Vec<_> = loaded.bundle.maps.iter().map(|m| m.id.as_str()).collect();
                return Err(format!("no map {map}; maps: {}", known.join(", ")).into());
            };
            let mut session = tankyard_server::LocalSession::with_mods(
                WorldConfig {
                    mode,
                    seed,
                    map,
                    width,
                    height,
                    ..Default::default()
                },
                bots,
                loaded,
            )?;
            let groups: Vec<u8> = session.world.map.bodies.iter().map(|b| b.group).collect();
            if shove && groups.is_empty() {
                return Err("--shove needs a map with bodies".into());
            }
            let mut mod_times = Vec::with_capacity(ticks);
            let mut tick_times = Vec::with_capacity(ticks);
            let mut bot_times = Vec::with_capacity(ticks);
            let mut mod_allocations = 0u64;
            let mut sliding = 0;
            let mut offsets = Vec::new();
            for tick in 0..ticks {
                let tick_start = Instant::now();
                if shove {
                    // A second each way, at a pushing tank's pace.
                    let way = if tick / 60 % 2 == 0 { 1. } else { -1. };
                    for &group in &groups {
                        session
                            .world
                            .shove_body(group, V2::new(way * 60., way * 60.));
                    }
                }
                let bots_start = Instant::now();
                session.bots.update(&mut session.world);
                bot_times.push(bots_start.elapsed().as_secs_f64() * 1000.);
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
                let now: Vec<V2> = groups
                    .iter()
                    .filter_map(|&group| session.world.body_state(group))
                    .map(|state| state.offset)
                    .collect();
                if now != offsets {
                    sliding += 1;
                    offsets = now;
                }
            }
            let mods_distribution = distribution(&mut mod_times);
            let tick_distribution = distribution(&mut tick_times);
            let bots_distribution = distribution(&mut bot_times);
            println!(
                "{} packs, {bots} bots, {ticks} ticks, {width}x{height} {}, load {load_ms:.1} ms",
                session.bundle.mods.len(),
                session.world.config.map
            );
            for (name, d) in [
                ("mods", &mods_distribution),
                ("bots", &bots_distribution),
                ("tick", &tick_distribution),
            ] {
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
            if !groups.is_empty() {
                println!("  bodies moved during {sliding} of {ticks} ticks");
            }
            if !session.mods.errors.is_empty() {
                return Err(session.mods.errors.join("; ").into());
            }
        }
        Command::TrainDriver {
            generations,
            pairs,
            sigma,
            rate,
            seed,
            bots,
            ticks,
            threads,
            out,
            resume,
        } => {
            train::train_driver(&train::Options {
                generations,
                pairs,
                sigma,
                rate,
                seed,
                bots,
                ticks,
                threads,
                out,
                start: resume.then(|| tankyard_ai::driver::trained().weights()),
            })?;
        }
        Command::BossFight {
            bots,
            seconds,
            rounds,
            bosses,
        } => {
            if !(1..=32).contains(&bots) || !(1. ..=1200.).contains(&seconds) || rounds > 64 {
                return Err(
                    "boss-fight takes 1 to 32 bots, up to 1200 seconds and 64 rounds".into(),
                );
            }
            let kinds = if bosses.is_empty() {
                BossKind::ALL.to_vec()
            } else {
                bosses
                    .iter()
                    .map(|id| BossKind::from_id(id).ok_or(format!("Unknown boss: {id}")))
                    .collect::<Result<Vec<_>, _>>()?
            };
            println!("boss,health,defeated,average_seconds,bot_deaths_per_fight,health_left");
            for kind in kinds {
                let (mut defeated, mut time, mut deaths, mut health, mut left) = (0, 0., 0, 0., 0.);
                for round in 0..rounds {
                    let mut world = World::new(
                        WorldConfig {
                            seed: 1000 + round,
                            mode: Mode::Sandbox,
                            target_count: 0,
                            ..Default::default()
                        },
                        Content::builtin(),
                    );
                    for i in 0..bots {
                        let id = world.add_player(&format!("Bot{i}"), true);
                        world.sandbox_level(id, MAX_LEVEL);
                    }
                    // One side against the boss, as a party is.
                    for tank in &mut world.tanks {
                        tank.team = 1;
                    }
                    let mut ai = BotController::new(Difficulty::Expert);
                    ai.adaptive = false;
                    // Let the bots spend their points and pick their tanks.
                    for _ in 0..600 {
                        ai.update(&mut world);
                        world.step();
                    }
                    let center = world.config.bounds().center();
                    let boss = world.spawn_boss_near(kind, center);
                    if boss == 0 {
                        return Err(format!("{} found no room", kind.id()).into());
                    }
                    health += world.bosses[0].max_health;
                    let before: u32 = world.tanks.iter().map(|tank| tank.deaths).sum();
                    let ticks = (seconds / DT) as usize;
                    let mut lasted = ticks;
                    for tick in 0..ticks {
                        ai.update(&mut world);
                        world.step();
                        // A Sunderworm fights on in two halves; the fight is
                        // over when nothing of it is left.
                        if world.bosses.is_empty() {
                            lasted = tick;
                            defeated += 1;
                            break;
                        }
                    }
                    let _ = boss;
                    time += lasted as f32 * DT;
                    left += world
                        .bosses
                        .iter()
                        .map(|b| b.health / b.max_health)
                        .fold(0., f32::max);
                    deaths += world.tanks.iter().map(|tank| tank.deaths).sum::<u32>() - before;
                }
                let rounds = rounds.max(1) as f32;
                println!(
                    "{},{:.0},{defeated}/{rounds},{:.1},{:.2},{:.0}%",
                    kind.id(),
                    health / rounds,
                    time / rounds,
                    deaths as f32 / rounds,
                    left / rounds * 100.,
                );
            }
        }
        Command::Duels {
            tier,
            rounds,
            seconds,
            classes: only,
        } => {
            let content = Content::builtin();
            let classes: Vec<_> = content
                .classes
                .iter()
                .filter(|class| class.tier == tier)
                .filter(|class| only.is_empty() || only.contains(&class.id))
                .map(|class| (class.id.clone(), class.min_level.max(1)))
                .collect();
            if classes.len() < 2 || rounds > 64 || !(5. ..=600.).contains(&seconds) {
                return Err("duels needs a tier of two or more classes, up to 64 rounds and 5 to 600 seconds".into());
            }
            // wins, losses, draws
            let mut record = vec![[0u32; 3]; classes.len()];
            let mut rows = Vec::new();
            for a in 0..classes.len() {
                for b in a + 1..classes.len() {
                    let mut score = [0u32; 3];
                    for round in 0..rounds {
                        let mut world = World::new(
                            WorldConfig {
                                seed: 77 + round * 131 + (a * 31 + b) as u64,
                                mode: Mode::Sandbox,
                                target_count: 0,
                                ..Default::default()
                            },
                            content.clone(),
                        );
                        // Alternate sides so neither spawn favors a class.
                        let (first, second) = if round % 2 == 0 { (a, b) } else { (b, a) };
                        let center = world.config.bounds().center();
                        let mut ids = [0; 2];
                        for (slot, &which) in [first, second].iter().enumerate() {
                            let id = world.add_player(&format!("Bot{slot}"), true);
                            // The class first, at the top level, then back
                            // to the level it opens at, so the bot can't
                            // upgrade past it.
                            world.sandbox_class(id, &classes[which].0);
                            world.sandbox_level(id, classes[which].1);
                            if let Some(tank) = world.tanks.iter_mut().find(|t| t.id == id) {
                                tank.pos =
                                    center + V2::new(if slot == 0 { -450. } else { 450. }, 0.);
                                tank.protection = 0.;
                            }
                            ids[slot] = id;
                        }
                        let mut ai = BotController::new(Difficulty::Expert);
                        ai.adaptive = false;
                        let mut winner = None;
                        for _ in 0..(seconds / DT) as usize {
                            ai.update(&mut world);
                            world.step();
                            let dead = |id| world.tanks.iter().any(|t| t.id == id && t.deaths > 0);
                            match (dead(ids[0]), dead(ids[1])) {
                                (false, false) => continue,
                                (true, false) => winner = Some(second),
                                (false, true) => winner = Some(first),
                                (true, true) => {}
                            }
                            break;
                        }
                        match winner {
                            Some(w) if w == a => score[0] += 1,
                            Some(_) => score[1] += 1,
                            None => score[2] += 1,
                        }
                    }
                    record[a][0] += score[0];
                    record[a][1] += score[1];
                    record[b][0] += score[1];
                    record[b][1] += score[0];
                    record[a][2] += score[2];
                    record[b][2] += score[2];
                    rows.push(format!(
                        "{},{},{},{},{}",
                        classes[a].0, classes[b].0, score[0], score[1], score[2]
                    ));
                }
            }
            println!("class,wins,losses,draws,win_rate");
            let mut order: Vec<_> = (0..classes.len()).collect();
            order.sort_by(|&x, &y| record[y][0].cmp(&record[x][0]));
            for index in order {
                let [wins, losses, draws] = record[index];
                println!(
                    "{},{wins},{losses},{draws},{:.2}",
                    classes[index].0,
                    f64::from(wins) / f64::from((wins + losses).max(1))
                );
            }
            println!("\nclass,opponent,wins,losses,draws");
            for row in rows {
                println!("{row}");
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
