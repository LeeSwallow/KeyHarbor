//! Key material and the vault error, shared by the key-store port and the crypto adapter.

use std::{error::Error, fmt};
use zeroize::Zeroizing;

/// A 256-bit symmetric key. Never printed; zeroized on drop.
#[derive(Clone)]
pub struct SecretKey(Zeroizing<[u8; 32]>);
impl SecretKey {
    pub fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; 32]);
        getrandom::fill(bytes.as_mut()).expect("OS random number generator failed");
        Self(bytes)
    }
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey([REDACTED])")
    }
}

#[derive(Debug)]
pub enum VaultError {
    /// Authentication failed. Deliberately does not say which part was wrong.
    Decrypt,
    Format(&'static str),
    KeyNotFound,
    KeyExists,
    Keystore(String),
    Tampered,
}
impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decrypt => f.write_str("decryption failed"),
            Self::Format(reason) => write!(f, "invalid record: {reason}"),
            Self::KeyNotFound => f.write_str("vault key not found"),
            Self::KeyExists => f.write_str("vault key already exists"),
            Self::Keystore(reason) => write!(f, "key store error: {reason}"),
            Self::Tampered => f.write_str("item metadata failed verification"),
        }
    }
}
impl Error for VaultError {}
