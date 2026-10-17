//! WebSocket-to-UDP relay for publishing a server behind an HTTP-only proxy
//! such as a Cloudflare Tunnel hostname. Players join `wss://<hostname>`.
use clap::Parser;
use std::net::ToSocketAddrs;

#[derive(Parser)]
#[command(
    name = "tankyard-relay",
    about = "Relay Tank Yard WebSocket clients to a UDP server",
    version
)]
struct Args {
    /// Plain WebSocket listener; put TLS (e.g. cloudflared) in front of it.
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,
    /// The game server's UDP address.
    #[arg(long, default_value = "127.0.0.1:5000")]
    server: String,
    #[arg(long, default_value_t = 256)]
    max_connections: usize,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let server = args
        .server
        .to_socket_addrs()?
        .next()
        .ok_or("server address did not resolve")?;
    ctrlc::set_handler(|| std::process::exit(0))?;
    tankyard_server::websocket::run_relay(&args.listen, server, args.max_connections)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Tank Yard relay: {error}");
        std::process::exit(1);
    }
}
