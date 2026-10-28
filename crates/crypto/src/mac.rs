//! HMAC-SHA256 over caller-built input, with a constant-time check.

use crate::SecretKey;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

fn keyed(key: &SecretKey, input: &[u8]) -> Hmac<Sha256> {
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(key.expose()).expect("HMAC takes any key length");
    mac.update(input);
    mac
}

pub fn mac(key: &SecretKey, input: &[u8]) -> [u8; 32] {
    keyed(key, input).finalize().into_bytes().into()
}

/// Whether `tag` is the MAC of `input`, compared in constant time.
pub fn verify_mac(key: &SecretKey, input: &[u8], tag: &[u8]) -> bool {
    keyed(key, input).verify_slice(tag).is_ok()
}
