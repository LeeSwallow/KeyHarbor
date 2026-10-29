//! Reading and recording a session's sign-in state at a site (02 §2.7): one implementation
//! for the CLI and the MCP server. The state is what the agent says it saw; nothing here
//! visits the site.

use crate::session::session;
use anyhow::Result;
use keyharbor_domain::{
    SessionId,
    audit::{AuditEvent, AuditKind},
    ports::{Clock, Store},
    site::{SiteRecord, SiteState},
    vault::{ItemName, Origin},
};

/// What an agent or the user records.
#[derive(Clone, Debug)]
pub struct SiteChange {
    pub session: SessionId,
    pub origin: Origin,
    pub state: SiteState,
    /// The vault item that signed in, if any (a name, never a value).
    pub item: Option<ItemName>,
}

/// The recorded state, or `unknown` when nothing was recorded for `origin`.
pub fn get(store: &dyn Store, id: SessionId, origin: &Origin) -> Result<SiteState> {
    session(store, id)?;
    Ok(store
        .site_get(id, origin)?
        .map_or(SiteState::Unknown, |record| record.state))
}

/// Every recorded state of a session, ordered by origin.
pub fn list(store: &dyn Store, id: SessionId) -> Result<Vec<SiteRecord>> {
    session(store, id)?;
    store.site_list(id)
}

/// Records `change` as said by `agent` now, and notes it in the usage record.
pub fn set(
    store: &dyn Store,
    clock: &dyn Clock,
    agent: &str,
    change: SiteChange,
) -> Result<SiteRecord> {
    session(store, change.session)?;
    let record = SiteRecord {
        session: change.session,
        origin: change.origin,
        state: change.state,
        item: change.item,
        checked_at: clock.now_rfc3339(),
        recorded_by: agent.to_owned(),
    };
    store.site_set(&record)?;
    let mut event = AuditEvent::new(agent, AuditKind::SiteStateChanged, record.state.as_str())
        .session(record.session)
        .origin(Some(record.origin.as_str()));
    if let Some(item) = &record.item {
        event = event.item(item.as_str());
    }
    let _ = store.record(&event);
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::{
        SessionName,
        ports::{AuditStore, ManualClock, SessionStore},
    };
    use keyharbor_sqlite::SqliteStore;

    fn setup() -> (tempfile::TempDir, SqliteStore, SessionId) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        let id = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        (tmp, store, id)
    }
    fn github() -> Origin {
        Origin::parse("https://github.com").unwrap()
    }

    #[test]
    fn an_origin_without_a_record_is_unknown() {
        let (_tmp, store, id) = setup();
        assert_eq!(get(&store, id, &github()).unwrap(), SiteState::Unknown);
    }

    #[test]
    fn a_recorded_state_is_read_back_with_who_and_when() {
        let (_tmp, store, id) = setup();
        let clock = ManualClock::at_unix_seconds(1_793_232_000);
        let change = SiteChange {
            session: id,
            origin: github(),
            state: SiteState::LoggedIn,
            item: Some(ItemName::parse("github").unwrap()),
        };
        let record = set(&store, &clock, "claude", change).unwrap();
        assert_eq!(get(&store, id, &github()).unwrap(), SiteState::LoggedIn);
        assert_eq!(list(&store, id).unwrap(), std::slice::from_ref(&record));
        assert_eq!(record.recorded_by, "claude");
        assert_eq!(record.checked_at, clock.now_rfc3339());

        let events = store.list_audit(Some(10)).unwrap();
        let event = &events.last().unwrap().event;
        assert_eq!(event.kind, AuditKind::SiteStateChanged);
        assert_eq!(event.result, "logged_in");
        assert_eq!(event.item_name.as_deref(), Some("github"));
        assert_eq!(event.origin.as_deref(), Some("https://github.com"));
    }

    #[test]
    fn an_unknown_session_is_refused() {
        let (_tmp, store, _id) = setup();
        let stranger = keyharbor_crypto::random::id::<SessionId>();
        assert!(get(&store, stranger, &github()).is_err());
        let change = SiteChange {
            session: stranger,
            origin: github(),
            state: SiteState::Expired,
            item: None,
        };
        let clock = ManualClock::at_unix_seconds(0);
        assert!(set(&store, &clock, "claude", change).is_err());
    }
}
