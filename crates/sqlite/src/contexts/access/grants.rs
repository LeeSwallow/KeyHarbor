//! `GrantStore` for the access context (R-43).

use super::{
    mappers::{SESSION_SUBJECT, grant, grant_model},
    models::GrantModel,
    schema::grants,
};
use crate::SqliteStore;
use anyhow::{Context, Result};
use diesel::prelude::*;
use keyharbor_domain::{
    SessionId,
    grant::{Grant, GrantId},
    ports::GrantStore,
};

impl GrantStore for SqliteStore {
    fn grant_add(&self, value: &Grant) -> Result<()> {
        let model = grant_model(value)?;
        self.read(|conn| {
            diesel::insert_into(grants::table)
                .values(&model)
                .execute(conn)?;
            Ok(())
        })
    }

    fn grants_for(&self, session: SessionId) -> Result<Vec<Grant>> {
        self.read(|conn| {
            grants::table
                .filter(grants::subject_kind.eq(SESSION_SUBJECT))
                .filter(grants::subject.eq(session.to_string()))
                .order((grants::created_at, grants::id))
                .select(GrantModel::as_select())
                .load(conn)?
                .into_iter()
                .map(grant)
                .collect()
        })
    }

    fn grant_get(&self, id: GrantId) -> Result<Option<Grant>> {
        self.read(|conn| {
            grants::table
                .find(id.to_string())
                .select(GrantModel::as_select())
                .first(conn)
                .optional()?
                .map(grant)
                .transpose()
        })
    }

    fn grant_revoke(&self, id: GrantId, at: u64) -> Result<bool> {
        let at = i64::try_from(at).context("grant time out of range")?;
        self.read(|conn| {
            let changed = diesel::update(
                grants::table
                    .find(id.to_string())
                    .filter(grants::revoked_at.is_null()),
            )
            .set(grants::revoked_at.eq(at))
            .execute(conn)?;
            Ok(changed == 1)
        })
    }

    fn grant_mark_cleared(&self, id: GrantId) -> Result<()> {
        self.read(|conn| {
            diesel::update(grants::table.find(id.to_string()))
                .set(grants::cleared.eq(true))
                .execute(conn)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::{
        SessionId,
        grant::{Grant, GrantId},
        ports::GrantStore,
        vault::{Description, Origin},
    };

    fn grant(id: u8, session: SessionId, created_at: u64) -> Grant {
        Grant {
            id: GrantId::from_bytes([id; 16]),
            session,
            origin: Origin::parse("https://github.com").unwrap(),
            until: Some(created_at + 60),
            created_by: "cli".into(),
            reason: Some(Description::parse("push the fix").unwrap()),
            created_at,
            revoked_at: None,
            cleared: false,
        }
    }

    #[test]
    fn grants_round_trip_are_revoked_once_and_marked_cleared() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        let (work, other) = (
            SessionId::from_bytes([1; 16]),
            SessionId::from_bytes([2; 16]),
        );
        let (first, second) = (grant(1, work, 20), grant(2, work, 10));
        store.grant_add(&first).unwrap();
        store.grant_add(&second).unwrap();
        store.grant_add(&grant(3, other, 5)).unwrap();
        assert_eq!(
            store.grants_for(work).unwrap(),
            [second.clone(), first.clone()]
        );
        assert!(store.grant_revoke(first.id, 30).unwrap());
        assert!(!store.grant_revoke(first.id, 31).unwrap());
        store.grant_mark_cleared(first.id).unwrap();
        let ended = store.grant_get(first.id).unwrap().unwrap();
        assert_eq!((ended.revoked_at, ended.cleared), (Some(30), true));
        assert_eq!(store.grant_get(GrantId::from_bytes([9; 16])).unwrap(), None);
    }
}
