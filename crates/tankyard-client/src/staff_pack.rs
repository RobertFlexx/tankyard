//! Staff credential packs replace the separately compiled creator and
//! central-owner editions. Any game build reads them, so they keep working
//! across versions without a private rebuild.
//!
//! A pack is trusted only when the identity's own key signed it, checked
//! against the public key compiled into the game, so nobody else can make a
//! pack that asks for a staff password. The key inside opens only with its
//! password, and servers verify every staff request on their own.
use crate::credential_vault;
use tankyard_mods::{
    Manifest, PackFiles,
    staff::{self, StaffSection},
};
use zeroize::Zeroizing;

/// Shown as a staff pack's author; packs are signed, so this is not a claim
/// anyone else can make.
pub const PUBLISHER: &str = "Tankyard Moderation";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Identity {
    Creator,
    CentralOwner,
}

impl Identity {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "creator" => Some(Self::Creator),
            "central-owner" => Some(Self::CentralOwner),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Creator => "creator",
            Self::CentralOwner => "central-owner",
        }
    }
    /// The key file the public client also accepts for this identity.
    pub fn key_file(self) -> &'static str {
        match self {
            Self::Creator => "creator.key",
            Self::CentralOwner => "central-owner.key",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Creator => "Creator tools",
            Self::CentralOwner => "Central owner tools",
        }
    }
    pub fn public_key(self) -> [u8; 32] {
        match self {
            Self::Creator => tankyard_protocol::CREATOR_PUBLIC_KEY,
            Self::CentralOwner => tankyard_protocol::CENTRAL_OWNER_PUBLIC_KEY,
        }
    }
}

/// A staff pack whose signature checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    pub identity: Identity,
    pub envelope: [u8; staff::VAULT_BYTES],
}

impl Verified {
    /// Open the sealed key with `password`. Slow on purpose (PBKDF2), so call
    /// it off the frame loop.
    pub fn open(&self, password: &str) -> Result<Zeroizing<[u8; 32]>, credential_vault::Error> {
        credential_vault::open(&self.envelope, password, &self.identity.public_key())
    }
}

/// Check that a parsed staff pack was signed by the identity it names.
pub fn verify(manifest: &Manifest, pack: &PackFiles) -> Result<Verified, String> {
    let section = manifest.staff.as_ref().ok_or("not a staff pack")?;
    verify_section(section, pack)
}

fn verify_section(section: &StaffSection, pack: &PackFiles) -> Result<Verified, String> {
    let identity = Identity::parse(&section.identity).ok_or("unknown staff identity")?;
    if !staff::verify(section, pack, &identity.public_key()) {
        return Err(format!(
            "this staff pack isn't signed by {PUBLISHER}; it may have been changed"
        ));
    }
    let envelope = section.envelope().ok_or("staff pack vault is malformed")?;
    Ok(Verified { identity, envelope })
}

/// Everything a staff pack holds, ready to zip: `files` gains the manifest
/// and signature. `seed` must belong to `identity`.
pub fn assemble(
    identity: Identity,
    seed: &[u8; 32],
    password: &str,
    version: &str,
    mut files: Vec<(String, Vec<u8>)>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    use ring::signature::Ed25519KeyPair;
    let envelope = credential_vault::seal(seed, password, &identity.public_key())
        .map_err(|error| format!("unable to seal the credential: {error:?}"))?;
    let manifest = serde_json::json!({
        "api_version": tankyard_mods::API_VERSION,
        "id": format!("tankyard_moderation_{}", identity.name().replace('-', "_")),
        "name": identity.title(),
        "version": version,
        "author": PUBLISHER,
        "description": format!(
            "{} for Tank Yard. Enter the password when the game asks; servers verify every action.",
            identity.title()
        ),
        "staff": {
            "format": staff::FORMAT,
            "identity": identity.name(),
            "vault": staff::encode_hex(&envelope),
        },
    });
    files.retain(|(name, _)| name != "mod.json" && name != staff::SIGNATURE_FILE);
    files.push((
        "mod.json".into(),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    ));
    let pack = PackFiles::from_files(files.clone())?;
    let key = Ed25519KeyPair::from_seed_unchecked(seed).map_err(|_| "invalid seed")?;
    let signature = key.sign(&staff::message_for(identity.name(), &pack));
    files.push((
        staff::SIGNATURE_FILE.into(),
        format!("{}\n", staff::encode_hex(signature.as_ref())).into_bytes(),
    ));
    // The finished pack must pass the same checks every client runs.
    let pack = PackFiles::from_files(files.clone())?;
    let manifest = tankyard_mods::parse_manifest(&pack)?;
    verify(&manifest, &pack)?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_signed_by_another_key_are_never_trusted() {
        // Only the real identity key can sign; any other seed is refused
        // before a pack is ever written.
        let error = assemble(
            Identity::Creator,
            &[5; 32],
            "Random-test-password-that-is-not-used-anywhere",
            "1",
            vec![("README.md".into(), b"# Tools".to_vec())],
        )
        .unwrap_err();
        assert!(error.contains("seal"), "{error}");
    }

    #[test]
    fn identities_map_to_their_compiled_public_keys() {
        for identity in [Identity::Creator, Identity::CentralOwner] {
            assert_eq!(Identity::parse(identity.name()), Some(identity));
        }
        assert_ne!(
            Identity::Creator.public_key(),
            Identity::CentralOwner.public_key()
        );
        assert_eq!(Identity::parse("owner"), None);
    }
}
