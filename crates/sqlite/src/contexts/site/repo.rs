//! `SiteStore` for the site context.

use super::{mappers, models::SiteModel, schema::site_states};
use crate::SqliteStore;
use anyhow::Result;
use diesel::prelude::*;
use keyharbor_domain::{SessionId, ports::SiteStore, site::SiteRecord, vault::Origin};

impl SiteStore for SqliteStore {
    fn site_get(&self, session: SessionId, origin: &Origin) -> Result<Option<SiteRecord>> {
        self.read(|conn| {
            site_states::table
                .find((session.to_string(), origin.as_str()))
                .select(SiteModel::as_select())
                .first(conn)
                .optional()?
                .map(mappers::record)
                .transpose()
        })
    }

    fn site_set(&self, record: &SiteRecord) -> Result<()> {
        let model = mappers::model(record);
        self.read(|conn| {
            diesel::insert_into(site_states::table)
                .values(&model)
                .on_conflict((site_states::session_id, site_states::origin))
                .do_update()
                .set(&model)
                .execute(conn)?;
            Ok(())
        })
    }

    fn site_list(&self, session: SessionId) -> Result<Vec<SiteRecord>> {
        self.read(|conn| {
            site_states::table
                .filter(site_states::session_id.eq(session.to_string()))
                .order(site_states::origin)
                .select(SiteModel::as_select())
                .load(conn)?
                .into_iter()
                .map(mappers::record)
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::{
        SessionId, SessionName,
        ports::{SessionStore, SiteStore},
        site::{SiteRecord, SiteState},
        vault::{ItemName, Origin},
    };

    fn store() -> (tempfile::TempDir, SqliteStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        (tmp, store)
    }
    fn record(session: SessionId, origin: &str, state: SiteState) -> SiteRecord {
        SiteRecord {
            session,
            origin: Origin::parse(origin).unwrap(),
            state,
            item: None,
            checked_at: "2026-10-29T01:02:03Z".into(),
            recorded_by: "claude".into(),
        }
    }

    #[test]
    fn a_state_is_replaced_per_session_and_origin() {
        let (_tmp, store) = store();
        let work = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        let other = store
            .create(SessionName::parse("other").unwrap())
            .unwrap()
            .id;
        let github = Origin::parse("https://github.com").unwrap();
        assert_eq!(store.site_get(work, &github).unwrap(), None);

        let mut signed_in = record(work, "https://github.com", SiteState::LoggedIn);
        signed_in.item = Some(ItemName::parse("github").unwrap());
        store.site_set(&signed_in).unwrap();
        store
            .site_set(&record(other, "https://github.com", SiteState::Expired))
            .unwrap();
        assert_eq!(store.site_get(work, &github).unwrap(), Some(signed_in));

        let expired = record(work, "https://github.com", SiteState::Expired);
        store.site_set(&expired).unwrap();
        assert_eq!(store.site_get(work, &github).unwrap(), Some(expired));
    }

    #[test]
    fn a_session_lists_its_origins_in_order_and_takes_them_when_deleted() {
        let (_tmp, store) = store();
        let work = store
            .create(SessionName::parse("work").unwrap())
            .unwrap()
            .id;
        for origin in ["https://z.example", "https://a.example"] {
            store
                .site_set(&record(work, origin, SiteState::Unknown))
                .unwrap();
        }
        let origins: Vec<_> = store
            .site_list(work)
            .unwrap()
            .into_iter()
            .map(|r| r.origin.as_str().to_owned())
            .collect();
        assert_eq!(origins, ["https://a.example", "https://z.example"]);

        assert!(store.delete(work).unwrap());
        assert!(store.site_list(work).unwrap().is_empty());
    }
}
