//! Vault storage. Only ciphertext, wrapped keys and non-secret metadata cross this port.

use anyhow::Result;
use std::path::Path;

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
    /// `keyharbor run` may inject the values (signed in the metadata MAC).
    pub run_allowed: bool,
    pub status: String,
    pub source: String,
    pub meta_mac: Vec<u8>,
    pub updated_at: String,
    /// Sorted ascending.
    pub origins: Vec<String>,
}
/// `sealed` is a vault record; the field value itself never reaches storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldRow {
    pub field: String,
    pub sealed: Vec<u8>,
}

/// The key generation a writer sealed and signed with. Writes are refused if it is stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyGuard {
    pub kek_id: [u8; 16],
    pub domain: u8,
    pub dk_id: [u8; 16],
}

/// Writes bump the vault version; key rotations commit only from an expected version.
pub trait VaultStore: Send {
    /// The data directory the store lives in (rotations lock `vault.lock` there).
    fn root(&self) -> &Path;
    fn vault_get(&self) -> Result<Option<VaultRow>>;
    /// Fails with "vault already initialized" if a vault exists.
    fn vault_create(&self, vault: &VaultRow, dks: &[DomainKeyRow]) -> Result<()>;
    fn domain_keys(&self) -> Result<Vec<DomainKeyRow>>;
    /// Fails with "item already exists" on a name clash, "vault changed …" on a stale guard.
    fn item_insert(&self, guard: &KeyGuard, item: &ItemRow, fields: &[FieldRow]) -> Result<()>;
    /// Replaces the item's metadata, fields and origins.
    fn item_update(&self, guard: &KeyGuard, item: &ItemRow, fields: &[FieldRow]) -> Result<()>;
    fn item_delete(&self, name: &str) -> Result<bool>;
    fn item_get(&self, name: &str) -> Result<Option<(ItemRow, Vec<FieldRow>)>>;
    /// Every item, ordered by name, with its origins.
    fn item_list(&self) -> Result<Vec<ItemRow>>;
    /// Re-wraps all domain keys under a new KEK and replaces every item's metadata MAC atomically.
    fn rekey_domain_keys(
        &self,
        expected_version: i64,
        new_kek_id: [u8; 16],
        dks: &[DomainKeyRow],
        metas: &[([u8; 16], Vec<u8>)],
    ) -> Result<()>;
    /// Replaces one domain key and the given re-encrypted fields atomically.
    fn replace_domain(
        &self,
        expected_version: i64,
        dk: &DomainKeyRow,
        fields: &[([u8; 16], FieldRow)],
    ) -> Result<()>;
    fn vault_version(&self) -> Result<i64>;
    /// Adds a domain key and moves items into it — each item's metadata row and its
    /// re-sealed fields replaced — atomically, from an expected version. Fails, changing
    /// nothing, if the version moved or the domain already has a key.
    fn add_domain(
        &self,
        expected_version: i64,
        dk: &DomainKeyRow,
        items: &[(ItemRow, Vec<FieldRow>)],
    ) -> Result<()>;
}
