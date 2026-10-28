use anyhow::{Context, Result, bail};
use keyharbor_domain::{Generation, Session, SessionId, SessionName, State};
use rusqlite::{Connection, params};
use std::{fs, path::Path, time::Duration};

pub struct SqliteStore {
    conn: Connection,
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
        conn.execute_batch("CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, state TEXT NOT NULL CHECK(state IN ('stopped','starting','running','awaiting_user','stopping','failed')));
        CREATE TABLE IF NOT EXISTS generations (session_id TEXT PRIMARY KEY, generation TEXT NOT NULL);")?;
        Ok(Self { conn })
    }
    pub fn create(&self, name: SessionName) -> Result<Session> {
        let id = SessionId::new();
        self.conn.execute(
            "INSERT INTO sessions (id,name,state) VALUES (?1,?2,'stopped')",
            params![id.to_string(), name.as_str()],
        )?;
        Ok(Session {
            id,
            name,
            state: State::Stopped,
        })
    }
    pub fn list(&self) -> Result<Vec<Session>> {
        let mut query = self
            .conn
            .prepare("SELECT id,name,state FROM sessions ORDER BY name")?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, state) = row?;
            Ok(Session {
                id: SessionId::parse(&id).context("invalid stored session id")?,
                name: SessionName::parse(&name).context("invalid stored session name")?,
                state: State::parse(&state).context("invalid stored session state")?,
            })
        })
        .collect()
    }
    pub fn reserve(&self, id: SessionId) -> Result<Option<Generation>> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let changed = tx.execute(
            "UPDATE sessions SET state='starting' WHERE id=?1 AND state='stopped'",
            [id.to_string()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let generation = Generation::new();
        tx.execute("INSERT INTO generations(session_id,generation) VALUES (?1,?2) ON CONFLICT(session_id) DO UPDATE SET generation=excluded.generation", params![id.to_string(),generation.to_string()])?;
        tx.commit()?;
        Ok(Some(generation))
    }
    pub fn transition(
        &self,
        id: SessionId,
        generation: Generation,
        from: State,
        to: State,
    ) -> Result<bool> {
        if !from.permits(to) {
            bail!("invalid session transition");
        }
        Ok(self.conn.execute("UPDATE sessions SET state=?1 WHERE id=?2 AND state=?3 AND EXISTS (SELECT 1 FROM generations WHERE session_id=?2 AND generation=?4)", params![to.as_str(),id.to_string(),from.as_str(),generation.to_string()])? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    store.reserve(id).unwrap()
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
        let next = store.reserve(id).unwrap().unwrap();
        assert_ne!(generation, next);
        assert!(
            !store
                .transition(id, generation, State::Starting, State::Running)
                .unwrap()
        );
    }
}
