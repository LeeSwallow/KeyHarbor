//! OS file locks: one supervisor per session, one scoped MCP server per session.

use crate::paths;
use anyhow::{Context, Result, bail};
use keyharbor_domain::SessionId;
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
    thread,
    time::{Duration, Instant},
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

/// Like [`acquire_lock`], but keeps trying for `within`. The supervisor takes its lock this
/// way: `open` holds the same lock only while it reserves the session and releases it before
/// the supervisor starts, and a second `open` may briefly hold it to find the session already
/// starting. Without the retry both opens fail (#28). A supervisor that really is running
/// keeps the lock, so this still fails, and the reservation check stops a stale one anyway.
pub fn acquire_lock_within(path: &Path, within: Duration) -> Result<File> {
    let deadline = Instant::now() + within;
    loop {
        match acquire_lock(path) {
            Ok(file) => return Ok(file),
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => thread::sleep(LOCK_RETRY),
        }
    }
}

const LOCK_RETRY: Duration = Duration::from_millis(20);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn a_lock_released_during_the_handoff_is_taken() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run").join("s.lock");
        let held = acquire_lock(&path).unwrap();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            drop(held);
        });
        assert!(acquire_lock_within(&path, Duration::from_secs(2)).is_ok());
        release.join().unwrap();
    }

    #[test]
    fn a_lock_held_past_the_handoff_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run").join("s.lock");
        let _held = acquire_lock(&path).unwrap();
        assert!(acquire_lock_within(&path, Duration::from_millis(100)).is_err());
    }
}
