//! A real UDP relay for reproducible latency, jitter, loss, and reordering tests.
//! Each relay serves one client; it forwards the actual encrypted transport
//! datagrams unchanged and bounds both queued packets and work per iteration.
use crate::NetError;
use std::{
    io,
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct EmulationConfig {
    /// Total added round-trip latency. Half is applied in each direction.
    pub latency_ms: u64,
    pub jitter_ms: u64,
    pub loss_percent: f32,
    pub seed: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct EmulationStats {
    pub received: u64,
    pub forwarded: u64,
    pub dropped: u64,
}
struct Counters {
    received: AtomicU64,
    forwarded: AtomicU64,
    dropped: AtomicU64,
}
struct Packet {
    due: Instant,
    to_server: bool,
    bytes: Vec<u8>,
}

pub struct NetworkEmulator {
    pub address: SocketAddr,
    running: Arc<AtomicBool>,
    counters: Arc<Counters>,
    worker: Option<JoinHandle<()>>,
}
impl NetworkEmulator {
    pub fn start(server: SocketAddr, config: EmulationConfig) -> Result<Self, NetError> {
        if config.latency_ms > 5000
            || config.jitter_ms > 2000
            || !config.loss_percent.is_finite()
            || !(0.0..=80.0).contains(&config.loss_percent)
        {
            return Err(NetError(
                "emulation latency/jitter/loss is outside supported limits".into(),
            ));
        }
        let binding = if server.is_ipv4() {
            "127.0.0.1:0"
        } else {
            "[::1]:0"
        };
        let front = UdpSocket::bind(binding)?;
        let backend = UdpSocket::bind(if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })?;
        front.set_nonblocking(true)?;
        backend.set_nonblocking(true)?;
        let address = front.local_addr()?;
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();
        let counters = Arc::new(Counters {
            received: AtomicU64::new(0),
            forwarded: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        });
        let worker_counters = counters.clone();
        let worker = thread::Builder::new()
            .name("udp-emulation".into())
            .spawn(move || {
                let mut queue: Vec<Packet> = Vec::with_capacity(4096);
                let mut client = None;
                let mut rng = config.seed.max(1);
                let mut buffer = [0u8; 1600];
                while worker_running.load(Ordering::Relaxed) {
                    for (socket, to_server) in [(&front, true), (&backend, false)] {
                        for _ in 0..64 {
                            let (length, source) = match socket.recv_from(&mut buffer) {
                                Ok(packet) => packet,
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                                Err(_) => break,
                            };
                            if to_server {
                                if client.is_none() {
                                    client = Some(source);
                                }
                                if client != Some(source) {
                                    continue;
                                }
                            } else if source != server {
                                continue;
                            }
                            worker_counters.received.fetch_add(1, Ordering::Relaxed);
                            let roll = random(&mut rng);
                            if roll * 100. < config.loss_percent || queue.len() >= 4096 {
                                worker_counters.dropped.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            let jitter = (random(&mut rng) * 2. - 1.) * config.jitter_ms as f32;
                            let delay_ms = (config.latency_ms as f32 * 0.5 + jitter).max(0.);
                            queue.push(Packet {
                                due: Instant::now() + Duration::from_secs_f32(delay_ms / 1000.),
                                to_server,
                                bytes: buffer[..length].to_vec(),
                            });
                        }
                    }
                    let now = Instant::now();
                    let mut i = 0;
                    while i < queue.len() {
                        if queue[i].due > now {
                            i += 1;
                            continue;
                        }
                        // swap_remove deliberately allows naturally reordered
                        // packets when their jitter makes them eligible sooner.
                        let packet = queue.swap_remove(i);
                        let sent = if packet.to_server {
                            backend.send_to(&packet.bytes, server)
                        } else if let Some(client) = client {
                            front.send_to(&packet.bytes, client)
                        } else {
                            continue;
                        };
                        if sent.is_ok() {
                            worker_counters.forwarded.fetch_add(1, Ordering::Relaxed);
                        } else {
                            worker_counters.dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            })?;
        Ok(Self {
            address,
            running,
            counters,
            worker: Some(worker),
        })
    }
    pub fn stats(&self) -> EmulationStats {
        EmulationStats {
            received: self.counters.received.load(Ordering::Relaxed),
            forwarded: self.counters.forwarded.load(Ordering::Relaxed),
            dropped: self.counters.dropped.load(Ordering::Relaxed),
        }
    }
    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for NetworkEmulator {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn random(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwards_actual_udp_and_applies_delay() {
        let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
        echo.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let address = echo.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let mut buffer = [0u8; 100];
            let (n, peer) = echo.recv_from(&mut buffer).unwrap();
            echo.send_to(&buffer[..n], peer).unwrap();
        });
        let relay = NetworkEmulator::start(
            address,
            EmulationConfig {
                latency_ms: 50,
                seed: 42,
                ..Default::default()
            },
        )
        .unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let started = Instant::now();
        client.send_to(b"tankyard", relay.address).unwrap();
        let mut buffer = [0u8; 100];
        let (n, _) = client.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], b"tankyard");
        assert!(started.elapsed() >= Duration::from_millis(45));
        worker.join().unwrap();
        assert_eq!(relay.stats().forwarded, 2);
    }
}
