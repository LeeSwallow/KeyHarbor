//! Local vault cryptography: key separation, sealed records, metadata MACs and key storage.
//! Format and constants follow docs/design/07-cryptography-design.md.

pub mod keys;
pub mod keystore;
pub mod meta;
pub mod record;

/// The vault error, defined with the key-store port in `keyharbor-domain`.
pub use keyharbor_domain::secret::VaultError;
