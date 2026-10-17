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
        Arc,
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
    while !stop.load(Ordering::Relaxed) {
        let mut active = false;
        loop {
            match udp.recv_from(&mut buffer) {
                Ok((len, from)) => {
                    if *peer.get_or_insert(from) != from {
                        continue;
                    }
                    match ws.write(Message::binary(buffer[..len].to_vec())) {
                        Ok(()) => {}
                        Err(e) if would_block(&e) => {}
                        Err(e) => return Err(e),
                    }
                    active = true;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                // Linux reports an earlier send's ICMP unreachable here; the
                // netcode layer handles the loss.
                Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => break,
                Err(e) => return Err(e.into()),
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(e) if would_block(&e) => {}
            Err(e) => return Err(e),
        }
        loop {
            match ws.read() {
                Ok(Message::Binary(data)) => {
                    if let Some(peer) = peer {
                        let _ = udp.send_to(&data, peer);
                    }
                    active = true;
                }
                Ok(Message::Close(_)) => {
                    let _ = ws.flush();
                    return Ok(());
                }
                Ok(_) => {}
                Err(e) if would_block(&e) => break,
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
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
}
impl ClientBridge {
    pub fn connect(url: &str) -> Result<Self, NetError> {
        let mut ws = open(url)?;
        let udp = UdpSocket::bind("127.0.0.1:0")?;
        udp.set_nonblocking(true)?;
        let address = udp.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let url = url.to_owned();
        thread::Builder::new()
            .name("websocket-bridge".into())
            .spawn(move || {
                if let Err(e) = pump(&mut ws, &udp, None, &flag) {
                    log::warn!("WebSocket connection to {url} ended: {e}");
                }
            })?;
        Ok(Self { address, stop })
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
