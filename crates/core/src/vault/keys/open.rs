//! Creating and opening the vault, including finishing an interrupted root-key rotation.

use super::rotate;
use crate::vault::{Vault, VaultBackend, account, vault_err};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::{Clock, DomainKeyRow, KeyStore, VaultRow},
    vault::Domain,
};
use keyharbor_vault::{
    VaultError,
    keys::{KeyId, KeySet, SecretKey, VaultId},
    record::wrap_domain_key,
};
use std::{collections::BTreeMap, sync::Arc};

impl Vault {
    pub fn init(
        store: impl VaultBackend + 'static,
        keystore: Box<dyn KeyStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Vault> {
        let store: Box<dyn VaultBackend> = Box::new(store);
        if store.vault_get()?.is_some() {
            bail!("vault already initialized");
        }
        let vault_id = VaultId::new();
        let root = keyharbor_crypto::random::key();
        keystore
            .create(&account(&vault_id), &root)
            .map_err(vault_err)?;
        let keys = KeySet::derive(&root, &vault_id);
        let kek_id = KeyId::new();
        let mut domain_keys = BTreeMap::new();
        let mut rows = Vec::new();
        for domain in Domain::ALL {
            let (dk_id, dk) = (KeyId::new(), keyharbor_crypto::random::key());
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
            clock,
            vault_id,
            kek_id,
            keys,
            domain_keys,
        })
    }

    pub fn open(
        store: impl VaultBackend + 'static,
        keystore: Box<dyn KeyStore>,
        clock: Arc<dyn Clock>,
    ) -> Result<Vault> {
        let store: Box<dyn VaultBackend> = Box::new(store);
        let row = store.vault_get()?.context("vault not initialized")?;
        let vault_id = VaultId::from_bytes(row.vault_id);
        let main = account(&vault_id);
        let next = rotate::next_account(&main);
        let keys = KeySet::derive(&main_root(&*keystore, &main)?, &vault_id);
        let (keys, domain_keys) = match Self::unwrap_all(&*store, &keys, &vault_id) {
            Ok(domain_keys) => {
                // A parked key left by an interrupted rotation is only safe to drop when no
                // rotation is running; otherwise it may be the key the DB is about to depend on.
                if let Some(_lock) = store.try_lock_rotation()? {
                    match keystore.delete(&next) {
                        Ok(()) | Err(VaultError::KeyNotFound) => {}
                        Err(e) => return Err(vault_err(e)),
                    }
                }
                (keys, domain_keys)
            }
            Err(original) => {
                Self::recover_parked_root(&*store, &*keystore, &vault_id, &main, &next, original)?
            }
        };
        let kek_id = KeyId::from_bytes(store.vault_get()?.context("vault not initialized")?.kek_id);
        let mut vault = Vault {
            store,
            keystore,
            clock,
            vault_id,
            kek_id,
            keys,
            domain_keys,
        };
        vault.ensure_developer_domain()?;
        Ok(vault)
    }

    /// The DB was re-keyed under a root key that is not promoted yet. Waits for any running
    /// rotation, then retries the main key before finishing the promotion of the parked one.
    /// `original` is the error returned when neither key opens the vault.
    fn recover_parked_root(
        store: &dyn VaultBackend,
        keystore: &dyn KeyStore,
        vault_id: &VaultId,
        main: &str,
        next: &str,
        original: anyhow::Error,
    ) -> Result<(KeySet, DomainKeys)> {
        let _lock = store.lock_rotation()?;
        let keys = KeySet::derive(&main_root(keystore, main)?, vault_id);
        if let Ok(domain_keys) = Self::unwrap_all(store, &keys, vault_id) {
            return Ok((keys, domain_keys));
        }
        let Ok(parked) = keystore.get(next) else {
            return Err(original);
        };
        let parked_keys = KeySet::derive(&parked, vault_id);
        let domain_keys = Self::unwrap_all(store, &parked_keys, vault_id).map_err(|_| original)?;
        keystore.replace(main, &parked).map_err(vault_err)?;
        keystore.delete(next).map_err(vault_err)?;
        Ok((parked_keys, domain_keys))
    }
}

type DomainKeys = BTreeMap<Domain, (KeyId, SecretKey)>;

/// The root key under `main`. A missing key is an error, never a reason to make a new one.
fn main_root(keystore: &dyn KeyStore, main: &str) -> Result<SecretKey> {
    match keystore.get(main) {
        Ok(root) => Ok(root),
        Err(VaultError::KeyNotFound) => {
            bail!("vault key not found; refusing to create a new one")
        }
        Err(e) => Err(vault_err(e)),
    }
}
