//! `VaultStore` for the vault context. Every write runs in one immediate transaction and
//! bumps `vault.version`; key rotations commit only from the version they read
//! (compare-and-swap), and item writes only under the key generation they sealed with.

use super::{
    mappers::{
        ItemChildren, domain_key_model, domain_key_row, field_model, field_row, item_model,
        item_row, vault_row,
    },
    models::{
        DomainKeyModel, FieldModel, ItemModel, NewFieldDescription, NewOrigin, NewVault, VaultModel,
    },
    schema::{
        domain_keys, item_field_descriptions, item_origins, vault, vault_fields, vault_items,
    },
};
use crate::SqliteStore;
use anyhow::{Context, Result, bail};
use diesel::prelude::*;
use keyharbor_domain::ports::vault::{
    DomainKeyRow, FieldRow, ItemRow, KeyGuard, VaultRow, VaultStore,
};

fn check_guard(conn: &mut SqliteConnection, guard: &KeyGuard) -> Result<()> {
    let kek: i64 = vault::table
        .filter(vault::kek_id.eq(&guard.kek_id[..]))
        .count()
        .get_result(conn)?;
    let dk: i64 = domain_keys::table
        .filter(domain_keys::domain.eq(i32::from(guard.domain)))
        .filter(domain_keys::dk_id.eq(&guard.dk_id[..]))
        .count()
        .get_result(conn)?;
    if kek * dk != 1 {
        bail!("vault changed (keys were rotated); reopen it and retry");
    }
    Ok(())
}

/// Bumps the version only if it is still `expected_version`.
fn bump_from(conn: &mut SqliteConnection, expected_version: i64) -> Result<()> {
    let changed = diesel::update(vault::table.filter(vault::version.eq(expected_version)))
        .set(vault::version.eq(vault::version + 1))
        .execute(conn)?;
    if changed != 1 {
        bail!("vault changed during rotation; nothing was changed");
    }
    Ok(())
}

fn bump(conn: &mut SqliteConnection) -> Result<()> {
    let changed = diesel::update(vault::table)
        .set(vault::version.eq(vault::version + 1))
        .execute(conn)?;
    if changed != 1 {
        bail!("vault not initialized");
    }
    Ok(())
}

fn write_children(conn: &mut SqliteConnection, item: &ItemRow, fields: &[FieldRow]) -> Result<()> {
    for field in fields {
        diesel::insert_into(vault_fields::table)
            .values(field_model(&item.id, field))
            .execute(conn)?;
    }
    for origin in &item.origins {
        diesel::insert_or_ignore_into(item_origins::table)
            .values(NewOrigin {
                item_id: &item.id,
                origin,
            })
            .execute(conn)?;
    }
    for (field, description) in &item.field_descriptions {
        diesel::insert_into(item_field_descriptions::table)
            .values(NewFieldDescription {
                item_id: &item.id,
                field,
                description,
            })
            .execute(conn)?;
    }
    Ok(())
}

fn origins_of(conn: &mut SqliteConnection, id: &[u8]) -> Result<Vec<String>> {
    Ok(item_origins::table
        .filter(item_origins::item_id.eq(id))
        .select(item_origins::origin)
        .order(item_origins::origin)
        .load(conn)?)
}

fn children_of(conn: &mut SqliteConnection, id: &[u8]) -> Result<ItemChildren> {
    Ok(ItemChildren {
        origins: origins_of(conn, id)?,
        field_descriptions: item_field_descriptions::table
            .filter(item_field_descriptions::item_id.eq(id))
            .select((
                item_field_descriptions::field,
                item_field_descriptions::description,
            ))
            .order(item_field_descriptions::field)
            .load(conn)?,
    })
}

fn with_children(conn: &mut SqliteConnection, models: Vec<ItemModel>) -> Result<Vec<ItemRow>> {
    models
        .into_iter()
        .map(|model| {
            let children = children_of(conn, &model.id)?;
            item_row(model, children)
        })
        .collect()
}

fn replace_domain_key(conn: &mut SqliteConnection, dk: &DomainKeyRow) -> Result<()> {
    let changed = diesel::update(domain_keys::table.find(i32::from(dk.domain)))
        .set((
            domain_keys::dk_id.eq(&dk.dk_id[..]),
            domain_keys::wrapped.eq(&dk.wrapped),
        ))
        .execute(conn)?;
    if changed != 1 {
        bail!("unknown domain {}", dk.domain);
    }
    Ok(())
}

/// Replaces an item's metadata row (found by id), fields, origins and field descriptions.
fn update_item(conn: &mut SqliteConnection, item: &ItemRow, fields: &[FieldRow]) -> Result<()> {
    let updated = diesel::update(vault_items::table.find(&item.id[..]))
        .set(item_model(item))
        .execute(conn)?;
    if updated == 0 {
        bail!("item not found");
    }
    diesel::delete(vault_fields::table.filter(vault_fields::item_id.eq(&item.id[..])))
        .execute(conn)?;
    diesel::delete(item_origins::table.filter(item_origins::item_id.eq(&item.id[..])))
        .execute(conn)?;
    diesel::delete(
        item_field_descriptions::table.filter(item_field_descriptions::item_id.eq(&item.id[..])),
    )
    .execute(conn)?;
    write_children(conn, item, fields)
}

impl VaultStore for SqliteStore {
    fn vault_get(&self) -> Result<Option<VaultRow>> {
        self.read(|conn| {
            vault::table
                .select(VaultModel::as_select())
                .first(conn)
                .optional()?
                .map(vault_row)
                .transpose()
        })
    }

    fn vault_create(&self, row: &VaultRow, dks: &[DomainKeyRow]) -> Result<()> {
        self.write(|conn| {
            diesel::insert_into(vault::table)
                .values(NewVault {
                    id: &row.vault_id,
                    kek_id: &row.kek_id,
                    version: row.version,
                })
                .execute(conn)
                .context("vault already initialized")?;
            for dk in dks {
                diesel::insert_into(domain_keys::table)
                    .values(domain_key_model(dk))
                    .execute(conn)?;
            }
            Ok(())
        })
    }

    fn domain_keys(&self) -> Result<Vec<DomainKeyRow>> {
        self.read(|conn| {
            domain_keys::table
                .select(DomainKeyModel::as_select())
                .order(domain_keys::domain)
                .load(conn)?
                .into_iter()
                .map(domain_key_row)
                .collect()
        })
    }

    fn item_insert(&self, guard: &KeyGuard, item: &ItemRow, fields: &[FieldRow]) -> Result<()> {
        self.write(|conn| {
            check_guard(conn, guard)?;
            let inserted = diesel::insert_into(vault_items::table)
                .values(item_model(item))
                .on_conflict(vault_items::name)
                .do_nothing()
                .execute(conn)?;
            if inserted == 0 {
                bail!("item already exists");
            }
            write_children(conn, item, fields)?;
            bump(conn)
        })
    }

    fn item_replace(
        &self,
        guard: &KeyGuard,
        replaced: &str,
        item: &ItemRow,
        fields: &[FieldRow],
    ) -> Result<()> {
        self.write(|conn| {
            check_guard(conn, guard)?;
            diesel::delete(vault_items::table.filter(vault_items::name.eq(replaced)))
                .execute(conn)?;
            update_item(conn, item, fields)?;
            bump(conn)
        })
    }

    fn item_update(&self, guard: &KeyGuard, item: &ItemRow, fields: &[FieldRow]) -> Result<()> {
        self.write(|conn| {
            check_guard(conn, guard)?;
            update_item(conn, item, fields)?;
            bump(conn)
        })
    }

    fn item_delete(&self, name: &str) -> Result<bool> {
        self.write(|conn| {
            let deleted = diesel::delete(vault_items::table.filter(vault_items::name.eq(name)))
                .execute(conn)?
                == 1;
            if deleted {
                bump(conn)?;
            }
            Ok(deleted)
        })
    }

    fn item_get(&self, name: &str) -> Result<Option<(ItemRow, Vec<FieldRow>)>> {
        self.read(|conn| {
            let Some(model) = vault_items::table
                .filter(vault_items::name.eq(name))
                .select(ItemModel::as_select())
                .first(conn)
                .optional()?
            else {
                return Ok(None);
            };
            let fields = vault_fields::table
                .filter(vault_fields::item_id.eq(&model.id))
                .select(FieldModel::as_select())
                .order(vault_fields::field)
                .load(conn)?
                .into_iter()
                .map(field_row)
                .collect();
            let children = children_of(conn, &model.id)?;
            Ok(Some((item_row(model, children)?, fields)))
        })
    }

    fn item_list(&self) -> Result<Vec<ItemRow>> {
        self.read(|conn| {
            let models = vault_items::table
                .select(ItemModel::as_select())
                .order(vault_items::name)
                .load(conn)?;
            with_children(conn, models)
        })
    }

    fn rekey_domain_keys(
        &self,
        expected_version: i64,
        new_kek_id: [u8; 16],
        dks: &[DomainKeyRow],
        metas: &[([u8; 16], Vec<u8>)],
    ) -> Result<()> {
        self.write(|conn| {
            bump_from(conn, expected_version)?;
            diesel::update(vault::table)
                .set(vault::kek_id.eq(&new_kek_id[..]))
                .execute(conn)?;
            for dk in dks {
                replace_domain_key(conn, dk)?;
            }
            for (id, mac) in metas {
                diesel::update(vault_items::table.find(&id[..]))
                    .set(vault_items::meta_mac.eq(mac))
                    .execute(conn)?;
            }
            Ok(())
        })
    }

    fn replace_domain(
        &self,
        expected_version: i64,
        dk: &DomainKeyRow,
        fields: &[([u8; 16], FieldRow)],
    ) -> Result<()> {
        self.write(|conn| {
            bump_from(conn, expected_version)?;
            replace_domain_key(conn, dk)?;
            for (item, field) in fields {
                let changed = diesel::update(vault_fields::table.find((&item[..], &field.field)))
                    .set((
                        vault_fields::sealed.eq(&field.sealed),
                        vault_fields::previous.eq(field.previous.as_deref()),
                    ))
                    .execute(conn)?;
                if changed != 1 {
                    bail!("field not found during domain rotation");
                }
            }
            Ok(())
        })
    }

    fn vault_version(&self) -> Result<i64> {
        self.read(|conn| Ok(vault::table.select(vault::version).first(conn)?))
    }

    fn add_domain(
        &self,
        expected_version: i64,
        dk: &DomainKeyRow,
        items: &[(ItemRow, Vec<FieldRow>)],
    ) -> Result<()> {
        self.write(|conn| {
            bump_from(conn, expected_version)?;
            diesel::insert_into(domain_keys::table)
                .values(domain_key_model(dk))
                .execute(conn)
                .context("the domain already has a key")?;
            for (item, fields) in items {
                let updated = diesel::update(vault_items::table.find(&item.id[..]))
                    .set(item_model(item))
                    .execute(conn)?;
                if updated != 1 {
                    bail!("item not found while moving it to a new domain");
                }
                diesel::delete(vault_fields::table.filter(vault_fields::item_id.eq(&item.id[..])))
                    .execute(conn)?;
                for field in fields {
                    diesel::insert_into(vault_fields::table)
                        .values(field_model(&item.id, field))
                        .execute(conn)?;
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    n: i64,
}

#[cfg(test)]
impl SqliteStore {
    fn count_rows(&self, table: &str) -> Result<i64> {
        self.read(|conn| {
            Ok(
                diesel::sql_query(format!("SELECT count(*) AS n FROM {table}"))
                    .get_result::<Count>(conn)?
                    .n,
            )
        })
    }
}

/// Raw SQL for other crates' tests (`test-support` feature): tampering with stored rows.
#[cfg(feature = "test-support")]
impl SqliteStore {
    /// Runs one statement and panics on error.
    pub fn execute_for_tests(&self, sql: &str) -> usize {
        self.read(|conn| Ok(diesel::sql_query(sql).execute(conn)?))
            .expect("test SQL")
    }
    /// The sealed field records of `item`, ordered by field name.
    pub fn sealed_fields_for_tests(&self, item: &str) -> Vec<Vec<u8>> {
        self.read(|conn| {
            Ok(vault_fields::table
                .inner_join(vault_items::table)
                .filter(vault_items::name.eq(item))
                .order(vault_fields::field)
                .select(vault_fields::sealed)
                .load(conn)?)
        })
        .expect("test SQL")
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
            run_allowed: false,
            expires_at: None,
            description: None,
            field_descriptions: vec![],
            tags: vec![],
            plain_fields: vec![],
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
                previous: None,
            },
            FieldRow {
                field: "username".into(),
                sealed: vec![4, 5],
                previous: None,
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
                previous: None,
            },
        )];
        s.replace_domain(s.vault_version().unwrap(), &dk, &new_fields)
            .unwrap();
        assert!(s.domain_keys().unwrap().contains(&dk));
        let (_, f) = s.item_get("github").unwrap().unwrap();
        assert!(f.contains(&FieldRow {
            field: "password".into(),
            sealed: vec![9, 9],
            previous: None,
        }));
        assert!(f.contains(&FieldRow {
            field: "username".into(),
            sealed: vec![4, 5],
            previous: None,
        }));
    }

    #[test]
    fn add_domain_moves_items_with_their_key_atomically() {
        let (_t, s) = store();
        with_vault(&s);
        s.item_insert(&G, &item("api", 1), &fields()).unwrap();
        let version = s.vault_version().unwrap();
        let dk = DomainKeyRow {
            domain: 4,
            dk_id: [4; 16],
            wrapped: vec![4; 8],
        };
        let mut moved = item("api", 1);
        moved.domain = 4;
        moved.meta_mac = vec![7; 32];
        let sealed = vec![FieldRow {
            field: "value".into(),
            sealed: vec![9, 9],
            previous: None,
        }];
        // A stale version changes nothing.
        assert!(
            s.add_domain(version - 1, &dk, &[(moved.clone(), sealed.clone())])
                .is_err()
        );
        assert_eq!(s.domain_keys().unwrap().len(), 3);
        s.add_domain(version, &dk, &[(moved.clone(), sealed.clone())])
            .unwrap();
        assert_eq!(s.vault_version().unwrap(), version + 1);
        assert_eq!(s.domain_keys().unwrap()[3], dk);
        let (row, f) = s.item_get("api").unwrap().unwrap();
        assert_eq!((row.domain, row.meta_mac.clone()), (4, vec![7; 32]));
        assert_eq!(row.origins, vec!["https://github.com"], "origins are kept");
        assert_eq!(f, sealed);
        // The domain gets one key only.
        let version = s.vault_version().unwrap();
        assert!(s.add_domain(version, &dk, &[]).is_err());
        assert_eq!(s.vault_version().unwrap(), version);
    }
}
