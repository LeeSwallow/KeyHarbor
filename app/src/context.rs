//! What every command runs on: the data folder, key store, clock, the CLI binary that runs
//! session supervisors (`Contents/Helpers/keyharbor` in the bundle, R-45) and the
//! authenticator. Each binary's `main` builds it (the composition root).

use crate::{
    auth::{AuthScope, Authenticator},
    clipboard::Clipboard,
};
use anyhow::{Result, anyhow};
use keyharbor_core::Vault;
use keyharbor_domain::ports::{Clock, KeyStore, SessionRuntime, Store};
use keyharbor_sqlite::SqliteStore;
use keyharbor_supervisor::ProcessRuntime;
use std::{
    ffi::OsString,
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

/// The data folder the CLI uses: `KEYHARBOR_HOME`, else the macOS application data folder
/// under `HOME`. `var` reads the environment (`std::env::var_os` in `main`). Without an
/// absolute `HOME` it is an error, never a folder relative to wherever the app started (J-176).
pub fn data_dir(var: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    if let Some(dir) = var("KEYHARBOR_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home = var("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or_else(|| {
            anyhow!("HOME is not set to an absolute path; set HOME or KEYHARBOR_HOME")
        })?;
    Ok(home.join("Library/Application Support/KeyHarbor"))
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
    auth: Arc<dyn Authenticator>,
}

impl Ctx {
    pub fn new(
        root: PathBuf,
        keys: KeyStores,
        clock: Arc<dyn Clock>,
        cli: PathBuf,
        clipboard: Arc<dyn Clipboard>,
        auth: Arc<dyn Authenticator>,
    ) -> Self {
        Self {
            root,
            keys,
            clock,
            cli,
            clipboard,
            auth,
        }
    }
    /// The authentication method's name, for the screens.
    pub fn auth_id(&self) -> &'static str {
        self.auth.id()
    }
    /// Asks the authenticator before a command does anything for `scope` (03 §3.10).
    pub fn challenge(&self, scope: AuthScope) -> Result<()> {
        Ok(self.auth.challenge(scope)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn the_data_folder_is_keyharbor_home_when_set() {
        let dir = data_dir(env(&[("KEYHARBOR_HOME", "/tmp/kh"), ("HOME", "/Users/me")]));
        assert_eq!(dir.unwrap(), PathBuf::from("/tmp/kh"));
    }

    #[test]
    fn the_data_folder_is_under_home_otherwise() {
        let dir = data_dir(env(&[("HOME", "/Users/me")]));
        assert_eq!(
            dir.unwrap(),
            PathBuf::from("/Users/me/Library/Application Support/KeyHarbor")
        );
    }

    #[test]
    fn without_a_usable_home_there_is_no_data_folder() {
        for home in [&[][..], &[("HOME", "")], &[("HOME", "relative")]] {
            assert!(
                data_dir(env(home)).is_err(),
                "a vault is never opened at a relative path ({home:?})"
            );
        }
    }
}
