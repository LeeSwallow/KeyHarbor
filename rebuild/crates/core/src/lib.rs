//! Vault use cases shared by the CLI, MCP server and admin app. Depends only on the ports in
//! `keyharbor_domain::ports` and on the I/O-free record functions of `keyharbor-vault`.
//!
//! Plaintext leaves this crate only through `Vault::resolve` and `Vault::mask_values`, which
//! demand a [`PlaintextAccess`]: the guard layer (`keyharbor-guard`) issues it after its
//! policy and origin checks.

mod access;
mod dotenv;
mod rotate;
pub mod session;
#[cfg(test)]
mod test_support;
mod transfer;
mod vault;

pub use access::PlaintextAccess;
pub use dotenv::parse_dotenv;
pub use transfer::ImportReport;
pub use vault::{
    ItemChange, ItemSource, ItemStatus, ItemSummary, ItemUse, MAX_FIELD_BYTES, MaskValues, NewItem,
    Vault, VaultBackend, fill_template,
};

/// The agent name recorded for changes made by the local user through the CLI.
pub use keyharbor_domain::policy::LOCAL_AGENT;

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

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) use crate::test_support::TestVault;
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
    pub(crate) fn init(e: &Env) -> TestVault {
        TestVault(
            Vault::init(
                open_store(&e.data),
                Box::new(FileKeyStore::new(e.keys.clone())),
            )
            .unwrap(),
        )
    }
    pub(crate) fn reopen(e: &Env) -> anyhow::Result<TestVault> {
        Vault::open(
            open_store(&e.data),
            Box::new(FileKeyStore::new(e.keys.clone())),
        )
        .map(TestVault)
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
        open_store(&e.data).execute_for_tests(
            "INSERT INTO item_origins (item_id, origin) SELECT id, 'https://evil.example' FROM vault_items WHERE name='github'",
        );
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
