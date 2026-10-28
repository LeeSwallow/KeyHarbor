mod audit_repo;
mod policy_repo;
mod session_repo;
mod vault_repo;
pub use audit_repo::AUDIT_ENABLED;
pub use session_repo::SessionRow;
pub use vault_repo::*;

use anyhow::{Result, bail};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct SqliteStore {
    conn: Connection,
    root: PathBuf,
}

impl SqliteStore {
    pub fn open(root: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(root)?;
            let metadata = fs::symlink_metadata(root)?;
            if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
                bail!("data directory must be a private directory");
            }
        }
        #[cfg(not(unix))]
        fs::create_dir_all(root)?;
        let conn = Connection::open(root.join("keyharbor.sqlite3"))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        session_repo::migrate(&conn)?;
        vault_repo::migrate(&conn)?;
        policy_repo::migrate(&conn)?;
        audit_repo::migrate(&conn)?;
        Ok(Self {
            conn,
            root: root.to_path_buf(),
        })
    }
    /// The data directory this store was opened from.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::{Generation, SessionName, State};
    #[test]
    fn sessions_persist_and_duplicate_names_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let first = SqliteStore::open(&root).unwrap();
        let session = first.create(SessionName::parse("work").unwrap()).unwrap();
        let second = SqliteStore::open(&root).unwrap();
        assert!(second.create(SessionName::parse("work").unwrap()).is_err());
        assert_eq!(second.list().unwrap()[0].id, session.id);
    }
    #[test]
    fn concurrent_reservation_has_one_owner_and_fences_old_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let store = SqliteStore::open(&root).unwrap();
        let id = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let root = root.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let store = SqliteStore::open(&root).unwrap();
                    barrier.wait();
                    store.reserve(id, false).unwrap()
                })
            })
            .collect();
        let owners: Vec<_> = workers
            .into_iter()
            .filter_map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(owners.len(), 1);
        let generation = owners[0];
        assert!(
            !store
                .transition(id, Generation::new(), State::Starting, State::Running)
                .unwrap()
        );
        assert!(
            store
                .transition(id, generation, State::Starting, State::Running)
                .unwrap()
        );
        assert!(
            store
                .transition(id, generation, State::Running, State::Stopped)
                .unwrap()
        );
        let next = store.reserve(id, false).unwrap().unwrap();
        assert_ne!(generation, next);
        assert!(
            !store
                .transition(id, generation, State::Starting, State::Running)
                .unwrap()
        );
    }
}
