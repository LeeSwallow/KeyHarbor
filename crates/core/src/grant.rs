//! Grants (R-43): adding (a weakening, so a person confirms), listing, revoking, and the
//! rule a supervisor applies to every document its browser loads.

use crate::{session::session, weaken};
use anyhow::{Context, Result};
use keyharbor_crypto::random;
use keyharbor_domain::{
    SessionId,
    audit::{AuditEvent, AuditKind},
    grant::{Grant, GrantId, GrantTtl, allows_document},
    ports::{Clock, Confirm, Store},
    vault::{Description, Origin},
};

/// What a person grants.
#[derive(Clone, Debug)]
pub struct NewGrant {
    pub session: SessionId,
    pub origin: Origin,
    /// `None` lasts until revoked.
    pub ttl: Option<GrantTtl>,
    pub reason: Option<Description>,
}

fn note(store: &dyn Store, agent: &str, kind: AuditKind, grant: &Grant, result: &str) {
    let _ = store.record(
        &AuditEvent::new(agent, kind, result)
            .session(grant.session)
            .origin(Some(grant.origin.as_str())),
    );
}

/// Adds a grant after `confirm` says yes (it widens where the session may go).
pub fn add(
    store: &dyn Store,
    clock: &dyn Clock,
    confirm: &dyn Confirm,
    agent: &str,
    new: NewGrant,
) -> Result<Grant> {
    session(store, new.session)?;
    weaken::require(
        confirm,
        &[weaken::WeakeningOp::Grant {
            session: new.session.to_string(),
            origin: new.origin.as_str().to_owned(),
        }],
    )?;
    let now = clock.unix_seconds();
    let grant = Grant {
        id: random::id(),
        session: new.session,
        origin: new.origin,
        until: new.ttl.map(|ttl| now + ttl.seconds()),
        created_by: agent.to_owned(),
        reason: new.reason,
        created_at: now,
        revoked_at: None,
        cleared: false,
    };
    store.grant_add(&grant)?;
    note(store, agent, AuditKind::GrantChanged, &grant, "added");
    Ok(grant)
}

pub fn list(store: &dyn Store, id: SessionId) -> Result<Vec<Grant>> {
    session(store, id)?;
    store.grants_for(id)
}

/// Ends a grant now; the session's supervisor then clears its cookies.
pub fn revoke(store: &dyn Store, clock: &dyn Clock, agent: &str, id: GrantId) -> Result<Grant> {
    let grant = store.grant_get(id)?.context("grant not found")?;
    if !store.grant_revoke(id, clock.unix_seconds())? {
        anyhow::bail!("grant was already revoked");
    }
    note(store, agent, AuditKind::GrantChanged, &grant, "revoked");
    Ok(grant)
}

/// Whether session `id` may load the document at `url` now ([`allows_document`]). A refusal
/// is recorded with the document's origin only.
pub fn allows(store: &dyn Store, clock: &dyn Clock, id: SessionId, url: &str) -> Result<bool> {
    let allowed = allows_document(&store.grants_for(id)?, clock.unix_seconds(), url);
    if !allowed {
        let origin = url::Url::parse(url)
            .ok()
            .map(|u| u.origin().ascii_serialization());
        let _ = store.record(
            &AuditEvent::new("session", AuditKind::NavigationDenied, "out-of-grant")
                .session(id)
                .origin(origin.as_deref()),
        );
    }
    Ok(allowed)
}

/// Grants of session `id` that have ended and whose cookies are still in its profile.
pub fn ended_uncleared(store: &dyn Store, clock: &dyn Clock, id: SessionId) -> Result<Vec<Grant>> {
    let now = clock.unix_seconds();
    Ok(store
        .grants_for(id)?
        .into_iter()
        .filter(|g| !g.cleared && !g.is_active(now))
        .collect())
}

/// Records that an ended grant's cookies were removed.
pub fn cleared(store: &dyn Store, grant: &Grant) -> Result<()> {
    store.grant_mark_cleared(grant.id)?;
    note(store, "session", AuditKind::GrantChanged, grant, "cleared");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::{
        SessionName,
        ports::{Answer, AuditStore, ManualClock, SessionStore},
    };
    use keyharbor_sqlite::SqliteStore;
    use std::time::Duration;

    struct Says(Answer);
    impl Confirm for Says {
        fn confirm(&self, _action: &str) -> Answer {
            self.0
        }
    }

    fn setup() -> (tempfile::TempDir, SqliteStore, SessionId) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        let id = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        (tmp, store, id)
    }
    fn github(id: SessionId, ttl: Option<&str>) -> NewGrant {
        NewGrant {
            session: id,
            origin: Origin::parse("https://github.com").unwrap(),
            ttl: ttl.map(|t| GrantTtl::parse(t).unwrap()),
            reason: None,
        }
    }

    #[test]
    fn a_grant_needs_a_person_and_limits_the_session_until_it_ends() {
        let (_tmp, store, id) = setup();
        let clock = ManualClock::at_unix_seconds(1_000);
        assert!(allows(&store, &clock, id, "https://example.com/").unwrap());
        assert!(
            add(
                &store,
                &clock,
                &Says(Answer::Unavailable),
                "cli",
                github(id, None)
            )
            .is_err()
        );
        assert!(add(&store, &clock, &Says(Answer::No), "cli", github(id, None)).is_err());
        assert!(list(&store, id).unwrap().is_empty());

        let grant = add(
            &store,
            &clock,
            &Says(Answer::Yes),
            "cli",
            github(id, Some("1h")),
        )
        .unwrap();
        assert_eq!(grant.until, Some(1_000 + 3_600));
        assert!(allows(&store, &clock, id, "https://github.com/login").unwrap());
        assert!(!allows(&store, &clock, id, "https://example.com/").unwrap());
        assert!(ended_uncleared(&store, &clock, id).unwrap().is_empty());

        clock.advance(Duration::from_secs(3_600));
        assert!(!allows(&store, &clock, id, "https://github.com/").unwrap());
        let ended = ended_uncleared(&store, &clock, id).unwrap();
        assert_eq!(ended.len(), 1);
        cleared(&store, &ended[0]).unwrap();
        assert!(ended_uncleared(&store, &clock, id).unwrap().is_empty());

        let kinds: Vec<_> = store
            .list_audit(None)
            .unwrap()
            .into_iter()
            .map(|r| (r.event.kind, r.event.result))
            .collect();
        assert!(kinds.contains(&(AuditKind::NavigationDenied, "out-of-grant".into())));
        assert!(kinds.contains(&(AuditKind::GrantChanged, "cleared".into())));
    }

    #[test]
    fn a_revoked_grant_ends_at_once_and_only_once() {
        let (_tmp, store, id) = setup();
        let clock = ManualClock::at_unix_seconds(1_000);
        let grant = add(&store, &clock, &Says(Answer::Yes), "cli", github(id, None)).unwrap();
        revoke(&store, &clock, "cli", grant.id).unwrap();
        assert!(revoke(&store, &clock, "cli", grant.id).is_err());
        assert!(!allows(&store, &clock, id, "https://github.com/").unwrap());
        assert_eq!(ended_uncleared(&store, &clock, id).unwrap().len(), 1);
        let stranger = keyharbor_crypto::random::id::<SessionId>();
        assert!(
            add(
                &store,
                &clock,
                &Says(Answer::Yes),
                "cli",
                github(stranger, None)
            )
            .is_err()
        );
    }
}
