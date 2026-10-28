//! The `RotationLock` port: an exclusive OS lock on `<data dir>/vault.lock`, next to the
//! database whose keys it protects.

use crate::SqliteStore;
use anyhow::{Context, Result};
use keyharbor_domain::ports::{RotationGuard, RotationLock};
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
    fn lock_rotation(&self) -> Result<RotationGuard> {
        let file = self.rotation_file()?;
        file.lock().context("cannot take the vault rotation lock")?;
        Ok(Box::new(file))
    }

    fn try_lock_rotation(&self) -> Result<Option<RotationGuard>> {
        let file = self.rotation_file()?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Box::new(file))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e).context("cannot take the vault rotation lock"),
        }
    }
}
