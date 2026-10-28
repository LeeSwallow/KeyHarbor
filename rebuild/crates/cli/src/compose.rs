//! The composition root (ADR-0014): the only place in this binary that picks adapters — the
//! SQLite store, the system key store, the supervisor process runtime. Command modules get
//! ports from [`App`] and call core and guard use cases with them.
//!
//! Nothing is built before a command asks for it: a session command never touches the key
//! store. A vault owns its store connection, so a command that needs both the vault and the
//! store (`run`) holds two connections (SQLite opens one per owner, J-48).

use anyhow::Result;
use keyharbor_core::Vault;
use keyharbor_domain::ports::{SessionRuntime, Store};
use keyharbor_sqlite::{SqliteStore, SqliteStores};
use keyharbor_supervisor::{Adapters, ProcessRuntime};
use keyharbor_vault::keystore::keystore_from_env;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct App {
    root: PathBuf,
}

impl App {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The data directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn store(&self) -> Result<Box<dyn Store>> {
        Ok(Box::new(SqliteStore::open(&self.root)?))
    }

    pub fn vault(&self) -> Result<Vault> {
        Vault::open(SqliteStore::open(&self.root)?, keystore_from_env())
    }

    pub fn init_vault(&self) -> Result<Vault> {
        Vault::init(SqliteStore::open(&self.root)?, keystore_from_env())
    }

    /// Supervisors are this same binary, run as `__supervise`.
    pub fn runtime(&self) -> Result<Box<dyn SessionRuntime>> {
        Ok(Box::new(ProcessRuntime::new(
            &self.root,
            &std::env::current_exe()?,
        )))
    }

    /// What a supervisor runs with: this data directory's store, the system key store, and
    /// whether the browser runs headless (`KEYHARBOR_BROWSER_HEADLESS=1`).
    pub fn supervisor_adapters(&self) -> Adapters {
        let data = self.root.clone();
        Adapters {
            stores: Arc::new(SqliteStores::new(&self.root)),
            open_vault: Box::new(move || {
                Vault::open(SqliteStore::open(&data)?, keystore_from_env())
            }),
            headless: std::env::var_os("KEYHARBOR_BROWSER_HEADLESS").is_some_and(|v| v == "1"),
        }
    }

    /// The MCP server's dependencies: one store for every tool, and a vault opener for the
    /// verified item list.
    pub fn mcp_deps(&self) -> Result<keyharbor_mcp::Deps> {
        let data = self.root.clone();
        Ok(keyharbor_mcp::Deps {
            store: Arc::new(Mutex::new(self.store()?)),
            open_vault: Box::new(move || {
                Vault::open(SqliteStore::open(&data)?, keystore_from_env())
            }),
            runtime: self.runtime()?,
        })
    }
}
