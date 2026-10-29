//! Root and domain key rotation (docs/design/07-cryptography-design.md §7.8).
//! Root rotation never re-encrypts fields: it re-wraps the three domain keys and re-signs metadata.
//!
//! Rotations hold the store's exclusive `RotationLock` (`<data dir>/vault.lock` in the SQLite
//! adapter) and commit only if the vault version is unchanged since they read it, so
//! concurrent writers make them abort, not corrupt.

use crate::{
    Vault,
    vault::{account, vault_err},
};
use anyhow::{Context, Result, bail};
use keyharbor_domain::ports::{DomainKeyRow, FieldRow};
use keyharbor_domain::vault::{Domain, FieldName};
use keyharbor_vault::{
    keys::{KeyId, KeySet},
    record::{FieldAd, open_field, seal_field, wrap_domain_key},
};

pub(crate) fn next_account(vault: &str) -> String {
    format!("{vault}#next")
}

impl Vault {
    pub fn rotate_root(&mut self) -> Result<()> {
        self.rotate_root_inner(0, || {})
    }

    /// `stop_after` 1–3 aborts after that step, to test recovery.
    #[doc(hidden)]
    pub fn rotate_root_with_failpoint(&mut self, stop_after: u8) -> Result<()> {
        self.rotate_root_inner(stop_after, || {})
    }

    /// Runs `hook` after the vault was read and before the re-key commits, to test races.
    #[doc(hidden)]
    pub fn rotate_root_with_hook(&mut self, hook: impl FnOnce()) -> Result<()> {
        self.rotate_root_inner(0, hook)
    }

    fn rotate_root_inner(&mut self, stop_after: u8, hook: impl FnOnce()) -> Result<()> {
        let _lock = self.store.lock_rotation()?;
        let version = self.store.vault_version()?;
        let items = self.store.item_list()?;
        for row in &items {
            self.verified(row).with_context(|| {
                format!(
                    "item {} failed verification; refusing to re-sign it",
                    row.name
                )
            })?;
        }
        let main = account(&self.vault_id);
        let next = next_account(&main);
        let new_root = keyharbor_crypto::random::key();
        // Step 1: park the new root key next to the current one.
        self.keystore.create(&next, &new_root).map_err(vault_err)?;
        if stop_after == 1 {
            bail!("failpoint");
        }
        // Step 2: re-wrap domain keys and re-sign metadata in one transaction.
        let new_keys = KeySet::derive(&new_root, &self.vault_id);
        let kek_id = KeyId::new();
        let dks: Vec<DomainKeyRow> = self
            .domain_keys
            .iter()
            .map(|(domain, (dk_id, dk))| DomainKeyRow {
                domain: domain.as_byte(),
                dk_id: *dk_id.as_bytes(),
                wrapped: wrap_domain_key(
                    new_keys.kek(),
                    &kek_id,
                    &self.vault_id,
                    *domain,
                    dk_id,
                    dk,
                ),
            })
            .collect();
        let old_keys = std::mem::replace(&mut self.keys, new_keys);
        let committed = items
            .iter()
            .map(|row| {
                let (kind, name, origins) = self.verified_with(&old_keys, row)?;
                Ok((row.id, self.mac_for(row, kind, &name, &origins)))
            })
            .collect::<Result<Vec<_>>>()
            .and_then(|metas| {
                hook();
                self.store
                    .rekey_domain_keys(version, *kek_id.as_bytes(), &dks, &metas)
            });
        if let Err(e) = committed {
            self.keys = old_keys;
            // Nothing references the parked key yet.
            let _ = self.keystore.delete(&next);
            return Err(e);
        }
        self.kek_id = kek_id;
        if stop_after == 2 {
            bail!("failpoint");
        }
        // Step 3: promote the new root key, then drop the parked copy.
        self.keystore.replace(&main, &new_root).map_err(vault_err)?;
        if stop_after == 3 {
            bail!("failpoint");
        }
        self.keystore.delete(&next).map_err(vault_err)?;
        Ok(())
    }

    pub fn rotate_domain(&mut self, domain: Domain) -> Result<()> {
        self.rotate_domain_with_hook(domain, || {})
    }

    /// Runs `hook` after the fields were re-encrypted and before the commit, to test races.
    #[doc(hidden)]
    pub fn rotate_domain_with_hook(&mut self, domain: Domain, hook: impl FnOnce()) -> Result<()> {
        let _lock = self.store.lock_rotation()?;
        let version = self.store.vault_version()?;
        let vault = self.store.vault_get()?.context("vault not initialized")?;
        if vault.kek_id != *self.kek_id.as_bytes() {
            bail!("vault changed (keys were rotated); reopen it and retry");
        }
        let old_dk = self.domain_keys[&domain].1.clone();
        let (new_id, new_dk) = (KeyId::new(), keyharbor_crypto::random::key());
        let mut fields = Vec::new();
        for row in self.store.item_list()? {
            let (kind, _, _) = self.verified(&row)?;
            if kind.domain() != domain {
                continue;
            }
            let (_, stored) = self
                .store
                .item_get(&row.name)?
                .context("item vanished during rotation")?;
            for field in stored {
                let name =
                    FieldName::parse(&field.field).map_err(|e| anyhow::anyhow!(e.to_string()))?;
                let ad = FieldAd {
                    vault: &self.vault_id,
                    domain,
                    item: &row.id,
                    field: name.as_str(),
                };
                let reseal = |record: &[u8]| -> Result<Vec<u8>> {
                    let plain = open_field(&old_dk, &ad, record).map_err(vault_err)?;
                    Ok(seal_field(&new_dk, &new_id, &ad, &plain))
                };
                let sealed = reseal(&field.sealed)?;
                let previous = field.previous.as_deref().map(reseal).transpose()?;
                fields.push((
                    row.id,
                    FieldRow {
                        field: field.field.clone(),
                        sealed,
                        previous,
                    },
                ));
            }
        }
        let dk_row = DomainKeyRow {
            domain: domain.as_byte(),
            dk_id: *new_id.as_bytes(),
            wrapped: wrap_domain_key(
                self.keys.kek(),
                &self.kek_id,
                &self.vault_id,
                domain,
                &new_id,
                &new_dk,
            ),
        };
        hook();
        self.store.replace_domain(version, &dk_row, &fields)?;
        self.domain_keys.insert(domain, (new_id, new_dk));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{env, init, login, pref, reopen};
    use crate::{ItemSource, ItemStatus, NewItem};
    use keyharbor_domain::vault::{Domain, FieldName, ItemKind, ItemName};
    use zeroize::Zeroizing;

    fn card(name: &str, number: &str) -> NewItem {
        NewItem {
            name: ItemName::parse(name).unwrap(),
            kind: ItemKind::Card,
            fields: vec![(
                FieldName::parse("number").unwrap(),
                Zeroizing::new(number.into()),
            )],
            origins: vec![],
            origin_binding: None,
            details: Default::default(),
            status: ItemStatus::Active,
            source: ItemSource::User,
        }
    }
    fn sealed(e: &crate::tests::Env, item: &str) -> Vec<Vec<u8>> {
        crate::tests::open_store(&e.data).sealed_fields_for_tests(item)
    }
    fn next_key_exists(e: &crate::tests::Env) -> bool {
        std::fs::read_dir(&e.keys)
            .unwrap()
            .any(|f| f.unwrap().file_name().to_string_lossy().contains("#next"))
    }

    #[test]
    fn rotate_root_keeps_values() {
        let e = env();
        let mut v = init(&e);
        v.add(login("github", "pw")).unwrap();
        v.add(card("visa", "4242")).unwrap();
        v.rotate_root().unwrap();
        assert_eq!(&*v.resolve(&pref("github", "password")).unwrap(), "pw");
        let again = reopen(&e).unwrap();
        assert_eq!(&*again.resolve(&pref("visa", "number")).unwrap(), "4242");
        assert!(!again.list().unwrap().iter().any(|i| i.tampered));
        assert!(!next_key_exists(&e));
    }

    #[test]
    fn rotate_domain_only_touches_that_domain() {
        let e = env();
        let mut v = init(&e);
        v.add(login("github", "pw")).unwrap();
        v.add(card("visa", "4242")).unwrap();
        let (cred_before, card_before) = (sealed(&e, "github"), sealed(&e, "visa"));
        v.rotate_domain(Domain::Payment).unwrap();
        assert_eq!(sealed(&e, "github"), cred_before);
        assert_ne!(sealed(&e, "visa"), card_before);
        assert_eq!(&*v.resolve(&pref("visa", "number")).unwrap(), "4242");
        let again = reopen(&e).unwrap();
        assert_eq!(&*again.resolve(&pref("visa", "number")).unwrap(), "4242");
        assert_eq!(&*again.resolve(&pref("github", "password")).unwrap(), "pw");
    }

    #[test]
    fn interrupted_root_rotation_recovers() {
        for stop_after in 1..=3 {
            let e = env();
            let mut v = init(&e);
            v.add(login("github", "pw")).unwrap();
            v.add(card("visa", "4242")).unwrap();
            assert!(
                v.rotate_root_with_failpoint(stop_after).is_err(),
                "stop_after {stop_after}"
            );
            drop(v);
            let again = reopen(&e).unwrap_or_else(|err| panic!("stop_after {stop_after}: {err}"));
            assert_eq!(&*again.resolve(&pref("github", "password")).unwrap(), "pw");
            assert_eq!(&*again.resolve(&pref("visa", "number")).unwrap(), "4242");
            assert!(!next_key_exists(&e), "stop_after {stop_after}");
        }
    }

    #[test]
    fn rotation_refuses_to_resign_tampered_items() {
        let e = env();
        let mut v = init(&e);
        v.add(login("github", "pw")).unwrap();
        crate::tests::open_store(&e.data)
            .execute_for_tests("UPDATE vault_items SET origin_binding=0 WHERE name='github'");
        assert!(v.rotate_root().is_err());
        assert!(v.list().unwrap()[0].tampered);
    }

    #[test]
    fn open_does_not_delete_parked_key_during_rotation() {
        use keyharbor_domain::ports::KeyStore;
        use keyharbor_keystore::FileKeyStore;
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let parked = format!("{}#next", v.vault_id().to_uuid_string());
        FileKeyStore::new(e.keys.clone())
            .create(&parked, &keyharbor_crypto::random::key())
            .unwrap();
        let lock = std::fs::File::create(e.data.join("vault.lock")).unwrap();
        lock.lock().unwrap();
        reopen(&e).unwrap();
        assert!(
            next_key_exists(&e),
            "parked key deleted while a rotation holds the lock"
        );
        drop(lock);
        reopen(&e).unwrap();
        assert!(!next_key_exists(&e));
    }

    #[test]
    fn stale_vault_cannot_write_after_root_rotation() {
        let e = env();
        let v1 = init(&e);
        let mut v2 = reopen(&e).unwrap();
        v2.rotate_root().unwrap();
        let err = v1.add(login("late", "x")).unwrap_err().to_string();
        assert!(err.contains("vault changed"), "{err}");
        reopen(&e).unwrap().add(login("late", "x")).unwrap();
        assert!(
            !reopen(&e)
                .unwrap()
                .list()
                .unwrap()
                .iter()
                .any(|i| i.tampered)
        );
    }

    #[test]
    fn stale_vault_cannot_write_after_domain_rotation() {
        let e = env();
        let v1 = init(&e);
        let mut v2 = reopen(&e).unwrap();
        v2.rotate_domain(Domain::Payment).unwrap();
        let err = v1.add(card("visa", "4242")).unwrap_err().to_string();
        assert!(err.contains("vault changed"), "{err}");
        v1.add(login("github", "pw")).unwrap();
        let fresh = reopen(&e).unwrap();
        fresh.add(card("visa", "4242")).unwrap();
        assert_eq!(&*fresh.resolve(&pref("visa", "number")).unwrap(), "4242");
    }

    #[test]
    fn domain_rotation_aborts_when_vault_changes_midway() {
        let e = env();
        let mut v = init(&e);
        v.add(card("visa", "4242")).unwrap();
        let err = v
            .rotate_domain_with_hook(Domain::Payment, || {
                reopen(&e)
                    .unwrap()
                    .edit(
                        &ItemName::parse("visa").unwrap(),
                        crate::ItemChange {
                            set_fields: vec![(
                                FieldName::parse("number").unwrap(),
                                Zeroizing::new("5555".into()),
                            )],
                            ..Default::default()
                        },
                    )
                    .unwrap();
            })
            .unwrap_err()
            .to_string();
        assert!(err.contains("vault changed"), "{err}");
        assert_eq!(
            &*reopen(&e)
                .unwrap()
                .resolve(&pref("visa", "number"))
                .unwrap(),
            "5555"
        );
    }

    #[test]
    fn root_rotation_aborts_when_vault_changes_midway() {
        let e = env();
        let mut v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let err = v
            .rotate_root_with_hook(|| reopen(&e).unwrap().add(login("late", "x")).unwrap())
            .unwrap_err()
            .to_string();
        assert!(err.contains("vault changed"), "{err}");
        let again = reopen(&e).unwrap();
        assert!(!again.list().unwrap().iter().any(|i| i.tampered));
        assert_eq!(&*again.resolve(&pref("late", "password")).unwrap(), "x");
        assert!(!next_key_exists(&e));
    }
}
