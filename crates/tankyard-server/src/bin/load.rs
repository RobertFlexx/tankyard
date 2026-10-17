//! Real UDP multi-client workload. Report measured client-processing times and
//! transport bandwidth; server timing comes from the host's actual metrics.
use clap::Parser;
use std::{
    thread,
    time::{Duration, Instant},
};
use tankyard_core::{PlayerInput, V2};
use tankyard_server::netem::{EmulationConfig, NetworkEmulator};
use tankyard_server::{HostedServer, NetworkClient, ServerSettings};

#[derive(Parser)]
#[command(
    name = "tankyard-load",
    about = "Generate and measure a real Tank Yard UDP multiplayer workload",
    version
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:5000")]
    server: String,
    #[arg(long, default_value_t = 8)]
    clients: usize,
    #[arg(long, default_value_t = 15.)]
    seconds: f64,
    #[arg(long)]
    local: bool,
    #[arg(long, default_value_t = 12)]
    bots: usize,
    /// Keep the requested bot count while measuring a fixed workload.
    #[arg(long)]
    fixed_bots: bool,
    #[arg(long, default_value = "")]
    password: String,
    #[arg(long, default_value_t = 0)]
    latency_ms: u64,
    #[arg(long, default_value_t = 0)]
    jitter_ms: u64,
    #[arg(long, default_value_t = 0.)]
    loss_percent: f32,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !(1..=128).contains(&args.clients)
        || !args.seconds.is_finite()
        || !(1.0..=3600.0).contains(&args.seconds)
    {
        return Err("clients must be 1..128 and seconds 1..3600".into());
    }
    let mut host = if args.local {
        Some(HostedServer::start(ServerSettings {
            bind: "127.0.0.1:0".into(),
            max_players: args.clients,
            bot_count: args.bots,
            adaptive_bots: !args.fixed_bots,
            ban_file: String::new(),
            password: args.password.clone(),
            ..Default::default()
        })?)
    } else {
        None
    };
    let address = host
        .as_ref()
        .map(|h| h.address.to_string())
        .unwrap_or(args.server);
    let mut relays = Vec::new();
    if args.latency_ms > 0 || args.jitter_ms > 0 || args.loss_percent > 0. {
        use std::net::ToSocketAddrs;
        if args.clients > 16 {
            return Err("latency emulation supports at most 16 clients per process".into());
        }
        let server_address = address
            .to_socket_addrs()?
            .next()
            .ok_or("server address did not resolve")?;
        for i in 0..args.clients {
            relays.push(NetworkEmulator::start(
                server_address,
                EmulationConfig {
                    latency_ms: args.latency_ms,
                    jitter_ms: args.jitter_ms,
                    loss_percent: args.loss_percent,
                    seed: 42 + i as u64,
                },
            )?);
        }
    }
    let mut clients: Vec<_> = (0..args.clients)
        .map(|i| {
            let address = relays
                .get(i)
                .map(|r| r.address.to_string())
                .unwrap_or_else(|| address.clone());
            NetworkClient::connect(&address, &format!("Load {:03}", i + 1), &args.password)
        })
        .collect::<Result<_, _>>()?;
    let start = Instant::now();
    let mut previous = start;
    let mut input_accumulator = Duration::ZERO;
    let mut update_times = Vec::new();
    let mut sequence = 0u32;
    let mut peak_connected = 0;
    let mut received_snapshots = 0u64;
    let mut ticks = vec![0u64; clients.len()];
    let mut last_snapshot_at = vec![start; clients.len()];
    while start.elapsed().as_secs_f64() < args.seconds {
        let now = Instant::now();
        let elapsed = now.duration_since(previous);
        previous = now;
        input_accumulator += elapsed;
        let send = input_accumulator.as_secs_f64() >= 1. / 60.;
        if send {
            input_accumulator = Duration::ZERO;
            sequence = sequence.wrapping_add(1);
        }
        let update_start = Instant::now();
        for (i, client) in clients.iter_mut().enumerate() {
            if client.error.is_some() {
                continue;
            }
            let _ = client.update(elapsed);
            if send && client.is_ready() {
                let direction = start.elapsed().as_secs_f32() * 0.4 + i as f32 * 0.7;
                client.send_input(PlayerInput {
                    sequence,
                    movement: V2::from_angle(direction),
                    aim: direction + 0.7,
                    fire: true,
                    ..Default::default()
                });
                let _ = client.flush();
            }
            if let Some(snapshot) = &client.snapshot
                && snapshot.tick > ticks[i]
            {
                ticks[i] = snapshot.tick;
                last_snapshot_at[i] = Instant::now();
                received_snapshots += 1;
            }
        }
        update_times.push(update_start.elapsed().as_secs_f64() * 1000.);
        peak_connected = peak_connected.max(clients.iter().filter(|c| c.is_ready()).count());
        thread::sleep(Duration::from_millis(2));
    }
    let elapsed = start.elapsed().as_secs_f64();
    let ready_at_end = clients.iter().filter(|client| client.is_ready()).count();
    let stale_at_end = last_snapshot_at
        .iter()
        .filter(|received_at| received_at.elapsed() > Duration::from_secs(2))
        .count();
    let average = update_times.iter().sum::<f64>() / update_times.len().max(1) as f64;
    update_times.sort_by(f64::total_cmp);
    let percentile =
        |p: f64| update_times[((update_times.len().saturating_sub(1) as f64) * p) as usize];
    let mut outgoing = 0.;
    let mut incoming = 0.;
    let mut loss = 0.;
    let mut ping = 0.;
    for client in &clients {
        let d = client.diagnostics();
        outgoing += d.sent_bytes_per_sec;
        incoming += d.received_bytes_per_sec;
        loss += d.packet_loss;
        ping += d.ping_ms;
    }
    println!(
        "Real UDP workload: {} clients, {} seconds, peak {} ready, {} ready at finish, {} stale, {} received snapshots",
        args.clients, elapsed, peak_connected, ready_at_end, stale_at_end, received_snapshots
    );
    println!(
        "Combined client update time: average {:.3} ms, p95 {:.3} ms, p99 {:.3} ms, max {:.3} ms",
        average,
        percentile(0.95),
        percentile(0.99),
        percentile(1.)
    );
    println!(
        "Per-client transport: {:.0} B/s sent, {:.0} B/s received, {:.2} ms RTT, {:.2}% packet loss",
        outgoing / args.clients as f64,
        incoming / args.clients as f64,
        ping / args.clients as f64,
        100. * loss / args.clients as f64
    );
    for client in &clients {
        if let Some(error) = &client.error {
            println!("Client {}: {error}", client.client_id());
        }
    }
    for (i, relay) in relays.iter().enumerate() {
        println!("Relay {i}: {:?}", relay.stats());
    }
    if let Some(host) = &host {
        println!("{}", host.command("status")?);
    }
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(rss) = status.lines().find(|line| line.starts_with("VmRSS:"))
    {
        println!(
            "Combined load-test process resident memory: {}",
            rss.trim_start_matches("VmRSS:").trim()
        );
    }
    for client in &mut clients {
        client.disconnect();
    }
    if let Some(host) = &mut host {
        host.shutdown();
    }
    if peak_connected != args.clients {
        return Err("not all workload clients completed their handshake".into());
    }
    if ready_at_end != args.clients || stale_at_end > 0 || ticks.contains(&0) {
        return Err("workload clients disconnected or stopped receiving snapshots".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Tank Yard load test: {error}");
        std::process::exit(1);
    }
}
