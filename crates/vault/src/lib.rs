//! Local vault formats: key separation, sealed records and metadata MACs. No I/O: the root
//! key is stored by a `KeyStore` adapter (`keyharbor-keystore`).
//! Format and constants follow docs/design/07-cryptography-design.md.

pub mod keys;
pub mod meta;
pub mod record;

/// The vault error, defined with the key-store port in `keyharbor-domain`.
pub use keyharbor_domain::secret::VaultError;
