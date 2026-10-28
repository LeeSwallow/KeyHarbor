//! Authenticated encryption, chosen by the suite byte a record carries (crypto agility:
//! a new suite is one variant here).

use crate::{SecretKey, VaultError};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suite {
    /// `0x01`: XChaCha20-Poly1305, 24-byte nonce, 16-byte tag.
    XChaCha20Poly1305,
}

impl Suite {
    pub fn as_byte(self) -> u8 {
        match self {
            Self::XChaCha20Poly1305 => 0x01,
        }
    }
    pub fn from_byte(value: u8) -> Option<Self> {
        (value == 0x01).then_some(Self::XChaCha20Poly1305)
    }
    pub fn nonce_len(self) -> usize {
        match self {
            Self::XChaCha20Poly1305 => 24,
        }
    }
    pub fn tag_len(self) -> usize {
        match self {
            Self::XChaCha20Poly1305 => 16,
        }
    }

    /// `ciphertext ‖ tag`. `nonce` must be `nonce_len` bytes and never reused with `key`.
    pub fn encrypt(self, key: &SecretKey, nonce: &[u8], aad: &[u8], msg: &[u8]) -> Vec<u8> {
        match self {
            Self::XChaCha20Poly1305 => {
                let cipher = XChaCha20Poly1305::new_from_slice(key.expose()).expect("32-byte key");
                cipher
                    .encrypt(
                        &XNonce::try_from(nonce).expect("nonce of the suite length"),
                        Payload { msg, aad },
                    )
                    .expect("XChaCha20-Poly1305 encryption does not fail for in-memory input")
            }
        }
    }

    /// The plaintext, or `Decrypt` when anything (key, nonce, AD, ciphertext) is wrong.
    pub fn decrypt(
        self,
        key: &SecretKey,
        nonce: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        match self {
            Self::XChaCha20Poly1305 => {
                let cipher = XChaCha20Poly1305::new_from_slice(key.expose()).expect("32-byte key");
                cipher
                    .decrypt(
                        &XNonce::try_from(nonce).expect("nonce of the suite length"),
                        Payload {
                            msg: ciphertext,
                            aad,
                        },
                    )
                    .map(Zeroizing::new)
                    .map_err(|_| VaultError::Decrypt)
            }
        }
    }
}
