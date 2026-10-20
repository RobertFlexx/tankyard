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
