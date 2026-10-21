//! Password-encrypted identity seeds. Neither a password nor a plaintext seed is embedded.
use ring::{
    aead, pbkdf2,
    rand::{SecureRandom, SystemRandom},
    signature::{Ed25519KeyPair, KeyPair},
};
use std::num::NonZeroU32;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"TYVAULT1";
pub const ENVELOPE_BYTES: usize = 84;
const ROUNDS: u32 = 600_000;
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Envelope,
    Password,
    Credential,
    Random,
}
fn derive(password: &str, salt: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0; 32]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(ROUNDS).unwrap(),
        salt,
        password.as_bytes(),
        key.as_mut(),
    );
    key
}
fn aad(expected: &[u8; 32]) -> [u8; 40] {
    let mut aad = [0; 40];
    aad[..8].copy_from_slice(MAGIC);
    aad[8..].copy_from_slice(expected);
    aad
}
pub fn seal(
    seed: &[u8; 32],
    password: &str,
    expected: &[u8; 32],
) -> Result<[u8; ENVELOPE_BYTES], Error> {
    if password.len() < 24 || password.len() > 256 {
        return Err(Error::Password);
    }
    let keypair = Ed25519KeyPair::from_seed_unchecked(seed).map_err(|_| Error::Credential)?;
    if keypair.public_key().as_ref() != expected {
        return Err(Error::Credential);
    }
    let mut envelope = [0; ENVELOPE_BYTES];
    envelope[..8].copy_from_slice(MAGIC);
    SystemRandom::new()
        .fill(&mut envelope[8..36])
        .map_err(|_| Error::Random)?;
    let key = derive(password, &envelope[8..24]);
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key.as_ref()).map_err(|_| Error::Credential)?,
    );
    let nonce: [u8; 12] = envelope[24..36].try_into().map_err(|_| Error::Envelope)?;
    let mut plaintext = Zeroizing::new(seed.to_vec());
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(aad(expected)),
        &mut *plaintext,
    )
    .map_err(|_| Error::Credential)?;
    envelope[36..].copy_from_slice(&plaintext);
    Ok(envelope)
}
pub fn open(
    envelope: &[u8],
    password: &str,
    expected: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, Error> {
    if envelope.len() != ENVELOPE_BYTES || &envelope[..8] != MAGIC {
        return Err(Error::Envelope);
    }
    if password.is_empty() || password.len() > 256 {
        return Err(Error::Password);
    }
    let key = derive(password, &envelope[8..24]);
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, key.as_ref()).map_err(|_| Error::Credential)?,
    );
    let nonce: [u8; 12] = envelope[24..36].try_into().map_err(|_| Error::Envelope)?;
    let mut ciphertext = Zeroizing::new(envelope[36..].to_vec());
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad(expected)),
            &mut ciphertext,
        )
        .map_err(|_| Error::Password)?;
    let mut seed = Zeroizing::new([0; 32]);
    seed.copy_from_slice(plaintext);
    let keypair =
        Ed25519KeyPair::from_seed_unchecked(seed.as_ref()).map_err(|_| Error::Credential)?;
    if keypair.public_key().as_ref() != expected {
        return Err(Error::Credential);
    }
    Ok(seed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_credential_requires_password_identity_and_authentic_envelope() {
        let seed = [71; 32];
        let key = Ed25519KeyPair::from_seed_unchecked(&seed).unwrap();
        let expected = key.public_key().as_ref().try_into().unwrap();
        let password = "Random-test-password-that-is-not-used-for-any-real-identity";
        let sealed = seal(&seed, password, &expected).unwrap();
        assert!(!sealed.windows(seed.len()).any(|part| part == seed));
        assert_eq!(*open(&sealed, password, &expected).unwrap(), seed);
        assert_eq!(
            open(&sealed, "wrong-password", &expected).unwrap_err(),
            Error::Password
        );
        assert!(open(&sealed, password, &[0; 32]).is_err());
        let mut changed = sealed;
        changed[48] ^= 1;
        assert!(open(&changed, password, &expected).is_err());
        assert!(open(&sealed[..83], password, &expected).is_err());
        assert_ne!(sealed, seal(&seed, password, &expected).unwrap());
        assert!(seal(&seed, "short", &expected).is_err());
    }
}
