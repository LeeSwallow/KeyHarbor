//! Root key storage. The real store is the macOS login keychain; `FileKeyStore` exists for tests.

use crate::{VaultError, keys::SecretKey};
use std::path::PathBuf;
use zeroize::Zeroizing;

pub const KEYCHAIN_SERVICE: &str = "io.github.leeswallow.keyharbor";
pub const TEST_KEYSTORE_ENV: &str = "KEYHARBOR_TEST_KEYSTORE_DIR";

/// Creates never overwrite; reads never create. A missing key is `KeyNotFound`.
pub trait KeyStore: Send {
    fn create(&self, account: &str, key: &SecretKey) -> Result<(), VaultError>;
    fn get(&self, account: &str) -> Result<SecretKey, VaultError>;
    fn replace(&self, account: &str, key: &SecretKey) -> Result<(), VaultError>;
    fn delete(&self, account: &str) -> Result<(), VaultError>;
}

fn to_key(bytes: &[u8]) -> Result<SecretKey, VaultError> {
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| VaultError::Keystore("stored key has the wrong length".into()))?;
    Ok(SecretKey::from_bytes(array))
}

/// INSECURE: tests only — stores the root key in plaintext.
pub struct FileKeyStore {
    dir: PathBuf,
}
impl FileKeyStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
    fn path(&self, account: &str) -> Result<PathBuf, VaultError> {
        if account.is_empty()
            || !account
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'#')
        {
            return Err(VaultError::Keystore("invalid account name".into()));
        }
        Ok(self.dir.join(format!("{account}.key")))
    }
    fn write(&self, account: &str, key: &SecretKey, create_new: bool) -> Result<(), VaultError> {
        use std::io::Write;
        let path = self.path(account)?;
        std::fs::create_dir_all(&self.dir).map_err(|e| VaultError::Keystore(e.to_string()))?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true);
        if create_new {
            options.create_new(true);
        } else {
            if !path.exists() {
                return Err(VaultError::KeyNotFound);
            }
            options.truncate(true);
        }
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => VaultError::KeyExists,
            _ => VaultError::Keystore(e.to_string()),
        })?;
        file.write_all(key.expose())
            .and_then(|_| file.sync_all())
            .map_err(|e| VaultError::Keystore(e.to_string()))
    }
}
impl KeyStore for FileKeyStore {
    fn create(&self, account: &str, key: &SecretKey) -> Result<(), VaultError> {
        self.write(account, key, true)
    }
    fn get(&self, account: &str) -> Result<SecretKey, VaultError> {
        match std::fs::read(self.path(account)?) {
            Ok(bytes) => to_key(&Zeroizing::new(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(VaultError::KeyNotFound),
            Err(e) => Err(VaultError::Keystore(e.to_string())),
        }
    }
    fn replace(&self, account: &str, key: &SecretKey) -> Result<(), VaultError> {
        self.write(account, key, false)
    }
    fn delete(&self, account: &str) -> Result<(), VaultError> {
        match std::fs::remove_file(self.path(account)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(VaultError::KeyNotFound),
            Err(e) => Err(VaultError::Keystore(e.to_string())),
        }
    }
}

/// Generic password items in the login keychain, service `io.github.leeswallow.keyharbor`.
#[cfg(target_os = "macos")]
pub struct MacKeychain;

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    fn map(e: security_framework::base::Error) -> VaultError {
        if e.code() == ERR_SEC_ITEM_NOT_FOUND {
            VaultError::KeyNotFound
        } else {
            VaultError::Keystore(e.to_string())
        }
    }

    impl KeyStore for MacKeychain {
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

/// The plaintext test key directory, honoured only when built with `insecure-test-keystore`.
pub fn test_keystore_dir() -> Option<PathBuf> {
    #[cfg(feature = "insecure-test-keystore")]
    {
        std::env::var_os(TEST_KEYSTORE_ENV).map(PathBuf::from)
    }
    #[cfg(not(feature = "insecure-test-keystore"))]
    {
        None
    }
}

/// The macOS keychain, or the insecure file store in test builds that set `KEYHARBOR_TEST_KEYSTORE_DIR`.
pub fn keystore_from_env() -> Box<dyn KeyStore> {
    if let Some(dir) = test_keystore_dir() {
        eprintln!(
            "warning: INSECURE test key store in use ({TEST_KEYSTORE_ENV}); the root key is stored in plaintext"
        );
        return Box::new(FileKeyStore::new(dir));
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(MacKeychain)
    }
    #[cfg(not(target_os = "macos"))]
    {
        panic!("KeyHarbor currently supports only the macOS keychain")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VaultError, keys::SecretKey};

    fn exercise(store: &dyn KeyStore, account: &str) {
        let first = SecretKey::generate();
        assert!(matches!(store.get(account), Err(VaultError::KeyNotFound)));
        assert!(matches!(
            store.replace(account, &first),
            Err(VaultError::KeyNotFound)
        ));
        store.create(account, &first).unwrap();
        assert_eq!(store.get(account).unwrap().expose(), first.expose());
        assert!(matches!(
            store.create(account, &SecretKey::generate()),
            Err(VaultError::KeyExists)
        ));
        assert_eq!(store.get(account).unwrap().expose(), first.expose());
        let second = SecretKey::generate();
        store.replace(account, &second).unwrap();
        assert_eq!(store.get(account).unwrap().expose(), second.expose());
        store.delete(account).unwrap();
        assert!(matches!(store.get(account), Err(VaultError::KeyNotFound)));
        assert!(matches!(
            store.delete(account),
            Err(VaultError::KeyNotFound)
        ));
    }

    #[test]
    fn file_keystore_follows_create_get_replace_delete_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FileKeyStore::new(tmp.path().join("keys"));
        exercise(&store, "0b3f0c7e-1111-4222-8333-444455556666");
        exercise(&store, "0b3f0c7e-1111-4222-8333-444455556666#next");
    }

    #[cfg(unix)]
    #[test]
    fn file_keystore_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let store = FileKeyStore::new(tmp.path().join("keys"));
        store.create("acct", &SecretKey::generate()).unwrap();
        let mode = std::fs::metadata(tmp.path().join("keys/acct.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn file_keystore_rejects_path_like_accounts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FileKeyStore::new(tmp.path().join("keys"));
        assert!(store.create("../escape", &SecretKey::generate()).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "touches the real login keychain and may show a macOS prompt"]
    fn mac_keychain_follows_create_get_replace_delete_rules() {
        let account = format!("test-{}", uuid::Uuid::new_v4());
        exercise(&MacKeychain, &account);
    }

    #[cfg(not(feature = "insecure-test-keystore"))]
    #[test]
    fn test_keystore_dir_is_ignored_without_the_feature() {
        // SAFETY: no other test in this crate reads or writes this variable.
        unsafe { std::env::set_var(TEST_KEYSTORE_ENV, "/tmp/should-not-be-used") };
        assert!(test_keystore_dir().is_none());
        unsafe { std::env::remove_var(TEST_KEYSTORE_ENV) };
    }

    #[test]
    fn keychain_service_name_is_final() {
        assert_eq!(KEYCHAIN_SERVICE, "io.github.leeswallow.keyharbor");
    }
}
