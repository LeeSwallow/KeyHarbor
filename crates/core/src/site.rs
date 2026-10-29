//! Reading and recording a session's sign-in state at a site (02 §2.7): one implementation
//! for the CLI and the MCP server. The state is what the agent says it saw; nothing here
//! visits the site.

use crate::{Vault, session::session};
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

/// [`set`], then, for `logged_in`, confirms the login draft the sign-in used (the named
/// item if it is one, else the newest generated or captured draft for the origin; 08 §8.3).
/// Returns the record and the confirmed item, if any. Without a vault nothing is confirmed.
pub fn set_and_confirm(
    store: &dyn Store,
    open_vault: impl FnOnce() -> Result<Vault>,
    clock: &dyn Clock,
    agent: &str,
    change: SiteChange,
) -> Result<(SiteRecord, Option<ItemName>)> {
    let mut record = set(store, clock, agent, change)?;
    if record.state != SiteState::LoggedIn || store.vault_get()?.is_none() {
        return Ok((record, None));
    }
    let vault = open_vault()?;
    let Some(draft) = vault.login_draft_for(&record.origin, record.item.as_ref())? else {
        return Ok((record, None));
    };
    let confirmed = vault.confirm(&draft, agent)?;
    record.item = Some(confirmed.clone());
    store.site_set(&record)?;
    Ok((record, Some(confirmed)))
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
    fn signing_in_confirms_the_generated_draft_for_the_origin() {
        let e = crate::tests::env();
        let vault = crate::tests::init(&e);
        let store = crate::tests::open_store(&e.data);
        let id = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        vault.add(crate::tests::login("github", "old")).unwrap();
        let draft = vault
            .generate_login(
                crate::GeneratedLogin {
                    name: ItemName::parse("github").unwrap(),
                    origin: github(),
                    username: None,
                    rule: Default::default(),
                },
                "claude",
            )
            .unwrap();
        assert_eq!(draft.as_str(), "github~draft-1");
        let clock = ManualClock::at_unix_seconds(1_793_232_000);
        let change = |state| SiteChange {
            session: id,
            origin: github(),
            state,
            item: None,
        };
        let (_, none) = set_and_confirm(
            &store,
            || crate::tests::reopen(&e).map(|v| v.0),
            &clock,
            "claude",
            change(SiteState::Expired),
        )
        .unwrap();
        assert_eq!(none, None);
        let (record, confirmed) = set_and_confirm(
            &store,
            || crate::tests::reopen(&e).map(|v| v.0),
            &clock,
            "claude",
            change(SiteState::LoggedIn),
        )
        .unwrap();
        assert_eq!(confirmed, Some(ItemName::parse("github").unwrap()));
        assert_eq!(record.item, confirmed);
        assert_eq!(list(&store, id).unwrap()[0].item, confirmed);
        let items = vault.list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, crate::ItemStatus::Active);
        assert_ne!(
            *vault
                .resolve(&crate::tests::pref("github", "password"))
                .unwrap(),
            "old"
        );
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
