//! The macOS login keychain: generic password items, service [`KEYCHAIN_SERVICE`]. On other
//! systems the type exists so the binary builds, and every call fails.

use crate::KEYCHAIN_SERVICE;
use keyharbor_domain::{
    ports::KeyStore,
    secret::{SecretKey, VaultError},
};

/// The system key store (the login keychain on macOS).
pub struct KeychainKeyStore;

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use crate::to_key;
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };
    use zeroize::Zeroizing;

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    fn map(e: security_framework::base::Error) -> VaultError {
        if e.code() == ERR_SEC_ITEM_NOT_FOUND {
            VaultError::KeyNotFound
        } else {
            VaultError::Keystore(e.to_string())
        }
    }

    impl KeyStore for KeychainKeyStore {
        fn create(&self, account: &str, key: &SecretKey) -> Result<(), VaultError> {
            match self.get(account) {
                Ok(_) => Err(VaultError::KeyExists),
                Err(VaultError::KeyNotFound) => {
                    set_generic_password(KEYCHAIN_SERVICE, account, key.expose()).map_err(map)
                }
                Err(e) => Err(e),
            }
        }
        fn get(&self, account: &str) -> Result<SecretKey, VaultError> {
            let bytes =
                Zeroizing::new(get_generic_password(KEYCHAIN_SERVICE, account).map_err(map)?);
            to_key(&bytes)
        }
        fn replace(&self, account: &str, key: &SecretKey) -> Result<(), VaultError> {
            self.get(account)?;
            set_generic_password(KEYCHAIN_SERVICE, account, key.expose()).map_err(map)
        }
        fn delete(&self, account: &str) -> Result<(), VaultError> {
            delete_generic_password(KEYCHAIN_SERVICE, account).map_err(map)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod other {
    use super::*;

    fn unsupported() -> VaultError {
        VaultError::Keystore(format!(
            "KeyHarbor currently supports only the macOS keychain ({KEYCHAIN_SERVICE})"
        ))
    }

    impl KeyStore for KeychainKeyStore {
        fn create(&self, _: &str, _: &SecretKey) -> Result<(), VaultError> {
            Err(unsupported())
        }
        fn get(&self, _: &str) -> Result<SecretKey, VaultError> {
            Err(unsupported())
        }
        fn replace(&self, _: &str, _: &SecretKey) -> Result<(), VaultError> {
            Err(unsupported())
        }
        fn delete(&self, _: &str) -> Result<(), VaultError> {
            Err(unsupported())
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "touches the real login keychain and may show a macOS prompt"]
    fn mac_keychain_follows_create_get_replace_delete_rules() {
        let id = keyharbor_crypto::random::uuid_v4();
        let account = format!(
            "test-{}",
            id.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        crate::tests::exercise(&super::KeychainKeyStore, &account);
    }
}
