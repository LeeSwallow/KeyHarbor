//! R-40: `secret` and `env` items moved from the credentials domain into their own developer
//! domain. A vault created before that has no developer key; the first open after the
//! upgrade creates one and moves those items into it — fields re-sealed under the new key,
//! metadata re-signed — in one transaction together with the new key, so either every item
//! moved and the key exists, or nothing changed. The key's existence is what makes it run
//! once.

use crate::vault::{Vault, vault_err};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::{DomainKeyRow, FieldRow, ItemRow, StoreError},
    vault::{Domain, ItemKind},
};
use keyharbor_vault::{
    keys::KeyId,
    record::{FieldAd, open_field, seal_field, wrap_domain_key},
};

/// Concurrent writers can move the vault version between reading and committing.
const ATTEMPTS: usize = 5;

impl Vault {
    pub(super) fn ensure_developer_domain(&mut self) -> Result<()> {
        if self.domain_keys.contains_key(&Domain::Developer) {
            return Ok(());
        }
        // One process moves the items; the others wait and then find the key.
        let _lock = self.store.lock_rotation()?;
        for _ in 0..ATTEMPTS {
            if self
                .store
                .domain_keys()?
                .iter()
                .any(|row| row.domain == Domain::Developer.as_byte())
            {
                self.domain_keys = Self::unwrap_all(&*self.store, &self.keys, &self.vault_id)?;
                return Ok(());
            }
            match self.move_developer_items() {
                Ok(()) => return Ok(()),
                Err(e) if lost_a_race(&e) => continue,
                Err(e) => return Err(e),
            }
        }
        bail!("vault changed while moving items to the developer domain; open it again")
    }

    fn move_developer_items(&mut self) -> Result<()> {
        let version = self.store.vault_version()?;
        let (dk_id, dk) = (KeyId::new(), keyharbor_crypto::random::key());
        let mut moves: Vec<(ItemRow, Vec<FieldRow>)> = Vec::new();
        for row in self.store.item_list()? {
            let Some(kind) = ItemKind::parse(&row.kind) else {
                continue;
            };
            if kind.domain() != Domain::Developer || row.domain == Domain::Developer.as_byte() {
                continue;
            }
            // Only items whose metadata verifies where they are are re-signed; a tampered
            // one stays where it is and keeps failing verification.
            let Ok((kind, name, origins)) = self.verified_as_stored(&self.keys, &row) else {
                continue;
            };
            let old = Domain::from_byte(row.domain).context("unknown domain in database")?;
            let (_, fields) = self
                .store
                .item_get(&row.name)?
                .context("item vanished while moving it")?;
            let mut sealed = Vec::with_capacity(fields.len());
            for field in fields {
                let from = FieldAd {
                    vault: &self.vault_id,
                    domain: old,
                    item: &row.id,
                    field: &field.field,
                };
                let to = FieldAd {
                    domain: Domain::Developer,
                    ..from
                };
                let reseal = |record: &[u8]| -> Result<Vec<u8>> {
                    let plain =
                        open_field(&self.domain_key(old)?.1, &from, record).map_err(vault_err)?;
                    Ok(seal_field(&dk, &dk_id, &to, &plain))
                };
                sealed.push(FieldRow {
                    field: field.field.clone(),
                    sealed: reseal(&field.sealed)?,
                    previous: field.previous.as_deref().map(reseal).transpose()?,
                });
            }
            let mut moved = row.clone();
            moved.domain = Domain::Developer.as_byte();
            moved.meta_mac = self.mac_for(&moved, kind, &name, &origins);
            moves.push((moved, sealed));
        }
        let dk_row = DomainKeyRow {
            domain: Domain::Developer.as_byte(),
            dk_id: *dk_id.as_bytes(),
            wrapped: wrap_domain_key(
                self.keys.kek(),
                &self.kek_id,
                &self.vault_id,
                Domain::Developer,
                &dk_id,
                &dk,
            ),
        };
        self.store.add_domain(version, &dk_row, &moves)?;
        self.domain_keys.insert(Domain::Developer, (dk_id, dk));
        Ok(())
    }
}

/// The store refused the move because the vault moved meanwhile: decided by the variant, not
/// by the store's words (#200).
fn lost_a_race(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<StoreError>()
        .is_some_and(StoreError::is_stale)
}

#[cfg(test)]
mod tests {
    use crate::tests::{env, field, init, login, open_store, pref, reopen};
    use crate::{ItemSource, ItemStatus, NewItem};
    use keyharbor_domain::{
        ports::VaultStore,
        vault::{Domain, ItemKind, ItemName},
    };

    fn item(name: &str, kind: ItemKind, fields: &[(&str, &str)]) -> NewItem {
        NewItem {
            name: ItemName::parse(name).unwrap(),
            kind,
            fields: fields.iter().map(|(k, v)| field(k, v)).collect(),
            origins: vec![],
            origin_binding: None,
            status: ItemStatus::Active,
            source: ItemSource::User,
            details: Default::default(),
        }
    }

    /// A vault as an earlier build left it: three domain keys, secret and env items sealed
    /// and signed in the credentials domain.
    fn legacy_vault(e: &crate::tests::Env) {
        let mut v = init(e);
        v.add(login("github", "KH-SYNTH-gh")).unwrap();
        v.0.domain_keys.remove(&Domain::Developer);
        open_store(&e.data).execute_for_tests("DELETE FROM domain_keys WHERE domain = 4");
        for new in [
            item("api", ItemKind::Secret, &[("value", "KH-SYNTH-api")]),
            item(
                "app",
                ItemKind::Env,
                &[("API_KEY", "KH-SYNTH-env"), ("DB", "KH-SYNTH-db")],
            ),
        ] {
            v.add_in_domain(new, Domain::Credentials).unwrap();
        }
    }

    #[test]
    fn opening_an_old_vault_moves_secret_and_env_items_once() {
        let e = env();
        legacy_vault(&e);
        assert_eq!(open_store(&e.data).domain_keys().unwrap().len(), 3);
        let before = open_store(&e.data).sealed_fields_for_tests("app");
        let v = reopen(&e).unwrap();
        // Same values, new domain, new ciphertext under the developer key.
        assert_eq!(&*v.resolve(&pref("api", "value")).unwrap(), "KH-SYNTH-api");
        assert_eq!(
            &*v.resolve(&pref("app", "API_KEY")).unwrap(),
            "KH-SYNTH-env"
        );
        assert_eq!(
            &*v.resolve(&pref("github", "password")).unwrap(),
            "KH-SYNTH-gh"
        );
        let list = v.list().unwrap();
        let domain = |name: &str| list.iter().find(|i| i.name == name).unwrap();
        assert_eq!(domain("api").domain, Domain::Developer);
        assert_eq!(domain("app").domain, Domain::Developer);
        assert_eq!(domain("github").domain, Domain::Credentials);
        assert!(list.iter().all(|i| !i.tampered));
        let after = open_store(&e.data).sealed_fields_for_tests("app");
        assert_ne!(before, after);
        let rows = open_store(&e.data).item_list().unwrap();
        assert!(
            rows.iter()
                .filter(|r| r.name != "github")
                .all(|r| r.domain == 4)
        );
        // Opening again changes nothing.
        let version = v.version().unwrap();
        drop(v);
        let v = reopen(&e).unwrap();
        assert_eq!(v.version().unwrap(), version);
        assert_eq!(open_store(&e.data).sealed_fields_for_tests("app"), after);
        assert_eq!(open_store(&e.data).domain_keys().unwrap().len(), 4);
    }

    #[test]
    fn a_tampered_item_is_not_laundered_by_the_move() {
        let e = env();
        legacy_vault(&e);
        open_store(&e.data)
            .execute_for_tests("UPDATE vault_items SET origin_binding = 1 WHERE name = 'api'");
        let v = reopen(&e).unwrap();
        let list = v.list().unwrap();
        let api = list.iter().find(|i| i.name == "api").unwrap();
        assert!(api.tampered);
        assert!(v.resolve(&pref("api", "value")).is_err());
        assert_eq!(&*v.resolve(&pref("app", "DB")).unwrap(), "KH-SYNTH-db");
    }

    #[test]
    fn new_vaults_have_four_domain_keys_and_seal_secrets_in_the_developer_domain() {
        let e = env();
        let v = init(&e);
        v.add(item("api", ItemKind::Secret, &[("value", "KH-SYNTH-new")]))
            .unwrap();
        assert_eq!(open_store(&e.data).domain_keys().unwrap().len(), 4);
        assert_eq!(open_store(&e.data).item_list().unwrap()[0].domain, 4);
        assert_eq!(&*v.resolve(&pref("api", "value")).unwrap(), "KH-SYNTH-new");
    }

    /// A store that refuses `add_domain` with `refusal` the first `refusals` times and
    /// otherwise passes every call to the real store.
    mod refusing {
        use keyharbor_domain::{
            audit::{AuditEvent, AuditRecord},
            ports::{
                AuditStore, DomainKeyRow, FieldRow, ItemRow, KeyGuard, RotationGuard, RotationLock,
                SettingsStore, StoreError, VaultRow, VaultStore,
            },
        };
        use keyharbor_sqlite::SqliteStore;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        type Result<T, E = StoreError> = std::result::Result<T, E>;

        pub struct RefusingStore {
            pub inner: SqliteStore,
            pub refusals: usize,
            pub refusal: fn() -> StoreError,
            pub add_domain_calls: Arc<AtomicUsize>,
        }

        impl VaultStore for RefusingStore {
            fn add_domain(
                &self,
                expected_version: i64,
                dk: &DomainKeyRow,
                items: &[(ItemRow, Vec<FieldRow>)],
            ) -> Result<()> {
                let call = self.add_domain_calls.fetch_add(1, Ordering::SeqCst);
                if call < self.refusals {
                    return Err((self.refusal)());
                }
                self.inner.add_domain(expected_version, dk, items)
            }
            fn vault_get(&self) -> Result<Option<VaultRow>> {
                self.inner.vault_get()
            }
            fn vault_create(&self, vault: &VaultRow, dks: &[DomainKeyRow]) -> Result<()> {
                self.inner.vault_create(vault, dks)
            }
            fn domain_keys(&self) -> Result<Vec<DomainKeyRow>> {
                self.inner.domain_keys()
            }
            fn item_insert(&self, g: &KeyGuard, item: &ItemRow, f: &[FieldRow]) -> Result<()> {
                self.inner.item_insert(g, item, f)
            }
            fn item_update(&self, g: &KeyGuard, item: &ItemRow, f: &[FieldRow]) -> Result<()> {
                self.inner.item_update(g, item, f)
            }
            fn item_replace(
                &self,
                g: &KeyGuard,
                replaced: &str,
                item: &ItemRow,
                f: &[FieldRow],
            ) -> Result<()> {
                self.inner.item_replace(g, replaced, item, f)
            }
            fn item_delete(&self, name: &str) -> Result<bool> {
                self.inner.item_delete(name)
            }
            fn item_get(&self, name: &str) -> Result<Option<(ItemRow, Vec<FieldRow>)>> {
                self.inner.item_get(name)
            }
            fn item_list(&self) -> Result<Vec<ItemRow>> {
                self.inner.item_list()
            }
            fn rekey_domain_keys(
                &self,
                version: i64,
                kek: [u8; 16],
                dks: &[DomainKeyRow],
                metas: &[([u8; 16], Vec<u8>)],
            ) -> Result<()> {
                self.inner.rekey_domain_keys(version, kek, dks, metas)
            }
            fn replace_domain(
                &self,
                version: i64,
                dk: &DomainKeyRow,
                fields: &[([u8; 16], FieldRow)],
            ) -> Result<()> {
                self.inner.replace_domain(version, dk, fields)
            }
            fn vault_version(&self) -> Result<i64> {
                self.inner.vault_version()
            }
        }
        impl RotationLock for RefusingStore {
            fn lock_rotation(&self) -> Result<RotationGuard> {
                self.inner.lock_rotation()
            }
            fn try_lock_rotation(&self) -> Result<Option<RotationGuard>> {
                self.inner.try_lock_rotation()
            }
        }
        impl AuditStore for RefusingStore {
            fn record(&self, event: &AuditEvent) -> Result<bool> {
                self.inner.record(event)
            }
            fn list_audit(&self, limit: Option<usize>) -> Result<Vec<AuditRecord>> {
                self.inner.list_audit(limit)
            }
        }
        impl SettingsStore for RefusingStore {
            fn setting_get(&self, key: &str) -> anyhow::Result<Option<String>> {
                self.inner.setting_get(key)
            }
            fn setting_set(&self, key: &str, value: &str) -> anyhow::Result<()> {
                self.inner.setting_set(key, value)
            }
        }
    }

    fn open_refusing(
        e: &crate::tests::Env,
        refusal: fn() -> keyharbor_domain::ports::StoreError,
    ) -> (
        anyhow::Result<crate::Vault>,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let calls = std::sync::Arc::default();
        let store = refusing::RefusingStore {
            inner: open_store(&e.data),
            refusals: 1,
            refusal,
            add_domain_calls: std::sync::Arc::clone(&calls),
        };
        let opened = crate::Vault::open(
            store,
            Box::new(keyharbor_keystore::FileKeyStore::new(e.keys.clone())),
            crate::tests::test_clock(),
        );
        (opened, calls)
    }

    #[test]
    fn a_stale_store_error_retries_the_move() {
        use keyharbor_domain::ports::{Staleness, StoreError};
        let e = env();
        legacy_vault(&e);
        let (opened, calls) = open_refusing(&e, || StoreError::Stale(Staleness::VersionMoved));
        let v = crate::tests::TestVault(opened.unwrap());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(open_store(&e.data).domain_keys().unwrap().len(), 4);
        assert_eq!(&*v.resolve(&pref("api", "value")).unwrap(), "KH-SYNTH-api");
    }

    #[test]
    fn the_words_vault_changed_alone_do_not_retry() {
        use keyharbor_domain::ports::StoreError;
        let e = env();
        legacy_vault(&e);
        let (opened, calls) = open_refusing(&e, || {
            StoreError::backend(std::io::Error::other("vault changed, but not a race"))
        });
        let err = opened.err().expect("a backend error is not retried");
        assert_eq!(err.to_string(), "vault changed, but not a race");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(open_store(&e.data).domain_keys().unwrap().len(), 3);
    }
}
