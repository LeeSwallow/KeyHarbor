//! Vault use cases shared by the CLI, MCP server and admin app.
//! Plaintext leaves this crate only through `Vault::resolve`, which the CLI never calls.

mod dotenv;
mod rotate;
mod transfer;

pub use dotenv::parse_dotenv;
pub use transfer::ImportReport;

use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::vault::{Domain, FieldName, ItemKind, ItemName, Origin, PlaceholderRef};
use keyharbor_sqlite::{DomainKeyRow, FieldRow, ItemRow, SqliteStore, VaultRow};
use keyharbor_vault::{
    VaultError,
    keys::{KeyId, KeySet, SecretKey, VaultId},
    keystore::KeyStore,
    meta::{ItemMeta, meta_mac, verify_meta_mac},
    record::{FieldAd, open_field, seal_field, unwrap_domain_key, wrap_domain_key},
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
}

#[derive(Default)]
pub struct ItemChange {
    pub set_fields: Vec<(FieldName, Zeroizing<String>)>,
    pub remove_fields: Vec<FieldName>,
    pub origins: Option<Vec<Origin>>,
    pub origin_binding: Option<bool>,
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
    pub status: ItemStatus,
    pub source: ItemSource,
    pub updated_at: String,
    /// Metadata MAC failed: the item is not used for input until re-signed.
    pub tampered: bool,
}

pub struct Vault {
    store: SqliteStore,
    keystore: Box<dyn KeyStore>,
    vault_id: VaultId,
    keys: KeySet,
    domain_keys: BTreeMap<Domain, (KeyId, SecretKey)>,
}

pub(crate) fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil-from-days (Howard Hinnant), valid for the Unix era.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn vault_err(e: VaultError) -> anyhow::Error {
    anyhow!(e.to_string())
}

pub(crate) fn account(vault_id: &VaultId) -> String {
    vault_id.to_uuid_string()
}

fn check_fields(kind: ItemKind, fields: &[(FieldName, Zeroizing<String>)]) -> Result<()> {
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

fn sorted_origins(origins: &[Origin]) -> Vec<String> {
    let mut out: Vec<String> = origins.iter().map(|o| o.as_str().to_owned()).collect();
    out.sort();
    out.dedup();
    out
}

impl Vault {
    pub fn init(store: SqliteStore, keystore: Box<dyn KeyStore>) -> Result<Vault> {
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
            keys,
            domain_keys,
        })
    }

    pub fn open(store: SqliteStore, keystore: Box<dyn KeyStore>) -> Result<Vault> {
        let row = store.vault_get()?.context("vault not initialized")?;
        let vault_id = VaultId::from_bytes(row.vault_id);
        let root = match keystore.get(&account(&vault_id)) {
            Ok(root) => root,
            Err(VaultError::KeyNotFound) => {
                bail!("vault key not found; refusing to create a new one")
            }
            Err(e) => return Err(vault_err(e)),
        };
        let main = account(&vault_id);
        let next = rotate::next_account(&main);
        let keys = KeySet::derive(&root, &vault_id);
        let (keys, domain_keys) = match Self::unwrap_all(&store, &keys, &vault_id) {
            Ok(domain_keys) => {
                // A parked key from an interrupted rotation that never reached the DB, or that was
                // already promoted, is no longer needed.
                match keystore.delete(&next) {
                    Ok(()) | Err(VaultError::KeyNotFound) => {}
                    Err(e) => return Err(vault_err(e)),
                }
                (keys, domain_keys)
            }
            Err(original) => {
                // The DB was re-keyed but the new root key was not promoted yet: finish the rotation.
                let Ok(parked) = keystore.get(&next) else {
                    return Err(original);
                };
                let parked_keys = KeySet::derive(&parked, &vault_id);
                let domain_keys =
                    Self::unwrap_all(&store, &parked_keys, &vault_id).map_err(|_| original)?;
                keystore.replace(&main, &parked).map_err(vault_err)?;
                keystore.delete(&next).map_err(vault_err)?;
                (parked_keys, domain_keys)
            }
        };
        Ok(Vault {
            store,
            keystore,
            vault_id,
            keys,
            domain_keys,
        })
    }

    pub(crate) fn unwrap_all(
        store: &SqliteStore,
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
        if out.len() != Domain::ALL.len() {
            bail!("domain keys are missing");
        }
        Ok(out)
    }

    pub fn vault_id(&self) -> &VaultId {
        &self.vault_id
    }

    pub(crate) fn mac_for(
        &self,
        row: &ItemRow,
        kind: ItemKind,
        name: &ItemName,
        origins: &[Origin],
    ) -> Vec<u8> {
        meta_mac(
            self.keys.mac(),
            &ItemMeta {
                vault: &self.vault_id,
                item: &row.id,
                domain: kind.domain(),
                kind,
                name,
                origin_binding: row.origin_binding,
                origins,
                status: &row.status,
                updated_at: &row.updated_at,
            },
        )
        .to_vec()
    }

    fn seal(&self, item: &[u8; 16], domain: Domain, field: &FieldName, value: &str) -> FieldRow {
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
    fn verified(&self, row: &ItemRow) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        self.verified_with(&self.keys, row)
    }

    pub(crate) fn verified_with(
        &self,
        keys: &KeySet,
        row: &ItemRow,
    ) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
        let name = ItemName::parse(&row.name).map_err(|e| anyhow!(e.to_string()))?;
        let origins = row
            .origins
            .iter()
            .map(|o| Origin::parse(o).map_err(|e| anyhow!(e.to_string())))
            .collect::<Result<Vec<_>>>()?;
        if kind.domain().as_byte() != row.domain {
            return Err(vault_err(VaultError::Tampered));
        }
        verify_meta_mac(
            keys.mac(),
            &ItemMeta {
                vault: &self.vault_id,
                item: &row.id,
                domain: kind.domain(),
                kind,
                name: &name,
                origin_binding: row.origin_binding,
                origins: &origins,
                status: &row.status,
                updated_at: &row.updated_at,
            },
            &row.meta_mac,
        )
        .map_err(vault_err)?;
        Ok((kind, name, origins))
    }

    pub fn add(&self, item: NewItem) -> Result<()> {
        check_fields(item.kind, &item.fields)?;
        let id = *KeyId::new().as_bytes();
        let domain = item.kind.domain();
        let mut row = ItemRow {
            id,
            name: item.name.as_str().to_owned(),
            domain: domain.as_byte(),
            kind: item.kind.as_str().to_owned(),
            origin_binding: item
                .origin_binding
                .unwrap_or_else(|| item.kind.default_origin_binding()),
            status: item.status.as_str().to_owned(),
            source: item.source.as_str().to_owned(),
            meta_mac: Vec::new(),
            updated_at: now_rfc3339(),
            origins: sorted_origins(&item.origins),
        };
        row.meta_mac = self.mac_for(&row, item.kind, &item.name, &item.origins);
        let fields: Vec<FieldRow> = item
            .fields
            .iter()
            .map(|(name, value)| self.seal(&id, domain, name, value))
            .collect();
        self.store.item_insert(&row, &fields)
    }

    pub fn edit(&self, name: &ItemName, change: ItemChange) -> Result<()> {
        let (mut row, fields) = self
            .store
            .item_get(name.as_str())?
            .context("item not found")?;
        let (kind, name, origins) = self.verified(&row)?;
        check_fields(kind, &change.set_fields)?;
        let domain = kind.domain();
        let mut kept: Vec<FieldRow> = fields
            .into_iter()
            .filter(|f| {
                !change.remove_fields.iter().any(|r| r.as_str() == f.field)
                    && !change.set_fields.iter().any(|(s, _)| s.as_str() == f.field)
            })
            .collect();
        kept.extend(
            change
                .set_fields
                .iter()
                .map(|(n, v)| self.seal(&row.id, domain, n, v)),
        );
        let origins = change.origins.unwrap_or(origins);
        row.origins = sorted_origins(&origins);
        if let Some(binding) = change.origin_binding {
            row.origin_binding = binding;
        }
        if let Some(status) = change.status {
            row.status = status.as_str().to_owned();
        }
        row.updated_at = now_rfc3339();
        row.meta_mac = self.mac_for(&row, kind, &name, &origins);
        self.store.item_update(&row, &kept)
    }

    pub fn remove(&self, name: &ItemName) -> Result<bool> {
        self.store.item_delete(name.as_str())
    }

    pub fn list(&self) -> Result<Vec<ItemSummary>> {
        let mut out = Vec::new();
        for row in self.store.item_list()? {
            let tampered = self.verified(&row).is_err();
            let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
            let fields = self
                .store
                .item_get(&row.name)?
                .map(|(_, f)| f.into_iter().map(|f| f.field).collect())
                .unwrap_or_default();
            out.push(ItemSummary {
                name: row.name,
                kind,
                domain: kind.domain(),
                fields,
                origins: row.origins,
                origin_binding: row.origin_binding,
                status: ItemStatus::parse(&row.status).context("unknown status in database")?,
                source: ItemSource::parse(&row.source).context("unknown source in database")?,
                updated_at: row.updated_at,
                tampered,
            });
        }
        Ok(out)
    }

    /// Decrypts one field after verifying the item's metadata. Not exposed by the CLI.
    pub fn resolve(&self, reference: &PlaceholderRef) -> Result<Zeroizing<String>> {
        let (row, fields) = self
            .store
            .item_get(reference.item.as_str())?
            .context("item not found")?;
        let (kind, _, _) = self.verified(&row)?;
        let field = fields
            .iter()
            .find(|f| f.field == reference.field.as_str())
            .context("field not found")?;
        let domain = kind.domain();
        let ad = FieldAd {
            vault: &self.vault_id,
            domain,
            item: &row.id,
            field: &field.field,
        };
        let bytes =
            open_field(&self.domain_keys[&domain].1, &ad, &field.sealed).map_err(vault_err)?;
        let text = std::str::from_utf8(&bytes).context("stored field is not UTF-8")?;
        Ok(Zeroizing::new(text.to_owned()))
    }

    pub fn item_origins_verified(&self, name: &ItemName) -> Result<(bool, Vec<Origin>)> {
        let (row, _) = self
            .store
            .item_get(name.as_str())?
            .context("item not found")?;
        let (_, _, origins) = self.verified(&row)?;
        Ok((row.origin_binding, origins))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::vault::{FieldName, ItemKind, ItemName, Origin, PlaceholderRef};
    use keyharbor_sqlite::SqliteStore;
    use keyharbor_vault::keystore::FileKeyStore;
    use std::path::{Path, PathBuf};
    use zeroize::Zeroizing;

    pub(crate) struct Env {
        pub _tmp: tempfile::TempDir,
        pub data: PathBuf,
        pub keys: PathBuf,
    }
    pub(crate) fn env() -> Env {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let keys = tmp.path().join("keys");
        Env {
            _tmp: tmp,
            data,
            keys,
        }
    }
    pub(crate) fn open_store(data: &Path) -> SqliteStore {
        SqliteStore::open(data).unwrap()
    }
    pub(crate) fn init(e: &Env) -> Vault {
        Vault::init(
            open_store(&e.data),
            Box::new(FileKeyStore::new(e.keys.clone())),
        )
        .unwrap()
    }
    pub(crate) fn reopen(e: &Env) -> anyhow::Result<Vault> {
        Vault::open(
            open_store(&e.data),
            Box::new(FileKeyStore::new(e.keys.clone())),
        )
    }
    pub(crate) fn field(name: &str, value: &str) -> (FieldName, Zeroizing<String>) {
        (
            FieldName::parse(name).unwrap(),
            Zeroizing::new(value.to_owned()),
        )
    }
    pub(crate) fn login(name: &str, password: &str) -> NewItem {
        NewItem {
            name: ItemName::parse(name).unwrap(),
            kind: ItemKind::Login,
            fields: vec![field("username", "me@x.com"), field("password", password)],
            origins: vec![Origin::parse("https://github.com").unwrap()],
            origin_binding: None,
            status: ItemStatus::Active,
            source: ItemSource::User,
        }
    }
    pub(crate) fn pref(item: &str, field: &str) -> PlaceholderRef {
        PlaceholderRef {
            item: ItemName::parse(item).unwrap(),
            field: FieldName::parse(field).unwrap(),
        }
    }

    #[test]
    fn init_twice_fails() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        assert!(
            Vault::init(
                open_store(&e.data),
                Box::new(FileKeyStore::new(e.keys.clone()))
            )
            .is_err()
        );
        assert_eq!(
            &*reopen(&e)
                .unwrap()
                .resolve(&pref("github", "password"))
                .unwrap(),
            "pw"
        );
    }

    #[test]
    fn open_without_key_fails_and_keeps_db() {
        let e = env();
        init(&e).add(login("github", "pw")).unwrap();
        std::fs::remove_dir_all(&e.keys).unwrap();
        let db = e.data.join("keyharbor.sqlite3");
        let before = std::fs::read(&db).unwrap();
        let err = reopen(&e).err().expect("open must fail without the key");
        assert!(err.to_string().contains("vault key not found"), "{err}");
        assert_eq!(std::fs::read(&db).unwrap(), before);
        assert!(!e.keys.exists());
    }

    #[test]
    fn open_uninitialized_fails() {
        let e = env();
        let err = reopen(&e).err().unwrap();
        assert!(err.to_string().contains("vault not initialized"), "{err}");
    }

    #[test]
    fn add_list_resolve_roundtrip() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "KH-SYNTH-roundtrip")).unwrap();
        let list = v.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].fields,
            vec!["password".to_string(), "username".to_string()]
        );
        assert_eq!(list[0].origins, vec!["https://github.com".to_string()]);
        assert!(list[0].origin_binding);
        assert!(!format!("{list:?}").contains("KH-SYNTH"));
        assert_eq!(
            &*v.resolve(&pref("github", "password")).unwrap(),
            "KH-SYNTH-roundtrip"
        );
        assert!(v.resolve(&pref("github", "note")).is_err());
        assert!(v.resolve(&pref("nope", "password")).is_err());
    }

    #[test]
    fn schema_and_size_are_enforced() {
        let e = env();
        let v = init(&e);
        let mut card = login("visa", "x");
        card.kind = ItemKind::Card;
        card.fields = vec![field("password", "x")];
        assert!(v.add(card).is_err());
        let mut big = login("big", "x");
        big.fields = vec![field("password", &"a".repeat(64 * 1024 + 1))];
        assert!(v.add(big).is_err());
        let mut env_item = login("app", "x");
        env_item.kind = ItemKind::Env;
        env_item.fields = vec![field("lower_case", "1")];
        assert!(v.add(env_item).is_err());
        let mut dup = login("dup", "x");
        dup.fields.push(field("password", "y"));
        assert!(v.add(dup).is_err());
        assert!(v.list().unwrap().is_empty());
    }

    #[test]
    fn edit_changes_fields_origins_and_status() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "old")).unwrap();
        v.edit(
            &ItemName::parse("github").unwrap(),
            ItemChange {
                set_fields: vec![field("password", "new")],
                remove_fields: vec![FieldName::parse("username").unwrap()],
                origins: Some(vec![Origin::parse("https://gist.github.com").unwrap()]),
                origin_binding: Some(false),
                status: Some(ItemStatus::Draft),
            },
        )
        .unwrap();
        assert_eq!(&*v.resolve(&pref("github", "password")).unwrap(), "new");
        assert!(v.resolve(&pref("github", "username")).is_err());
        let (binding, origins) = v
            .item_origins_verified(&ItemName::parse("github").unwrap())
            .unwrap();
        assert!(!binding);
        assert_eq!(
            origins,
            vec![Origin::parse("https://gist.github.com").unwrap()]
        );
        assert_eq!(v.list().unwrap()[0].status, ItemStatus::Draft);
        assert!(v.remove(&ItemName::parse("github").unwrap()).unwrap());
        assert!(!v.remove(&ItemName::parse("github").unwrap()).unwrap());
    }

    #[test]
    fn tampered_origins_block_resolve() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let conn = rusqlite::Connection::open(e.data.join("keyharbor.sqlite3")).unwrap();
        conn.execute(
            "INSERT INTO item_origins (item_id, origin) SELECT id, 'https://evil.example' FROM vault_items WHERE name='github'",
            [],
        )
        .unwrap();
        assert!(v.resolve(&pref("github", "password")).is_err());
        assert!(
            v.item_origins_verified(&ItemName::parse("github").unwrap())
                .is_err()
        );
        assert!(v.list().unwrap()[0].tampered);
    }

    #[test]
    fn concurrent_add_same_name_one_wins() {
        let e = env();
        init(&e);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|i| {
                let (data, keys, barrier) = (e.data.clone(), e.keys.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let v = Vault::open(
                        SqliteStore::open(&data).unwrap(),
                        Box::new(FileKeyStore::new(keys)),
                    )
                    .unwrap();
                    barrier.wait();
                    v.add(login("github", &format!("pw{i}"))).is_ok()
                })
            })
            .collect();
        let wins = workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .filter(|ok| *ok)
            .count();
        assert_eq!(wins, 1);
        assert_eq!(reopen(&e).unwrap().list().unwrap().len(), 1);
    }

    #[test]
    fn db_and_wal_never_contain_plaintext() {
        let e = env();
        let v = init(&e);
        let secret = format!("KH-SYNTH-{}", uuid_like());
        v.add(login("github", &secret)).unwrap();
        let mut card = login("visa", "");
        card.kind = ItemKind::Card;
        card.origins = vec![];
        card.fields = vec![
            field("number", &format!("{secret}-card")),
            field("cvc", "1234"),
        ];
        v.add(card).unwrap();
        let mut ident = login("me", "");
        ident.kind = ItemKind::Identity;
        ident.fields = vec![field("full_name", &format!("{secret}-name"))];
        v.add(ident).unwrap();
        let mut env_item = login("app", "");
        env_item.kind = ItemKind::Env;
        env_item.fields = vec![field("API_KEY", &format!("{secret}-env"))];
        v.add(env_item).unwrap();
        for file in ["keyharbor.sqlite3", "keyharbor.sqlite3-wal"] {
            let path = e.data.join(file);
            if let Ok(bytes) = std::fs::read(&path) {
                assert!(
                    !contains(&bytes, secret.as_bytes()),
                    "{file} leaks plaintext"
                );
            }
        }
    }

    pub(crate) fn uuid_like() -> String {
        let mut b = [0u8; 8];
        getrandom_fill(&mut b);
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
    fn getrandom_fill(b: &mut [u8]) {
        let k = keyharbor_vault::keys::SecretKey::generate();
        b.copy_from_slice(&k.expose()[..b.len()]);
    }
    pub(crate) fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }
}
