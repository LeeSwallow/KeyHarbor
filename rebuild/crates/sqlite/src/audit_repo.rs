//! The usage record and settings. The `audit` table has no value column by design (02 §2.7.2).

use crate::SqliteStore;
use anyhow::{Context, Result};
use keyharbor_domain::audit::{AuditEvent, AuditKind, AuditRecord};
use keyharbor_domain::ports::{AuditStore, SettingsStore};
use rusqlite::{Connection, OptionalExtension, params};

pub use keyharbor_domain::ports::AUDIT_ENABLED;

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS audit (seq INTEGER PRIMARY KEY, at TEXT NOT NULL, agent TEXT NOT NULL, session_id TEXT, kind TEXT NOT NULL, item_name TEXT, origin TEXT, result TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    Ok(())
}

impl AuditStore for SqliteStore {
    /// Appends the event unless `audit_enabled` is `false` (then nothing at all is kept, J-23).
    /// Returns whether it was written.
    fn record(&self, event: &AuditEvent) -> Result<bool> {
        Ok(self.conn.execute(
            "INSERT INTO audit (at, agent, session_id, kind, item_name, origin, result) SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now'), ?1, ?2, ?3, ?4, ?5, ?6 WHERE coalesce((SELECT value FROM settings WHERE key='audit_enabled'), 'true') = 'true'",
            params![
                event.agent,
                event.session_id,
                event.kind.as_str(),
                event.item_name,
                event.origin,
                event.result
            ],
        )? == 1)
    }

    /// The most recent `limit` events (all when `None`), oldest first.
    fn list_audit(&self, limit: Option<usize>) -> Result<Vec<AuditRecord>> {
        let limit = limit.map_or(-1, |n| i64::try_from(n).unwrap_or(i64::MAX));
        let mut query = self.conn.prepare(
            "SELECT seq, at, agent, session_id, kind, item_name, origin, result FROM (SELECT * FROM audit ORDER BY seq DESC LIMIT ?1) ORDER BY seq",
        )?;
        let rows = query.query_map([limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;
        rows.map(|raw| {
            let (seq, at, agent, session_id, kind, item_name, origin, result) = raw?;
            Ok(AuditRecord {
                seq,
                at,
                event: AuditEvent {
                    agent,
                    session_id,
                    kind: AuditKind::parse(&kind).context("invalid stored audit kind")?,
                    item_name,
                    origin,
                    result,
                },
            })
        })
        .collect()
    }
}

impl SettingsStore for SqliteStore {
    fn setting_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    fn setting_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
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
        assert_eq!(store.setting_get(super::AUDIT_ENABLED).unwrap(), None);
        store.setting_set(super::AUDIT_ENABLED, "false").unwrap();
        for kind in AuditKind::ALL {
            assert!(!store.record(&AuditEvent::new("a", kind, "ok")).unwrap());
        }
        assert!(store.list_audit(None).unwrap().is_empty());
        store.setting_set(super::AUDIT_ENABLED, "true").unwrap();
        assert!(
            store
                .record(&AuditEvent::new("a", AuditKind::SecretDenied, "origin"))
                .unwrap()
        );
        assert_eq!(store.list_audit(None).unwrap().len(), 1);
    }

    #[test]
    fn audit_schema_has_no_value_column() {
        let (_tmp, store) = store();
        let mut query = store
            .conn
            .prepare("SELECT name FROM pragma_table_info('audit') ORDER BY cid")
            .unwrap();
        let columns: Vec<String> = query
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
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
