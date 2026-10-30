//! The key-store adapter (`KeyStore` port, #19): [`KeychainKeyStore`] is the system store,
//! [`FileKeyStore`] the plaintext store tests use. Which one a process uses is chosen by the
//! binary's composition root, which also reads [`TEST_KEYSTORE_ENV`]. [`LocalAuthConfirm`] is the
//! `Confirm` port as Touch ID, for autofill (ADR-0017).

mod file;
mod keychain;
pub mod presence;

pub use file::FileKeyStore;
pub use keychain::KeychainKeyStore;
pub use presence::{LocalAuthConfirm, TEST_PRESENCE_ENV, test_presence};

use keyharbor_domain::secret::{SecretKey, VaultError};
use std::{ffi::OsString, path::PathBuf};

pub const KEYCHAIN_SERVICE: &str = "io.github.leeswallow.keyharbor";
pub const TEST_KEYSTORE_ENV: &str = "KEYHARBOR_TEST_KEYSTORE_DIR";

/// The plaintext test key directory named by `value` (the [`TEST_KEYSTORE_ENV`] variable),
/// honoured only when built with `insecure-test-keystore`.
pub fn test_keystore_dir(value: Option<OsString>) -> Option<PathBuf> {
    if cfg!(feature = "insecure-test-keystore") {
        value.map(PathBuf::from)
    } else {
        None
    }
}

fn to_key(bytes: &[u8]) -> Result<SecretKey, VaultError> {
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| VaultError::Keystore("stored key has the wrong length".into()))?;
    Ok(SecretKey::from_bytes(array))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::ports::KeyStore;

    pub(crate) fn exercise(store: &dyn KeyStore, account: &str) {
        let first = keyharbor_crypto::random::key();
        assert!(matches!(store.get(account), Err(VaultError::KeyNotFound)));
        assert!(matches!(
            store.replace(account, &first),
            Err(VaultError::KeyNotFound)
        ));
        store.create(account, &first).unwrap();
        assert_eq!(store.get(account).unwrap().expose(), first.expose());
        assert!(matches!(
            store.create(account, &keyharbor_crypto::random::key()),
            Err(VaultError::KeyExists)
        ));
        assert_eq!(store.get(account).unwrap().expose(), first.expose());
        let second = keyharbor_crypto::random::key();
        store.replace(account, &second).unwrap();
        assert_eq!(store.get(account).unwrap().expose(), second.expose());
        store.delete(account).unwrap();
        assert!(matches!(store.get(account), Err(VaultError::KeyNotFound)));
        assert!(matches!(
            store.delete(account),
            Err(VaultError::KeyNotFound)
        ));
    }

    #[cfg(not(feature = "insecure-test-keystore"))]
    #[test]
    fn test_keystore_dir_is_ignored_without_the_feature() {
        assert!(test_keystore_dir(Some("/tmp/should-not-be-used".into())).is_none());
    }

    #[test]
    fn keychain_service_name_is_final() {
        assert_eq!(KEYCHAIN_SERVICE, "io.github.leeswallow.keyharbor");
    }
}
