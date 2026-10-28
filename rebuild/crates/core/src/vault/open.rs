//! Creating and opening the vault, including finishing an interrupted root-key rotation.

use super::{Vault, VaultBackend, account, vault_err};
use crate::rotate;
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::{DomainKeyRow, KeyStore, VaultRow},
    vault::Domain,
};
use keyharbor_vault::{
    VaultError,
    keys::{KeyId, KeySet, SecretKey, VaultId},
    record::wrap_domain_key,
};
use std::collections::BTreeMap;

impl Vault {
    pub fn init(store: impl VaultBackend + 'static, keystore: Box<dyn KeyStore>) -> Result<Vault> {
        let store: Box<dyn VaultBackend> = Box::new(store);
        if store.vault_get()?.is_some() {
            bail!("vault already initialized");
        }
        let vault_id = VaultId::new();
        let root = SecretKey::generate();
        keystore
            .create(&account(&vault_id), &root)
            .map_err(vault_err)?;
        let keys = KeySet::derive(&root, &vault_id);
        let kek_id = KeyId::new();
        let mut domain_keys = BTreeMap::new();
        let mut rows = Vec::new();
        for domain in Domain::ALL {
            let (dk_id, dk) = (KeyId::new(), SecretKey::generate());
            rows.push(DomainKeyRow {
                domain: domain.as_byte(),
                dk_id: *dk_id.as_bytes(),
                wrapped: wrap_domain_key(keys.kek(), &kek_id, &vault_id, domain, &dk_id, &dk),
            });
            domain_keys.insert(domain, (dk_id, dk));
        }
        let created = store.vault_create(
            &VaultRow {
                vault_id: *vault_id.as_bytes(),
                kek_id: *kek_id.as_bytes(),
                version: 0,
            },
            &rows,
        );
        if let Err(e) = created {
            // Another process won the race; do not leave an orphan key behind.
            let _ = keystore.delete(&account(&vault_id));
            return Err(e);
        }
        Ok(Vault {
            store,
            keystore,
            vault_id,
            kek_id,
            keys,
            domain_keys,
        })
    }

    pub fn open(store: impl VaultBackend + 'static, keystore: Box<dyn KeyStore>) -> Result<Vault> {
        let store: Box<dyn VaultBackend> = Box::new(store);
        let row = store.vault_get()?.context("vault not initialized")?;
        let vault_id = VaultId::from_bytes(row.vault_id);
        let main = account(&vault_id);
        let next = rotate::next_account(&main);
        let get_main = || match keystore.get(&main) {
            Ok(root) => Ok(root),
            Err(VaultError::KeyNotFound) => {
                bail!("vault key not found; refusing to create a new one")
            }
            Err(e) => Err(vault_err(e)),
        };
        let keys = KeySet::derive(&get_main()?, &vault_id);
        let (keys, domain_keys) = match Self::unwrap_all(&*store, &keys, &vault_id) {
            Ok(domain_keys) => {
                // A parked key left by an interrupted rotation is only safe to drop when no
                // rotation is running; otherwise it may be the key the DB is about to depend on.
                if let Some(_lock) = rotate::try_rotation_lock(&*store)? {
                    match keystore.delete(&next) {
                        Ok(()) | Err(VaultError::KeyNotFound) => {}
                        Err(e) => return Err(vault_err(e)),
                    }
                }
                (keys, domain_keys)
            }
            Err(original) => {
                // The DB was re-keyed under a root key that is not promoted yet. Wait for any
                // running rotation, then retry the main key before finishing the promotion.
                let _lock = rotate::rotation_lock(&*store)?;
                let keys = KeySet::derive(&get_main()?, &vault_id);
                if let Ok(domain_keys) = Self::unwrap_all(&*store, &keys, &vault_id) {
                    (keys, domain_keys)
                } else {
                    let Ok(parked) = keystore.get(&next) else {
                        return Err(original);
                    };
                    let parked_keys = KeySet::derive(&parked, &vault_id);
                    let domain_keys =
                        Self::unwrap_all(&*store, &parked_keys, &vault_id).map_err(|_| original)?;
                    keystore.replace(&main, &parked).map_err(vault_err)?;
                    keystore.delete(&next).map_err(vault_err)?;
                    (parked_keys, domain_keys)
                }
            }
        };
        let kek_id = KeyId::from_bytes(store.vault_get()?.context("vault not initialized")?.kek_id);
        let mut vault = Vault {
            store,
            keystore,
            vault_id,
            kek_id,
            keys,
            domain_keys,
        };
        vault.ensure_developer_domain()?;
        Ok(vault)
    }
}
