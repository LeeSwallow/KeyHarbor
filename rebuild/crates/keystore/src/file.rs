//! INSECURE, tests only: root keys as plaintext files (0600) in one directory.

use crate::to_key;
use keyharbor_domain::{
    ports::KeyStore,
    secret::{SecretKey, VaultError},
};
use std::path::PathBuf;
use zeroize::Zeroizing;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::exercise;

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
        store
            .create("acct", &keyharbor_crypto::random::key())
            .unwrap();
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
        assert!(
            store
                .create("../escape", &keyharbor_crypto::random::key())
                .is_err()
        );
    }
}
