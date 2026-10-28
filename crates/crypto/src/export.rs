//! Passphrase encryption in the age format (07 §7.7): exports any age tool can open.

use anyhow::{Result, anyhow};
use zeroize::Zeroizing;

/// A passphrase, zeroized on drop and never printed.
pub struct Passphrase(age::secrecy::SecretString);

impl Passphrase {
    pub fn new(value: String) -> Self {
        Self(age::secrecy::SecretString::from(value))
    }
}

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Passphrase([REDACTED])")
    }
}

/// `plaintext` as an age file for the scrypt recipient of `passphrase`.
pub fn encrypt_age(passphrase: Passphrase, plaintext: &[u8]) -> Result<Vec<u8>> {
    age::encrypt(&age::scrypt::Recipient::new(passphrase.0), plaintext)
        .map_err(|e| anyhow!("age encryption failed: {e}"))
}

/// The plaintext of an age file encrypted for `passphrase`.
pub fn decrypt_age(passphrase: Passphrase, encrypted: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    age::decrypt(&age::scrypt::Identity::new(passphrase.0), encrypted)
        .map(Zeroizing::new)
        .map_err(|_| anyhow!("wrong passphrase or not a KeyHarbor export"))
}
