//! UDP-over-WebSocket bridging, so players can reach a server published behind
//! an HTTP-only proxy such as a Cloudflare Tunnel hostname. Each netcode
//! datagram travels as one binary WebSocket message.
//!
//! Clients run [`ClientBridge`]: a loopback UDP endpoint that the ordinary
//! netcode transport talks to, forwarding over `ws://` or `wss://`. The server
//! side runs [`run_relay`] (the `tankyard-relay` binary), which gives every
//! WebSocket its own UDP socket towards the game server. The server therefore
//! sees each player as a distinct port on the relay's address.

use crate::NetError;
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{
    Message, WebSocket,
    client::IntoClientRequest,
    handshake::server::{Request, Response},
    protocol::WebSocketConfig,
};

/// Netcode packets stay well under this; anything larger is not ours.
const MAX_DATAGRAM: usize = 2048;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Netcode keepalives flow several times per second, so silence this long
/// means the game or the remote side has gone away.
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const PUMP_BATCH: usize = 32;
const WRITE_BUDGET: usize = 64 * 1024;

/// Returns the WebSocket URL for a join address, or `None` for a plain UDP
/// `host:port`. A bare hostname without a port means `wss://hostname`.
pub fn websocket_url(address: &str) -> Option<String> {
    let address = address.trim();
    if address.starts_with("ws://") || address.starts_with("wss://") {
        Some(address.to_owned())
    } else if !address.is_empty() && !address.contains(':') {
        Some(format!("wss://{address}"))
    } else {
        None
    }
}

fn config() -> WebSocketConfig {
    WebSocketConfig::default()
        .write_buffer_size(16 * 1024)
        .max_write_buffer_size(WRITE_BUDGET)
        .max_message_size(Some(MAX_DATAGRAM))
        .max_frame_size(Some(MAX_DATAGRAM))
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}
impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Self::Plain(tcp) => tcp,
            Self::Tls(tls) => &tls.sock,
        }
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(tcp) => tcp.read(buf),
            Self::Tls(tls) => tls.read(buf),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(tcp) => tcp.write(buf),
            Self::Tls(tls) => tls.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(tcp) => tcp.flush(),
            Self::Tls(tls) => tls.flush(),
        }
    }
}

fn tls_config() -> Result<Arc<rustls::ClientConfig>, NetError> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| NetError(e.to_string()))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(Arc::new(config))
}

fn open(url: &str) -> Result<WebSocket<Stream>, NetError> {
    let request = url
        .into_client_request()
        .map_err(|e| NetError(format!("invalid server URL: {e}")))?;
    let uri = request.uri();
    let tls = uri.scheme_str() == Some("wss");
    let host = uri
        .host()
        .ok_or_else(|| NetError("server URL has no host".into()))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    let mut last_error = None;
    let tcp = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| NetError(format!("could not resolve {host}: {e}")))?
        .find_map(
            |addr| match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(tcp) => Some(tcp),
                Err(e) => {
                    last_error = Some(e);
                    None
                }
            },
        )
        .ok_or_else(|| {
            NetError(match last_error {
                Some(e) => format!("could not reach {host}:{port}: {e}"),
                None => format!("{host} did not resolve"),
            })
        })?;
    tcp.set_nodelay(true)?;
    tcp.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    tcp.set_write_timeout(Some(CONNECT_TIMEOUT))?;
    let stream = if tls {
        let name = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|e| NetError(format!("invalid TLS server name {host}: {e}")))?;
        let connection = rustls::ClientConnection::new(tls_config()?, name)
            .map_err(|e| NetError(e.to_string()))?;
        Stream::Tls(Box::new(rustls::StreamOwned::new(connection, tcp)))
    } else {
        Stream::Plain(tcp)
    };
    let (socket, _) = tungstenite::client::client_with_config(request, stream, Some(config()))
        .map_err(|e| NetError(format!("WebSocket handshake with {host} failed: {e}")))?;
    socket.get_ref().tcp().set_nonblocking(true)?;
    Ok(socket)
}

fn would_block(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(e) if e.kind() == io::ErrorKind::WouldBlock)
}

/// Moves datagrams between `udp` and `ws` until either side closes, `stop`
/// is set, or nothing arrives for [`IDLE_TIMEOUT`]. Both sockets must be
/// non-blocking. With `peer` unset, the first UDP sender becomes the peer.
fn pump<S: Read + Write>(
    ws: &mut WebSocket<S>,
    udp: &UdpSocket,
    mut peer: Option<SocketAddr>,
    stop: &AtomicBool,
) -> Result<(), tungstenite::Error> {
    let mut buffer = [0u8; MAX_DATAGRAM];
    let mut last_activity = Instant::now();
    let mut pending_ws = None;
    let mut pending_udp: Option<tungstenite::Bytes> = None;
    loop {
        // Dropping a client queues its netcode disconnect immediately before
        // dropping this bridge. Give that final UDP batch one bounded pass.
        let finishing = stop.load(Ordering::Relaxed);
        let mut active = false;
        // Bound each direction's work, and stop reading UDP under TCP
        // backpressure. Retain the one frame tungstenite could not queue.
        for _ in 0..PUMP_BATCH {
            let message = if let Some(message) = pending_ws.take() {
                message
            } else {
                match udp.recv_from(&mut buffer) {
                    Ok((len, from)) => {
                        if *peer.get_or_insert(from) != from {
                            continue;
                        }
                        active = true;
                        Message::binary(buffer[..len].to_vec())
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    // Linux reports an earlier send's ICMP unreachable here; the
                    // netcode layer handles the loss.
                    Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => break,
                    Err(e) => return Err(e.into()),
                }
            };
            match ws.write(message) {
                Ok(()) => {}
                // WouldBlock means tungstenite has already queued this frame.
                Err(e) if would_block(&e) => break,
                Err(tungstenite::Error::WriteBufferFull(message)) => {
                    pending_ws = Some(*message);
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(e) if would_block(&e) => {}
            Err(e) => return Err(e),
        }
        for _ in 0..PUMP_BATCH {
            let data = if let Some(data) = pending_udp.take() {
                data
            } else {
                match ws.read() {
                    Ok(Message::Binary(data)) => {
                        active = true;
                        data
                    }
                    Ok(Message::Close(_)) => {
                        let _ = ws.flush();
                        return Ok(());
                    }
                    Ok(_) => {
                        active = true;
                        continue;
                    }
                    Err(e) if would_block(&e) => break,
                    Err(
                        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed,
                    ) => {
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
            };
            if let Some(peer) = peer {
                match udp.send_to(&data, peer) {
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        pending_udp = Some(data);
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        if finishing {
            break;
        }
        if active {
            last_activity = Instant::now();
        } else if last_activity.elapsed() > IDLE_TIMEOUT {
            break;
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }
    let _ = ws.close(None);
    let _ = ws.flush();
    Ok(())
}

/// A loopback UDP endpoint that forwards to a WebSocket relay. Point the
/// netcode client at [`ClientBridge::address`]. Dropping it closes the
/// WebSocket.
pub(crate) struct ClientBridge {
    pub address: SocketAddr,
    stop: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
}
impl ClientBridge {
    pub fn connect(url: &str) -> Result<Self, NetError> {
        let mut ws = open(url)?;
        let udp = UdpSocket::bind("127.0.0.1:0")?;
        crate::sockets::tune_udp(&udp, 64 * 1024);
        udp.set_nonblocking(true)?;
        let address = udp.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let failure = Arc::new(Mutex::new(None));
        let ended = failure.clone();
        let url = url.to_owned();
        thread::Builder::new()
            .name("websocket-bridge".into())
            .spawn(move || {
                let result = pump(&mut ws, &udp, None, &flag);
                if !flag.load(Ordering::Relaxed) {
                    let message = match result {
                        Ok(()) => "Server connection closed. Try reconnecting.".to_owned(),
                        Err(e) => format!("Server connection ended: {e}"),
                    };
                    log::debug!("WebSocket connection to {url}: {message}");
                    if let Ok(mut failure) = ended.lock() {
                        *failure = Some(message);
                    }
                }
            })?;
        Ok(Self {
            address,
            stop,
            failure,
        })
    }
    pub fn failure(&self) -> Option<String> {
        self.failure.lock().ok()?.clone()
    }
}
impl Drop for ClientBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Accepts WebSocket connections on `listen` and bridges each one to the UDP
/// game server at `server`. Runs until the listener fails.
pub fn run_relay(listen: &str, server: SocketAddr, max_connections: usize) -> io::Result<()> {
    let listener = TcpListener::bind(listen)?;
    log::info!(
        "Relaying WebSocket {} to UDP {server}",
        listener.local_addr()?
    );
    // Relay connections end on their own; nothing asks them to stop early.
    static NEVER: AtomicBool = AtomicBool::new(false);
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                log::warn!("accept failed: {e}");
                continue;
            }
        };
        if active.load(Ordering::Relaxed) >= max_connections {
            log::warn!("Refusing connection: {max_connections} relays already active");
            continue;
        }
        active.fetch_add(1, Ordering::Relaxed);
        let finished = active.clone();
        let spawned = thread::Builder::new()
            .name("websocket-relay".into())
            .spawn(move || {
                relay_connection(stream, server, &NEVER);
                finished.fetch_sub(1, Ordering::Relaxed);
            });
        if let Err(e) = spawned {
            active.fetch_sub(1, Ordering::Relaxed);
            log::warn!("could not start relay thread: {e}");
        }
    }
    Ok(())
}

// tungstenite's handshake callback signature fixes the large error type.
#[allow(clippy::result_large_err)]
fn relay_connection(stream: TcpStream, server: SocketAddr, stop: &AtomicBool) {
    let from = stream
        .peer_addr()
        .map_or_else(|_| "unknown".into(), |a| a.to_string());
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(CONNECT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECT_TIMEOUT));
    // Behind Cloudflare the TCP peer is cloudflared; the header has the player.
    let mut client = from.clone();
    let callback = |request: &Request, response: Response| {
        if let Some(ip) = request
            .headers()
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
        {
            client = ip.to_owned();
        }
        Ok(response)
    };
    let mut ws = match tungstenite::accept_hdr_with_config(stream, callback, Some(config())) {
        Ok(ws) => ws,
        Err(e) => {
            log::debug!("rejected non-WebSocket request from {from}: {e}");
            return;
        }
    };
    let udp = match UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })
    .and_then(|udp| udp.set_nonblocking(true).map(|()| udp))
    .and_then(|udp| ws.get_ref().set_nonblocking(true).map(|()| udp))
    {
        Ok(udp) => udp,
        Err(e) => {
            log::warn!("relay setup for {client} failed: {e}");
            return;
        }
    };
    let port = udp.local_addr().map_or(0, |a| a.port());
    crate::sockets::tune_udp(&udp, 64 * 1024);
    log::info!("{client} connected (relay port {port})");
    match pump(&mut ws, &udp, Some(server), stop) {
        Ok(()) => log::info!("{client} disconnected (relay port {port})"),
        Err(e) => log::info!("{client} disconnected (relay port {port}): {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_websocket_applies_backpressure_and_resumes_without_losing_datagrams() {
        use tungstenite::protocol::Role;
        struct Stalled {
            writable: Arc<AtomicBool>,
            blocked: Arc<AtomicUsize>,
            output: Arc<Mutex<Vec<u8>>>,
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
                self.output.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let writable = Arc::new(AtomicBool::new(false));
        let blocked = Arc::new(AtomicUsize::new(0));
        let output = Arc::new(Mutex::new(Vec::new()));
        let mut ws = WebSocket::from_raw_socket(
            Stalled {
                writable: writable.clone(),
                blocked: blocked.clone(),
                output: output.clone(),
            },
            Role::Server,
            Some(config()),
        );
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        crate::sockets::tune_udp(&udp, 1024 * 1024);
        udp.set_nonblocking(true).unwrap();
        let address = udp.local_addr().unwrap();
        let peek = udp.try_clone().unwrap();
        let game = UdpSocket::bind("127.0.0.1:0").unwrap();
        let peer = game.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let worker = thread::spawn(move || pump(&mut ws, &udp, Some(peer), &flag));
        for i in 0..96u8 {
            game.send_to(&[i; 900], address).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while blocked.load(Ordering::Relaxed) < 200 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(blocked.load(Ordering::Relaxed) >= 200);
        assert!(
            peek.peek_from(&mut [0u8; MAX_DATAGRAM]).is_ok(),
            "a stalled TCP writer must stop draining UDP"
        );
        writable.store(true, Ordering::Relaxed);
        while output.lock().unwrap().len() < 96 * 904 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap();
        let mut received = WebSocket::from_raw_socket(
            io::Cursor::new(output.lock().unwrap().clone()),
            Role::Client,
            Some(config()),
        );
        for i in 0..96u8 {
            assert_eq!(received.read().unwrap(), Message::binary(vec![i; 900]));
        }
    }

    #[test]
    fn closed_websocket_is_reported_to_the_client_without_waiting_for_udp_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.close(None).unwrap();
        });
        let bridge = ClientBridge::connect(&format!("ws://{address}")).unwrap();
        let start = Instant::now();
        while bridge.failure().is_none() && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(1));
        }
        assert!(bridge.failure().is_some());
        worker.join().unwrap();
    }

    #[test]
    fn join_addresses() {
        assert_eq!(websocket_url("127.0.0.1:5000"), None);
        assert_eq!(websocket_url("[::1]:5000"), None);
        assert_eq!(
            websocket_url("tankyard.example.com"),
            Some("wss://tankyard.example.com".into())
        );
        assert_eq!(
            websocket_url(" ws://localhost:8080 "),
            Some("ws://localhost:8080".into())
        );
    }

    #[test]
    fn relays_datagrams_both_ways() {
        let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server = echo.local_addr().unwrap();
        thread::spawn(move || {
            let mut buffer = [0u8; 64];
            while let Ok((len, from)) = echo.recv_from(&mut buffer) {
                echo.send_to(&buffer[..len], from).unwrap();
            }
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let relay = listener.local_addr().unwrap();
        drop(listener);
        thread::spawn(move || run_relay(&relay.to_string(), server, 4));
        let start = Instant::now();
        let bridge = loop {
            match ClientBridge::connect(&format!("ws://{relay}")) {
                Ok(bridge) => break bridge,
                Err(_) if start.elapsed() < Duration::from_secs(5) => {
                    thread::sleep(Duration::from_millis(20))
                }
                Err(e) => panic!("{e}"),
            }
        };
        let game = UdpSocket::bind("127.0.0.1:0").unwrap();
        game.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        game.send_to(b"tank", bridge.address).unwrap();
        let mut buffer = [0u8; 64];
        let (len, from) = game.recv_from(&mut buffer).unwrap();
        assert_eq!((&buffer[..len], from), (&b"tank"[..], bridge.address));
    }
}
