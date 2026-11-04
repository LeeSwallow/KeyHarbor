//! The `RotationLock` port: an exclusive OS lock on `<data dir>/vault.lock`, next to the
//! database whose keys it protects.

use crate::{SqliteStore, error::store_error};
use anyhow::{Context, Result};
use keyharbor_domain::ports::{RotationGuard, RotationLock, StoreError};
use std::fs::{File, TryLockError};

impl SqliteStore {
    fn rotation_file(&self) -> Result<File> {
        let path = self.root().join("vault.lock");
        File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("cannot open {}", path.display()))
    }
}

impl RotationLock for SqliteStore {
    fn lock_rotation(&self) -> Result<RotationGuard, StoreError> {
        let file = self.rotation_file().map_err(store_error)?;
        file.lock()
            .context("cannot take the vault rotation lock")
            .map_err(store_error)?;
        Ok(Box::new(file))
    }

    fn try_lock_rotation(&self) -> Result<Option<RotationGuard>, StoreError> {
        let file = self.rotation_file().map_err(store_error)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Box::new(file))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e)
                .context("cannot take the vault rotation lock")
                .map_err(store_error),
        }
    }
}
