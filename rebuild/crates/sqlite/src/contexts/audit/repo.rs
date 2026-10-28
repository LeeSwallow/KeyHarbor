//! `AuditStore` for the audit context.

use super::{mappers::record, models::AuditModel, schema::audit};
use crate::{SqliteStore, contexts::config};
use anyhow::Result;
use diesel::{prelude::*, sql_types::Text};
use keyharbor_domain::{
    audit::{AuditEvent, AuditRecord},
    ports::AuditStore,
};

diesel::define_sql_function! {
    /// SQLite's `strftime(format, time)`.
    fn strftime(format: Text, time: Text) -> Text;
}

/// UTC, RFC 3339 with seconds: `2026-10-28T12:34:56Z`.
const AT_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

impl AuditStore for SqliteStore {
    fn record(&self, event: &AuditEvent) -> Result<bool> {
        self.write(|conn| {
            if !config::audit_enabled(conn)? {
                return Ok(false);
            }
            let written = diesel::insert_into(audit::table)
                .values((
                    audit::at.eq(strftime(AT_FORMAT, "now")),
                    audit::agent.eq(&event.agent),
                    audit::session_id.eq(&event.session_id),
                    audit::kind.eq(event.kind.as_str()),
                    audit::item_name.eq(&event.item_name),
                    audit::origin.eq(&event.origin),
                    audit::result.eq(&event.result),
                ))
                .execute(conn)?;
            Ok(written == 1)
        })
    }

    fn list_audit(&self, limit: Option<usize>) -> Result<Vec<AuditRecord>> {
        let limit = limit.map_or(i64::MAX, |n| i64::try_from(n).unwrap_or(i64::MAX));
        let mut records = self.read(|conn| {
            audit::table
                .select(AuditModel::as_select())
                .order(audit::seq.desc())
                .limit(limit)
                .load(conn)?
                .into_iter()
                .map(record)
                .collect::<Result<Vec<_>>>()
        })?;
        records.reverse();
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::audit::{AuditEvent, AuditKind};
    use keyharbor_domain::ports::{AuditStore, SettingsStore};

    fn store() -> (tempfile::TempDir, SqliteStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        (tmp, store)
    }

    #[test]
    fn events_are_recorded_and_listed_in_order() {
        let (_tmp, store) = store();
        let used = AuditEvent::new("claude", AuditKind::SecretUsed, "ok")
            .session("s-1")
            .item("github")
            .origin(Some("https://github.com"));
        assert!(store.record(&used).unwrap());
        assert!(
            store
                .record(&AuditEvent::new("cli", AuditKind::PolicyChanged, "deny"))
                .unwrap()
        );
        assert!(
            store
                .record(&AuditEvent::new("codex", AuditKind::SecretDenied, "policy").item("bank"))
                .unwrap()
        );
        let all = store.list_audit(None).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].event, used);
        assert!(all[0].seq < all[1].seq && all[1].seq < all[2].seq);
        assert_eq!(all[0].at.len(), 20, "{}", all[0].at);
        assert!(all[0].at.ends_with('Z'));
        let last = store.list_audit(Some(2)).unwrap();
        assert_eq!(last.len(), 2);
        assert_eq!(last[0].event.kind, AuditKind::PolicyChanged);
        assert_eq!(last[1].event.item_name.as_deref(), Some("bank"));
    }

    #[test]
    fn disabled_audit_records_nothing() {
        let (_tmp, store) = store();
        assert_eq!(store.setting_get(crate::AUDIT_ENABLED).unwrap(), None);
        store.setting_set(crate::AUDIT_ENABLED, "false").unwrap();
        for kind in AuditKind::ALL {
            assert!(!store.record(&AuditEvent::new("a", kind, "ok")).unwrap());
        }
        assert!(store.list_audit(None).unwrap().is_empty());
        store.setting_set(crate::AUDIT_ENABLED, "true").unwrap();
        assert!(
            store
                .record(&AuditEvent::new("a", AuditKind::SecretDenied, "origin"))
                .unwrap()
        );
        assert_eq!(store.list_audit(None).unwrap().len(), 1);
    }

    #[test]
    fn only_an_explicit_false_turns_the_record_off() {
        let (_tmp, store) = store();
        for value in ["true", "TRUE", "0", "off", ""] {
            store.setting_set(crate::AUDIT_ENABLED, value).unwrap();
            assert!(
                store
                    .record(&AuditEvent::new("a", AuditKind::SecretDenied, "origin"))
                    .unwrap(),
                "{value:?}"
            );
        }
    }

    #[test]
    fn audit_schema_has_no_value_column() {
        let (_tmp, store) = store();
        #[derive(diesel::QueryableByName)]
        struct Column {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
        }
        let columns: Vec<String> = store
            .read(|conn| {
                Ok(diesel::RunQueryDsl::load::<Column>(
                    diesel::sql_query("SELECT name FROM pragma_table_info('audit') ORDER BY cid"),
                    conn,
                )?)
            })
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(
            columns,
            [
                "seq",
                "at",
                "agent",
                "session_id",
                "kind",
                "item_name",
                "origin",
                "result"
            ]
        );
    }
}
