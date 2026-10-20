//! Private player-hosted parties. Invite keys grant entry, never host authority.
use crate::NetError;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartySettings {
    pub public_key: [u8; 32],
    pub ally_slots: usize,
}
#[derive(Clone)]
pub struct Invite {
    pub address: SocketAddr,
    seed: [u8; 32],
}
impl Invite {
    pub fn create(address: SocketAddr) -> Self {
        Self {
            address,
            seed: renet_netcode::generate_random_bytes(),
        }
    }
    pub fn parse(text: &str) -> Result<Self, NetError> {
        let invalid = || NetError("Paste a complete tankyard://party/IP:PORT#invite link.".into());
        if text.len() > 256 {
            return Err(invalid());
        }
        let (address, token) = text
            .trim()
            .strip_prefix("tankyard://party/")
            .ok_or_else(invalid)?
            .split_once('#')
            .ok_or_else(invalid)?;
        let address: SocketAddr = address.parse().map_err(|_| invalid())?;
        if address.port() == 0
            || address.ip().is_unspecified()
            || address.ip().is_multicast()
            || (token.len() != 64 || !token.is_ascii())
        {
            return Err(invalid());
        }
        let mut seed = [0; 32];
        for (slot, pair) in seed.iter_mut().zip(token.as_bytes().as_chunks::<2>().0) {
            *slot = u8::from_str_radix(std::str::from_utf8(pair).map_err(|_| invalid())?, 16)
                .map_err(|_| invalid())?;
        }
        Ok(Self { address, seed })
    }
    pub fn link(&self) -> String {
        let token: String = self.seed.iter().map(|b| format!("{b:02x}")).collect();
        format!("tankyard://party/{}#{token}", self.address)
    }
    pub fn key(&self) -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&self.seed).expect("32-byte party seed")
    }
    pub fn settings(&self, ally_slots: usize) -> PartySettings {
        let mut public_key = [0; 32];
        public_key.copy_from_slice(self.key().public_key().as_ref());
        PartySettings {
            public_key,
            ally_slots: ally_slots.clamp(2, 8),
        }
    }
}
/// Route discovery sends no application packet and performs no external lookup.
pub fn local_ip() -> IpAddr {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .ok()
        .and_then(|socket| {
            socket.connect("192.0.2.1:9").ok()?;
            socket.local_addr().ok().map(|a| a.ip())
        })
        .filter(|ip| !ip.is_unspecified())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invitations_round_trip_and_reject_bad_endpoints_and_tokens() {
        for address in ["192.168.1.8:5000", "[::1]:5000"] {
            let invite = Invite::create(address.parse().unwrap());
            assert_eq!(Invite::parse(&invite.link()).unwrap().link(), invite.link());
            assert_eq!(invite.settings(4).ally_slots, 4);
        }
        for input in [
            "",
            "tankyard://party/0.0.0.0:5000#abc",
            "tankyard://party/127.0.0.1:0#abc",
            "https://example.org",
            "tankyard://party/127.0.0.1:5000#😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀",
        ] {
            assert!(Invite::parse(input).is_err());
        }
        assert_ne!(
            Invite::create("127.0.0.1:5000".parse().unwrap()).link(),
            Invite::create("127.0.0.1:5000".parse().unwrap()).link()
        );
    }
}
