//! `SessionStore` for the session context. A state change applies only while the session is
//! in the expected state under the expected generation: the `UPDATE … WHERE … AND EXISTS
//! (generation)` affects one row or none, and that count is the compare-and-swap outcome.

use super::{
    mappers::{handoff_status, request_id, session_row},
    models::{NewGeneration, NewHandoff, NewSession, SessionModel},
    schema::{generations, handoff_requests, sessions},
};
use crate::SqliteStore;
use anyhow::{Result, bail};
use diesel::{dsl::exists, prelude::*, upsert::excluded};
use keyharbor_domain::{
    Generation, HandoffStatus, RequestId, Session, SessionId, SessionName, State,
    ports::{SessionRow, SessionStore},
    vault::Origin,
};

/// The session's run is still `generation`.
macro_rules! current_run {
    ($id:expr, $generation:expr) => {
        exists(
            generations::table
                .filter(generations::session_id.eq($id))
                .filter(generations::generation.eq($generation)),
        )
    };
}

/// Marks the session's pending handoff requests as cancelled.
fn cancel_pending(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    diesel::update(
        handoff_requests::table
            .filter(handoff_requests::session_id.eq(id))
            .filter(handoff_requests::status.eq(HandoffStatus::Pending.as_str())),
    )
    .set(handoff_requests::status.eq(HandoffStatus::Cancelled.as_str()))
    .execute(conn)?;
    Ok(())
}

fn load_rows(conn: &mut SqliteConnection, id: Option<&str>) -> Result<Vec<SessionRow>> {
    let mut query = sessions::table
        .left_join(generations::table.on(generations::session_id.eq(sessions::id)))
        .select((
            sessions::id,
            sessions::name,
            sessions::state,
            generations::generation.nullable(),
            generations::cdp.nullable(),
        ))
        .order(sessions::name)
        .into_boxed();
    if let Some(id) = id {
        query = query.filter(sessions::id.eq(id.to_owned()));
    }
    query
        .load::<SessionModel>(conn)?
        .into_iter()
        .map(session_row)
        .collect()
}

impl SessionStore for SqliteStore {
    fn create(&self, name: SessionName) -> Result<Session> {
        let id = SessionId::new();
        self.read(|conn| {
            diesel::insert_into(sessions::table)
                .values(NewSession {
                    id: &id.to_string(),
                    name: name.as_str(),
                    state: State::Stopped.as_str(),
                })
                .execute(conn)?;
            Ok(())
        })?;
        Ok(Session {
            id,
            name,
            state: State::Stopped,
        })
    }

    fn list(&self) -> Result<Vec<SessionRow>> {
        self.read(|conn| load_rows(conn, None))
    }

    fn get(&self, id: SessionId) -> Result<Option<SessionRow>> {
        self.read(|conn| Ok(load_rows(conn, Some(&id.to_string()))?.into_iter().next()))
    }

    fn delete(&self, id: SessionId) -> Result<bool> {
        let id = id.to_string();
        self.write(|conn| {
            let changed = diesel::delete(
                sessions::table
                    .filter(sessions::id.eq(&id))
                    .filter(sessions::state.eq(State::Stopped.as_str())),
            )
            .execute(conn)?;
            if changed == 1 {
                diesel::delete(generations::table.find(&id)).execute(conn)?;
            }
            Ok(changed == 1)
        })
    }

    fn reserve(&self, id: SessionId, cdp: bool) -> Result<Option<Generation>> {
        let id = id.to_string();
        self.write(|conn| {
            let changed = diesel::update(
                sessions::table
                    .filter(sessions::id.eq(&id))
                    .filter(sessions::state.eq(State::Stopped.as_str())),
            )
            .set(sessions::state.eq(State::Starting.as_str()))
            .execute(conn)?;
            if changed == 0 {
                return Ok(None);
            }
            let generation = Generation::new();
            diesel::insert_into(generations::table)
                .values(NewGeneration {
                    session_id: &id,
                    generation: &generation.to_string(),
                    cdp,
                })
                .on_conflict(generations::session_id)
                .do_update()
                .set((
                    generations::generation.eq(excluded(generations::generation)),
                    generations::cdp.eq(excluded(generations::cdp)),
                ))
                .execute(conn)?;
            Ok(Some(generation))
        })
    }

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
        let (id, generation) = (id.to_string(), generation.to_string());
        self.write(|conn| {
            let changed = diesel::update(
                sessions::table
                    .filter(sessions::id.eq(&id))
                    .filter(sessions::state.eq(from.as_str()))
                    .filter(current_run!(&id, &generation)),
            )
            .set(sessions::state.eq(to.as_str()))
            .execute(conn)?
                == 1;
            if changed && matches!(to, State::Stopping | State::Stopped | State::Failed) {
                cancel_pending(conn, &id)?;
            }
            Ok(changed)
        })
    }

    fn force_stopped(&self, id: SessionId) -> Result<bool> {
        let id = id.to_string();
        self.write(|conn| {
            let changed = diesel::update(
                sessions::table
                    .filter(sessions::id.eq(&id))
                    .filter(sessions::state.ne(State::Stopped.as_str())),
            )
            .set(sessions::state.eq(State::Stopped.as_str()))
            .execute(conn)?
                == 1;
            if changed {
                diesel::delete(generations::table.find(&id)).execute(conn)?;
                cancel_pending(conn, &id)?;
            }
            Ok(changed)
        })
    }

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
        let (id, generation) = (id.to_string(), generation.to_string());
        self.write(|conn| {
            let changed = diesel::update(
                sessions::table
                    .filter(sessions::id.eq(&id))
                    .filter(
                        sessions::state
                            .eq_any([State::Running.as_str(), State::AwaitingUser.as_str()]),
                    )
                    .filter(current_run!(&id, &generation)),
            )
            .set(sessions::state.eq(State::AwaitingUser.as_str()))
            .execute(conn)?;
            if changed != 1 {
                bail!("session is not running under this generation");
            }
            diesel::update(
                handoff_requests::table
                    .filter(handoff_requests::session_id.eq(&id))
                    .filter(handoff_requests::status.eq(HandoffStatus::Pending.as_str())),
            )
            .set(handoff_requests::status.eq(HandoffStatus::Superseded.as_str()))
            .execute(conn)?;
            let request = RequestId::new();
            diesel::insert_into(handoff_requests::table)
                .values(NewHandoff {
                    request_id: &request.to_string(),
                    session_id: &id,
                    generation: &generation,
                    origin: origin.as_ref().map(Origin::as_str),
                    status: HandoffStatus::Pending.as_str(),
                })
                .execute(conn)?;
            Ok(request)
        })
    }

    fn pending_handoff(&self, id: SessionId) -> Result<Option<(RequestId, Option<String>)>> {
        let id = id.to_string();
        let found: Option<(String, Option<String>)> = self.read(|conn| {
            Ok(handoff_requests::table
                .inner_join(sessions::table)
                .inner_join(
                    generations::table.on(generations::session_id
                        .eq(handoff_requests::session_id)
                        .and(generations::generation.eq(handoff_requests::generation))),
                )
                .filter(handoff_requests::session_id.eq(&id))
                .filter(handoff_requests::status.eq(HandoffStatus::Pending.as_str()))
                .filter(sessions::state.eq(State::AwaitingUser.as_str()))
                .select((handoff_requests::request_id, handoff_requests::origin))
                .first(conn)
                .optional()?)
        })?;
        found
            .map(|(request, origin)| Ok((request_id(&request)?, origin)))
            .transpose()
    }

    fn resume_handoff(
        &self,
        id: SessionId,
        generation: Generation,
        request: RequestId,
    ) -> Result<()> {
        let (id, generation) = (id.to_string(), generation.to_string());
        self.write(|conn| {
            let changed = diesel::update(
                handoff_requests::table
                    .filter(handoff_requests::request_id.eq(request.to_string()))
                    .filter(handoff_requests::session_id.eq(&id))
                    .filter(handoff_requests::generation.eq(&generation))
                    .filter(handoff_requests::status.eq(HandoffStatus::Pending.as_str())),
            )
            .set(handoff_requests::status.eq(HandoffStatus::Resumed.as_str()))
            .execute(conn)?;
            if changed != 1 {
                bail!(
                    "authentication request is not pending; inspect the current request with handoff"
                );
            }
            let changed = diesel::update(
                sessions::table
                    .filter(sessions::id.eq(&id))
                    .filter(sessions::state.eq(State::AwaitingUser.as_str()))
                    .filter(current_run!(&id, &generation)),
            )
            .set(sessions::state.eq(State::Running.as_str()))
            .execute(conn)?;
            if changed != 1 {
                bail!("session is no longer awaiting the user");
            }
            Ok(())
        })
    }

    fn handoff_status(&self, id: SessionId, request: RequestId) -> Result<Option<HandoffStatus>> {
        let status: Option<String> = self.read(|conn| {
            Ok(handoff_requests::table
                .filter(handoff_requests::session_id.eq(id.to_string()))
                .filter(handoff_requests::request_id.eq(request.to_string()))
                .select(handoff_requests::status)
                .first(conn)
                .optional()?)
        })?;
        status.as_deref().map(handoff_status).transpose()
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
