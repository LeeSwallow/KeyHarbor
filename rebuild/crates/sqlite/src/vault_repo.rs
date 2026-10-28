//! Vault tables. Only ciphertext, wrapped keys and non-secret metadata are stored here.

use crate::SqliteStore;
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS vault (id BLOB PRIMARY KEY CHECK(length(id)=16), kek_id BLOB NOT NULL CHECK(length(kek_id)=16), version INTEGER NOT NULL DEFAULT 0, singleton INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK(singleton=1));
        CREATE TABLE IF NOT EXISTS domain_keys (domain INTEGER PRIMARY KEY CHECK(domain IN (1,2,3)), dk_id BLOB NOT NULL CHECK(length(dk_id)=16), wrapped BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS vault_items (id BLOB PRIMARY KEY CHECK(length(id)=16), name TEXT NOT NULL UNIQUE, domain INTEGER NOT NULL CHECK(domain IN (1,2,3)), kind TEXT NOT NULL, origin_binding INTEGER NOT NULL, status TEXT NOT NULL CHECK(status IN ('draft','active','discarded')), source TEXT NOT NULL, meta_mac BLOB NOT NULL, updated_at TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS vault_fields (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, field TEXT NOT NULL, sealed BLOB NOT NULL, PRIMARY KEY(item_id, field));
        CREATE TABLE IF NOT EXISTS item_origins (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, origin TEXT NOT NULL, PRIMARY KEY(item_id, origin));",
    )?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRow {
    pub vault_id: [u8; 16],
    pub kek_id: [u8; 16],
    pub version: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainKeyRow {
    pub domain: u8,
    pub dk_id: [u8; 16],
    pub wrapped: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemRow {
    pub id: [u8; 16],
    pub name: String,
    pub domain: u8,
    pub kind: String,
    pub origin_binding: bool,
    pub status: String,
    pub source: String,
    pub meta_mac: Vec<u8>,
    pub updated_at: String,
    /// Sorted ascending.
    pub origins: Vec<String>,
}
/// `sealed` is a vault record; the field value itself never reaches SQLite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldRow {
    pub field: String,
    pub sealed: Vec<u8>,
}

fn id16(bytes: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
    bytes.try_into().map_err(|_| {
        rusqlite::Error::InvalidColumnType(0, "id".into(), rusqlite::types::Type::Blob)
    })
}

/// The key generation a writer sealed and signed with. Writes are refused if it is stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyGuard {
    pub kek_id: [u8; 16],
    pub domain: u8,
    pub dk_id: [u8; 16],
}

fn check_guard(tx: &Transaction, guard: &KeyGuard) -> Result<()> {
    let current: i64 = tx.query_row(
        "SELECT count(*) FROM vault v, domain_keys d WHERE v.kek_id=?1 AND d.domain=?2 AND d.dk_id=?3",
        params![guard.kek_id, guard.domain, guard.dk_id],
        |row| row.get(0),
    )?;
    if current != 1 {
        bail!("vault changed (keys were rotated); reopen it and retry");
    }
    Ok(())
}

fn bump_from(tx: &Transaction, expected_version: i64) -> Result<()> {
    if tx.execute(
        "UPDATE vault SET version = version + 1 WHERE version = ?1",
        [expected_version],
    )? != 1
    {
        bail!("vault changed during rotation; nothing was changed");
    }
    Ok(())
}

fn bump(tx: &Transaction) -> Result<()> {
    if tx.execute("UPDATE vault SET version = version + 1", [])? != 1 {
        bail!("vault not initialized");
    }
    Ok(())
}

fn write_children(tx: &Transaction, item: &ItemRow, fields: &[FieldRow]) -> Result<()> {
    for field in fields {
        tx.execute(
            "INSERT INTO vault_fields (item_id, field, sealed) VALUES (?1, ?2, ?3)",
            params![item.id, field.field, field.sealed],
        )?;
    }
    for origin in &item.origins {
        tx.execute(
            "INSERT OR IGNORE INTO item_origins (item_id, origin) VALUES (?1, ?2)",
            params![item.id, origin],
        )?;
    }
    Ok(())
}

impl SqliteStore {
    fn write_tx(&self) -> Result<Transaction<'_>> {
        Ok(Transaction::new_unchecked(
            &self.conn,
            TransactionBehavior::Immediate,
        )?)
    }

    pub fn vault_get(&self) -> Result<Option<VaultRow>> {
        Ok(self
            .conn
            .query_row("SELECT id, kek_id, version FROM vault", [], |row| {
                Ok(VaultRow {
                    vault_id: id16(row.get(0)?)?,
                    kek_id: id16(row.get(1)?)?,
                    version: row.get(2)?,
                })
            })
            .optional()?)
    }

    pub fn vault_create(&self, vault: &VaultRow, dks: &[DomainKeyRow]) -> Result<()> {
        let tx = self.write_tx()?;
        tx.execute(
            "INSERT INTO vault (id, kek_id, version) VALUES (?1, ?2, ?3)",
            params![vault.vault_id, vault.kek_id, vault.version],
        )
        .context("vault already initialized")?;
        for dk in dks {
            tx.execute(
                "INSERT INTO domain_keys (domain, dk_id, wrapped) VALUES (?1, ?2, ?3)",
                params![dk.domain, dk.dk_id, dk.wrapped],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn domain_keys(&self) -> Result<Vec<DomainKeyRow>> {
        let mut query = self
            .conn
            .prepare("SELECT domain, dk_id, wrapped FROM domain_keys ORDER BY domain")?;
        let rows = query.query_map([], |row| {
            Ok(DomainKeyRow {
                domain: row.get(0)?,
                dk_id: id16(row.get(1)?)?,
                wrapped: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn item_insert(&self, guard: &KeyGuard, item: &ItemRow, fields: &[FieldRow]) -> Result<()> {
        let tx = self.write_tx()?;
        check_guard(&tx, guard)?;
        let inserted = tx.execute(
            "INSERT INTO vault_items (id, name, domain, kind, origin_binding, status, source, meta_mac, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(name) DO NOTHING",
            params![item.id, item.name, item.domain, item.kind, item.origin_binding, item.status, item.source, item.meta_mac, item.updated_at],
        )?;
        if inserted == 0 {
            bail!("item already exists");
        }
        write_children(&tx, item, fields)?;
        bump(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn item_update(&self, guard: &KeyGuard, item: &ItemRow, fields: &[FieldRow]) -> Result<()> {
        let tx = self.write_tx()?;
        check_guard(&tx, guard)?;
        let updated = tx.execute(
            "UPDATE vault_items SET name=?2, domain=?3, kind=?4, origin_binding=?5, status=?6, source=?7, meta_mac=?8, updated_at=?9 WHERE id=?1",
            params![item.id, item.name, item.domain, item.kind, item.origin_binding, item.status, item.source, item.meta_mac, item.updated_at],
        )?;
        if updated == 0 {
            bail!("item not found");
        }
        tx.execute("DELETE FROM vault_fields WHERE item_id=?1", [item.id])?;
        tx.execute("DELETE FROM item_origins WHERE item_id=?1", [item.id])?;
        write_children(&tx, item, fields)?;
        bump(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn item_delete(&self, name: &str) -> Result<bool> {
        let tx = self.write_tx()?;
        let deleted = tx.execute("DELETE FROM vault_items WHERE name=?1", [name])? == 1;
        if deleted {
            bump(&tx)?;
        }
        tx.commit()?;
        Ok(deleted)
    }

    fn origins_of(&self, id: &[u8; 16]) -> Result<Vec<String>> {
        let mut query = self
            .conn
            .prepare("SELECT origin FROM item_origins WHERE item_id=?1 ORDER BY origin")?;
        let rows = query.query_map([id], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn items_where(&self, filter: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<ItemRow>> {
        let mut query = self.conn.prepare(&format!(
            "SELECT id, name, domain, kind, origin_binding, status, source, meta_mac, updated_at FROM vault_items {filter} ORDER BY name"
        ))?;
        let rows = query.query_map(args, |row| {
            Ok(ItemRow {
                id: id16(row.get(0)?)?,
                name: row.get(1)?,
                domain: row.get(2)?,
                kind: row.get(3)?,
                origin_binding: row.get(4)?,
                status: row.get(5)?,
                source: row.get(6)?,
                meta_mac: row.get(7)?,
                updated_at: row.get(8)?,
                origins: Vec::new(),
            })
        })?;
        let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        for item in &mut items {
            item.origins = self.origins_of(&item.id)?;
        }
        Ok(items)
    }

    pub fn item_get(&self, name: &str) -> Result<Option<(ItemRow, Vec<FieldRow>)>> {
        let Some(item) = self
            .items_where("WHERE name=?1", &[&name])?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let mut query = self
            .conn
            .prepare("SELECT field, sealed FROM vault_fields WHERE item_id=?1 ORDER BY field")?;
        let fields = query
            .query_map([item.id], |row| {
                Ok(FieldRow {
                    field: row.get(0)?,
                    sealed: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Some((item, fields)))
    }

    pub fn item_list(&self) -> Result<Vec<ItemRow>> {
        self.items_where("", &[])
    }

    /// Re-wraps all domain keys under a new KEK and replaces every item's metadata MAC atomically.
    pub fn rekey_domain_keys(
        &self,
        expected_version: i64,
        new_kek_id: [u8; 16],
        dks: &[DomainKeyRow],
        metas: &[([u8; 16], Vec<u8>)],
    ) -> Result<()> {
        let tx = self.write_tx()?;
        bump_from(&tx, expected_version)?;
        tx.execute("UPDATE vault SET kek_id=?1", [new_kek_id])?;
        for dk in dks {
            if tx.execute(
                "UPDATE domain_keys SET dk_id=?2, wrapped=?3 WHERE domain=?1",
                params![dk.domain, dk.dk_id, dk.wrapped],
            )? != 1
            {
                bail!("unknown domain {}", dk.domain);
            }
        }
        for (id, mac) in metas {
            tx.execute(
                "UPDATE vault_items SET meta_mac=?2 WHERE id=?1",
                params![id, mac],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Replaces one domain key and the given re-encrypted fields atomically.
    pub fn replace_domain(
        &self,
        expected_version: i64,
        dk: &DomainKeyRow,
        fields: &[([u8; 16], FieldRow)],
    ) -> Result<()> {
        let tx = self.write_tx()?;
        bump_from(&tx, expected_version)?;
        if tx.execute(
            "UPDATE domain_keys SET dk_id=?2, wrapped=?3 WHERE domain=?1",
            params![dk.domain, dk.dk_id, dk.wrapped],
        )? != 1
        {
            bail!("unknown domain {}", dk.domain);
        }
        for (item, field) in fields {
            if tx.execute(
                "UPDATE vault_fields SET sealed=?3 WHERE item_id=?1 AND field=?2",
                params![item, field.field, field.sealed],
            )? != 1
            {
                bail!("field not found during domain rotation");
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn vault_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT version FROM vault", [], |row| row.get(0))?)
    }

    #[cfg(test)]
    fn count_rows(&self, table: &str) -> Result<i64> {
        Ok(self
            .conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStore;

    const G: KeyGuard = KeyGuard {
        kek_id: [8; 16],
        domain: 1,
        dk_id: [1; 16],
    };

    fn store() -> (tempfile::TempDir, SqliteStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        (tmp, store)
    }
    fn with_vault(store: &SqliteStore) {
        let dks: Vec<_> = (1..=3)
            .map(|d| DomainKeyRow {
                domain: d,
                dk_id: [d; 16],
                wrapped: vec![d; 8],
            })
            .collect();
        store
            .vault_create(
                &VaultRow {
                    vault_id: [9; 16],
                    kek_id: [8; 16],
                    version: 0,
                },
                &dks,
            )
            .unwrap();
    }
    fn item(name: &str, id: u8) -> ItemRow {
        ItemRow {
            id: [id; 16],
            name: name.into(),
            domain: 1,
            kind: "login".into(),
            origin_binding: true,
            status: "active".into(),
            source: "user".into(),
            meta_mac: vec![id; 32],
            updated_at: "2026-10-28T00:00:00Z".into(),
            origins: vec!["https://github.com".into()],
        }
    }
    fn fields() -> Vec<FieldRow> {
        vec![
            FieldRow {
                field: "password".into(),
                sealed: vec![1, 2, 3],
            },
            FieldRow {
                field: "username".into(),
                sealed: vec![4, 5],
            },
        ]
    }

    #[test]
    fn vault_create_once() {
        let (_t, s) = store();
        assert!(s.vault_get().unwrap().is_none());
        with_vault(&s);
        assert_eq!(s.vault_get().unwrap().unwrap().kek_id, [8; 16]);
        assert_eq!(s.domain_keys().unwrap().len(), 3);
        let again = s.vault_create(
            &VaultRow {
                vault_id: [7; 16],
                kek_id: [7; 16],
                version: 0,
            },
            &[],
        );
        assert!(again.is_err());
    }
    #[test]
    fn item_roundtrip_and_cascade() {
        let (_t, s) = store();
        with_vault(&s);
        s.item_insert(&G, &item("github", 1), &fields()).unwrap();
        let (row, f) = s.item_get("github").unwrap().unwrap();
        assert_eq!(row, item("github", 1));
        assert_eq!(f, fields());
        assert_eq!(s.item_list().unwrap().len(), 1);
        assert!(s.item_delete("github").unwrap());
        assert!(!s.item_delete("github").unwrap());
        assert_eq!(s.count_rows("vault_fields").unwrap(), 0);
        assert_eq!(s.count_rows("item_origins").unwrap(), 0);
    }
    #[test]
    fn duplicate_name_fails_without_partial_rows() {
        let (_t, s) = store();
        with_vault(&s);
        s.item_insert(&G, &item("github", 1), &fields()).unwrap();
        let err = s
            .item_insert(&G, &item("github", 2), &fields())
            .unwrap_err();
        assert!(err.to_string().contains("item already exists"), "{err}");
        assert_eq!(s.count_rows("vault_items").unwrap(), 1);
        assert_eq!(s.count_rows("vault_fields").unwrap(), 2);
        assert_eq!(s.count_rows("item_origins").unwrap(), 1);
    }
    #[test]
    fn update_replaces_fields_and_origins() {
        let (_t, s) = store();
        with_vault(&s);
        s.item_insert(&G, &item("github", 1), &fields()).unwrap();
        let mut changed = item("github", 1);
        changed.origins = vec![
            "https://gist.github.com".into(),
            "https://github.com".into(),
        ];
        changed.status = "draft".into();
        s.item_update(&G, &changed, &fields()[..1]).unwrap();
        let (row, f) = s.item_get("github").unwrap().unwrap();
        assert_eq!(row, changed);
        assert_eq!(f.len(), 1);
    }
    #[test]
    fn writes_bump_version() {
        let (_t, s) = store();
        with_vault(&s);
        let v0 = s.vault_version().unwrap();
        s.item_insert(&G, &item("github", 1), &fields()).unwrap();
        s.item_update(&G, &item("github", 1), &fields()).unwrap();
        s.item_delete("github").unwrap();
        assert_eq!(s.vault_version().unwrap(), v0 + 3);
    }
    #[test]
    fn stale_guard_and_version_are_refused() {
        let (_t, s) = store();
        with_vault(&s);
        let stale = KeyGuard {
            kek_id: [7; 16],
            ..G
        };
        assert!(
            s.item_insert(&stale, &item("github", 1), &fields())
                .unwrap_err()
                .to_string()
                .contains("vault changed")
        );
        let stale_dk = KeyGuard {
            dk_id: [2; 16],
            ..G
        };
        assert!(
            s.item_insert(&stale_dk, &item("github", 1), &fields())
                .is_err()
        );
        assert!(s.item_list().unwrap().is_empty());
        let v = s.vault_version().unwrap();
        let dk = DomainKeyRow {
            domain: 1,
            dk_id: [0x10; 16],
            wrapped: vec![0x11; 8],
        };
        assert!(
            s.replace_domain(v + 1, &dk, &[])
                .unwrap_err()
                .to_string()
                .contains("vault changed")
        );
        assert!(s.rekey_domain_keys(v + 1, [5; 16], &[], &[]).is_err());
        assert_eq!(s.vault_get().unwrap().unwrap().kek_id, [8; 16]);
    }
    #[test]
    fn rekey_is_atomic() {
        let (_t, s) = store();
        with_vault(&s);
        s.item_insert(&G, &item("github", 1), &fields()).unwrap();
        let bad = [DomainKeyRow {
            domain: 4,
            dk_id: [0; 16],
            wrapped: vec![0],
        }];
        assert!(
            s.rekey_domain_keys(
                s.vault_version().unwrap(),
                [5; 16],
                &bad,
                &[([1; 16], vec![0; 32])]
            )
            .is_err()
        );
        assert_eq!(s.vault_get().unwrap().unwrap().kek_id, [8; 16]);
        assert_eq!(
            s.item_get("github").unwrap().unwrap().0.meta_mac,
            vec![1; 32]
        );
        let good: Vec<_> = (1..=3)
            .map(|d| DomainKeyRow {
                domain: d,
                dk_id: [d; 16],
                wrapped: vec![0xee; 8],
            })
            .collect();
        s.rekey_domain_keys(
            s.vault_version().unwrap(),
            [5; 16],
            &good,
            &[([1; 16], vec![0xaa; 32])],
        )
        .unwrap();
        assert_eq!(s.vault_get().unwrap().unwrap().kek_id, [5; 16]);
        assert_eq!(
            s.item_get("github").unwrap().unwrap().0.meta_mac,
            vec![0xaa; 32]
        );
        assert!(
            s.domain_keys()
                .unwrap()
                .iter()
                .all(|d| d.wrapped == vec![0xee; 8])
        );
    }
    #[test]
    fn replace_domain_swaps_key_and_fields_together() {
        let (_t, s) = store();
        with_vault(&s);
        s.item_insert(&G, &item("github", 1), &fields()).unwrap();
        let dk = DomainKeyRow {
            domain: 1,
            dk_id: [0x10; 16],
            wrapped: vec![0x11; 8],
        };
        let new_fields = [(
            [1u8; 16],
            FieldRow {
                field: "password".into(),
                sealed: vec![9, 9],
            },
        )];
        s.replace_domain(s.vault_version().unwrap(), &dk, &new_fields)
            .unwrap();
        assert!(s.domain_keys().unwrap().contains(&dk));
        let (_, f) = s.item_get("github").unwrap().unwrap();
        assert!(f.contains(&FieldRow {
            field: "password".into(),
            sealed: vec![9, 9]
        }));
        assert!(f.contains(&FieldRow {
            field: "username".into(),
            sealed: vec![4, 5]
        }));
    }
}
