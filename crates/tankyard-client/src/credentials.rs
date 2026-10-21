//! Local credentials enable UI; the server independently verifies every privilege.
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::sync::OnceLock;
use std::{io::Read, path::Path};
use zeroize::Zeroizing;

pub struct Vault {
    pub identity: &'static str,
    pub expected: [u8; 32],
    pub envelope: &'static [u8],
}
static UNLOCKED: OnceLock<Zeroizing<[u8; 32]>> = OnceLock::new();
pub fn bundled_vault() -> Option<Vault> {
    None
}
pub fn unlock(seed: Zeroizing<[u8; 32]>) -> bool {
    let Some(vault) = bundled_vault() else {
        return false;
    };
    if parse(seed.as_ref(), &vault.expected).is_none() {
        return false;
    }
    UNLOCKED.set(seed).is_ok()
}

fn read_key(path: &Path, expected: &[u8; 32]) -> Option<Ed25519KeyPair> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() != 32 {
        return None;
    }
    let mut seed = Vec::with_capacity(33);
    file.take(33).read_to_end(&mut seed).ok()?;
    let key = parse(&seed, expected);
    seed.fill(0);
    key
}
pub fn parse(seed: &[u8], expected: &[u8; 32]) -> Option<Ed25519KeyPair> {
    if seed.len() != 32 {
        return None;
    }
    Ed25519KeyPair::from_seed_unchecked(seed)
        .ok()
        .filter(|key| key.public_key().as_ref() == expected)
}
pub fn load(name: &str, expected: &[u8; 32]) -> Option<Ed25519KeyPair> {
    if let Some(vault) = bundled_vault() {
        let seed = UNLOCKED.get()?;
        if expected == &vault.expected && name == format!("{}.key", vault.identity) {
            return parse(seed.as_ref(), expected);
        }
    }
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(name)));
    let profile = crate::settings::Settings::path()
        .parent()
        .map(|parent| parent.join("keys").join(name));
    executable
        .into_iter()
        .chain(profile)
        .find_map(|path| read_key(&path, expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_require_the_exact_expected_key_and_bounded_regular_files() {
        let seed = [19; 32];
        let key = Ed25519KeyPair::from_seed_unchecked(&seed).unwrap();
        let expected: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
        let path = std::env::temp_dir().join(format!("tankyard-key-test-{}", std::process::id()));
        std::fs::write(&path, seed).unwrap();
        assert!(read_key(&path, &expected).is_some());
        assert!(read_key(&path, &tankyard_protocol::CREATOR_PUBLIC_KEY).is_none());
        assert!(read_key(&path, &tankyard_protocol::CENTRAL_OWNER_PUBLIC_KEY).is_none());
        std::fs::write(&path, [19; 33]).unwrap();
        assert!(read_key(&path, &expected).is_none());
        std::fs::remove_file(path).unwrap();
        assert!(parse(&seed[..31], &expected).is_none());
    }
}
