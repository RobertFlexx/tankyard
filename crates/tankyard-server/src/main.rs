use clap::Parser;
use std::{
    io::{self, BufRead},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tankyard_server::{ServerSettings, UdpServer, difficulty_from_name, mode_from_name};

#[derive(Parser, Debug)]
#[command(about = "Tank Yard authoritative community UDP server", version)]
struct Args {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    bind: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, value_parser = mode_from_name)]
    mode: Option<tankyard_core::Mode>,
    #[arg(long)]
    map: Option<String>,
    #[arg(long)]
    max_players: Option<usize>,
    #[arg(long)]
    bots: Option<usize>,
    #[arg(long, value_parser = difficulty_from_name)]
    difficulty: Option<tankyard_ai::Difficulty>,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long)]
    allow_sandbox: bool,
    #[arg(long)]
    no_console: bool,
    #[arg(long)]
    write_config: Option<PathBuf>,
    /// Stop after a fixed wall-clock interval, useful for automated smoke tests.
    #[arg(long)]
    seconds: Option<f64>,
    /// Enable a runtime mod directory; repeat for multiple mods.
    #[arg(long = "mod")]
    mods: Vec<PathBuf>,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let mut settings = if let Some(path) = args.config {
        ServerSettings::load(path)?
    } else if std::path::Path::new("configs/server.toml").exists() {
        ServerSettings::load("configs/server.toml")?
    } else {
        ServerSettings::default()
    };
    if let Some(bind) = args.bind {
        settings.bind = bind;
    }
    if let Some(name) = args.name {
        settings.name = name;
    }
    if let Some(mode) = args.mode {
        settings.world.mode = mode;
    }
    if let Some(map) = args.map {
        settings.world.map = map;
    }
    if let Some(max_players) = args.max_players {
        settings.max_players = max_players;
    }
    if let Some(bots) = args.bots {
        settings.bot_count = bots;
    }
    if let Some(difficulty) = args.difficulty {
        settings.difficulty = difficulty;
    }
    if let Some(seed) = args.seed {
        settings.world.seed = seed;
    }
    if let Some(password) = args.password {
        settings.password = password;
    }
    if args.allow_sandbox {
        settings.allow_sandbox = true;
    }
    if !args.mods.is_empty() {
        settings.mods = args.mods;
    }
    settings.validate()?;
    if let Some(path) = args.write_config {
        settings.save(&path)?;
        println!("Wrote {}", path.display());
        return Ok(());
    }
    if args
        .seconds
        .is_some_and(|s| !s.is_finite() || s <= 0. || s > 86400.)
    {
        return Err("--seconds must be between 0 and 86400".into());
    }
    let mut server = UdpServer::bind(settings)?;
    println!(
        "{} listening on UDP {} | {} | 60 Hz simulation, {} Hz snapshots",
        server.settings.name,
        server.address,
        server.settings.world.mode.name(),
        server.settings.snapshot_rate
    );
    println!("Type help for local administration.");
    let stopping = Arc::new(AtomicBool::new(false));
    let signal_flag = stopping.clone();
    ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::Relaxed);
    })?;
    let (sender, receiver) = mpsc::sync_channel(16);
    if !args.no_console {
        thread::Builder::new()
            .name("server-console".into())
            .spawn(move || {
                for line in io::stdin().lock().lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    if line.len() <= 512 && sender.send(line).is_err() {
                        break;
                    }
                }
            })?;
    }
    let started = Instant::now();
    let mut previous = started;
    while !stopping.load(Ordering::Relaxed) {
        if args
            .seconds
            .is_some_and(|s| started.elapsed().as_secs_f64() >= s)
        {
            break;
        }
        for command in receiver.try_iter().take(16) {
            let reply = server.admin_command(&command);
            if !reply.message.is_empty() {
                println!("{}", reply.message);
            }
            if reply.shutdown {
                return Ok(());
            }
        }
        let now = Instant::now();
        server.update(now.duration_since(previous))?;
        previous = now;
        thread::sleep(Duration::from_millis(2));
    }
    println!("{}", server.admin_command("status").message);
    server.shutdown();
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Tank Yard server: {error}");
        std::process::exit(1);
    }
}
