//! Sealed record format v1 (docs/design/07 §7.6):
//! `version ‖ suite ‖ key_id(16) ‖ nonce ‖ ciphertext ‖ tag`. The associated data a record
//! is bound to is built by the caller from the header, so the header is authenticated too.

use crate::{SecretKey, VaultError, aead::Suite, random};
use zeroize::Zeroizing;

pub const VERSION: u8 = 0x01;
/// `version ‖ suite ‖ key_id`.
pub const HEADER_LEN: usize = 2 + 16;
const SUITE: Suite = Suite::XChaCha20Poly1305;

fn header(key_id: &[u8; 16]) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[0] = VERSION;
    out[1] = SUITE.as_byte();
    out[2..].copy_from_slice(key_id);
    out
}

/// Seals `plaintext` under `key` with a fresh random nonce. `ad(header)` gives the
/// associated data.
pub fn seal(
    key: &SecretKey,
    key_id: &[u8; 16],
    ad: impl FnOnce(&[u8]) -> Vec<u8>,
    plaintext: &[u8],
) -> Vec<u8> {
    seal_with_nonce(key, key_id, ad, plaintext, &random::nonce(SUITE))
}

/// `seal` with a given nonce: for known-answer tests only.
#[doc(hidden)]
pub fn seal_with_nonce(
    key: &SecretKey,
    key_id: &[u8; 16],
    ad: impl FnOnce(&[u8]) -> Vec<u8>,
    plaintext: &[u8],
    nonce: &[u8],
) -> Vec<u8> {
    let header = header(key_id);
    let ciphertext = SUITE.encrypt(key, nonce, &ad(&header), plaintext);
    let mut out = Vec::with_capacity(HEADER_LEN + nonce.len() + ciphertext.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ciphertext);
    out
}

/// Opens a record sealed by [`seal`] with the same `ad`.
pub fn open(
    key: &SecretKey,
    record: &[u8],
    ad: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    if record.len() < HEADER_LEN {
        return Err(VaultError::Format("record too short"));
    }
    if record[0] != VERSION {
        return Err(VaultError::Format("unknown record version"));
    }
    let suite = Suite::from_byte(record[1]).ok_or(VaultError::Format("unknown cipher suite"))?;
    let body = HEADER_LEN + suite.nonce_len();
    if record.len() < body + suite.tag_len() {
        return Err(VaultError::Format("record too short"));
    }
    let aad = ad(&record[..HEADER_LEN]);
    suite.decrypt(key, &record[HEADER_LEN..body], &aad, &record[body..])
}

/// The key id in a record's header, read without decrypting.
pub fn key_id(record: &[u8]) -> Result<[u8; 16], VaultError> {
    record
        .get(2..HEADER_LEN)
        .and_then(|id| id.try_into().ok())
        .ok_or(VaultError::Format("record too short"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_open_only_with_their_key_and_ad() {
        let key = random::key();
        let record = seal(
            &key,
            &[7; 16],
            |h| [b"ad".as_slice(), h].concat(),
            b"KH-SYNTH",
        );
        let opened = open(&key, &record, |h| [b"ad".as_slice(), h].concat()).unwrap();
        assert_eq!(&*opened, b"KH-SYNTH");
        assert_eq!(key_id(&record).unwrap(), [7; 16]);
        assert!(open(&key, &record, |h| [b"other".as_slice(), h].concat()).is_err());
        assert!(open(&random::key(), &record, |h| [b"ad".as_slice(), h].concat()).is_err());
        let mut header = record.clone();
        header[5] ^= 1;
        assert!(open(&key, &header, |h| [b"ad".as_slice(), h].concat()).is_err());
        let mut suite = record.clone();
        suite[1] = 9;
        assert!(matches!(
            open(&key, &suite, |h| h.to_vec()),
            Err(VaultError::Format(_))
        ));
        assert!(matches!(
            open(&key, &record[..30], |h| h.to_vec()),
            Err(VaultError::Format(_))
        ));
    }
}
