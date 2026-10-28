//! Every cryptographic primitive KeyHarbor uses, behind a small purpose-oriented API
//! (ADR-0016, docs/design/07). No other crate names an AEAD, KDF, MAC, random source or
//! the age library; `scripts/check-layers.sh` enforces it. This crate does no I/O.
//!
//! - [`aead`] — the cipher suites by their record byte (XChaCha20-Poly1305 only, for now).
//! - [`envelope`] — the sealed record format v1: header, nonce, ciphertext, AD binding.
//! - [`kdf`] — keys derived from the root key, one [`kdf::KeyPurpose`] each.
//! - [`mac`] — HMAC-SHA256 and its constant-time check.
//! - [`random`] — keys and nonces from the OS generator.
//! - [`export`] — passphrase encryption in the age format.

pub mod aead;
pub mod envelope;
pub mod export;
pub mod kdf;
pub mod mac;
pub mod random;

pub use keyharbor_domain::secret::{SecretKey, VaultError};
