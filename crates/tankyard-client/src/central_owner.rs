use crate::{
    credentials,
    session::{Engine, Session},
};
use ring::signature::Ed25519KeyPair;

pub struct CentralOwner {
    key: Option<Ed25519KeyPair>,
}
impl CentralOwner {
    pub fn new() -> Self {
        Self {
            key: credentials::load(
                "central-owner.key",
                &tankyard_protocol::CENTRAL_OWNER_PUBLIC_KEY,
            ),
        }
    }
    pub fn enabled(&self) -> bool {
        self.key.is_some()
    }
    pub fn unavailable(client: &tankyard_server::NetworkClient) -> String {
        match client.admin_server_id.as_deref() {
            Some(id) if id != "tankyard-central" => format!(
                "Owner tools unavailable: server ID is \"{id}\"; the central profile requires \"tankyard-central\"."
            ),
            Some(_) => "Owner access is disabled or verification has not succeeded. Check the server response and audit log.".into(),
            None => "Waiting for the server administration handshake.".into(),
        }
    }
    pub fn authenticate(&self, session: &mut Session) {
        let Some(key) = &self.key else {
            return;
        };
        if let Engine::Online { client, .. } = &mut session.engine
            && let Some(nonce) = client.central_owner_challenge.take()
        {
            let message = tankyard_protocol::central_owner_challenge(
                client.client_id(),
                &nonce,
                client.admin_server_id.as_deref().unwrap_or(""),
                &client.admin_public_key(),
            );
            if !client.submit_central_owner_proof(key.sign(&message).as_ref().to_vec()) {
                session.notice =
                    Some("Central owner verification could not be queued. Reconnect.".into());
            }
        }
    }
}
