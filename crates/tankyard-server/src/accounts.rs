//! Adapters for an account service. Token signing stays outside the client.
use crate::{NetError, unix_time};
use renet_netcode::ConnectToken;
use std::{
    io::{Cursor, Read},
    net::SocketAddr,
    path::Path,
    time::Duration,
};
use tankyard_protocol::PROTOCOL_ID;

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
