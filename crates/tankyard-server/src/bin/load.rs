//! Real UDP multi-client workload. Report measured client-processing times and
//! transport bandwidth; server timing comes from the host's actual metrics.
use clap::Parser;
use std::{
    path::PathBuf,
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
    /// Reconnect one client every N seconds; zero disables churn.
    #[arg(long, default_value_t = 0.)]
    reconnect_every: f64,
    /// Write measured results as JSON, including reconnects and memory.
    #[arg(long)]
    output: Option<PathBuf>,
}

/// Uniform reservoir: a day-long soak keeps the same bounded memory as a
/// short test. Mean and worst time still account for every observation.
#[derive(Default)]
struct Timings {
    samples: Vec<f64>,
    count: u64,
    sum: f64,
    worst: f64,
    rng: u64,
}
impl Timings {
    const CAPACITY: usize = 65_536;
    fn push(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.worst = self.worst.max(value);
        if self.samples.len() < Self::CAPACITY {
            self.samples.push(value);
        } else {
            if self.rng == 0 {
                self.rng = 0x917a_5c31_7ea9_4b6d;
            }
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            let i = self.rng % self.count;
            if i < Self::CAPACITY as u64 {
                self.samples[i as usize] = value;
            }
        }
    }
    fn average(&self) -> f64 {
        self.sum / self.count.max(1) as f64
    }
    fn percentile(&self, p: f64) -> f64 {
        if self.samples.is_empty() {
            return 0.;
        }
        self.samples[((self.samples.len() - 1) as f64 * p) as usize]
    }
}
fn rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|v| v.split_whitespace().next()?.parse().ok())
            })
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !(1..=128).contains(&args.clients)
        || !args.seconds.is_finite()
        || !(1.0..=86400.0).contains(&args.seconds)
        || !args.reconnect_every.is_finite()
        || (args.reconnect_every != 0. && !(10.0..=3600.).contains(&args.reconnect_every))
        || !args.loss_percent.is_finite()
        || !(0.0..=80.).contains(&args.loss_percent)
        || args.latency_ms > 5000
        || args.jitter_ms > 2000
    {
        return Err("clients must be 1..128, seconds 1..86400, reconnect-every 0 or 10..3600, latency 0..5000 ms, jitter 0..2000 ms and loss 0..80%".into());
    }
    let mut host = if args.local {
        Some(HostedServer::start(ServerSettings {
            bind: "127.0.0.1:0".into(),
            // A lost disconnect can leave its old transport alive briefly.
            max_players: args.clients + usize::from(args.reconnect_every > 0.),
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
    let emulated = args.latency_ms > 0 || args.jitter_ms > 0 || args.loss_percent > 0.;
    let mut emulation_address = None;
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
        emulation_address = Some(server_address);
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
    let mut update_times = Timings::default();
    let mut sequence = 0u32;
    let mut peak_connected = 0;
    let mut received_snapshots = 0u64;
    let mut ticks = vec![None; clients.len()];
    let mut last_snapshot_at = vec![start; clients.len()];
    let mut next_reconnect = start + Duration::from_secs_f64(args.reconnect_every.max(1.));
    let mut pending_reconnect = None;
    let mut awaiting_rejoin = vec![false; clients.len()];
    let mut reconnects_started = 0_usize;
    let mut reconnects_completed = 0_usize;
    let mut unexpected_errors = Vec::new();
    let mut error_seen = vec![false; clients.len()];
    let mut peak_rss = rss_kib();
    let mut memory_at = start;
    while start.elapsed().as_secs_f64() < args.seconds {
        let now = Instant::now();
        let elapsed = now.duration_since(previous);
        previous = now;
        if args.reconnect_every > 0.
            && pending_reconnect.is_none()
            && now >= next_reconnect
            && args.seconds - start.elapsed().as_secs_f64() > 8.
        {
            let i = reconnects_started % clients.len();
            clients[i].disconnect();
            pending_reconnect = Some((i, now + Duration::from_millis(500 + args.latency_ms)));
            reconnects_started += 1;
            next_reconnect = now + Duration::from_secs_f64(args.reconnect_every);
        }
        if let Some((i, at)) = pending_reconnect
            && now >= at
        {
            let destination = if let Some(server) = emulation_address {
                relays[i] = NetworkEmulator::start(
                    server,
                    EmulationConfig {
                        latency_ms: args.latency_ms,
                        jitter_ms: args.jitter_ms,
                        loss_percent: args.loss_percent,
                        seed: 42 + reconnects_started as u64 * 131 + i as u64,
                    },
                )?;
                relays[i].address.to_string()
            } else {
                address.clone()
            };
            clients[i] = NetworkClient::connect(
                &destination,
                &format!("Load {:03}", i + 1),
                &args.password,
            )?;
            ticks[i] = None;
            last_snapshot_at[i] = now;
            error_seen[i] = false;
            awaiting_rejoin[i] = true;
            pending_reconnect = None;
        }
        input_accumulator += elapsed;
        let send = input_accumulator.as_secs_f64() >= 1. / 60.;
        if send {
            input_accumulator = Duration::ZERO;
            sequence = sequence.wrapping_add(1);
        }
        let update_start = Instant::now();
        for (i, client) in clients.iter_mut().enumerate() {
            if client.error.is_some() {
                if !error_seen[i] && pending_reconnect.is_none_or(|(index, _)| index != i) {
                    unexpected_errors.push(format!(
                        "client {}: {}",
                        i + 1,
                        client.error.as_deref().unwrap_or("connection failed")
                    ));
                    error_seen[i] = true;
                }
                continue;
            }
            let _ = client.update(elapsed);
            if client.is_ready() && awaiting_rejoin[i] {
                awaiting_rejoin[i] = false;
                reconnects_completed += 1;
            }
            if send && client.is_ready() {
                let direction = start.elapsed().as_secs_f32() * 0.4 + i as f32 * 0.7;
                let respawn =
                    client
                        .player_snapshot()
                        .zip(client.player_id)
                        .is_some_and(|(snapshot, id)| {
                            snapshot
                                .tanks
                                .iter()
                                .any(|tank| tank.id == id && !tank.alive)
                        });
                client.send_input(PlayerInput {
                    sequence,
                    movement: V2::from_angle(direction),
                    aim: direction + 0.7,
                    fire: true,
                    ability: respawn,
                    ..Default::default()
                });
                let _ = client.flush();
            }
            if let Some(snapshot) = &client.snapshot
                && Some((snapshot.match_state.round, snapshot.tick)) != ticks[i]
            {
                ticks[i] = Some((snapshot.match_state.round, snapshot.tick));
                last_snapshot_at[i] = Instant::now();
                received_snapshots += 1;
            }
        }
        update_times.push(update_start.elapsed().as_secs_f64() * 1000.);
        if now.duration_since(memory_at) >= Duration::from_secs(1) {
            peak_rss = peak_rss.max(rss_kib());
            memory_at = now;
        }
        peak_connected = peak_connected.max(clients.iter().filter(|c| c.is_ready()).count());
        thread::sleep(Duration::from_millis(2));
    }
    let elapsed = start.elapsed().as_secs_f64();
    let ready_at_end = clients.iter().filter(|client| client.is_ready()).count();
    let stale_at_end = last_snapshot_at
        .iter()
        .filter(|received_at| received_at.elapsed() > Duration::from_secs(2))
        .count();
    let average = update_times.average();
    update_times.samples.sort_by(f64::total_cmp);
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
        update_times.percentile(0.95),
        update_times.percentile(0.99),
        update_times.worst
    );
    println!(
        "Reconnects: {reconnects_completed}/{reconnects_started}; timing reservoir: {}/{} observations",
        update_times.samples.len(),
        update_times.count
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
    let server_status = host
        .as_ref()
        .map(|host| host.command("status"))
        .transpose()?;
    if let Some(status) = &server_status {
        println!("{status}");
    }
    if let Some(path) = &args.output {
        let report = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"protocol":tankyard_protocol::PROTOCOL_VERSION,
            "clients":args.clients,"seconds":elapsed,"peak_ready":peak_connected,
            "ready_at_end":ready_at_end,"stale_at_end":stale_at_end,"received_snapshots":received_snapshots,
            "reconnects_started":reconnects_started,"reconnects_completed":reconnects_completed,"unexpected_errors":unexpected_errors,
            "client_update_ms":{"mean":average,"p95":update_times.percentile(0.95),"p99":update_times.percentile(0.99),"max":update_times.worst},
            "timing_observations":update_times.count,"timing_samples":update_times.samples.len(),"peak_rss_kib":peak_rss,
            "transport":{"emulated":emulated,"latency_ms":args.latency_ms,"jitter_ms":args.jitter_ms,"imposed_loss_percent":args.loss_percent,
                "reported_loss_percent":100.*loss/args.clients as f64,"rtt_ms":ping/args.clients as f64},"server_status":server_status});
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
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
    if ready_at_end != args.clients
        || stale_at_end > 0
        || ticks.contains(&None)
        || reconnects_completed != reconnects_started
        || !unexpected_errors.is_empty()
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_long_soak_has_bounded_measurements_but_keeps_its_true_mean_and_worst() {
        let mut times = Timings::default();
        for i in 0..1_000_000 {
            times.push((i % 1000) as f64);
        }
        assert_eq!(times.count, 1_000_000);
        assert_eq!(times.samples.len(), Timings::CAPACITY);
        assert_eq!(times.average(), 499.5);
        assert_eq!(times.worst, 999.);
        times.samples.sort_by(f64::total_cmp);
        assert!((times.percentile(0.99) - 989.).abs() < 8.);
    }
}
