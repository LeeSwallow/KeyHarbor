//! Randomness from the operating system's generator.

use crate::{SecretKey, aead::Suite};
use zeroize::Zeroizing;

/// Fills `out`. Panics if the OS generator fails: nothing safe can be done without it.
pub fn fill(out: &mut [u8]) {
    getrandom::fill(out).expect("OS random number generator failed");
}

/// A new 256-bit key.
pub fn key() -> SecretKey {
    let mut bytes = Zeroizing::new([0u8; 32]);
    fill(bytes.as_mut());
    SecretKey::from_bytes(*bytes)
}

/// A new nonce for `suite`.
pub fn nonce(suite: Suite) -> Vec<u8> {
    let mut nonce = vec![0u8; suite.nonce_len()];
    fill(&mut nonce);
    nonce
}
