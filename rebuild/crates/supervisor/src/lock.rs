//! OS file locks: one supervisor per session, one scoped MCP server per session.

use crate::paths;
use anyhow::{Context, Result, bail};
use keyharbor_domain::SessionId;
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

/// Takes the exclusive lock at `path`, creating it. Fails at once if another process holds it.
pub fn acquire_lock(path: &Path) -> Result<File> {
    paths::private_dir(path.parent().context("invalid lock path")?)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => bail!("lock is held by another process"),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

/// True while some process holds `path` locked.
pub fn is_locked(path: &Path) -> bool {
    match File::open(path) {
        Ok(file) => matches!(file.try_lock(), Err(TryLockError::WouldBlock)),
        Err(_) => false,
    }
}

/// True while a supervisor holds this session's lock.
pub fn is_alive(data: &Path, id: &SessionId) -> bool {
    is_locked(&paths::lock_path(data, id))
}

/// Takes the lock that lets one scoped MCP server own the session (`mcp --session`).
pub fn lock_mcp_scope(data: &Path, id: &SessionId) -> Result<File> {
    acquire_lock(&paths::mcp_lock_path(data, id))
}
