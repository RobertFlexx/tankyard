use clap::Parser;
use std::{
    io::{self, BufRead},
    net::SocketAddr,
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
#[command(
    about = "Tank Yard authoritative community UDP server",
    version,
    after_help = "Options override the config file. Without --config, configs/server.toml is \
                  used when it exists. Start with:\n  tankyard-server --write-config my-server.toml\n  \
                  tankyard-server --config my-server.toml\nType help in the console for \
                  administration commands. Guide: docs/hosting.md"
)]
struct Args {
    /// Read settings from this TOML file (see --write-config).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Run the official central arena with its built-in public server profile.
    #[arg(long)]
    central: bool,
    /// UDP address to listen on, such as 0.0.0.0:5000 or [::]:5000.
    #[arg(long)]
    bind: Option<String>,
    /// Server name shown in Join server and the server list.
    #[arg(long)]
    name: Option<String>,
    /// ffa, two-teams, four-teams, maze, domination, survival, tag, infection,
    /// king-of-the-hill, mothership, capture-core, boss-rush, deathmatch,
    /// sandbox, random-events or coop-pve.
    #[arg(long, value_parser = mode_from_name)]
    mode: Option<tankyard_core::Mode>,
    /// Map id; the console's maps command lists them, mods included.
    #[arg(long)]
    map: Option<String>,
    /// Most players connected at once, across every lobby.
    #[arg(long)]
    max_players: Option<usize>,
    /// Most bots per lobby; with adaptive bots, they leave as players join.
    #[arg(long)]
    bots: Option<usize>,
    /// Keep the configured bot population regardless of human player count.
    #[arg(long)]
    fixed_bots: bool,
    /// Bot skill: beginner, normal, advanced or expert.
    #[arg(long, value_parser = difficulty_from_name)]
    difficulty: Option<tankyard_ai::Difficulty>,
    /// Fixed random seed, for reproducible rounds.
    #[arg(long)]
    seed: Option<u64>,
    /// Ask players for this password. It keeps strangers out; it does not
    /// encrypt anything.
    #[arg(long)]
    password: Option<String>,
    /// Allow Sandbox mode and its tools.
    #[arg(long)]
    allow_sandbox: bool,
    /// Don't read commands from standard input; use under systemd or Docker.
    #[arg(long)]
    no_console: bool,
    /// Write the complete settings, with these options applied, to this file
    /// and exit. Edit it, then start with --config.
    #[arg(long)]
    write_config: Option<PathBuf>,
    /// Listen on all interfaces at this UDP port (shorthand for --bind).
    #[arg(long, conflicts_with = "bind")]
    port: Option<u16>,
    /// Also accept WebSocket players on this TCP address (for example
    /// 127.0.0.1:8080 behind Caddy or nginx, so players join wss://your.domain).
    #[arg(long)]
    websocket: Option<String>,
    /// Split players into lobbies of this size; 0 keeps one lobby.
    #[arg(long)]
    arena_capacity: Option<usize>,
    /// Stop after a fixed wall-clock interval, useful for automated smoke tests.
    #[arg(long)]
    seconds: Option<f64>,
    /// Enable a runtime mod directory; repeat for multiple mods.
    #[arg(long = "mod")]
    mods: Vec<PathBuf>,
}

/// Where players can reach this server, so hosts know what to share.
fn print_addresses(server: &UdpServer) {
    let port = server.address.port();
    let ip = server.address.ip();
    let address = |ip: std::net::IpAddr| SocketAddr::new(ip, port).to_string();
    if ip.is_loopback() {
        println!("Players on this computer join {}.", address(ip));
        return;
    }
    let lan = tankyard_server::party::local_ip();
    let here = match ip {
        std::net::IpAddr::V4(ip) if ip.is_unspecified() => {
            address(std::net::Ipv4Addr::LOCALHOST.into())
        }
        std::net::IpAddr::V6(ip) if ip.is_unspecified() => {
            address(std::net::Ipv6Addr::LOCALHOST.into())
        }
        _ => address(ip),
    };
    println!("Players join: {here} on this computer");
    if ip.is_unspecified() && !lan.is_loopback() && ip.is_ipv4() == lan.is_ipv4() {
        println!("              {} on your local network", address(lan));
    }
    if ip.is_ipv6() {
        println!(
            "              your public IPv6 address, port {port}, once UDP {port} is allowed through the firewall"
        );
    } else {
        println!(
            "              your public address, port {port}, once UDP {port} is forwarded to this machine"
        );
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let mut settings = if let Some(path) = args.config {
        ServerSettings::load(path)?
    } else if args.central {
        if std::path::Path::new("configs/central.toml").exists() {
            ServerSettings::load("configs/central.toml")?
        } else {
            ServerSettings::central()
        }
    } else if std::path::Path::new("configs/server.toml").exists() {
        ServerSettings::load("configs/server.toml")?
    } else {
        ServerSettings::default()
    };
    if args.central {
        settings.kind = tankyard_protocol::ServerKind::Official;
        if settings.server_id.is_empty() {
            settings.server_id = "tankyard-central".into();
        }
    }
    if let Some(bind) = args.bind {
        settings.bind = bind;
    }
    if let Some(port) = args.port {
        settings.bind = format!("0.0.0.0:{port}");
    }
    if let Some(websocket) = args.websocket {
        settings.websocket = websocket;
    }
    if let Some(capacity) = args.arena_capacity {
        settings.arena_capacity = capacity;
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
    if args.fixed_bots {
        settings.adaptive_bots = false;
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
    if !server.settings.websocket.is_empty() {
        // The relay reaches this process over loopback, which the server
        // already treats as a shared address: bans target players, not it.
        let target = SocketAddr::new(
            match server.address.ip() {
                ip if !ip.is_unspecified() => ip,
                std::net::IpAddr::V4(_) => std::net::Ipv4Addr::LOCALHOST.into(),
                std::net::IpAddr::V6(_) => std::net::Ipv6Addr::LOCALHOST.into(),
            },
            server.address.port(),
        );
        let listen = &server.settings.websocket;
        let listener = std::net::TcpListener::bind(listen)
            .map_err(|error| format!("WebSocket listener on {listen}: {error}"))?;
        thread::Builder::new()
            .name("websocket".into())
            .spawn(move || {
                if let Err(error) = tankyard_server::websocket::serve_relay(listener, target, 256) {
                    log::error!("WebSocket listener stopped: {error}");
                }
            })?;
        println!(
            "Accepting WebSocket players on TCP {} (put TLS in front for wss://)",
            server.settings.websocket
        );
    }
    print_addresses(&server);
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
