//! R-40: `secret` and `env` items moved from the credentials domain into their own developer
//! domain. A vault created before that has no developer key; the first open after the
//! upgrade creates one and moves those items into it — fields re-sealed under the new key,
//! metadata re-signed — in one transaction together with the new key, so either every item
//! moved and the key exists, or nothing changed. The key's existence is what makes it run
//! once.

use super::{Vault, vault_err};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::{DomainKeyRow, FieldRow, ItemRow},
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
                Err(e) if e.to_string().contains("vault changed") => continue,
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
                let plain = open_field(&self.domain_keys[&old].1, &from, &field.sealed)
                    .map_err(vault_err)?;
                let to = FieldAd {
                    domain: Domain::Developer,
                    ..from
                };
                sealed.push(FieldRow {
                    field: field.field.clone(),
                    sealed: seal_field(&dk, &dk_id, &to, &plain),
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
}
