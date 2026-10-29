//! The app's composition root: the data folder, key store, clock and the CLI binary that
//! runs session supervisors (`Contents/Helpers/keyharbor` in the bundle, R-45).

use crate::clipboard::Clipboard;
use anyhow::Result;
use keyharbor_core::Vault;
use keyharbor_domain::ports::{Clock, KeyStore, SessionRuntime, Store};
use keyharbor_sqlite::SqliteStore;
use keyharbor_supervisor::ProcessRuntime;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The `Clock` port over the system clock (the CLI's composition root has the same adapter).
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Makes the key store each vault opening reads the root key from.
pub type KeyStores = Arc<dyn Fn() -> Box<dyn KeyStore> + Send + Sync>;

/// What every command runs on.
pub struct Ctx {
    root: PathBuf,
    keys: KeyStores,
    clock: Arc<dyn Clock>,
    /// The `keyharbor` binary that supervisors are started from.
    cli: PathBuf,
    clipboard: Arc<dyn Clipboard>,
}

impl Ctx {
    pub fn new(
        root: PathBuf,
        keys: KeyStores,
        clock: Arc<dyn Clock>,
        cli: PathBuf,
        clipboard: Arc<dyn Clipboard>,
    ) -> Self {
        Self {
            root,
            keys,
            clock,
            cli,
            clipboard,
        }
    }
    pub fn clipboard(&self) -> Arc<dyn Clipboard> {
        self.clipboard.clone()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn clock(&self) -> &dyn Clock {
        &*self.clock
    }
    pub fn store(&self) -> Result<Box<dyn Store>> {
        Ok(Box::new(SqliteStore::open(&self.root)?))
    }
    pub fn vault(&self) -> Result<Vault> {
        Vault::open(
            SqliteStore::open(&self.root)?,
            (self.keys)(),
            self.clock.clone(),
        )
    }
    pub fn runtime(&self) -> Box<dyn SessionRuntime> {
        Box::new(ProcessRuntime::new(&self.root, &self.cli))
    }
}
