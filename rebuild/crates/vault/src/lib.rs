//! Local vault cryptography: key separation, sealed records, metadata MACs and key storage.
//! Format and constants follow docs/design/07-cryptography-design.md.

pub mod keys;
pub mod meta;
pub mod record;

use std::{error::Error, fmt};

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
