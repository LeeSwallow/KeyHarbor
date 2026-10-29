//! The app's command layer over a temporary vault (file key store), without a window.

use keyharbor_app::{Ctx, KeyStores, SystemClock, commands};
use std::sync::Mutex;

/// A clipboard in memory: tests must not touch the real one.
#[derive(Default)]
struct Board(Mutex<Vec<(String, bool)>>, Mutex<Vec<i64>>);
impl keyharbor_app::clipboard::Clipboard for Board {
    fn copy(&self, text: &str, concealed: bool) -> anyhow::Result<i64> {
        let mut copies = self.0.lock().unwrap();
        copies.push((text.to_owned(), concealed));
        Ok(copies.len() as i64)
    }
    fn clear_if_unchanged(&self, token: i64) {
        self.1.lock().unwrap().push(token);
    }
}
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

fn ctx_with(dir: &Path, board: Arc<Board>) -> Ctx {
    let keys_dir = dir.join("keys");
    let keys: KeyStores = Arc::new(move || Box::new(FileKeyStore::new(keys_dir.clone())));
    Ctx::new(
        dir.join("data"),
        keys,
        Arc::new(SystemClock),
        "keyharbor".into(),
        board,
    )
}
fn ctx(dir: &Path) -> Ctx {
    ctx_with(dir, Arc::default())
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

fn init_vault(dir: &Path) -> Vault {
    Vault::init(
        SqliteStore::open(&dir.join("data")).unwrap(),
        Box::new(FileKeyStore::new(dir.join("keys"))),
        Arc::new(SystemClock),
    )
    .unwrap()
}
fn ask_card(dir: &Path) -> String {
    let store = SqliteStore::open(&dir.join("data")).unwrap();
    request::ask(
        &store,
        &SystemClock,
        "codex",
        request::Ask {
            kind: ItemKind::Card,
            name: ItemName::parse("visa").unwrap(),
            reason: Description::parse("checkout").unwrap(),
            origin: Some(Origin::parse("https://shop.example").unwrap()),
        },
    )
    .unwrap()
    .to_string()
}

#[test]
fn the_secure_form_shows_the_request_and_stores_what_the_person_typed() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    init_vault(tmp.path());
    let id = ask_card(tmp.path());
    let form = commands::request_form(&c, &id).unwrap();
    assert_eq!(
        (form.agent.as_str(), form.kind, form.reason.as_str()),
        ("codex", "card", "checkout")
    );
    let number = form.fields.iter().find(|f| f.name == "number").unwrap();
    assert!(number.secret);
    assert!(!form.free_fields);

    let empty = std::collections::BTreeMap::from([("number".to_owned(), String::new())]);
    assert!(commands::fulfill_request(&c, &id, empty).is_err());
    let bad = std::collections::BTreeMap::from([("nope".to_owned(), "x".to_owned())]);
    assert!(commands::fulfill_request(&c, &id, bad).is_err());
    let typed = std::collections::BTreeMap::from([("number".to_owned(), "4111-SECRET".to_owned())]);
    assert_eq!(commands::fulfill_request(&c, &id, typed).unwrap(), "visa");
    let items = serde_json::to_string(&commands::items(&c).unwrap()).unwrap();
    assert!(items.contains("\"visa\"") && !items.contains("4111-SECRET"));
    assert!(commands::inbox(&c).unwrap().requests.is_empty());
    assert!(commands::handoffs(&c).unwrap().is_empty());
}

#[test]
fn weakening_changes_ask_first_and_stricter_ones_do_not() {
    use keyharbor_app::confirm::Outcome;
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    let allow = ("codex", "item", "github", "allow");
    let asked = commands::set_policy(&c, allow, false).unwrap();
    assert!(
        matches!(&asked, Outcome::Ask { question } if question.contains("policy allow codex item github"))
    );
    assert!(
        commands::policies(&c).unwrap().is_empty(),
        "nothing changed before the person confirmed"
    );
    assert_eq!(
        commands::set_policy(&c, allow, true).unwrap(),
        Outcome::Done
    );
    assert_eq!(
        commands::set_policy(&c, ("codex", "domain", "payment", "deny"), false).unwrap(),
        Outcome::Done
    );
    assert_eq!(commands::policies(&c).unwrap().len(), 2);
    assert!(matches!(
        commands::remove_policy(&c, "codex", "domain", "payment", false).unwrap(),
        Outcome::Ask { .. }
    ));
    assert_eq!(
        commands::remove_policy(&c, "codex", "item", "github", false).unwrap(),
        Outcome::Done
    );

    assert!(matches!(
        commands::set_setting(&c, "audit_enabled", "false", false).unwrap(),
        Outcome::Ask { .. }
    ));
    assert_eq!(
        commands::set_setting(&c, "draft_ttl", "3d", false).unwrap(),
        Outcome::Done
    );
    let settings = commands::settings(&c).unwrap();
    let get = |key: &str| {
        settings
            .iter()
            .find(|s| s.key == key)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(
        (get("audit_enabled"), get("draft_ttl")),
        ("true".to_owned(), "3d".to_owned())
    );
    assert!(matches!(
        commands::set_setting(&c, "clipboard_clear_seconds", "60", false).unwrap(),
        Outcome::Ask { .. }
    ));
    assert!(
        commands::set_setting(&c, "draft_ttl", "soon", true).is_err(),
        "a bad value is an error, not a question"
    );
}

#[test]
fn a_copied_value_is_concealed_and_recorded_and_a_placeholder_is_not_a_value() {
    let tmp = tempfile::tempdir().unwrap();
    let board = Arc::new(Board::default());
    let c = ctx_with(tmp.path(), board.clone());
    init_vault(tmp.path())
        .add(login("github", ItemStatus::Active))
        .unwrap();
    commands::copy_value(&c, "github", "password").unwrap();
    commands::copy_placeholder(&c, "{{secret:github.password}}").unwrap();
    assert!(commands::copy_placeholder(&c, "{{secret:github.password}} and more").is_err());
    assert!(commands::copy_placeholder(&c, "plain text").is_err());
    let copies = board.0.lock().unwrap().clone();
    assert_eq!(
        copies,
        [
            ("pw-SECRET-1".to_owned(), true),
            ("{{secret:github.password}}".to_owned(), false)
        ]
    );
    assert_eq!(commands::audit(&c, 1).unwrap()[0].kind, "value_revealed");
}

#[test]
fn recovering_a_session_needs_the_browser_closed_first() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
    let id = store
        .create(SessionName::parse("work").unwrap())
        .unwrap()
        .id
        .to_string();
    let refused = commands::recover_session(&c, &id, false)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("close the session's browser"), "{refused}");
    assert!(
        commands::recover_session(&c, &id, true).is_err(),
        "a stopped session needs no recovery"
    );
    assert!(
        commands::stop_session(&c, &id).is_err()
            || commands::sessions(&c).unwrap()[0].state == "stopped"
    );
}

#[test]
fn items_are_added_changed_and_removed_through_the_guard() {
    use keyharbor_app::{
        commands::{ChangeForm, NewItemForm},
        confirm::Outcome,
    };
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    init_vault(tmp.path());
    let kinds = commands::kinds();
    let login = kinds.iter().find(|k| k.kind == "login").unwrap();
    assert!(login.fields.contains(&("password", true)));
    assert!(kinds.iter().any(|k| k.kind == "env" && k.free_fields));

    let form = NewItemForm {
        name: "shop".into(),
        kind: "login".into(),
        origins: vec!["https://shop.example".into()],
        values: [("username".to_owned(), "me".to_owned())].into(),
        generate_password: true,
    };
    assert_eq!(commands::add_item(&c, form).unwrap(), "shop");
    let generated = commands::reveal(&c, "shop", "password").unwrap();
    assert_eq!(generated.chars().count(), 20);
    let bad = NewItemForm {
        name: "x".into(),
        kind: "secret".into(),
        origins: vec![],
        values: Default::default(),
        generate_password: true,
    };
    assert!(
        commands::add_item(&c, bad).is_err(),
        "only a login gets a generated password"
    );

    let new_value = ChangeForm {
        values: [("password".to_owned(), "chosen-1234".to_owned())].into(),
        origins: None,
    };
    assert_eq!(
        commands::edit_item(&c, "shop", new_value, false).unwrap(),
        Outcome::Done
    );
    assert_eq!(
        commands::reveal(&c, "shop", "password").unwrap(),
        "chosen-1234"
    );
    let more_sites = || ChangeForm {
        values: Default::default(),
        origins: Some(vec![
            "https://shop.example".into(),
            "https://pay.example".into(),
        ]),
    };
    assert!(matches!(
        commands::edit_item(&c, "shop", more_sites(), false).unwrap(),
        Outcome::Ask { .. }
    ));
    let item = commands::items(&c)
        .unwrap()
        .into_iter()
        .find(|i| i.name == "shop")
        .unwrap();
    assert_eq!(
        item.origins,
        ["https://shop.example"],
        "nothing changed before the person confirmed"
    );
    assert_eq!(
        commands::edit_item(&c, "shop", more_sites(), true).unwrap(),
        Outcome::Done
    );

    commands::remove_item(&c, "shop").unwrap();
    assert!(commands::items(&c).unwrap().is_empty());
    assert!(commands::remove_item(&c, "shop").is_err());
}

#[test]
fn a_copied_value_is_cleared_after_the_set_time() {
    let tmp = tempfile::tempdir().unwrap();
    let board = Arc::new(Board::default());
    let c = ctx_with(tmp.path(), board.clone());
    init_vault(tmp.path())
        .add(login("github", ItemStatus::Active))
        .unwrap();
    // Shorter than the default is stricter, so it needs no confirmation.
    assert_eq!(
        commands::set_setting(&c, "clipboard_clear_seconds", "5", false).unwrap(),
        keyharbor_app::confirm::Outcome::Done
    );
    let start = std::time::Instant::now();
    commands::copy_value(&c, "github", "password").unwrap();
    while board.1.lock().unwrap().is_empty() && start.elapsed().as_secs() < 15 {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(
        *board.1.lock().unwrap(),
        [1],
        "cleared well before the default 30 s"
    );
    assert!(start.elapsed().as_secs() >= 5);
}
