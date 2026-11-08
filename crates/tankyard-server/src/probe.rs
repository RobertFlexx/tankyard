//! Server status probes: lobbies, population and the connection's real round
//! trip, measured over the same path gameplay uses (direct UDP, or the
//! WebSocket relay bridge for hostnames without a port).
use crate::{NetError, websocket};
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::{Duration, Instant},
};
use tankyard_protocol::{ServerStatus, parse_status_reply, status_request};

/// Requests per probe, and the spacing between them.
const SAMPLES: usize = 5;
const SPACING: Duration = Duration::from_millis(90);
/// How long the last request may take to come back.
const GRACE: Duration = Duration::from_millis(1200);

#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub status: ServerStatus,
    /// Median round trip of the replies, in milliseconds.
    pub rtt_ms: f32,
    /// Mean absolute deviation of the round trips, in milliseconds.
    pub jitter_ms: f32,
    /// Fraction of requests without a reply.
    pub loss: f32,
}

/// Query `address` and measure its connection. Blocks for up to about two
/// seconds; call it from a background thread.
pub fn probe(address: &str) -> Result<Probe, NetError> {
    let address = address.trim();
    if address.is_empty() || address.len() > 256 || address.starts_with("tankyard://") {
        return Err(NetError("Not a server address.".into()));
    }
    // Keep the bridge alive for the whole measurement.
    let bridge = websocket::websocket_url(address)
        .map(|url| websocket::ClientBridge::connect(&url))
        .transpose()?;
    let target: SocketAddr = match &bridge {
        Some(bridge) => bridge.address,
        None => address
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| NetError("Server address did not resolve.".into()))?,
    };
    let socket = UdpSocket::bind(if target.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_nonblocking(true)?;
    let base = u64::from_le_bytes(renet_netcode::generate_random_bytes());
    let mut sent = [None::<Instant>; SAMPLES];
    // Kept apart from `sent`, so an answered sample is never sent again.
    let mut answered = [false; SAMPLES];
    let mut rtts = Vec::with_capacity(SAMPLES);
    let mut status = None;
    let start = Instant::now();
    let mut buffer = [0u8; 2048];
    loop {
        let now = Instant::now();
        let due = (now - start).as_millis() as usize / SPACING.as_millis() as usize;
        for (index, slot) in sent.iter_mut().enumerate().take((due + 1).min(SAMPLES)) {
            if slot.is_none() {
                *slot = Some(now);
                let _ = socket.send_to(&status_request(base.wrapping_add(index as u64)), target);
            }
        }
        loop {
            match socket.recv_from(&mut buffer) {
                Ok((len, from)) if from == target => {
                    let Some((nonce, reply)) = parse_status_reply(&buffer[..len]) else {
                        continue;
                    };
                    let index = nonce.wrapping_sub(base) as usize;
                    if let Some(at) = sent.get(index).copied().flatten()
                        && !std::mem::replace(&mut answered[index], true)
                    {
                        rtts.push(at.elapsed().as_secs_f32() * 1000.);
                        status = Some(reply);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let all_sent = start + SPACING * (SAMPLES as u32 - 1);
        if rtts.len() == SAMPLES || now > all_sent + GRACE {
            break;
        }
        if let Some(bridge) = &bridge
            && let Some(failure) = bridge.failure()
        {
            return Err(NetError(failure));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let status = status.ok_or_else(|| {
        NetError("No reply. The server may be offline, or older than v0.10.".into())
    })?;
    rtts.sort_by(f32::total_cmp);
    let rtt_ms = rtts[rtts.len() / 2];
    let jitter_ms = rtts.iter().map(|r| (r - rtt_ms).abs()).sum::<f32>() / rtts.len() as f32;
    Ok(Probe {
        status,
        rtt_ms,
        jitter_ms,
        loss: 1. - rtts.len() as f32 / SAMPLES as f32,
    })
}
