//! The server's netcode transport: renet_netcode's socket loop, plus answers
//! to out-of-band status queries before netcode sees a datagram.
use renet::RenetServer;
use renet_netcode::NetcodeTransportError;
use renetcode::{NETCODE_MAX_PACKET_BYTES, NetcodeServer, ServerConfig, ServerResult};
use std::{
    collections::HashMap,
    io,
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

/// Replies per second across all sources, and per source address.
const GLOBAL_RATE: f32 = 400.;
const SOURCE_RATE: f32 = 8.;
const SOURCE_BURST: f32 = 16.;
const MAX_SOURCES: usize = 4096;

struct Bucket {
    tokens: f32,
    at: Instant,
}
impl Bucket {
    fn new(tokens: f32, at: Instant) -> Self {
        Self { tokens, at }
    }
    fn take(&mut self, now: Instant, rate: f32, burst: f32) -> bool {
        let refill = now.saturating_duration_since(self.at).as_secs_f32() * rate;
        self.tokens = (self.tokens + refill).min(burst);
        self.at = now;
        (self.tokens >= 1.).then(|| self.tokens -= 1.).is_some()
    }
}

pub(crate) struct Transport {
    socket: UdpSocket,
    netcode: NetcodeServer,
    buffer: Box<[u8; NETCODE_MAX_PACKET_BYTES]>,
    /// The encoded status body; empty until the server publishes one.
    pub(crate) status: Vec<u8>,
    global: Bucket,
    sources: HashMap<SocketAddr, Bucket>,
    pub(crate) status_replies: u64,
}

impl Transport {
    pub fn new(config: ServerConfig, socket: UdpSocket) -> io::Result<Self> {
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            netcode: NetcodeServer::new(config),
            buffer: Box::new([0; NETCODE_MAX_PACKET_BYTES]),
            status: Vec::new(),
            global: Bucket::new(GLOBAL_RATE, Instant::now()),
            sources: HashMap::new(),
            status_replies: 0,
        })
    }

    pub fn client_addr(&self, client: u64) -> Option<SocketAddr> {
        self.netcode.client_addr(client)
    }

    pub fn disconnect_all(&mut self, server: &mut RenetServer) {
        for client in self.netcode.clients_id() {
            let result = self.netcode.disconnect(client);
            handle(result, &self.socket, server);
        }
    }

    fn answer_status(&mut self, nonce: u64, from: SocketAddr) {
        if self.status.is_empty() {
            return;
        }
        let now = Instant::now();
        if self.sources.len() >= MAX_SOURCES && !self.sources.contains_key(&from) {
            self.sources
                .retain(|_, b| now.duration_since(b.at) < Duration::from_secs(4));
            if self.sources.len() >= MAX_SOURCES {
                return;
            }
        }
        let source = self
            .sources
            .entry(from)
            .or_insert_with(|| Bucket::new(SOURCE_BURST, now));
        if !source.take(now, SOURCE_RATE, SOURCE_BURST)
            || !self.global.take(now, GLOBAL_RATE, GLOBAL_RATE)
        {
            return;
        }
        let reply = tankyard_protocol::status_reply(nonce, &self.status);
        if self.socket.send_to(&reply, from).is_ok() {
            self.status_replies += 1;
        }
    }

    pub fn update(
        &mut self,
        duration: Duration,
        server: &mut RenetServer,
    ) -> Result<(), NetcodeTransportError> {
        self.netcode.update(duration);
        loop {
            match self.socket.recv_from(&mut self.buffer[..]) {
                Ok((len, from)) => {
                    if let Some(nonce) =
                        tankyard_protocol::parse_status_request(&self.buffer[..len])
                    {
                        self.answer_status(nonce, from);
                        continue;
                    }
                    let result = self.netcode.process_packet(from, &mut self.buffer[..len]);
                    handle(result, &self.socket, server);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => break,
                Err(e) if e.kind() == io::ErrorKind::ConnectionReset => continue,
                Err(e) => return Err(e.into()),
            }
        }
        for client in self.netcode.clients_id() {
            let result = self.netcode.update_client(client);
            handle(result, &self.socket, server);
        }
        for client in server.disconnections_id() {
            let result = self.netcode.disconnect(client);
            handle(result, &self.socket, server);
        }
        Ok(())
    }

    pub fn send_packets(&mut self, server: &mut RenetServer) {
        'clients: for client in server.clients_id() {
            let Ok(packets) = server.get_packets_to_send(client) else {
                continue;
            };
            for packet in packets {
                match self.netcode.generate_payload_packet(client, &packet) {
                    Ok((addr, payload)) => {
                        if let Err(e) = self.socket.send_to(payload, addr) {
                            log::error!("Failed to send packet to client {client} ({addr}): {e}");
                            continue 'clients;
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to encrypt payload packet for client {client}: {e}");
                        continue 'clients;
                    }
                }
            }
        }
    }
}

fn handle(result: ServerResult, socket: &UdpSocket, server: &mut RenetServer) {
    let send = |packet: &[u8], addr: SocketAddr| {
        if let Err(e) = socket.send_to(packet, addr) {
            log::error!("Failed to send packet to {addr}: {e}");
        }
    };
    match result {
        ServerResult::None => {}
        ServerResult::PacketToSend { payload, addr } => send(payload, addr),
        ServerResult::Payload { client_id, payload } => {
            if let Err(e) = server.process_packet_from(payload, client_id) {
                log::error!("Error while processing payload for {client_id}: {e}");
            }
        }
        ServerResult::ClientConnected {
            client_id,
            addr,
            payload,
            ..
        } => {
            server.add_connection(client_id);
            send(payload, addr);
        }
        ServerResult::ClientDisconnected {
            client_id,
            addr,
            payload,
        } => {
            server.remove_connection(client_id);
            if let Some(payload) = payload {
                send(payload, addr);
            }
        }
    }
}
