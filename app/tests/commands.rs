//! The app's command layer over a temporary vault (file key store), without a window.

use keyharbor_app::{Ctx, KeyStores, SystemClock, commands};
use keyharbor_core::{ItemSource, ItemStatus, NewItem, Vault, request};
use keyharbor_domain::{
    SessionName,
    ports::{AuditStore, SessionStore},
    request::VaultRequestStatus,
    vault::{Description, FieldName, ItemKind, ItemName, Origin},
};
use keyharbor_keystore::FileKeyStore;
use keyharbor_sqlite::SqliteStore;
use std::{path::Path, sync::Arc};
use zeroize::Zeroizing;

fn ctx(dir: &Path) -> Ctx {
    let keys_dir = dir.join("keys");
    let keys: KeyStores = Arc::new(move || Box::new(FileKeyStore::new(keys_dir.clone())));
    Ctx::new(
        dir.join("data"),
        keys,
        Arc::new(SystemClock),
        "keyharbor".into(),
    )
}
fn login(name: &str, status: ItemStatus) -> NewItem {
    NewItem {
        name: ItemName::parse(name).unwrap(),
        kind: ItemKind::Login,
        fields: vec![(
            FieldName::parse("password").unwrap(),
            Zeroizing::new("pw-SECRET-1".into()),
        )],
        origins: vec![Origin::parse("https://github.com").unwrap()],
        origin_binding: None,
        status,
        source: ItemSource::Generated,
        details: Default::default(),
    }
}

#[test]
fn without_a_vault_the_app_says_so_and_makes_no_key() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    assert!(!commands::overview(&c).unwrap().vault_ready);
    assert!(commands::items(&c).unwrap().is_empty());
    assert!(commands::reveal(&c, "github", "password").is_err());
    assert!(!tmp.path().join("keys").exists(), "no key was created");
}

#[test]
fn items_carry_no_value_until_revealed_and_a_reveal_is_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    let vault = Vault::init(
        SqliteStore::open(&tmp.path().join("data")).unwrap(),
        Box::new(FileKeyStore::new(tmp.path().join("keys"))),
        Arc::new(SystemClock),
    )
    .unwrap();
    vault.add(login("github", ItemStatus::Active)).unwrap();
    let listed = serde_json::to_string(&commands::items(&c).unwrap()).unwrap();
    assert!(!listed.contains("pw-SECRET-1"), "{listed}");
    assert!(listed.contains("{{secret:github.password}}"));
    assert_eq!(
        commands::reveal(&c, "github", "password").unwrap(),
        "pw-SECRET-1"
    );
    let audit = commands::audit(&c, 10).unwrap();
    assert_eq!(audit[0].kind, "value_revealed");
    assert!(
        !serde_json::to_string(&audit)
            .unwrap()
            .contains("pw-SECRET-1")
    );
}

#[test]
fn the_inbox_lists_requests_and_drafts_and_settles_them() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    let vault = Vault::init(
        SqliteStore::open(&tmp.path().join("data")).unwrap(),
        Box::new(FileKeyStore::new(tmp.path().join("keys"))),
        Arc::new(SystemClock),
    )
    .unwrap();
    vault.add(login("shop", ItemStatus::Draft)).unwrap();
    vault.add(login("old", ItemStatus::Draft)).unwrap();
    let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
    let id = request::ask(
        &store,
        &SystemClock,
        "codex",
        request::Ask {
            kind: ItemKind::Card,
            name: ItemName::parse("visa").unwrap(),
            reason: Description::parse("<b>checkout</b>").unwrap(),
            origin: None,
        },
    )
    .unwrap();
    let inbox = commands::inbox(&c).unwrap();
    assert_eq!(inbox.requests.len(), 1);
    assert_eq!(inbox.drafts.len(), 2);

    commands::decline_request(&c, &id.to_string()).unwrap();
    assert_eq!(
        request::status(&store, &SystemClock, id).unwrap().status,
        VaultRequestStatus::Declined
    );
    assert_eq!(commands::confirm_draft(&c, "shop").unwrap(), "shop");
    commands::discard_draft(&c, "old").unwrap();
    let inbox = commands::inbox(&c).unwrap();
    assert!(inbox.requests.is_empty() && inbox.drafts.is_empty());
    assert!(
        store
            .list_audit(None)
            .unwrap()
            .iter()
            .all(|r| !format!("{r:?}").contains("pw-SECRET"))
    );
}

#[test]
fn sessions_show_their_sites() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
    store.create(SessionName::parse("work").unwrap()).unwrap();
    let sessions = commands::sessions(&c).unwrap();
    assert_eq!(
        (sessions[0].name.as_str(), sessions[0].state),
        ("work", "stopped")
    );
    assert!(sessions[0].sites.is_empty());
}
