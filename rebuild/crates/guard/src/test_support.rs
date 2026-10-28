//! Test helpers: a vault in a temporary directory with the file key store.

use crate::pep::mask::{Masker, masker};
use keyharbor_core::{ItemSource, ItemStatus, NewItem, Vault};
use keyharbor_domain::vault::{FieldName, ItemKind, ItemName, Origin, PlaceholderRef};
use keyharbor_sqlite::SqliteStore;
use keyharbor_vault::keystore::FileKeyStore;
use std::path::PathBuf;
use zeroize::Zeroizing;

pub struct Env {
    pub _tmp: tempfile::TempDir,
    pub data: PathBuf,
    pub keys: PathBuf,
}
pub fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let keys = tmp.path().join("keys");
    Env {
        _tmp: tmp,
        data,
        keys,
    }
}
pub fn open_vault(e: &Env) -> anyhow::Result<Vault> {
    Vault::open(
        SqliteStore::open(&e.data)?,
        Box::new(FileKeyStore::new(e.keys.clone())),
    )
}
pub fn init(e: &Env) -> TestVault {
    TestVault(
        Vault::init(
            SqliteStore::open(&e.data).unwrap(),
            Box::new(FileKeyStore::new(e.keys.clone())),
        )
        .unwrap(),
    )
}
pub fn field(name: &str, value: &str) -> (FieldName, Zeroizing<String>) {
    (
        FieldName::parse(name).unwrap(),
        Zeroizing::new(value.to_owned()),
    )
}
pub fn login(name: &str, password: &str) -> NewItem {
    NewItem {
        name: ItemName::parse(name).unwrap(),
        kind: ItemKind::Login,
        fields: vec![field("username", "me@x.com"), field("password", password)],
        origins: vec![Origin::parse("https://github.com").unwrap()],
        origin_binding: None,
        status: ItemStatus::Active,
        source: ItemSource::User,
        details: Default::default(),
    }
}
pub fn pref(item: &str, field: &str) -> PlaceholderRef {
    PlaceholderRef {
        item: ItemName::parse(item).unwrap(),
        field: FieldName::parse(field).unwrap(),
    }
}

/// A vault whose `masker()` reads like the use case it checks.
pub struct TestVault(pub Vault);
impl TestVault {
    pub fn masker(&self) -> anyhow::Result<Masker> {
        masker(&self.0)
    }
}
impl std::ops::Deref for TestVault {
    type Target = Vault;
    fn deref(&self) -> &Vault {
        &self.0
    }
}
