//! Root key storage. The real store is the macOS login keychain (`keyharbor-vault`).

use crate::secret::{SecretKey, VaultError};

/// Creates never overwrite; reads never create. A missing key is `KeyNotFound`.
pub trait KeyStore: Send {
    fn create(&self, account: &str, key: &SecretKey) -> Result<(), VaultError>;
    fn get(&self, account: &str) -> Result<SecretKey, VaultError>;
    fn replace(&self, account: &str, key: &SecretKey) -> Result<(), VaultError>;
    fn delete(&self, account: &str) -> Result<(), VaultError>;
}
