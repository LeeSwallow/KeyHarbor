//! The composition root (ADR-0014): the only place in this binary that picks adapters — the
//! SQLite store, the key store (`keystore_from_env`), the system clock, the supervisor process
//! runtime. Command modules get ports from [`App`] and call core and guard use cases with them.
//!
//! Nothing is built before a command asks for it: a session command never touches the key
//! store. A vault owns its store connection, so a command that needs both the vault and the
//! store (`run`) holds two connections (SQLite opens one per owner, J-48).

use anyhow::Result;
use keyharbor_core::Vault;
use keyharbor_domain::ports::{Clock, KeyStore, SessionRuntime, Store};
use keyharbor_keystore::{
    FileKeyStore, KeychainKeyStore, LocalAuthConfirm, TEST_KEYSTORE_ENV, TEST_PRESENCE_ENV,
    presence::FixedConfirm, test_keystore_dir, test_presence,
};
use keyharbor_sqlite::{SqliteStore, SqliteStores};
use keyharbor_supervisor::{Adapters, ProcessRuntime};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The `Clock` port over the system clock.
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

/// The macOS keychain, or the insecure file store in test builds that set
/// `KEYHARBOR_TEST_KEYSTORE_DIR`.
fn keystore_from_env() -> Box<dyn KeyStore> {
    if let Some(dir) = test_keystore_dir(std::env::var_os(TEST_KEYSTORE_ENV)) {
        eprintln!(
            "warning: INSECURE test key store in use ({TEST_KEYSTORE_ENV}); the root key is stored in plaintext"
        );
        return Box::new(FileKeyStore::new(dir));
    }
    Box::new(KeychainKeyStore)
}

pub struct App {
    root: PathBuf,
    clock: Arc<dyn Clock>,
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            clock: Arc::new(SystemClock),
        }
    }

    /// Test builds with a fixed Touch ID answer (`KEYHARBOR_TEST_PRESENCE`) also skip the check
    /// that the host was started by a browser; release builds never honour the variable.
    pub fn trusts_any_caller(&self) -> bool {
        test_presence(std::env::var_os(TEST_PRESENCE_ENV)).is_some()
    }

    /// How a person confirms an autofill (ADR-0017): Touch ID, or a fixed test answer in test
    /// builds that set `KEYHARBOR_TEST_PRESENCE`.
    pub fn presence(&self) -> Box<dyn keyharbor_domain::ports::Confirm> {
        match test_presence(std::env::var_os(TEST_PRESENCE_ENV)) {
            Some(answer) => Box::new(FixedConfirm(answer)),
            None => Box::new(LocalAuthConfirm),
        }
    }

    /// The data directory.
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
            keystore_from_env(),
            self.clock.clone(),
        )
    }

    pub fn init_vault(&self) -> Result<Vault> {
        Vault::init(
            SqliteStore::open(&self.root)?,
            keystore_from_env(),
            self.clock.clone(),
        )
    }

    /// Supervisors are this same binary, run as `__supervise`.
    pub fn runtime(&self) -> Result<Box<dyn SessionRuntime>> {
        Ok(Box::new(ProcessRuntime::new(
            &self.root,
            &std::env::current_exe()?,
        )))
    }

    /// Opens the vault on demand (a supervisor or the MCP server opens it per use).
    fn vault_opener(&self) -> keyharbor_guard::VaultOpener {
        let (data, clock) = (self.root.clone(), self.clock.clone());
        Box::new(move || {
            Vault::open(
                SqliteStore::open(&data)?,
                keystore_from_env(),
                clock.clone(),
            )
        })
    }

    /// What a supervisor runs with: this data directory's store, the system key store and
    /// clock, and whether the browser runs headless (`KEYHARBOR_BROWSER_HEADLESS=1`).
    pub fn supervisor_adapters(&self) -> Adapters {
        Adapters {
            stores: Arc::new(SqliteStores::new(&self.root)),
            open_vault: self.vault_opener(),
            clock: self.clock.clone(),
            headless: std::env::var_os("KEYHARBOR_BROWSER_HEADLESS").is_some_and(|v| v == "1"),
        }
    }

    /// The MCP server's dependencies: one store for every tool, and a vault opener for the
    /// verified item list.
    pub fn mcp_deps(&self) -> Result<keyharbor_mcp::Deps> {
        Ok(keyharbor_mcp::Deps {
            store: Arc::new(Mutex::new(self.store()?)),
            open_vault: self.vault_opener(),
            runtime: self.runtime()?,
            clock: self.clock.clone(),
        })
    }
}
