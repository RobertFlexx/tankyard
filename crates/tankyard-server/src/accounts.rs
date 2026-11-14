//! Adapters for an account service. Token signing stays outside the client.
use crate::{NetError, unix_time};
use renet_netcode::ConnectToken;
use std::{
    io::{Cursor, Read, Write},
    net::SocketAddr,
    path::Path,
    time::Duration,
};
use tankyard_protocol::PROTOCOL_ID;

/// Create a credential file without overwriting an existing key or following
/// a destination symlink. Unix permissions are restricted at creation time.
pub fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), NetError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

/// Prepare a fresh token-authenticated server config. Existing credentials
/// are refused rather than silently rotated, and nothing secret is logged.
pub fn initialize_secure(
    settings: &mut crate::ServerSettings,
    path: &Path,
) -> Result<(), NetError> {
    if settings.netcode_private_key.is_some() {
        return Err(NetError(
            "this configuration already has a server key; keep it and issue tokens instead".into(),
        ));
    }
    if settings.public_addresses.is_empty() {
        return Err(NetError(
            "secure setup needs --public-address IP:PORT for the address players reach".into(),
        ));
    }
    let mut candidate = settings.clone();
    candidate.netcode_private_key = Some(
        renet_netcode::generate_random_bytes::<32>()
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect(),
    );
    candidate.validate()?;
    let text = toml::to_string_pretty(&candidate)
        .map_err(|_| NetError("could not serialize secure settings".into()))?;
    write_private_file(path, text.as_bytes())?;
    *settings = candidate;
    Ok(())
}

/// A fresh, short-lived invitation to this operator's authenticated UDP server.
pub fn save_invitation(
    settings: &crate::ServerSettings,
    path: &Path,
    lifetime: Duration,
) -> Result<(), NetError> {
    let key = settings.netcode_private_key.as_ref().ok_or_else(|| {
        NetError(
            "this server has no private key; create it with --secure --write-config first".into(),
        )
    })?;
    let key = crate::config::parse_private_key(key)?;
    let client = u64::from_le_bytes(renet_netcode::generate_random_bytes()).max(1);
    let bytes = issue_connection_token(&key, client, settings.public_addresses.clone(), lifetime)?;
    write_private_file(path, &bytes)
}

/// The service issues this with the destination server's private key. Each
/// community server has its own key; this is never a global staff credential.
pub fn issue_connection_token(
    private_key: &[u8; 32],
    client_id: u64,
    addresses: Vec<SocketAddr>,
    lifetime: Duration,
) -> Result<Vec<u8>, NetError> {
    issue_at(private_key, client_id, addresses, lifetime, unix_time())
}
fn issue_at(
    private_key: &[u8; 32],
    client_id: u64,
    addresses: Vec<SocketAddr>,
    lifetime: Duration,
    now: Duration,
) -> Result<Vec<u8>, NetError> {
    if client_id == 0
        || !(1..=32).contains(&addresses.len())
        || !(10..=3600).contains(&lifetime.as_secs())
        || now.as_secs().checked_add(lifetime.as_secs()).is_none()
        || addresses
            .iter()
            .any(|a| a.port() == 0 || a.ip().is_unspecified() || a.ip().is_multicast())
    {
        return Err(NetError("invalid connection token request".into()));
    }
    let token = ConnectToken::generate(
        now,
        PROTOCOL_ID,
        lifetime.as_secs(),
        client_id,
        15,
        addresses,
        None,
        private_key,
    )
    .map_err(|e| NetError(e.to_string()))?;
    let mut bytes = Vec::with_capacity(2048);
    token.write(&mut bytes)?;
    Ok(bytes)
}
pub fn decode_connection_token(bytes: &[u8]) -> Result<ConnectToken, NetError> {
    if bytes.len() > 2048 {
        return Err(NetError("connection token exceeds 2048 bytes".into()));
    }
    let mut reader = Cursor::new(bytes);
    let token = ConnectToken::read(&mut reader).map_err(|e| NetError(e.to_string()))?;
    if reader.position() != bytes.len() as u64
        || token.client_id == 0
        || !(1..=30).contains(&token.timeout_seconds)
    {
        return Err(NetError("invalid connection token data".into()));
    }
    if token.protocol_id != PROTOCOL_ID || token.expire_timestamp <= unix_time().as_secs() {
        return Err(NetError("incompatible or expired connection token".into()));
    }
    Ok(token)
}
pub fn load_connection_token(path: &Path) -> Result<ConnectToken, NetError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(2049)
        .read_to_end(&mut bytes)?;
    decode_connection_token(&bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_server_accepts_its_invitation_and_refuses_guest_handshakes() {
        let key = [77; 32];
        let mut server = crate::UdpServer::bind(crate::ServerSettings {
            bind: "127.0.0.1:0".into(),
            bot_count: 0,
            ban_file: String::new(),
            netcode_private_key: Some("4d".repeat(32)),
            world: tankyard_core::WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
        let bytes =
            issue_connection_token(&key, 125, vec![server.address], Duration::from_secs(60))
                .unwrap();
        let mut authenticated = crate::NetworkClient::connect_authenticated(
            decode_connection_token(&bytes).unwrap(),
            "Invited",
            "",
            None,
        )
        .unwrap();
        let mut guest =
            crate::NetworkClient::connect(&server.address.to_string(), "Guest", "").unwrap();
        let dt = Duration::from_nanos(16_666_667);
        for _ in 0..240 {
            authenticated.update(dt).unwrap();
            let _ = guest.update(dt);
            server.update(dt).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(authenticated.is_ready(), "{:?}", authenticated.error);
        assert!(!guest.is_ready());
        assert_eq!(server.player_count(), 1);
        authenticated.disconnect();
        server.shutdown();
    }
    #[test]
    fn secure_setup_creates_private_files_and_never_replaces_keys_or_invites() {
        let path = std::env::temp_dir().join(format!(
            "tankyard-secure-{:016x}",
            u64::from_le_bytes(renet_netcode::generate_random_bytes())
        ));
        std::fs::create_dir(&path).unwrap();
        let config = path.join("server.toml");
        let invitation = path.join("friend.token");
        let mut settings = crate::ServerSettings {
            public_addresses: vec!["127.0.0.1:5000".parse().unwrap()],
            ..Default::default()
        };
        initialize_secure(&mut settings, &config).unwrap();
        assert!(initialize_secure(&mut settings, &config).is_err());
        let loaded = crate::ServerSettings::load(&config).unwrap();
        save_invitation(&loaded, &invitation, Duration::from_secs(120)).unwrap();
        let token = load_connection_token(&invitation).unwrap();
        assert!(token.client_id > 0);
        assert!(save_invitation(&loaded, &invitation, Duration::from_secs(120)).is_err());
        assert_eq!(
            load_connection_token(&invitation).unwrap().client_id,
            token.client_id
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for file in [&config, &invitation] {
                assert_eq!(
                    std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            let link = path.join("link.token");
            std::os::unix::fs::symlink(&invitation, &link).unwrap();
            assert!(write_private_file(&link, b"replacement").is_err());
        }
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn tokens_are_bounded_and_protocol_scoped() {
        let addresses = vec!["127.0.0.1:5000".parse().unwrap()];
        let bytes =
            issue_connection_token(&[7; 32], 17, addresses.clone(), Duration::from_secs(60))
                .unwrap();
        let token = decode_connection_token(&bytes).unwrap();
        assert_eq!(token.client_id, 17);
        assert_eq!(token.protocol_id, PROTOCOL_ID);
        assert!(decode_connection_token(&bytes[..bytes.len() - 1]).is_err());
        assert!(
            issue_at(
                &[7; 32],
                17,
                addresses.clone(),
                Duration::from_secs(60),
                Duration::from_secs(u64::MAX)
            )
            .is_err()
        );
        let expired = issue_at(
            &[7; 32],
            17,
            addresses,
            Duration::from_secs(60),
            Duration::ZERO,
        )
        .unwrap();
        assert!(decode_connection_token(&expired).is_err());
    }
}
