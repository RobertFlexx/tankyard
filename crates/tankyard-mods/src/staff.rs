//! Staff credential packs: a password-sealed identity key signed by that same
//! identity. They unlock creator or central-owner tools in the game client and
//! carry no game content, so they never change a match or reach a server.
//!
//! The pack proves where it came from before anyone types a password: its
//! files are signed with the identity's key, and the client checks the
//! signature against the public key compiled into every build. The sealed
//! key opens only with its password, and servers still verify every request.
use crate::{Manifest, PackFiles};
use serde::{Deserialize, Serialize};

/// The pack format this version reads and writes.
pub const FORMAT: u32 = 1;
/// Holds the signature over every other file in the pack.
pub const SIGNATURE_FILE: &str = "staff.sig";
/// The sealed seed's size: magic, salt, nonce, ciphertext and tag.
pub const VAULT_BYTES: usize = 84;
/// Identities a staff pack can hold.
pub const IDENTITIES: [&str; 2] = ["creator", "central-owner"];
const SIGNATURE_BYTES: usize = 64;
const CONTEXT: &[u8] = b"tankyard-staff-pack\0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffSection {
    pub format: u32,
    /// `creator` or `central-owner`.
    pub identity: String,
    /// The password-sealed identity seed, in hexadecimal.
    pub vault: String,
}

impl StaffSection {
    /// The sealed seed's bytes. Validated packs always decode.
    pub fn envelope(&self) -> Option<[u8; VAULT_BYTES]> {
        decode_hex(&self.vault)?.try_into().ok()
    }
}

/// Structure only; whether the pack is authentic needs the identity's public
/// key, which `verify` checks.
pub(crate) fn validate(
    manifest: &Manifest,
    section: &StaffSection,
    pack: &PackFiles,
) -> Result<(), String> {
    if section.format != FORMAT {
        return Err(format!(
            "staff pack format {} needs a newer game; this version reads format {FORMAT}",
            section.format
        ));
    }
    if !IDENTITIES.contains(&section.identity.as_str()) {
        return Err("staff pack identity must be creator or central-owner".into());
    }
    if section.envelope().is_none_or(|e| &e[..8] != b"TYVAULT1") {
        return Err("staff pack vault is not a sealed credential".into());
    }
    if signature(pack).is_none() {
        return Err(format!(
            "staff pack needs a {SIGNATURE_FILE} with its signature"
        ));
    }
    let carries_content = manifest.api_version != crate::API_VERSION
        || !manifest.scripts.is_empty()
        || !manifest.dependencies.is_empty()
        || !manifest.classes.is_empty()
        || !manifest.maps.is_empty()
        || manifest.rules.is_some()
        || !manifest.hooks.is_empty()
        || manifest.mode_name.is_some()
        || !manifest.textures.is_empty()
        || manifest.menu.is_some()
        || manifest.theme.is_some()
        || !manifest.text.is_empty();
    if carries_content {
        return Err("staff packs use api_version 1 and carry no game content".into());
    }
    Ok(())
}

fn signature(pack: &PackFiles) -> Option<[u8; SIGNATURE_BYTES]> {
    let text = std::str::from_utf8(pack.get(SIGNATURE_FILE)?).ok()?;
    decode_hex(text.trim())?.try_into().ok()
}

/// What the identity signs: a fixed context, the identity and a digest of
/// every file except the signature itself.
pub fn signed_message(identity: &str, digest: &[u8; 32]) -> Vec<u8> {
    let mut message = Vec::with_capacity(CONTEXT.len() + identity.len() + 1 + 32);
    message.extend_from_slice(CONTEXT);
    message.extend_from_slice(identity.as_bytes());
    message.push(0);
    message.extend_from_slice(digest);
    message
}

/// The message to sign for `pack`, which must not yet contain a signature
/// (or whose signature is ignored).
pub fn message_for(identity: &str, pack: &PackFiles) -> Vec<u8> {
    signed_message(identity, &pack.digest_without(SIGNATURE_FILE))
}

/// True when `public_key` signed exactly these files for this identity.
pub fn verify(section: &StaffSection, pack: &PackFiles, public_key: &[u8; 32]) -> bool {
    let Some(signature) = signature(pack) else {
        return false;
    };
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
        .verify(&message_for(&section.identity, pack), &signature)
        .is_ok()
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > 4096 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn manifest(vault: &str) -> String {
        format!(
            r#"{{"api_version":1,"id":"staff_creator","name":"Creator Tools","version":"1","author":"Tankyard Moderation","staff":{{"format":1,"identity":"creator","vault":"{vault}"}}}}"#
        )
    }

    fn signed(key: &Ed25519KeyPair, files: Vec<(String, Vec<u8>)>) -> PackFiles {
        let unsigned = PackFiles::from_files(files.clone()).unwrap();
        let signature = key.sign(&message_for("creator", &unsigned));
        let mut files = files;
        files.push((
            SIGNATURE_FILE.into(),
            encode_hex(signature.as_ref()).into_bytes(),
        ));
        PackFiles::from_files(files).unwrap()
    }

    #[test]
    fn staff_packs_are_signed_by_their_identity_and_carry_no_content() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let public: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
        let mut vault = b"TYVAULT1".to_vec();
        vault.resize(VAULT_BYTES, 3);
        let files = vec![
            (
                "mod.json".to_owned(),
                manifest(&encode_hex(&vault)).into_bytes(),
            ),
            ("README.md".to_owned(), b"# Creator tools".to_vec()),
        ];
        let pack = signed(&key, files.clone());
        let parsed = crate::parse_manifest(&pack).unwrap();
        let section = parsed.staff.as_ref().unwrap();
        assert_eq!(section.envelope().unwrap().to_vec(), vault);
        assert!(verify(section, &pack, &public));
        // Another key, or any changed file, fails.
        let other = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
        assert!(!verify(
            section,
            &pack,
            other.public_key().as_ref().try_into().unwrap()
        ));
        let mut changed = files.clone();
        changed[1].1 = b"# Creator tools!".to_vec();
        let signature = pack.get(SIGNATURE_FILE).unwrap().to_vec();
        changed.push((SIGNATURE_FILE.into(), signature));
        let changed = PackFiles::from_files(changed).unwrap();
        assert!(!verify(section, &changed, &public));
        // A missing signature, a foreign vault or game content are refused.
        assert!(crate::parse_manifest(&PackFiles::from_files(files).unwrap()).is_err());
        let foreign = encode_hex(&[1; VAULT_BYTES]);
        let pack = signed(
            &key,
            vec![("mod.json".into(), manifest(&foreign).into_bytes())],
        );
        assert!(crate::parse_manifest(&pack).is_err());
        let content =
            manifest(&encode_hex(&vault)).replace(r#""staff""#, r#""mode_name":"Staff","staff""#);
        let pack = signed(&key, vec![("mod.json".into(), content.into_bytes())]);
        assert!(crate::parse_manifest(&pack).is_err());
    }

    #[test]
    fn hex_round_trips_and_rejects_malformed_text() {
        assert_eq!(
            decode_hex(&encode_hex(&[0, 15, 255])).unwrap(),
            [0, 15, 255]
        );
        assert!(decode_hex("abc").is_none());
        assert!(decode_hex("zz").is_none());
        assert!(decode_hex("é0").is_none());
    }
}
