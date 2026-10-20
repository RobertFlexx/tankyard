//! Server-scoped administration. Only public verification keys belong here.
use ring::signature::{ED25519, UnparsedPublicKey};

pub const CENTRAL_OWNER_PUBLIC_KEY: [u8; 32] = [
    23, 15, 227, 250, 87, 161, 20, 215, 25, 30, 215, 159, 156, 133, 206, 245, 32, 94, 176, 175,
    137, 24, 151, 37, 55, 173, 116, 108, 194, 128, 222, 12,
];

pub fn admin_command(
    client: u64,
    nonce: &[u8; 32],
    server: &str,
    round: u32,
    sequence: u64,
    payload: &[u8],
) -> Vec<u8> {
    let mut message = b"Tank Yard server administration v1\0".to_vec();
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

pub fn central_owner_challenge(
    client: u64,
    nonce: &[u8; 32],
    server: &str,
    admin_key: &[u8; 32],
) -> Vec<u8> {
    let mut message = b"Tank Yard central owner identity v1\0".to_vec();
    message.extend_from_slice(&admin_command(client, nonce, server, 0, 0, admin_key));
    message
}

pub fn verify_admin_signature(key: &[u8; 32], message: &[u8], signature: &[u8]) -> bool {
    signature.len() == 64
        && UnparsedPublicKey::new(&ED25519, key)
            .verify(message, signature)
            .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    #[test]
    fn signatures_bind_server_connection_nonce_round_sequence_payload_and_identity_domain() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[29; 32]).unwrap();
        let public = key.public_key().as_ref().try_into().unwrap();
        let message = admin_command(7, &[3; 32], "arena", 1, 4, b"command");
        let signature = key.sign(&message);
        assert!(verify_admin_signature(
            &public,
            &message,
            signature.as_ref()
        ));
        for changed in [
            admin_command(8, &[3; 32], "arena", 1, 4, b"command"),
            admin_command(7, &[4; 32], "arena", 1, 4, b"command"),
            admin_command(7, &[3; 32], "other", 1, 4, b"command"),
            admin_command(7, &[3; 32], "arena", 2, 4, b"command"),
            admin_command(7, &[3; 32], "arena", 1, 5, b"command"),
            admin_command(7, &[3; 32], "arena", 1, 4, b"tampered"),
            central_owner_challenge(7, &[3; 32], "arena", &public),
            crate::creator_command(7, &[3; 32], "arena", 1, 4, b"command"),
        ] {
            assert!(!verify_admin_signature(
                &public,
                &changed,
                signature.as_ref()
            ));
        }
        assert!(!verify_admin_signature(
            &CENTRAL_OWNER_PUBLIC_KEY,
            &message,
            signature.as_ref()
        ));
        assert!(!verify_admin_signature(
            &crate::CREATOR_PUBLIC_KEY,
            &message,
            signature.as_ref()
        ));
    }
}
