//! Sessions, run generations and authentication handoff requests.
//!
//! Every state change is conditional on the session's current generation (compare-and-swap),
//! so a writer from an older run cannot move a newer one.

use crate::SqliteStore;
use anyhow::{Context, Result, bail};
use keyharbor_domain::ports::session::{SessionRow, SessionStore};
use keyharbor_domain::{
    Generation, HandoffStatus, RequestId, Session, SessionId, SessionName, State, vault::Origin,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, state TEXT NOT NULL CHECK(state IN ('stopped','starting','running','awaiting_user','stopping','failed')));
        CREATE TABLE IF NOT EXISTS generations (session_id TEXT PRIMARY KEY, generation TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS handoff_requests (request_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, generation TEXT NOT NULL, origin TEXT, status TEXT NOT NULL CHECK(status IN ('pending','resumed','superseded','cancelled')));
        CREATE INDEX IF NOT EXISTS handoff_requests_session ON handoff_requests(session_id, status);",
    )?;
    let has_cdp: bool = conn.query_row(
        "SELECT count(*) FROM pragma_table_info('generations') WHERE name='cdp'",
        [],
        |row| row.get::<_, i64>(0).map(|n| n == 1),
    )?;
    if !has_cdp {
        conn.execute_batch("ALTER TABLE generations ADD COLUMN cdp INTEGER NOT NULL DEFAULT 0")?;
    }
    Ok(())
}

const SELECT_ROW: &str = "SELECT s.id, s.name, s.state, g.generation, coalesce(g.cdp, 0) FROM sessions s LEFT JOIN generations g ON g.session_id = s.id";

fn row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(String, String, String, Option<String>, bool)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}
fn decode(raw: (String, String, String, Option<String>, bool)) -> Result<SessionRow> {
    let (id, name, state, generation, cdp) = raw;
    Ok(SessionRow {
        id: SessionId::parse(&id).context("invalid stored session id")?,
        name: SessionName::parse(&name).context("invalid stored session name")?,
        state: State::parse(&state).context("invalid stored session state")?,
        generation: generation
            .map(|g| Generation::parse(&g))
            .transpose()
            .context("invalid stored generation")?,
        cdp,
    })
}

/// Marks the session's pending handoff requests as cancelled.
fn cancel_pending(tx: &Transaction<'_>, id: SessionId) -> Result<()> {
    tx.execute(
        "UPDATE handoff_requests SET status='cancelled' WHERE session_id=?1 AND status='pending'",
        [id.to_string()],
    )?;
    Ok(())
}

impl SqliteStore {
    fn immediate(&self) -> Result<Transaction<'_>> {
        Ok(Transaction::new_unchecked(
            &self.conn,
            TransactionBehavior::Immediate,
        )?)
    }
}

impl SessionStore for SqliteStore {
    fn create(&self, name: SessionName) -> Result<Session> {
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

    fn list(&self) -> Result<Vec<SessionRow>> {
        let mut query = self
            .conn
            .prepare(&format!("{SELECT_ROW} ORDER BY s.name"))?;
        let rows = query.query_map([], row)?;
        rows.map(|raw| decode(raw?)).collect()
    }

    fn get(&self, id: SessionId) -> Result<Option<SessionRow>> {
        self.conn
            .query_row(
                &format!("{SELECT_ROW} WHERE s.id=?1"),
                [id.to_string()],
                row,
            )
            .optional()?
            .map(decode)
            .transpose()
    }

    /// Deletes a stopped session and its records. Returns false if it is not stopped or absent.
    fn delete(&self, id: SessionId) -> Result<bool> {
        let tx = self.immediate()?;
        let changed = tx.execute(
            "DELETE FROM sessions WHERE id=?1 AND state='stopped'",
            [id.to_string()],
        )?;
        if changed == 1 {
            tx.execute(
                "DELETE FROM generations WHERE session_id=?1",
                [id.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(changed == 1)
    }

    /// stopped → starting with a fresh generation. `None` if the session is not stopped.
    fn reserve(&self, id: SessionId, cdp: bool) -> Result<Option<Generation>> {
        let tx = self.immediate()?;
        let changed = tx.execute(
            "UPDATE sessions SET state='starting' WHERE id=?1 AND state='stopped'",
            [id.to_string()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        let generation = Generation::new();
        tx.execute(
            "INSERT INTO generations(session_id,generation,cdp) VALUES (?1,?2,?3) ON CONFLICT(session_id) DO UPDATE SET generation=excluded.generation, cdp=excluded.cdp",
            params![id.to_string(), generation.to_string(), cdp],
        )?;
        tx.commit()?;
        Ok(Some(generation))
    }

    /// Moves `from → to` only if the session is still in `from` under `generation`.
    /// Leaving `awaiting_user` for anything but `running` cancels the pending request.
    fn transition(
        &self,
        id: SessionId,
        generation: Generation,
        from: State,
        to: State,
    ) -> Result<bool> {
        if !from.permits(to) {
            bail!("invalid session transition");
        }
        let tx = self.immediate()?;
        let changed = tx.execute(
            "UPDATE sessions SET state=?1 WHERE id=?2 AND state=?3 AND EXISTS (SELECT 1 FROM generations WHERE session_id=?2 AND generation=?4)",
            params![to.as_str(), id.to_string(), from.as_str(), generation.to_string()],
        )? == 1;
        if changed && matches!(to, State::Stopping | State::Stopped | State::Failed) {
            cancel_pending(&tx, id)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Recovery: any non-stopped state → stopped, dropping the generation so the old run is
    /// fenced off. The caller must hold the session's supervisor lock. False if already stopped.
    fn force_stopped(&self, id: SessionId) -> Result<bool> {
        let tx = self.immediate()?;
        let changed = tx.execute(
            "UPDATE sessions SET state='stopped' WHERE id=?1 AND state<>'stopped'",
            [id.to_string()],
        )? == 1;
        if changed {
            tx.execute(
                "DELETE FROM generations WHERE session_id=?1",
                [id.to_string()],
            )?;
            cancel_pending(&tx, id)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// running|awaiting_user → awaiting_user with a new request; an older pending one is superseded.
    fn request_handoff(
        &self,
        id: SessionId,
        generation: Generation,
        origin: Option<&str>,
    ) -> Result<RequestId> {
        let origin = origin
            .map(Origin::parse)
            .transpose()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let tx = self.immediate()?;
        let changed = tx.execute(
            "UPDATE sessions SET state='awaiting_user' WHERE id=?1 AND state IN ('running','awaiting_user') AND EXISTS (SELECT 1 FROM generations WHERE session_id=?1 AND generation=?2)",
            params![id.to_string(), generation.to_string()],
        )?;
        if changed != 1 {
            bail!("session is not running under this generation");
        }
        tx.execute(
            "UPDATE handoff_requests SET status='superseded' WHERE session_id=?1 AND status='pending'",
            [id.to_string()],
        )?;
        let request = RequestId::new();
        tx.execute(
            "INSERT INTO handoff_requests(request_id,session_id,generation,origin,status) VALUES (?1,?2,?3,?4,'pending')",
            params![
                request.to_string(),
                id.to_string(),
                generation.to_string(),
                origin.as_ref().map(Origin::as_str)
            ],
        )?;
        tx.commit()?;
        Ok(request)
    }

    /// The request the user is asked to finish, with the claimed origin.
    fn pending_handoff(&self, id: SessionId) -> Result<Option<(RequestId, Option<String>)>> {
        let found: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT h.request_id, h.origin FROM handoff_requests h JOIN sessions s ON s.id=h.session_id JOIN generations g ON g.session_id=h.session_id AND g.generation=h.generation WHERE h.session_id=?1 AND h.status='pending' AND s.state='awaiting_user'",
                [id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        found
            .map(|(request, origin)| {
                Ok((
                    RequestId::parse(&request).context("invalid stored request id")?,
                    origin,
                ))
            })
            .transpose()
    }

    /// awaiting_user → running, only for the pending request of the current generation.
    fn resume_handoff(
        &self,
        id: SessionId,
        generation: Generation,
        request: RequestId,
    ) -> Result<()> {
        let tx = self.immediate()?;
        let changed = tx.execute(
            "UPDATE handoff_requests SET status='resumed' WHERE request_id=?1 AND session_id=?2 AND generation=?3 AND status='pending'",
            params![request.to_string(), id.to_string(), generation.to_string()],
        )?;
        if changed != 1 {
            bail!(
                "authentication request is not pending; inspect the current request with handoff"
            );
        }
        let changed = tx.execute(
            "UPDATE sessions SET state='running' WHERE id=?1 AND state='awaiting_user' AND EXISTS (SELECT 1 FROM generations WHERE session_id=?1 AND generation=?2)",
            params![id.to_string(), generation.to_string()],
        )?;
        if changed != 1 {
            bail!("session is no longer awaiting the user");
        }
        tx.commit()?;
        Ok(())
    }

    fn handoff_status(&self, id: SessionId, request: RequestId) -> Result<Option<HandoffStatus>> {
        let status: Option<String> = self
            .conn
            .query_row(
                "SELECT status FROM handoff_requests WHERE session_id=?1 AND request_id=?2",
                params![id.to_string(), request.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        status
            .map(|s| HandoffStatus::parse(&s).context("invalid stored handoff status"))
            .transpose()
    }
}
#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::ports::SessionStore;
    use keyharbor_domain::{Generation, HandoffStatus, RequestId, SessionName, State};

    fn store() -> (tempfile::TempDir, SqliteStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        (tmp, store)
    }
    fn running(store: &SqliteStore, name: &str) -> (keyharbor_domain::SessionId, Generation) {
        let id = store.create(SessionName::parse(name).unwrap()).unwrap().id;
        let generation = store.reserve(id, false).unwrap().unwrap();
        assert!(
            store
                .transition(id, generation, State::Starting, State::Running)
                .unwrap()
        );
        (id, generation)
    }

    #[test]
    fn get_and_list_report_generation_and_cdp() {
        let (_tmp, store) = store();
        let id = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        let row = store.get(id).unwrap().unwrap();
        assert_eq!(row.state, State::Stopped);
        assert_eq!(row.generation, None);
        assert!(!row.cdp);
        let generation = store.reserve(id, true).unwrap().unwrap();
        let row = store.get(id).unwrap().unwrap();
        assert_eq!(row.state, State::Starting);
        assert_eq!(row.generation, Some(generation));
        assert!(row.cdp);
        assert!(store.cdp_enabled(id).unwrap());
        assert_eq!(store.generation(id).unwrap(), Some(generation));
        assert_eq!(store.list().unwrap()[0].generation, Some(generation));
        assert!(
            store
                .get(keyharbor_domain::SessionId::new())
                .unwrap()
                .is_none()
        );
        assert!(store.reserve(id, false).unwrap().is_none(), "not stopped");
    }

    #[test]
    fn delete_only_removes_stopped_sessions() {
        let (_tmp, store) = store();
        let (id, generation) = running(&store, "busy");
        assert!(!store.delete(id).unwrap());
        assert!(
            store
                .transition(id, generation, State::Running, State::Stopped)
                .unwrap()
        );
        assert!(store.delete(id).unwrap());
        assert!(store.get(id).unwrap().is_none());
        assert!(!store.delete(id).unwrap());
    }

    #[test]
    fn handoff_request_supersede_and_resume() {
        let (_tmp, store) = store();
        let (id, generation) = running(&store, "handoff");
        assert!(
            store
                .request_handoff(id, generation, Some("https://example.com/login"))
                .is_err(),
            "origin with a path"
        );
        assert_eq!(store.get(id).unwrap().unwrap().state, State::Running);
        assert!(
            store.request_handoff(id, Generation::new(), None).is_err(),
            "stale generation"
        );
        let first = store
            .request_handoff(id, generation, Some("https://EXAMPLE.com:443/"))
            .unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().state, State::AwaitingUser);
        assert_eq!(
            store.pending_handoff(id).unwrap(),
            Some((first, Some("https://example.com".to_owned())))
        );
        assert_eq!(
            store.handoff_status(id, first).unwrap(),
            Some(HandoffStatus::Pending)
        );
        // A second request while awaiting replaces the first.
        let second = store.request_handoff(id, generation, None).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            store.handoff_status(id, first).unwrap(),
            Some(HandoffStatus::Superseded)
        );
        assert!(store.resume_handoff(id, generation, first).is_err());
        assert!(
            store
                .resume_handoff(id, generation, RequestId::new())
                .is_err()
        );
        assert!(store.resume_handoff(id, Generation::new(), second).is_err());
        assert_eq!(store.get(id).unwrap().unwrap().state, State::AwaitingUser);
        store.resume_handoff(id, generation, second).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().state, State::Running);
        assert_eq!(
            store.handoff_status(id, second).unwrap(),
            Some(HandoffStatus::Resumed)
        );
        assert_eq!(store.pending_handoff(id).unwrap(), None);
        assert!(
            store.resume_handoff(id, generation, second).is_err(),
            "twice"
        );
        assert_eq!(store.handoff_status(id, RequestId::new()).unwrap(), None);
    }

    #[test]
    fn stopping_cancels_pending_handoff() {
        let (_tmp, store) = store();
        let (id, generation) = running(&store, "cancel");
        let request = store.request_handoff(id, generation, None).unwrap();
        assert!(
            store
                .transition(id, generation, State::AwaitingUser, State::Stopped)
                .unwrap()
        );
        assert_eq!(
            store.handoff_status(id, request).unwrap(),
            Some(HandoffStatus::Cancelled)
        );
        assert_eq!(store.pending_handoff(id).unwrap(), None);
    }

    #[test]
    fn concurrent_handoff_requests_leave_one_pending() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let store = SqliteStore::open(&root).unwrap();
        let (id, generation) = running(&store, "concurrent");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let (root, barrier) = (root.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let store = SqliteStore::open(&root).unwrap();
                    barrier.wait();
                    store.request_handoff(id, generation, None).unwrap()
                })
            })
            .collect();
        let requests: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        let pending: Vec<_> = requests
            .iter()
            .filter(|r| store.handoff_status(id, **r).unwrap() == Some(HandoffStatus::Pending))
            .collect();
        assert_eq!(pending.len(), 1);
        assert_eq!(store.pending_handoff(id).unwrap().unwrap().0, *pending[0]);
    }

    #[test]
    fn force_stopped_fences_the_old_generation() {
        let (_tmp, store) = store();
        let (id, generation) = running(&store, "recover");
        let request = store.request_handoff(id, generation, None).unwrap();
        assert!(store.force_stopped(id).unwrap());
        let row = store.get(id).unwrap().unwrap();
        assert_eq!(row.state, State::Stopped);
        assert_eq!(row.generation, None);
        assert!(!store.force_stopped(id).unwrap(), "already stopped");
        assert_eq!(
            store.handoff_status(id, request).unwrap(),
            Some(HandoffStatus::Cancelled)
        );
        assert!(store.resume_handoff(id, generation, request).is_err());
        assert!(
            !store
                .transition(id, generation, State::Stopped, State::Starting)
                .unwrap()
        );
        let next = store.reserve(id, false).unwrap().unwrap();
        assert_ne!(next, generation);
        assert!(
            !store
                .transition(id, generation, State::Starting, State::Running)
                .unwrap()
        );
    }

    #[test]
    fn failed_sessions_can_be_force_stopped() {
        let (_tmp, store) = store();
        let id = store.create(SessionName::parse("f").unwrap()).unwrap().id;
        let generation = store.reserve(id, false).unwrap().unwrap();
        assert!(
            store
                .transition(id, generation, State::Starting, State::Failed)
                .unwrap()
        );
        assert!(store.reserve(id, false).unwrap().is_none());
        assert!(store.force_stopped(id).unwrap());
        assert!(store.reserve(id, false).unwrap().is_some());
    }
}
