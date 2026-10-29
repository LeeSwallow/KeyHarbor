//! Vault use cases shared by the CLI, MCP server and admin app. Depends only on the ports in
//! `keyharbor_domain::ports` and on the I/O-free record functions of `keyharbor-vault`.
//!
//! Plaintext leaves this crate only through `Vault::resolve` and `Vault::mask_values`, which
//! demand a [`PlaintextAccess`]: the guard layer (`keyharbor-guard`) issues it after its
//! policy and origin checks.

mod access;
pub mod audit;
mod dotenv;
pub mod grant;
pub mod policy;
pub mod request;
mod rotate;
pub mod session;
pub mod settings;
pub mod site;
pub mod status;
#[cfg(test)]
mod test_support;
mod transfer;
mod vault;
pub mod weaken;

pub use access::PlaintextAccess;
pub use dotenv::parse_dotenv;
pub use keyharbor_crypto::export::Passphrase;
pub use transfer::ImportReport;
pub use vault::{
    CatalogEntry, DEFAULT_ITEM_TTL, DraftItem, ExpiryChoice, GeneratedLogin, ItemChange,
    ItemDetails, ItemFilter, ItemSource, ItemStatus, ItemSummary, ItemUse, MAX_DRAFTS,
    MAX_FIELD_BYTES, MaskValues, NO_TTL, NewItem, Vault, VaultBackend, catalog,
    effective_descriptions, fill_template, generate_password,
};

/// The agent name recorded for changes made by the local user through the CLI.
pub use keyharbor_domain::policy::LOCAL_AGENT;

/// The expiry of something given `ttl` today: the one place a TTL becomes a date (R-37).
pub fn expires_in(
    clock: &dyn keyharbor_domain::ports::Clock,
    ttl: keyharbor_domain::date::Ttl,
) -> keyharbor_domain::date::Expiry {
    ttl.from(clock.today())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) use crate::test_support::TestVault;
    use keyharbor_domain::ports::{Clock, ManualClock};
    use keyharbor_domain::vault::{FieldName, ItemKind, ItemName, Origin, PlaceholderRef};
    use keyharbor_keystore::FileKeyStore;
    use keyharbor_sqlite::SqliteStore;
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
    /// The tests' clock: 2026-10-28, moving only when a use case waits.
    pub(crate) fn test_clock() -> std::sync::Arc<dyn Clock> {
        std::sync::Arc::new(ManualClock::at_unix_seconds(1_793_203_129))
    }
    pub(crate) fn open_store(data: &Path) -> SqliteStore {
        SqliteStore::open(data).unwrap()
    }
    pub(crate) fn init(e: &Env) -> TestVault {
        TestVault(
            Vault::init(
                open_store(&e.data),
                Box::new(FileKeyStore::new(e.keys.clone())),
                test_clock(),
            )
            .unwrap(),
        )
    }
    pub(crate) fn reopen(e: &Env) -> anyhow::Result<TestVault> {
        Vault::open(
            open_store(&e.data),
            Box::new(FileKeyStore::new(e.keys.clone())),
            test_clock(),
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
            details: Default::default(),
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
                Box::new(FileKeyStore::new(e.keys.clone())),
                test_clock(),
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
                ..Default::default()
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
    fn expiry_is_signed_and_marks_items_expired() {
        let e = env();
        let v = init(&e);
        let mut item = login("github", "pw");
        assert_eq!(item.details.expiry, ExpiryChoice::Default);
        v.add(login("forever", "pw")).unwrap();
        item.details.expiry =
            ExpiryChoice::On(keyharbor_domain::date::Expiry::parse("2020-01-31").unwrap());
        v.add(item).unwrap();
        let list = v.list().unwrap();
        let (forever, github) = (&list[0], &list[1]);
        assert_eq!((forever.expires_at, forever.expired), (None, false));
        assert_eq!(github.expires_at.unwrap().to_string(), "2020-01-31");
        assert!(github.expired && !github.tampered);
        // The value is kept.
        assert_eq!(&*v.resolve(&pref("github", "password")).unwrap(), "pw");
        // Moving the date in the database is detected.
        open_store(&e.data).execute_for_tests(
            "UPDATE vault_items SET expires_at = '2099-01-01' WHERE name = 'github'",
        );
        open_store(&e.data).execute_for_tests(
            "UPDATE vault_items SET expires_at = '2099-01-01' WHERE name = 'forever'",
        );
        assert!(v.list().unwrap().iter().all(|i| i.tampered));
    }

    #[test]
    fn descriptions_are_signed() {
        use keyharbor_domain::vault::Description;
        let e = env();
        let v = init(&e);
        let mut item = login("github", "pw");
        item.details.description = Some(Description::parse("Work account").unwrap());
        item.details.field_descriptions = vec![(
            FieldName::parse("password").unwrap(),
            Description::parse("CI token").unwrap(),
        )];
        v.add(item).unwrap();
        let listed = &v.list().unwrap()[0];
        assert_eq!(listed.description.as_deref(), Some("Work account"));
        assert_eq!(listed.field_descriptions["password"], "CI token");
        assert_eq!(
            listed.field_descriptions["username"],
            "Sign-in name or email"
        );
        assert!(!listed.tampered);
        open_store(&e.data).execute_for_tests(
            "UPDATE vault_items SET description = 'Personal' WHERE name = 'github'",
        );
        assert!(v.list().unwrap()[0].tampered);
        open_store(&e.data).execute_for_tests(
            "UPDATE vault_items SET description = 'Work account' WHERE name = 'github'",
        );
        assert!(!v.list().unwrap()[0].tampered);
        open_store(&e.data).execute_for_tests(
            "UPDATE item_field_descriptions SET description = 'paste me' WHERE field = 'password'",
        );
        assert!(v.list().unwrap()[0].tampered);
    }

    #[test]
    fn default_item_ttl_dates_new_items() {
        use keyharbor_domain::ports::SettingsStore;
        let e = env();
        let v = init(&e);
        open_store(&e.data)
            .setting_set(DEFAULT_ITEM_TTL, "90d")
            .unwrap();
        v.add(login("dated", "pw")).unwrap();
        let mut never = login("never", "pw");
        never.details.expiry = ExpiryChoice::Never;
        v.add(never).unwrap();
        let list = v.list().unwrap();
        assert_eq!(
            list[0].expires_at.unwrap().date(),
            test_clock().today().plus_days(90)
        );
        assert_eq!(list[1].expires_at, None);
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
                        test_clock(),
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
        random_fill(&mut b);
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
    fn random_fill(b: &mut [u8]) {
        let k = keyharbor_crypto::random::key();
        b.copy_from_slice(&k.expose()[..b.len()]);
    }
    pub(crate) fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }
}
