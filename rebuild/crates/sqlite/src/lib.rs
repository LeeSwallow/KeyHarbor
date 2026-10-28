//! The SQLite adapter: implements the storage ports of `keyharbor_domain::ports` with Diesel.
//!
//! The schema is split into bounded contexts (`contexts/`), each owning its tables, models,
//! mappers, repository and migrations; `contexts::CONTEXTS` lists them in the order their
//! migrations run when a store is opened. Raw SQL lives only in `migrations/` (and the few
//! statements Diesel's DSL cannot express, inside a context's `repo.rs`).

mod connection;
mod contexts;

use anyhow::Result;
use diesel::SqliteConnection;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};

pub use keyharbor_domain::ports::AUDIT_ENABLED;

/// One connection to `<root>/keyharbor.sqlite3`. Not shared between threads; open one per thread.
pub struct SqliteStore {
    conn: RefCell<SqliteConnection>,
    root: PathBuf,
}

impl SqliteStore {
    /// Opens (creating if needed) the database in the private data directory `root` and runs
    /// every context's pending migrations.
    pub fn open(root: &Path) -> Result<Self> {
        let conn = connection::open(root)?;
        Ok(Self {
            conn: RefCell::new(conn),
            root: root.to_path_buf(),
        })
    }
    /// The data directory this store was opened from.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Runs `work` on the connection (one statement or a read).
    pub(crate) fn read<T>(
        &self,
        work: impl FnOnce(&mut SqliteConnection) -> Result<T>,
    ) -> Result<T> {
        work(&mut self.conn.borrow_mut())
    }
    /// Runs `work` in a `BEGIN IMMEDIATE` transaction: the write lock is taken up front, so
    /// concurrent writers wait (busy timeout) instead of failing on lock upgrade. An error
    /// rolls everything back.
    pub(crate) fn write<T>(
        &self,
        work: impl FnOnce(&mut SqliteConnection) -> Result<T>,
    ) -> Result<T> {
        self.conn.borrow_mut().immediate_transaction(work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::ports::SessionStore;
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

    /// A database from a build before migrations (and before `generations.cdp`) opens
    /// unchanged: the initial migrations find their tables and the session context adds the
    /// missing column.
    #[test]
    fn databases_from_before_migrations_open() {
        use diesel::{Connection, connection::SimpleConnection};
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir(&root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let id = keyharbor_domain::SessionId::new();
        let mut old =
            SqliteConnection::establish(root.join("keyharbor.sqlite3").to_str().unwrap()).unwrap();
        old.batch_execute(&format!(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, state TEXT NOT NULL CHECK(state IN ('stopped','starting','running','awaiting_user','stopping','failed')));
            CREATE TABLE generations (session_id TEXT PRIMARY KEY, generation TEXT NOT NULL);
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO sessions VALUES ('{id}', 'old', 'stopped');
            INSERT INTO settings VALUES ('audit_enabled', 'false');"
        ))
        .unwrap();
        drop(old);
        let store = SqliteStore::open(&root).unwrap();
        let rows = store.list().unwrap();
        assert_eq!((rows[0].id, rows[0].cdp), (id, false));
        let generation = store.reserve(id, true).unwrap().unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().generation, Some(generation));
        assert!(store.cdp_enabled(id).unwrap());
        use keyharbor_domain::ports::SettingsStore;
        assert_eq!(
            store.setting_get(AUDIT_ENABLED).unwrap().as_deref(),
            Some("false")
        );
        drop(store);
        SqliteStore::open(&root).unwrap();
    }
}
