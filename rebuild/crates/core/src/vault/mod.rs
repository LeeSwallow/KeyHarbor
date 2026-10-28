//! The vault: items sealed under per-domain keys, with signed metadata (07). Opening
//! ([`open`]), changing items ([`items`]), describing them without values ([`summary`]) and the
//! plaintext paths ([`resolve`]) are split by use case; key rotation and export live in
//! `rotate` and `transfer`.

mod developer;
mod items;
mod open;
mod resolve;
mod summary;

pub use resolve::fill_template;
pub use summary::ItemUse;

use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    ports::{AuditStore, FieldRow, ItemRow, KeyGuard, KeyStore, VaultStore},
    vault::{Domain, FieldName, ItemKind, ItemName, Origin, PlaceholderRef},
};
use keyharbor_vault::{
    VaultError,
    keys::{KeyId, KeySet, SecretKey, VaultId},
    meta::{ItemMeta, meta_mac, verify_meta_mac},
    record::{FieldAd, seal_field, unwrap_domain_key},
};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

pub const MAX_FIELD_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Draft,
    Active,
    Discarded,
}
impl ItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Discarded => "discarded",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [Self::Draft, Self::Active, Self::Discarded]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSource {
    User,
    Import,
    Generated,
    Captured,
    Requested,
    Agent,
}
impl ItemSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Import => "import",
            Self::Generated => "generated",
            Self::Captured => "captured",
            Self::Requested => "requested",
            Self::Agent => "agent",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::User,
            Self::Import,
            Self::Generated,
            Self::Captured,
            Self::Requested,
            Self::Agent,
        ]
        .into_iter()
        .find(|s| s.as_str() == value)
    }
}

pub struct NewItem {
    pub name: ItemName,
    pub kind: ItemKind,
    pub fields: Vec<(FieldName, Zeroizing<String>)>,
    pub origins: Vec<Origin>,
    /// `None` uses the kind's default (on for login and card).
    pub origin_binding: Option<bool>,
    pub status: ItemStatus,
    pub source: ItemSource,
    pub details: ItemDetails,
}

/// Attributes a new item may set beyond its fields and origins; each defaults sensibly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemDetails {
    /// `None` uses the kind's default (secret and env items yes, the others no; R-36).
    pub run_allowed: Option<bool>,
}

#[derive(Default)]
pub struct ItemChange {
    pub set_fields: Vec<(FieldName, Zeroizing<String>)>,
    pub remove_fields: Vec<FieldName>,
    pub origins: Option<Vec<Origin>>,
    pub origin_binding: Option<bool>,
    pub run_allowed: Option<bool>,
    pub status: Option<ItemStatus>,
}

/// What callers may show: never a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSummary {
    pub name: String,
    pub kind: ItemKind,
    pub domain: Domain,
    pub fields: Vec<String>,
    pub origins: Vec<String>,
    pub origin_binding: bool,
    pub run_allowed: bool,
    pub status: ItemStatus,
    pub source: ItemSource,
    pub updated_at: String,
    /// Metadata MAC failed: the item is not used for input until re-signed.
    pub tampered: bool,
}

/// What a vault needs from storage: its tables and the usage record.
pub trait VaultBackend: VaultStore + AuditStore {}
impl<T: VaultStore + AuditStore> VaultBackend for T {}

/// Every vault value with its placeholder, as of `version`. Values are zeroized on drop.
pub struct MaskValues {
    pub version: i64,
    pub values: Vec<(PlaceholderRef, Zeroizing<String>)>,
}

pub struct Vault {
    pub(crate) store: Box<dyn VaultBackend>,
    pub(crate) keystore: Box<dyn KeyStore>,
    pub(crate) vault_id: VaultId,
    pub(crate) kek_id: KeyId,
    pub(crate) keys: KeySet,
    pub(crate) domain_keys: BTreeMap<Domain, (KeyId, SecretKey)>,
}

pub(crate) fn vault_err(e: VaultError) -> anyhow::Error {
    anyhow!(e.to_string())
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
        match kind.schema() {
            Some(schema) if !schema.contains(&name.as_str()) => {
                bail!(
                    "field {} is not part of a {} item",
                    name.as_str(),
                    kind.as_str()
                )
            }
            None => {
                FieldName::parse_env(name.as_str()).map_err(|e| anyhow!(e.to_string()))?;
            }
            _ => {}
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

pub(super) fn sorted_origins(origins: &[Origin]) -> Vec<String> {
    let mut out: Vec<String> = origins.iter().map(|o| o.as_str().to_owned()).collect();
    out.sort();
    out.dedup();
    out
}

impl Vault {
    /// The key generation this handle seals and signs with; stale handles are refused on write.
    pub(super) fn guard(&self, domain: Domain) -> KeyGuard {
        KeyGuard {
            kek_id: *self.kek_id.as_bytes(),
            domain: domain.as_byte(),
            dk_id: *self.domain_keys[&domain].0.as_bytes(),
        }
    }

    pub(crate) fn unwrap_all(
        store: &dyn VaultBackend,
        keys: &KeySet,
        vault_id: &VaultId,
    ) -> Result<BTreeMap<Domain, (KeyId, SecretKey)>> {
        let mut out = BTreeMap::new();
        for row in store.domain_keys()? {
            let domain = Domain::from_byte(row.domain).context("unknown domain in database")?;
            let dk_id = KeyId::from_bytes(row.dk_id);
            let dk = unwrap_domain_key(keys.kek(), vault_id, domain, &dk_id, &row.wrapped)
                .map_err(vault_err)
                .context("could not unwrap domain key")?;
            out.insert(domain, (dk_id, dk));
        }
        // A vault from before R-40 has no developer key yet; `open` adds it.
        if Domain::ALL
            .iter()
            .any(|d| *d != Domain::Developer && !out.contains_key(d))
        {
            bail!("domain keys are missing");
        }
        Ok(out)
    }

    pub fn vault_id(&self) -> &VaultId {
        &self.vault_id
    }

    /// The signed view of a stored row: its metadata in the domain the row is stored in.
    fn meta<'a>(
        &'a self,
        row: &'a ItemRow,
        domain: Domain,
        kind: ItemKind,
        name: &'a ItemName,
        origins: &'a [Origin],
    ) -> ItemMeta<'a> {
        ItemMeta {
            vault: &self.vault_id,
            item: &row.id,
            domain,
            kind,
            name,
            origin_binding: row.origin_binding,
            origins,
            status: &row.status,
            updated_at: &row.updated_at,
            run_allowed: row.run_allowed,
            expires_at: None,
            description: None,
            field_descriptions: &[],
        }
    }

    /// Signs `row` for the domain it is stored in (`row.domain`).
    pub(crate) fn mac_for(
        &self,
        row: &ItemRow,
        kind: ItemKind,
        name: &ItemName,
        origins: &[Origin],
    ) -> Vec<u8> {
        let domain = Domain::from_byte(row.domain).unwrap_or_else(|| kind.domain());
        meta_mac(
            self.keys.mac(),
            &self.meta(row, domain, kind, name, origins),
        )
        .to_vec()
    }

    pub(super) fn seal(
        &self,
        item: &[u8; 16],
        domain: Domain,
        field: &FieldName,
        value: &str,
    ) -> FieldRow {
        let (dk_id, dk) = &self.domain_keys[&domain];
        let ad = FieldAd {
            vault: &self.vault_id,
            domain,
            item,
            field: field.as_str(),
        };
        FieldRow {
            field: field.as_str().to_owned(),
            sealed: seal_field(dk, dk_id, &ad, value.as_bytes()),
        }
    }

    /// Decodes a stored row and checks its metadata MAC.
    pub(crate) fn verified(&self, row: &ItemRow) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        self.verified_with(&self.keys, row)
    }

    pub(crate) fn verified_with(
        &self,
        keys: &KeySet,
        row: &ItemRow,
    ) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
        if kind.domain().as_byte() != row.domain {
            return Err(vault_err(VaultError::Tampered));
        }
        self.verified_as_stored(keys, row)
    }

    /// Checks the MAC of `row` for the domain it is stored in, even if its kind now belongs
    /// to another one (an item written before R-40 moved secret and env items).
    pub(crate) fn verified_as_stored(
        &self,
        keys: &KeySet,
        row: &ItemRow,
    ) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
        let domain = Domain::from_byte(row.domain).context("unknown domain in database")?;
        let name = ItemName::parse(&row.name).map_err(|e| anyhow!(e.to_string()))?;
        let origins = row
            .origins
            .iter()
            .map(|o| Origin::parse(o).map_err(|e| anyhow!(e.to_string())))
            .collect::<Result<Vec<_>>>()?;
        verify_meta_mac(
            keys.mac(),
            &self.meta(row, domain, kind, &name, &origins),
            &row.meta_mac,
        )
        .map_err(vault_err)?;
        Ok((kind, name, origins))
    }
}
