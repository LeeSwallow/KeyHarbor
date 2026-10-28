//! Root and domain key rotation (docs/design/07-cryptography-design.md §7.8).
//! Root rotation never re-encrypts fields: it re-wraps the three domain keys and re-signs metadata.

use crate::{Vault, account, vault_err};
use anyhow::{Context, Result, bail};
use keyharbor_domain::vault::{Domain, FieldName};
use keyharbor_sqlite::{DomainKeyRow, FieldRow};
use keyharbor_vault::{
    keys::{KeyId, KeySet, SecretKey},
    record::{FieldAd, open_field, seal_field, wrap_domain_key},
};

pub(crate) fn next_account(vault: &str) -> String {
    format!("{vault}#next")
}

impl Vault {
    pub fn rotate_root(&mut self) -> Result<()> {
        self.rotate_root_with_failpoint(0)
    }

    /// `stop_after` 1–3 aborts after that step, to test recovery. 0 runs to completion.
    #[doc(hidden)]
    pub fn rotate_root_with_failpoint(&mut self, stop_after: u8) -> Result<()> {
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
        let new_root = SecretKey::generate();
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
        let metas = items
            .iter()
            .map(|row| {
                let (kind, name, origins) = self.verified_with(&old_keys, row)?;
                Ok((row.id, self.mac_for(row, kind, &name, &origins)))
            })
            .collect::<Result<Vec<_>>>();
        let metas = match metas {
            Ok(m) => m,
            Err(e) => {
                self.keys = old_keys;
                return Err(e);
            }
        };
        if let Err(e) = self
            .store
            .rekey_domain_keys(*kek_id.as_bytes(), &dks, &metas)
        {
            self.keys = old_keys;
            return Err(e);
        }
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
        let vault = self.store.vault_get()?.context("vault not initialized")?;
        let old_dk = self.domain_keys[&domain].1.clone();
        let (new_id, new_dk) = (KeyId::new(), SecretKey::generate());
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
                let plain = open_field(&old_dk, &ad, &field.sealed).map_err(vault_err)?;
                fields.push((
                    row.id,
                    FieldRow {
                        field: field.field,
                        sealed: seal_field(&new_dk, &new_id, &ad, &plain),
                    },
                ));
            }
        }
        let kek_id = KeyId::from_bytes(vault.kek_id);
        let dk_row = DomainKeyRow {
            domain: domain.as_byte(),
            dk_id: *new_id.as_bytes(),
            wrapped: wrap_domain_key(
                self.keys.kek(),
                &kek_id,
                &self.vault_id,
                domain,
                &new_id,
                &new_dk,
            ),
        };
        self.store.replace_domain(&dk_row, &fields)?;
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
            status: ItemStatus::Active,
            source: ItemSource::User,
        }
    }
    fn sealed(e: &crate::tests::Env, item: &str) -> Vec<Vec<u8>> {
        let conn = rusqlite::Connection::open(e.data.join("keyharbor.sqlite3")).unwrap();
        let mut q = conn
            .prepare("SELECT f.sealed FROM vault_fields f JOIN vault_items i ON i.id=f.item_id WHERE i.name=?1 ORDER BY f.field")
            .unwrap();
        q.query_map([item], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
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
        let conn = rusqlite::Connection::open(e.data.join("keyharbor.sqlite3")).unwrap();
        conn.execute(
            "UPDATE vault_items SET origin_binding=0 WHERE name='github'",
            [],
        )
        .unwrap();
        assert!(v.rotate_root().is_err());
        assert!(v.list().unwrap()[0].tampered);
    }
}
