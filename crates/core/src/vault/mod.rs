//! The vault: items sealed under per-domain keys, with signed metadata (07). The words it
//! speaks are in [`item`], sealing and signing in [`signing`]. Opening, key rotation and the
//! R-40 developer-domain move are in [`keys`]; changing items ([`items`]), drafts ([`drafts`]),
//! describing items without values ([`summary`]) and the plaintext paths ([`resolve`]) are
//! split by use case. Export and import live in `crate::transfer`.

mod drafts;
mod history;
mod item;
mod items;
mod keys;
mod plain;
mod resolve;
mod signing;
mod summary;

pub use drafts::{DraftItem, GeneratedDraft, GeneratedLogin, generate_password};
pub use item::{
    ExpiryChoice, ItemChange, ItemDetails, ItemSource, ItemStatus, ItemSummary, NewItem,
};
pub use resolve::{OneTimeCode, fill_placeholders, fill_template};
pub use summary::{CatalogEntry, ItemFilter, ItemUse, catalog, effective_descriptions};

use anyhow::{Context, Result, bail};
use keyharbor_crypto::otp::OtpSeed;
use keyharbor_domain::{
    ports::{
        AuditStore, Clock, FieldRow, ItemRow, KeyStore, RotationLock, SettingsStore, VaultStore,
    },
    vault::{Domain, FieldName, ItemKind, ItemName, Origin, PlaceholderRef},
};
use keyharbor_vault::{
    VaultError,
    keys::{KeyId, KeySet, SecretKey, VaultId},
};
use std::{collections::BTreeMap, sync::Arc};
use zeroize::Zeroizing;

/// The largest value one field holds. Fixed, not a setting (J-274): it bounds what is sealed,
/// masked and scanned for in every page response, and a key file or certificate fits.
pub const MAX_FIELD_BYTES: usize = 64 * 1024;

/// What a vault needs from storage: its tables, the usage record and the settings
/// (`default_item_ttl`).
pub trait VaultBackend: VaultStore + RotationLock + AuditStore + SettingsStore {}
impl<T: VaultStore + RotationLock + AuditStore + SettingsStore> VaultBackend for T {}

pub use keyharbor_domain::settings::{DEFAULT_ITEM_TTL, NO_TTL};

/// Every vault value with its placeholder, as of `version`. Values are zeroized on drop.
pub struct MaskValues {
    pub version: i64,
    pub values: Vec<(PlaceholderRef, Zeroizing<String>)>,
}

pub struct Vault {
    pub(crate) store: Box<dyn VaultBackend>,
    pub(crate) keystore: Box<dyn KeyStore>,
    /// Dates for expiry and `updated_at`.
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) vault_id: VaultId,
    pub(crate) kek_id: KeyId,
    pub(crate) keys: KeySet,
    pub(crate) domain_keys: BTreeMap<Domain, (KeyId, SecretKey)>,
}

/// Keeps the `VaultError` as the error itself (not only its text), so callers can downcast it.
pub(crate) fn vault_err(e: VaultError) -> anyhow::Error {
    anyhow::Error::new(e)
}

pub(crate) fn account(vault_id: &VaultId) -> String {
    vault_id.to_uuid_string()
}

pub(super) fn check_fields(
    kind: ItemKind,
    fields: &[(FieldName, Zeroizing<String>)],
) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for (name, value) in fields {
        if kind.field(name.as_str()).is_none() {
            bail!(
                "field {} is not part of a {} item",
                name.as_str(),
                kind.as_str()
            )
        }
        if kind.spec().free_fields {
            FieldName::parse_env(name.as_str())?;
        }
        if value.len() > MAX_FIELD_BYTES {
            bail!("field {} exceeds {} bytes", name.as_str(), MAX_FIELD_BYTES);
        }
        if !seen.insert(name.as_str().to_owned()) {
            bail!("field {} given twice", name.as_str());
        }
    }
    Ok(())
}

/// Refuses a one-time code seed that is neither base32 nor an `otpauth://totp/` URI (#206).
/// The error names the field, never the value. Imports skip it: a backup from before #206 may
/// hold a seed field nobody could use, and refusing it would refuse the whole restore.
pub(super) fn check_seeds(kind: ItemKind, fields: &[(FieldName, Zeroizing<String>)]) -> Result<()> {
    for (name, value) in fields {
        if kind.field(name.as_str()).is_some_and(|spec| spec.otp_seed) {
            OtpSeed::parse(value).map_err(|e| anyhow::anyhow!("field {}: {e}", name.as_str()))?;
        }
    }
    Ok(())
}

/// The vault, opened with `open`, or `None` while none has been made. Tells from the store
/// alone, so the key store is not touched (no keychain prompt) when there is no vault.
pub fn open_existing(
    store: &dyn VaultStore,
    open: impl FnOnce() -> Result<Vault>,
) -> Result<Option<Vault>> {
    if store.vault_get()?.is_none() {
        return Ok(None);
    }
    open().map(Some)
}

pub(super) fn sorted_origins(origins: &[Origin]) -> Vec<String> {
    let mut out: Vec<String> = origins.iter().map(|o| o.as_str().to_owned()).collect();
    out.sort();
    out.dedup();
    out
}

impl Vault {
    pub fn vault_id(&self) -> &VaultId {
        &self.vault_id
    }

    /// The key of `domain`. Every domain has one once the vault is open: `init` makes all of
    /// them and `open` adds the developer key a vault from before R-40 lacks
    /// (`ensure_developer_domain`). An error here means that invariant broke, not bad input.
    pub(crate) fn domain_key(&self, domain: Domain) -> Result<&(KeyId, SecretKey)> {
        self.domain_keys
            .get(&domain)
            .with_context(|| format!("no key for the {} domain", domain.as_str()))
    }

    /// The stored row and sealed fields of the item `name`; "item not found" if there is none.
    pub(super) fn load_item_row(&self, name: &ItemName) -> Result<(ItemRow, Vec<FieldRow>)> {
        self.store
            .item_get(name.as_str())?
            .context("item not found")
    }
}

#[cfg(test)]
mod open_existing_tests {
    use super::open_existing;
    use crate::tests::{env, init, open_store, reopen};

    #[test]
    fn no_vault_is_none_without_opening_and_a_made_one_opens() {
        let e = env();
        let none = open_existing(&open_store(&e.data), || unreachable!()).unwrap();
        assert!(none.is_none());
        init(&e);
        let some = open_existing(&open_store(&e.data), || reopen(&e).map(|v| v.0)).unwrap();
        assert!(some.is_some());
    }
}

#[cfg(test)]
mod custom_field_tests {
    use crate::{
        ItemDetails, ItemSource, ItemStatus, NewItem,
        tests::{env, field, init},
    };
    use keyharbor_domain::vault::{ItemKind, ItemName};

    fn item(kind: ItemKind, fields: &[(&str, &str)]) -> NewItem {
        NewItem {
            name: ItemName::parse("me").unwrap(),
            kind,
            fields: fields.iter().map(|(n, v)| field(n, v)).collect(),
            origins: vec![],
            origin_binding: None,
            status: ItemStatus::Active,
            source: ItemSource::User,
            details: ItemDetails::default(),
        }
    }

    #[test]
    fn identity_accepts_a_custom_field() {
        let e = env();
        let vault = init(&e);
        vault
            .add(item(
                ItemKind::Identity,
                &[
                    ("full_name", "Hong Gildong"),
                    ("school", "Seoul National University"),
                ],
            ))
            .unwrap();
        let listed = vault.list().unwrap().remove(0);
        assert!(
            listed.fields.iter().any(|f| f == "school"),
            "{:?}",
            listed.fields
        );
        assert!(
            listed.plain_values.is_empty(),
            "custom fields are not shown"
        );
        let masked = vault.mask_values().unwrap().values;
        assert!(
            masked
                .iter()
                .any(|(_, v)| v.as_str() == "Seoul National University"),
            "custom values are masked"
        );
    }

    #[test]
    fn a_login_takes_a_custom_field_and_env_still_takes_only_variable_names() {
        let e = env();
        let vault = init(&e);
        vault
            .add(item(
                ItemKind::Login,
                &[
                    ("password", "pw-1234-long"),
                    ("second_password", "2nd-pw-9876"),
                ],
            ))
            .unwrap();
        assert!(
            vault.list().unwrap()[0]
                .fields
                .contains(&"second_password".to_owned())
        );
        let err = vault
            .add(item(ItemKind::Env, &[("school", "x")]))
            .unwrap_err();
        assert!(err.to_string().contains("environment variable"), "{err}");
    }
}
