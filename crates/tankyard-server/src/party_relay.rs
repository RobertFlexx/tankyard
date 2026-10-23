//! Outbound-only party transport. The leader still simulates the match; this
//! service forwards bounded netcode datagrams, never administration requests.
//! Host ownership and invite entry use separate challenge-bound signing keys.
use crate::{
    NetError,
    websocket::{self, DatagramBudget, MAX_DATAGRAM},
};
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

pub const RELAY: &str = "wss://tankyard.kokonico.me";
const MAX_ROOMS: usize = 32;
const MAX_GUESTS: usize = 8;
const TIMEOUT: Duration = Duration::from_secs(5);
type Error = tungstenite::Error;
type Queue = SyncSender<Vec<u8>>;

fn invalid(message: &'static str) -> Error {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
pub(crate) fn hex(key: &[u8; 32]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub(crate) fn unhex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut key = [0; 32];
    for (out, pair) in key.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
        *out = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    (key != [0; 32]).then_some(key)
}
pub(crate) fn route(path: &str) -> Option<(bool, [u8; 32])> {
    let (hosting, key) = path
        .strip_prefix("/party/host/")
        .map(|key| (true, key))
        .or_else(|| path.strip_prefix("/party/join/").map(|key| (false, key)))?;
    Some((hosting, unhex(key)?))
}
fn signed(role: u8, room: &[u8; 32], nonce: &[u8], entry: &[u8]) -> Vec<u8> {
    let mut bytes = b"TankYard party relay v1\0".to_vec();
    bytes.push(role);
    bytes.extend_from_slice(room);
    bytes.extend_from_slice(nonce);
    bytes.extend_from_slice(entry);
    bytes
}
fn write<S: Read + Write>(ws: &mut WebSocket<S>, message: Message) -> Result<(), Error> {
    let until = Instant::now() + TIMEOUT;
    let mut pending = Some(message);
    loop {
        if let Some(message) = pending.take() {
            match ws.write(message) {
                Ok(()) => {}
                Err(error) if websocket::would_block(&error) => {}
                Err(Error::WriteBufferFull(message)) => pending = Some(*message),
                Err(error) => return Err(error),
            }
        }
        match ws.flush() {
            Ok(()) if pending.is_none() => return Ok(()),
            Ok(()) => {}
            Err(error) if websocket::would_block(&error) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= until {
            return Err(invalid("Party relay handshake timed out"));
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn binary<S: Read + Write>(ws: &mut WebSocket<S>) -> Result<Vec<u8>, Error> {
    let until = Instant::now() + TIMEOUT;
    let mut controls = 0;
    loop {
        match ws.read() {
            Ok(Message::Binary(data)) => return Ok(data.to_vec()),
            Ok(Message::Ping(_) | Message::Pong(_)) if controls < 8 => {
                controls += 1;
                let _ = ws.flush();
            }
            Ok(_) => return Err(invalid("Invalid party relay handshake")),
            Err(error) if websocket::would_block(&error) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= until {
            return Err(invalid("Party relay handshake timed out"));
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn challenge<S: Read + Write>(ws: &mut WebSocket<S>, role: u8) -> Result<[u8; 32], Error> {
    let nonce = renet_netcode::generate_random_bytes::<32>();
    let mut message = vec![role];
    message.extend_from_slice(&nonce);
    write(ws, Message::binary(message))?;
    Ok(nonce)
}
fn answer<S: Read + Write>(
    ws: &mut WebSocket<S>,
    key: &Ed25519KeyPair,
    room: &[u8; 32],
    role: u8,
    entry: &[u8],
) -> Result<(), Error> {
    let challenge = binary(ws)?;
    if challenge.len() != 33 || challenge[0] != role {
        return Err(invalid("Server does not support party relaying"));
    }
    let proof = key.sign(&signed(role, room, &challenge[1..], entry));
    let mut message = entry.to_vec();
    message.extend_from_slice(proof.as_ref());
    write(ws, Message::binary(message))?;
    if binary(ws)? != b"R" {
        return Err(invalid("Party relay refused this session"));
    }
    Ok(())
}
pub(crate) fn authenticate_guest<S: Read + Write>(
    ws: &mut WebSocket<S>,
    key: &Ed25519KeyPair,
    room: &[u8; 32],
) -> Result<(), Error> {
    answer(ws, key, room, b'J', &[])
}

struct Room {
    entry: [u8; 32],
    incoming: Queue,
    guests: HashMap<u64, Queue>,
}
impl Room {
    // These queues carry UDP datagrams, not reliable application messages.
    // A full queue drops a packet; netcode retransmits reliable channels and
    // replaces obsolete snapshots. Congestion must not revoke membership.
    fn forward(&mut self, source: Option<u64>, data: &[u8]) -> Result<(), Error> {
        if let Some(id) = source {
            if data.is_empty() || data.len() > MAX_DATAGRAM {
                return Err(invalid("Invalid guest datagram"));
            }
            let mut frame = id.to_le_bytes().to_vec();
            frame.extend_from_slice(data);
            match self.incoming.try_send(frame) {
                Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(invalid("Party leader disconnected"));
                }
            }
        } else {
            if data.len() <= 8 || data.len() > MAX_DATAGRAM + 8 {
                return Err(invalid("Invalid host datagram"));
            }
            let id = u64::from_le_bytes(data[..8].try_into().expect("checked length"));
            if let Some(guest) = self.guests.get(&id)
                && matches!(
                    guest.try_send(data[8..].to_vec()),
                    Err(mpsc::TrySendError::Disconnected(_))
                )
            {
                self.guests.remove(&id);
                let _ = self.incoming.try_send(id.to_le_bytes().to_vec());
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct Hub {
    rooms: Mutex<HashMap<[u8; 32], Room>>,
    next: AtomicU64,
}
impl Hub {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<[u8; 32], Room>>, Error> {
        self.rooms
            .lock()
            .map_err(|_| invalid("Party relay unavailable"))
    }
}
struct Membership {
    hub: Arc<Hub>,
    room: [u8; 32],
    guest: Option<u64>,
}
impl Drop for Membership {
    fn drop(&mut self) {
        if let Ok(mut rooms) = self.hub.rooms.lock() {
            if let Some(id) = self.guest {
                if let Some(room) = rooms.get_mut(&self.room) {
                    room.guests.remove(&id);
                    let _ = room.incoming.try_send(id.to_le_bytes().to_vec());
                }
            } else {
                rooms.remove(&self.room);
            }
        }
    }
}

/// A bounded duplex pump. Queues apply backpressure to each participant, with
/// independent rate budgets and one retained frame under socket backpressure.
fn exchange<S: Read + Write>(
    ws: &mut WebSocket<S>,
    stop: &AtomicBool,
    participants: usize,
    mut next: impl FnMut() -> Result<Option<Vec<u8>>, Error>,
    mut receive: impl FnMut(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut pending = None;
    let mut outgoing = DatagramBudget::shared(participants);
    let mut incoming = DatagramBudget::shared(participants);
    let mut last_read = Instant::now();
    let mut last_ping = last_read;
    while !stop.load(Ordering::Relaxed) {
        let mut active = false;
        for _ in 0..32 {
            let message = if let Some(message) = pending.take() {
                message
            } else if let Some(bytes) = next()? {
                outgoing.take(Instant::now())?;
                if bytes.is_empty() || bytes.len() > MAX_DATAGRAM + 8 {
                    return Err(invalid("Invalid party relay datagram"));
                }
                Message::binary(bytes)
            } else {
                break;
            };
            active = true;
            match ws.write(message) {
                Ok(()) => {}
                Err(error) if websocket::would_block(&error) => break,
                Err(Error::WriteBufferFull(message)) => {
                    pending = Some(*message);
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        if last_ping.elapsed() >= Duration::from_secs(5) && pending.is_none() {
            match ws.write(Message::Ping(Vec::new().into())) {
                Ok(()) => {}
                Err(error) if websocket::would_block(&error) => {}
                Err(Error::WriteBufferFull(message)) => pending = Some(*message),
                Err(error) => return Err(error),
            }
            last_ping = Instant::now();
        }
        match ws.flush() {
            Ok(()) => {}
            Err(error) if websocket::would_block(&error) => {}
            Err(error) => return Err(error),
        }
        for _ in 0..32 {
            match ws.read() {
                Ok(Message::Binary(data)) => {
                    incoming.take(Instant::now())?;
                    last_read = Instant::now();
                    active = true;
                    receive(&data)?;
                }
                Ok(Message::Ping(_) | Message::Pong(_)) => {
                    incoming.take(Instant::now())?;
                    last_read = Instant::now();
                }
                Ok(Message::Close(_)) => return Ok(()),
                Ok(_) => return Err(invalid("Party relay accepts binary packets only")),
                Err(error) if websocket::would_block(&error) => break,
                Err(error) => return Err(error),
            }
        }
        if last_read.elapsed() > Duration::from_secs(20) {
            return Err(invalid("Party relay connection timed out"));
        }
        if !active {
            thread::sleep(Duration::from_millis(2));
        }
    }
    let _ = ws.close(None);
    Ok(())
}
fn queued(receiver: &Receiver<Vec<u8>>) -> Result<Option<Vec<u8>>, Error> {
    match receiver.try_recv() {
        Ok(data) => Ok(Some(data)),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => Err(invalid("Party leader or relay disconnected")),
    }
}
pub(crate) fn serve<S: Read + Write>(
    ws: &mut WebSocket<S>,
    hub: &Arc<Hub>,
    hosting: bool,
    key: [u8; 32],
    stop: &AtomicBool,
) -> Result<(), Error> {
    let (sender, receiver) = mpsc::sync_channel(if hosting { 256 } else { 32 });
    let member = if hosting {
        let nonce = challenge(ws, b'H')?;
        let proof = binary(ws)?;
        if proof.len() != 96
            || signature::UnparsedPublicKey::new(&signature::ED25519, key)
                .verify(&signed(b'H', &key, &nonce, &proof[..32]), &proof[32..])
                .is_err()
        {
            return Err(invalid("Invalid party host proof"));
        }
        let mut entry = [0; 32];
        entry.copy_from_slice(&proof[..32]);
        let mut rooms = hub.lock()?;
        if rooms.len() >= MAX_ROOMS || rooms.contains_key(&key) || entry == [0; 32] {
            return Err(invalid("Party relay is full or room already exists"));
        }
        rooms.insert(
            key,
            Room {
                entry,
                incoming: sender,
                guests: HashMap::new(),
            },
        );
        Membership {
            hub: hub.clone(),
            room: key,
            guest: None,
        }
    } else {
        let entry = hub
            .lock()?
            .get(&key)
            .ok_or_else(|| invalid("Party leader is offline"))?
            .entry;
        let nonce = challenge(ws, b'J')?;
        let proof = binary(ws)?;
        if proof.len() != 64
            || signature::UnparsedPublicKey::new(&signature::ED25519, entry)
                .verify(&signed(b'J', &key, &nonce, &[]), &proof)
                .is_err()
        {
            return Err(invalid("Invalid party invite proof"));
        }
        let mut rooms = hub.lock()?;
        let room = rooms
            .get_mut(&key)
            .ok_or_else(|| invalid("Party leader is offline"))?;
        if room.guests.len() >= MAX_GUESTS || room.entry != entry {
            return Err(invalid("Party relay is full"));
        }
        let id = hub
            .next
            .fetch_add(1, Ordering::Relaxed)
            .checked_add(1)
            .ok_or_else(|| invalid("Party relay session limit"))?;
        room.guests.insert(id, sender);
        Membership {
            hub: hub.clone(),
            room: key,
            guest: Some(id),
        }
    };
    write(ws, Message::binary(b"R".to_vec()))?;
    let mut roster_sent = Instant::now();
    exchange(
        ws,
        stop,
        if hosting { MAX_GUESTS } else { 1 },
        || {
            // An authoritative roster also retires sockets if a full data
            // queue could not retain a disconnect notification.
            if hosting && roster_sent.elapsed() >= Duration::from_secs(1) {
                let rooms = hub.lock()?;
                let room = rooms
                    .get(&key)
                    .ok_or_else(|| invalid("Party leader disconnected"))?;
                let mut roster = 0u64.to_le_bytes().to_vec();
                for id in room.guests.keys() {
                    roster.extend_from_slice(&id.to_le_bytes());
                }
                roster_sent = Instant::now();
                return Ok(Some(roster));
            }
            queued(&receiver)
        },
        |data| {
            let mut rooms = hub.lock()?;
            let room = rooms
                .get_mut(&key)
                .ok_or_else(|| invalid("Party leader disconnected"))?;
            room.forward(member.guest, data)
        },
    )
}

#[derive(Clone, Debug)]
pub enum Status {
    Ready,
    Unavailable,
}
pub struct HostedRelay {
    pub room: [u8; 32],
    updates: Receiver<Status>,
    stop: Arc<AtomicBool>,
}
impl HostedRelay {
    pub fn start(server: SocketAddr, entry: [u8; 32]) -> Result<Self, NetError> {
        Self::start_at(RELAY, server, entry)
    }
    fn start_at(base: &str, server: SocketAddr, entry: [u8; 32]) -> Result<Self, NetError> {
        if !server.ip().is_loopback() || server.port() == 0 {
            return Err(NetError(
                "Party transport requires the local hosted server.".into(),
            ));
        }
        let key =
            Ed25519KeyPair::from_seed_unchecked(&renet_netcode::generate_random_bytes::<32>())
                .expect("valid seed");
        let mut room = [0; 32];
        room.copy_from_slice(key.public_key().as_ref());
        let url = format!("{base}/party/host/{}", hex(&room));
        let (updates, receiver) = mpsc::sync_channel(4);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        thread::Builder::new()
            .name("party-host-relay".into())
            .spawn(move || {
                let mut delay = 1u32;
                while !flag.load(Ordering::Relaxed) {
                    let result = websocket::open(&url).and_then(|mut ws| {
                        answer(&mut ws, &key, &room, b'H', &entry)
                            .map_err(|_| NetError("Party relay registration unavailable".into()))?;
                        if flag.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        let _ = updates.try_send(Status::Ready);
                        delay = 1;
                        tunnel(&mut ws, server, &flag)
                            .map_err(|_| NetError("Party relay connection ended".into()))
                    });
                    let _ = result;
                    let _ = updates.try_send(Status::Unavailable);
                    for _ in 0..delay * 10 {
                        if flag.load(Ordering::Relaxed) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                    delay = (delay * 2).min(30);
                }
            })?;
        Ok(Self {
            room,
            updates: receiver,
            stop,
        })
    }
    pub fn poll(&self) -> Option<Status> {
        self.updates.try_iter().last()
    }
}
impl Drop for HostedRelay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn tunnel<S: Read + Write>(
    ws: &mut WebSocket<S>,
    server: SocketAddr,
    stop: &AtomicBool,
) -> Result<(), Error> {
    // Connected UDP sockets constrain every tunnel destination to this local
    // match. No remote request can turn the relay into a general UDP proxy.
    struct Peer {
        id: u64,
        socket: UdpSocket,
        seen: Instant,
    }
    let peers = std::cell::RefCell::new(Vec::<Peer>::new());
    let mut cursor = 0usize;
    exchange(
        ws,
        stop,
        MAX_GUESTS,
        || {
            let mut peers = peers.borrow_mut();
            peers.retain(|peer| peer.seen.elapsed() < Duration::from_secs(20));
            let mut buffer = [0; MAX_DATAGRAM + 1];
            for _ in 0..peers.len() {
                cursor = (cursor + 1) % peers.len();
                let peer = &mut peers[cursor];
                match peer.socket.recv(&mut buffer) {
                    Ok(length) if length > 0 && length <= MAX_DATAGRAM => {
                        peer.seen = Instant::now();
                        let mut frame = peer.id.to_le_bytes().to_vec();
                        frame.extend_from_slice(&buffer[..length]);
                        return Ok(Some(frame));
                    }
                    Ok(_) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::ConnectionRefused
                        ) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(None)
        },
        |frame| {
            if frame.len() < 8 || frame.len() > MAX_DATAGRAM + 8 {
                return Err(invalid("Invalid multiplexed datagram"));
            }
            let id = u64::from_le_bytes(frame[..8].try_into().expect("checked length"));
            let mut peers = peers.borrow_mut();
            if id == 0 {
                if !frame.len().is_multiple_of(8) || frame.len() > 8 * (MAX_GUESTS + 1) {
                    return Err(invalid("Invalid party transport roster"));
                }
                peers.retain(|peer| {
                    frame[8..]
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .any(|bytes| bytes == &peer.id.to_le_bytes())
                });
                return Ok(());
            }
            if frame.len() == 8 {
                peers.retain(|peer| peer.id != id);
                return Ok(());
            }
            let index = if let Some(index) = peers.iter().position(|peer| peer.id == id) {
                index
            } else {
                if peers.len() >= MAX_GUESTS {
                    // A previous guest's close frame may be behind congested
                    // data. Drop this UDP packet until the next roster frees
                    // the stale slot; never evict the other live guests.
                    return Ok(());
                }
                let socket = UdpSocket::bind(if server.is_ipv4() {
                    "127.0.0.1:0"
                } else {
                    "[::1]:0"
                })?;
                socket.connect(server)?;
                socket.set_nonblocking(true)?;
                crate::sockets::tune_udp(&socket, 64 * 1024);
                peers.push(Peer {
                    id,
                    socket,
                    seen: Instant::now(),
                });
                peers.len() - 1
            };
            peers[index].seen = Instant::now();
            match peers[index].socket.send(&frame[8..]) {
                Ok(_) => Ok(()),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    Ok(())
                }
                Err(error) => Err(error.into()),
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NetworkClient, ServerSettings, UdpServer, party::Invite};
    use std::net::TcpListener;

    struct RelayFixture {
        base: String,
        hub: Arc<Hub>,
        stop: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl RelayFixture {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let base = format!("ws://{}", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let hub = Arc::new(Hub::default());
            let rooms = hub.clone();
            let worker = thread::spawn(move || {
                let mut workers = Vec::new();
                while !flag.load(Ordering::Relaxed) {
                    if let Ok((socket, _)) = listener.accept() {
                        let rooms = rooms.clone();
                        let flag = flag.clone();
                        workers.push(thread::spawn(move || {
                            websocket::relay_connection_with_parties(
                                socket,
                                "127.0.0.1:9".parse().unwrap(),
                                &flag,
                                &rooms,
                            )
                        }));
                    } else {
                        thread::sleep(Duration::from_millis(2));
                    }
                }
                for worker in workers {
                    worker.join().unwrap();
                }
            });
            Self {
                base,
                hub,
                stop,
                worker: Some(worker),
            }
        }
    }
    impl Drop for RelayFixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            self.worker.take().unwrap().join().unwrap();
        }
    }
    fn wait_ready(relay: &HostedRelay) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(relay.poll(), Some(Status::Ready)) {
                return;
            }
            assert!(Instant::now() < deadline, "party host did not register");
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn pump(server: &mut UdpServer, clients: &mut [&mut NetworkClient], ticks: usize) {
        for _ in 0..ticks {
            for client in clients.iter_mut() {
                client.update(Duration::from_millis(17)).unwrap();
            }
            server.update(Duration::from_millis(17)).unwrap();
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn outbound_tunnel_joins_without_a_reachable_udp_address_and_retains_round_authority() {
        let fixture = RelayFixture::start();
        let mut invite = Invite::create("203.0.113.8:5000".parse().unwrap());
        let mut server = UdpServer::bind(ServerSettings {
            bind: "127.0.0.1:0".into(),
            party: Some(invite.settings(3)),
            max_players: 8,
            creator_admin: Some(false),
            ban_file: String::new(),
            bot_count: 0,
            adaptive_bots: false,
            world: tankyard_core::WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        let relay =
            HostedRelay::start_at(&fixture.base, server.address, invite.settings(3).public_key)
                .unwrap();
        wait_ready(&relay);
        invite.relay = Some(relay.room);
        let mut guest = NetworkClient::party_via(&invite, "Guest", &fixture.base).unwrap();
        let mut second = NetworkClient::party_via(&invite, "Second", &fixture.base).unwrap();
        for _ in 0..300 {
            pump(&mut server, &mut [&mut guest, &mut second], 1);
            if guest.is_ready() && second.is_ready() && guest.party_members.len() == 2 {
                break;
            }
        }
        assert!(guest.is_ready() && second.is_ready());
        assert_eq!(server.player_count(), 2);
        assert_eq!(guest.role, tankyard_protocol::ServerRole::Player);
        assert!(!guest.admin(tankyard_protocol::ServerCommand::RestartRound));
        assert!(guest.send_chat("lobby", "Relay chat works"));
        assert!(guest.party_presence(true));
        pump(&mut server, &mut [&mut guest, &mut second], 70);
        assert!(
            second
                .chat_messages
                .iter()
                .any(|message| message.text == "Relay chat works")
        );
        assert!(
            second
                .party_members
                .iter()
                .any(|member| member.client == guest.client_id() && member.in_menu)
        );
        let round = guest.player_round.unwrap();
        assert!(
            !server
                .admin_command("restart")
                .message
                .starts_with("Error:")
        );
        pump(&mut server, &mut [&mut guest, &mut second], 80);
        assert_eq!(guest.player_round, Some(round + 1));
        assert_eq!(second.player_round, Some(round + 1));
        guest.disconnect();
        second.disconnect();
        drop(guest);
        drop(second);
        drop(relay);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !fixture.hub.lock().unwrap().is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(fixture.hub.lock().unwrap().is_empty());
    }
    #[test]
    fn relay_requires_invite_proof_and_cannot_be_claimed_with_the_invite_key() {
        let fixture = RelayFixture::start();
        let entry =
            Ed25519KeyPair::from_seed_unchecked(&renet_netcode::generate_random_bytes::<32>())
                .unwrap();
        let mut entry_public = [0; 32];
        entry_public.copy_from_slice(entry.public_key().as_ref());
        let relay = HostedRelay::start_at(
            &fixture.base,
            "127.0.0.1:5000".parse().unwrap(),
            entry_public,
        )
        .unwrap();
        wait_ready(&relay);
        let join = format!("{}/party/join/{}", fixture.base, hex(&relay.room));
        let attacker = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let mut ws = websocket::open(&join).unwrap();
        assert!(authenticate_guest(&mut ws, &attacker, &relay.room).is_err());
        assert!(
            fixture
                .hub
                .lock()
                .unwrap()
                .get(&relay.room)
                .unwrap()
                .guests
                .is_empty()
        );
        // A leaked entry invitation does not authorize replacing its host.
        let host = format!("{}/party/host/{}", fixture.base, hex(&relay.room));
        let mut ws = websocket::open(&host).unwrap();
        assert!(answer(&mut ws, &entry, &relay.room, b'H', &entry_public).is_err());
        assert_eq!(fixture.hub.lock().unwrap().len(), 1);
        assert!(route("/party/host/garbage").is_none());
        assert!(route(&format!("/party/host/{}?extra", hex(&relay.room))).is_none());
    }
    #[test]
    fn proofs_are_bound_to_nonce_room_role_and_entry_key() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[5; 32]).unwrap();
        let room = [1; 32];
        let entry = [2; 32];
        let nonce = [3; 32];
        let proof = key.sign(&signed(b'H', &room, &nonce, &entry));
        let verify = |role, room, nonce, entry| {
            signature::UnparsedPublicKey::new(&signature::ED25519, key.public_key())
                .verify(&signed(role, room, nonce, entry), proof.as_ref())
                .is_ok()
        };
        assert!(verify(b'H', &room, &nonce, &entry));
        assert!(!verify(b'J', &room, &nonce, &entry));
        assert!(!verify(b'H', &[4; 32], &nonce, &entry));
        assert!(!verify(b'H', &room, &[4; 32], &entry));
        assert!(!verify(b'H', &room, &nonce, &[4; 32]));
    }
    #[test]
    fn room_limits_and_departures_are_bounded_and_isolated() {
        let hub = Arc::new(Hub::default());
        let (sender, receiver) = mpsc::sync_channel(1);
        let (guest, guest_rx) = mpsc::sync_channel(1);
        hub.lock().unwrap().insert(
            [1; 32],
            Room {
                entry: [2; 32],
                incoming: sender,
                guests: HashMap::from([(1, guest)]),
            },
        );
        let guest = Membership {
            hub: hub.clone(),
            room: [1; 32],
            guest: Some(1),
        };
        drop(guest);
        assert!(hub.lock().unwrap().get(&[1; 32]).unwrap().guests.is_empty());
        assert_eq!(receiver.try_recv().unwrap(), 1u64.to_le_bytes());
        assert!(matches!(
            guest_rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        drop(Membership {
            hub: hub.clone(),
            room: [1; 32],
            guest: None,
        });
        assert!(hub.lock().unwrap().is_empty());
    }

    #[test]
    fn a_stalled_party_socket_bounds_queued_frames_and_resumes_in_order() {
        use std::sync::atomic::AtomicUsize;
        use tungstenite::protocol::{Role, WebSocketConfig};
        struct Stalled {
            writable: Arc<AtomicBool>,
            blocked: Arc<AtomicUsize>,
            bytes: Arc<Mutex<Vec<u8>>>,
        }
        impl Read for Stalled {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::ErrorKind::WouldBlock.into())
            }
        }
        impl Write for Stalled {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.writable.load(Ordering::Relaxed) {
                    self.blocked.fetch_add(1, Ordering::Relaxed);
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                self.bytes.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let writable = Arc::new(AtomicBool::new(false));
        let blocked = Arc::new(AtomicUsize::new(0));
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let produced = Arc::new(AtomicUsize::new(0));
        let socket = Stalled {
            writable: writable.clone(),
            blocked: blocked.clone(),
            bytes: bytes.clone(),
        };
        let mut ws = WebSocket::from_raw_socket(
            socket,
            Role::Server,
            Some(
                WebSocketConfig::default()
                    .write_buffer_size(0)
                    .max_write_buffer_size(64 * 1024),
            ),
        );
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let count = produced.clone();
        let worker = thread::spawn(move || {
            exchange(
                &mut ws,
                &flag,
                8,
                || {
                    let index = count.load(Ordering::Relaxed);
                    if index >= 96 {
                        return Ok(None);
                    }
                    count.fetch_add(1, Ordering::Relaxed);
                    Ok(Some(vec![index as u8; 1024]))
                },
                |_| Ok(()),
            )
        });
        let until = Instant::now() + Duration::from_secs(5);
        while blocked.load(Ordering::Relaxed) < 150 && Instant::now() < until {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(blocked.load(Ordering::Relaxed) >= 150);
        assert!(
            produced.load(Ordering::Relaxed) <= 65,
            "backpressure must stop draining the bounded source"
        );
        writable.store(true, Ordering::Relaxed);
        while bytes.lock().unwrap().len() < 96 * 1028 && Instant::now() < until {
            thread::sleep(Duration::from_millis(1));
        }
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap();
        let mut received = WebSocket::from_raw_socket(
            io::Cursor::new(bytes.lock().unwrap().clone()),
            Role::Client,
            None,
        );
        for index in 0..96 {
            assert_eq!(received.read().unwrap(), Message::binary(vec![index; 1024]));
        }
    }

    #[test]
    fn a_full_guest_queue_preserves_membership_and_recovers_after_a_burst() {
        let (incoming, _) = mpsc::sync_channel(1);
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut room = Room {
            entry: [1; 32],
            incoming,
            guests: HashMap::from([(7, sender)]),
        };
        let mut packet = 7u64.to_le_bytes().to_vec();
        packet.extend_from_slice(b"snapshot");
        room.forward(None, &packet).unwrap();
        room.forward(None, &packet).unwrap();
        assert!(
            room.guests.contains_key(&7),
            "a full queue is packet loss, not a departed member"
        );
        assert_eq!(receiver.try_recv().unwrap(), b"snapshot");
        room.forward(None, &packet).unwrap();
        assert_eq!(receiver.try_recv().unwrap(), b"snapshot");
    }
    #[test]
    fn a_full_host_queue_does_not_kick_a_guest() {
        let (incoming, receiver) = mpsc::sync_channel(1);
        let mut room = Room {
            entry: [1; 32],
            incoming,
            guests: HashMap::new(),
        };
        room.forward(Some(7), b"input").unwrap();
        assert!(
            room.forward(Some(7), b"input").is_ok(),
            "brief congestion must not terminate a session"
        );
        assert!(receiver.try_recv().is_ok());
        room.forward(Some(7), b"input").unwrap();
        assert!(receiver.try_recv().is_ok());
    }

    #[test]
    fn eight_members_survive_heartbeats_combat_voice_and_a_short_pause() {
        use tankyard_core::{Mode, PlayerInput, V2, WorldConfig};
        use tankyard_protocol::{MAX_VOICE_PAYLOAD, VoiceFrame};
        let fixture = RelayFixture::start();
        let mut invite = Invite::create("203.0.113.8:5000".parse().unwrap());
        let mut server = UdpServer::bind(ServerSettings {
            bind: "127.0.0.1:0".into(),
            max_players: 8,
            party: Some(invite.settings(4)),
            creator_admin: Some(false),
            ban_file: String::new(),
            bot_count: 0,
            adaptive_bots: false,
            world: WorldConfig {
                mode: Mode::TwoTeams,
                dynamic_arena: false,
                target_count: 180,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        let mut local = invite.clone();
        local.address = server.address;
        let relay =
            HostedRelay::start_at(&fixture.base, server.address, invite.settings(4).public_key)
                .unwrap();
        wait_ready(&relay);
        invite.relay = Some(relay.room);
        let mut clients = vec![NetworkClient::connect(&local.link(), "Leader", "").unwrap()];
        for index in 1..8 {
            clients.push(
                NetworkClient::party_via(&invite, &format!("Guest {index}"), &fixture.base)
                    .unwrap(),
            );
        }
        let started = Instant::now();
        let mut last = started;
        let mut sent = started;
        let mut sequence = 0;
        let mut client_at = vec![started; clients.len()];
        let mut positioned = false;
        let mut paused = false;
        let mut received_voice = false;
        let mut last_voice = vec![None; clients.len()];
        while started.elapsed() < Duration::from_secs(35) {
            let now = Instant::now();
            let elapsed = now.duration_since(last);
            last = now;
            let transmit = sent.elapsed() >= Duration::from_millis(20);
            if transmit {
                sequence += 1;
                sent = now;
            }
            for (index, client) in clients.iter_mut().enumerate() {
                // A short render/update pause must not forfeit a connected guest.
                if index == 7 && started.elapsed().as_secs() == 12 && !paused {
                    continue;
                }
                let client_elapsed = now.duration_since(client_at[index]);
                client_at[index] = now;
                client.update(client_elapsed).unwrap_or_else(|error| {
                    panic!("client {index} at {:?}: {error}", started.elapsed())
                });
                if !client.voice_packets.is_empty() {
                    received_voice = true;
                    last_voice[index] = Some(Instant::now());
                }
                client.voice_packets.clear();
                if transmit && client.is_ready() {
                    let phase = started.elapsed().as_secs_f32() * 2.;
                    client.send_input(PlayerInput {
                        sequence,
                        movement: V2::new(phase.cos(), phase.sin()) * 0.15,
                        fire: true,
                        aim: 0.,
                        ..Default::default()
                    });
                    assert!(client.send_voice(VoiceFrame {
                        round: client.player_round.unwrap(),
                        sequence,
                        samples: vec![0; MAX_VOICE_PAYLOAD]
                    }));
                }
            }
            if started.elapsed().as_secs() >= 13 {
                paused = true;
            }
            server.update(elapsed).unwrap();
            if !positioned && clients.iter().all(|client| client.is_ready()) {
                for (index, client) in clients.iter().enumerate() {
                    let tank = server
                        .session
                        .world
                        .tanks
                        .iter_mut()
                        .find(|tank| Some(tank.id) == client.player_id)
                        .unwrap();
                    tank.pos = V2::new(120. + index as f32 * 30., 1000. + index as f32 * 45.);
                }
                positioned = true;
            }
            thread::sleep(Duration::from_millis(3));
        }
        assert!(positioned && paused && received_voice);
        assert!(
            last_voice
                .iter()
                .all(|time| time.is_some_and(|time| time.elapsed() < Duration::from_secs(2))),
            "every member must still receive voice after the pause and all heartbeats"
        );
        assert_eq!(server.player_count(), 8);
        assert!(
            clients
                .iter()
                .all(|client| client.is_ready() && client.party_members.len() == 8)
        );
    }

    #[test]
    fn an_offline_party_sends_a_closing_reason_instead_of_a_reset() {
        let fixture = RelayFixture::start();
        let mut ws =
            websocket::open(&format!("{}/party/join/{}", fixture.base, hex(&[7; 32]))).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match ws.read() {
                Ok(Message::Close(Some(frame))) => {
                    assert_eq!(
                        frame.code,
                        tungstenite::protocol::frame::coding::CloseCode::Away
                    );
                    assert!(frame.reason.contains("leader disconnected"));
                    let _ = ws.flush();
                    break;
                }
                Err(error) if websocket::would_block(&error) && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(2))
                }
                other => panic!("expected a closing handshake, received {other:?}"),
            }
        }
    }
    #[test]
    fn a_closed_guest_receiver_is_removed_without_affecting_other_members() {
        let (incoming, _) = mpsc::sync_channel(1);
        let (stale, stale_rx) = mpsc::sync_channel(1);
        drop(stale_rx);
        let (healthy, healthy_rx) = mpsc::sync_channel(1);
        let mut room = Room {
            entry: [1; 32],
            incoming,
            guests: HashMap::from([(7, stale), (8, healthy)]),
        };
        let mut packet = 7u64.to_le_bytes().to_vec();
        packet.extend_from_slice(b"data");
        room.forward(None, &packet).unwrap();
        assert!(!room.guests.contains_key(&7));
        packet[..8].copy_from_slice(&8u64.to_le_bytes());
        room.forward(None, &packet).unwrap();
        assert_eq!(healthy_rx.try_recv().unwrap(), b"data");
        assert_eq!(room.guests.len(), 1);
    }
}
