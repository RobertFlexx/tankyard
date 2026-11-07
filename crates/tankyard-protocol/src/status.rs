//! Out-of-band server status queries. A query is a single padded datagram
//! sent to the game port (directly, or through a WebSocket relay) before any
//! connection exists; the server answers with its lobbies and population.
//! Requests are padded so a reply is never much larger than its request.
use serde::{Deserialize, Serialize};

/// The first byte's low nibble is not a netcode packet type, so a game server
/// that predates status queries silently drops them.
const REQUEST_MAGIC: [u8; 4] = [0xef, b'T', b'Y', b'Q'];
const REPLY_MAGIC: [u8; 4] = [0xef, b'T', b'Y', b'R'];
const STATUS_VERSION: u8 = 1;
/// Every request is padded to this size.
pub const STATUS_REQUEST_BYTES: usize = 600;
/// Replies never exceed this, keeping amplification below 2x.
pub const MAX_STATUS_REPLY_BYTES: usize = 1100;
pub const MAX_STATUS_LOBBIES: usize = 16;
const MAX_STATUS_MODS: usize = 6;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LobbyStatus {
    pub humans: u8,
    pub bots: u8,
    pub capacity: u8,
    pub mode: String,
    pub map: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerStatus {
    pub protocol: u32,
    pub version: String,
    pub name: String,
    pub players: u16,
    pub max_players: u16,
    pub password: bool,
    pub official: bool,
    pub mods: Vec<String>,
    pub lobbies: Vec<LobbyStatus>,
}

pub fn status_request(nonce: u64) -> [u8; STATUS_REQUEST_BYTES] {
    let mut packet = [0; STATUS_REQUEST_BYTES];
    packet[..4].copy_from_slice(&REQUEST_MAGIC);
    packet[4] = STATUS_VERSION;
    packet[5..13].copy_from_slice(&nonce.to_le_bytes());
    packet
}

/// The nonce of a valid status request; `None` for every other datagram.
pub fn parse_status_request(packet: &[u8]) -> Option<u64> {
    (packet.len() >= STATUS_REQUEST_BYTES
        && packet[..4] == REQUEST_MAGIC
        && packet[4] == STATUS_VERSION)
        .then(|| u64::from_le_bytes(packet[5..13].try_into().unwrap()))
}

fn clip(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for c in text.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > limit {
            break;
        }
        out.push(c);
    }
    out
}

/// The reply body for a status, bounded to [`MAX_STATUS_REPLY_BYTES`] with
/// any nonce; lobbies and mods beyond the limits are dropped.
pub fn encode_status(status: &ServerStatus) -> Vec<u8> {
    let mut status = ServerStatus {
        version: clip(&status.version, 16),
        name: clip(&status.name, 64),
        mods: status
            .mods
            .iter()
            .take(MAX_STATUS_MODS)
            .map(|m| clip(m, 32))
            .collect(),
        lobbies: status
            .lobbies
            .iter()
            .take(MAX_STATUS_LOBBIES)
            .map(|l| LobbyStatus {
                mode: clip(&l.mode, 24),
                map: clip(&l.map, 24),
                ..l.clone()
            })
            .collect(),
        ..status.clone()
    };
    loop {
        let body = postcard::to_allocvec(&status).unwrap_or_default();
        if body.len() + 13 <= MAX_STATUS_REPLY_BYTES || status.lobbies.is_empty() {
            return body;
        }
        status.lobbies.pop();
    }
}

pub fn status_reply(nonce: u64, body: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(13 + body.len());
    packet.extend_from_slice(&REPLY_MAGIC);
    packet.push(STATUS_VERSION);
    packet.extend_from_slice(&nonce.to_le_bytes());
    packet.extend_from_slice(body);
    packet
}

pub fn parse_status_reply(packet: &[u8]) -> Option<(u64, ServerStatus)> {
    if packet.len() < 13
        || packet.len() > MAX_STATUS_REPLY_BYTES
        || packet[..4] != REPLY_MAGIC
        || packet[4] != STATUS_VERSION
    {
        return None;
    }
    let nonce = u64::from_le_bytes(packet[5..13].try_into().unwrap());
    let status: ServerStatus = postcard::from_bytes(&packet[13..]).ok()?;
    (status.lobbies.len() <= MAX_STATUS_LOBBIES && status.mods.len() <= MAX_STATUS_MODS)
        .then_some((nonce, status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_and_short_or_foreign_packets_are_ignored() {
        let packet = status_request(77);
        assert_eq!(parse_status_request(&packet), Some(77));
        assert_eq!(parse_status_request(&packet[..100]), None);
        let mut netcode = packet;
        netcode[0] = 0x05;
        assert_eq!(parse_status_request(&netcode), None);
        // Netcode never produces a packet type of 0xf.
        assert_eq!(REQUEST_MAGIC[0] & 0x0f, 0x0f);
    }

    #[test]
    fn replies_stay_bounded_and_round_trip() {
        let status = ServerStatus {
            protocol: 21,
            version: "0.9.0".into(),
            name: "x".repeat(500),
            players: 61,
            max_players: 80,
            mods: vec!["m".repeat(90); 40],
            lobbies: vec![
                LobbyStatus {
                    humans: 20,
                    bots: 4,
                    capacity: 20,
                    mode: "\u{7}Team Deathmatch and more words".into(),
                    map: "y".repeat(200),
                };
                64
            ],
            ..Default::default()
        };
        let body = encode_status(&status);
        let packet = status_reply(9, &body);
        assert!(packet.len() <= MAX_STATUS_REPLY_BYTES);
        assert!(packet.len() < STATUS_REQUEST_BYTES * 2);
        let (nonce, decoded) = parse_status_reply(&packet).unwrap();
        assert_eq!(nonce, 9);
        assert_eq!(decoded.name.len(), 64);
        // Worst-case strings drop trailing lobbies to stay under the cap.
        assert!((12..=MAX_STATUS_LOBBIES).contains(&decoded.lobbies.len()));
        assert!(!decoded.lobbies[0].mode.contains('\u{7}'));
        assert!(parse_status_reply(&packet[..10]).is_none());
        // Realistic names always list every lobby.
        let typical = ServerStatus {
            name: "Tank Yard Central".into(),
            lobbies: vec![
                LobbyStatus {
                    humans: 12,
                    bots: 8,
                    capacity: 20,
                    mode: "Free For All".into(),
                    map: "Signal Plain".into(),
                };
                MAX_STATUS_LOBBIES
            ],
            ..status
        };
        let (_, decoded) = parse_status_reply(&status_reply(1, &encode_status(&typical))).unwrap();
        assert_eq!(decoded.lobbies.len(), MAX_STATUS_LOBBIES);
    }
}
