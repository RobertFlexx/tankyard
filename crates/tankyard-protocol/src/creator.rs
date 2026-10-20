//! Public identity verification. Signing credentials never ship in a public client.
use ring::signature::{ED25519, UnparsedPublicKey};

pub const CREATOR_PUBLIC_KEY: [u8; 32] = [
    248, 236, 151, 40, 80, 61, 141, 68, 149, 27, 139, 212, 227, 47, 243, 62, 75, 177, 76, 215, 17,
    159, 161, 252, 223, 13, 80, 15, 27, 57, 130, 53,
];
const DOMAIN: &[u8] = b"Tank Yard creator identity v2\0";

pub fn creator_challenge(client: u64, nonce: &[u8; 32], server: &str) -> Vec<u8> {
    let mut message = DOMAIN.to_vec();
    message.extend_from_slice(&client.to_le_bytes());
    message.extend_from_slice(nonce);
    message.extend_from_slice(&(server.len() as u32).to_le_bytes());
    message.extend_from_slice(server.as_bytes());
    message
}

pub fn verify_creator(client: u64, nonce: &[u8; 32], server: &str, signature: &[u8]) -> bool {
    signature.len() == 64
        && UnparsedPublicKey::new(&ED25519, CREATOR_PUBLIC_KEY)
            .verify(&creator_challenge(client, nonce, server), signature)
            .is_ok()
}

/// Bind every privileged command to this connection, round, monotonic sequence
/// and exact payload. Knowing a connection's public identity proof is not enough
/// to manufacture administration packets on an unauthenticated transport.
pub fn creator_command(
    client: u64,
    nonce: &[u8; 32],
    server: &str,
    round: u32,
    sequence: u64,
    payload: &[u8],
) -> Vec<u8> {
    let mut message = b"Tank Yard owner command v2\0".to_vec();
    message.extend_from_slice(&client.to_le_bytes());
    message.extend_from_slice(nonce);
    message.extend_from_slice(&(server.len() as u32).to_le_bytes());
    message.extend_from_slice(server.as_bytes());
    message.extend_from_slice(&round.to_le_bytes());
    message.extend_from_slice(&sequence.to_le_bytes());
    message.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    message.extend_from_slice(payload);
    message
}

pub fn verify_creator_command(message: &[u8], signature: &[u8]) -> bool {
    signature.len() == 64
        && UnparsedPublicKey::new(&ED25519, CREATOR_PUBLIC_KEY)
            .verify(message, signature)
            .is_ok()
}

pub fn party_challenge(client: u64, nonce: &[u8; 32], server: &str, admin: &[u8; 32]) -> Vec<u8> {
    let mut message = b"Tank Yard party entry v1\0".to_vec();
    message.extend_from_slice(&creator_challenge(client, nonce, server));
    message.extend_from_slice(admin);
    message
}

pub fn verify_party(
    public: &[u8; 32],
    client: u64,
    nonce: &[u8; 32],
    server: &str,
    admin: &[u8; 32],
    signature: &[u8],
) -> bool {
    signature.len() == 64
        && UnparsedPublicKey::new(&ED25519, public)
            .verify(&party_challenge(client, nonce, server, admin), signature)
            .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_signatures_and_malformed_proofs_cannot_claim_the_creator_identity() {
        assert!(!verify_creator(7, &[0; 32], "arena", &[0; 64]));
        assert!(!verify_creator(7, &[0; 32], "arena", &[0; 63]));
        assert_ne!(
            creator_challenge(7, &[0; 32], "arena"),
            creator_challenge(8, &[0; 32], "arena")
        );
        assert_ne!(
            creator_challenge(7, &[0; 32], "arena"),
            creator_challenge(7, &[1; 32], "arena")
        );
    }
    #[test]
    fn owner_command_signatures_are_domain_separated_and_bound_to_all_context() {
        let message = creator_command(7, &[0; 32], "arena", 1, 0, b"command");
        for changed in [
            creator_challenge(7, &[0; 32], "arena"),
            creator_command(8, &[0; 32], "arena", 1, 0, b"command"),
            creator_command(7, &[1; 32], "arena", 1, 0, b"command"),
            creator_command(7, &[0; 32], "other", 1, 0, b"command"),
            creator_command(7, &[0; 32], "arena", 2, 0, b"command"),
            creator_command(7, &[0; 32], "arena", 1, 1, b"command"),
            creator_command(7, &[0; 32], "arena", 1, 0, b"other"),
        ] {
            assert_ne!(message, changed);
        }
        assert!(!verify_creator_command(&message, &[0; 64]));
    }
}

#[cfg(test)]
mod party_tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    #[test]
    fn party_proofs_cannot_replay_across_connections_or_replace_the_admin_key() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[11; 32]).unwrap();
        let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
        let message = party_challenge(3, &[4; 32], "party", &[5; 32]);
        let signature = key.sign(&message);
        assert!(verify_party(
            &public,
            3,
            &[4; 32],
            "party",
            &[5; 32],
            signature.as_ref()
        ));
        for (id, nonce, server, admin) in [
            (4, [4; 32], "party", [5; 32]),
            (3, [7; 32], "party", [5; 32]),
            (3, [4; 32], "other", [5; 32]),
            (3, [4; 32], "party", [8; 32]),
        ] {
            assert!(!verify_party(
                &public,
                id,
                &nonce,
                server,
                &admin,
                signature.as_ref()
            ));
        }
        assert!(!verify_party(
            &[0; 32],
            3,
            &[4; 32],
            "party",
            &[5; 32],
            signature.as_ref()
        ));
    }
}
