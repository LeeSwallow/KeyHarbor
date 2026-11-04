//! Every protected command asks the app's `Authenticator` first, and does nothing when it is
//! refused (03 §3.10, J-126).

use keyharbor_app::{
    Ctx, KeyStores, SystemClock,
    auth::{AuthScope, Authenticator, NoneAuth, Refused},
    commands::{self, ChangeForm, NewItemForm},
    window::{MainWindow, open_main_window},
};
use keyharbor_core::{ItemSource, ItemStatus, NewItem, Vault, request};
use keyharbor_domain::{
    request::VaultRequestStatus,
    vault::{Description, FieldName, ItemKind, ItemName, Origin},
};
use keyharbor_keystore::FileKeyStore;
use keyharbor_sqlite::SqliteStore;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

/// Refuses every challenge and remembers which scopes it was asked for.
#[derive(Default)]
struct RefuseAll(Mutex<Vec<AuthScope>>);
impl Authenticator for RefuseAll {
    fn id(&self) -> &'static str {
        "refuse-all"
    }
    fn challenge(&self, scope: AuthScope) -> Result<(), Refused> {
        self.0.lock().unwrap().push(scope);
        Err(Refused(scope))
    }
}

/// A clipboard in memory that remembers what was copied.
#[derive(Default)]
struct Board(Mutex<Vec<String>>);
impl keyharbor_app::clipboard::Clipboard for Board {
    fn copy(&self, text: &str, _concealed: bool) -> anyhow::Result<i64> {
        self.0.lock().unwrap().push(text.to_owned());
        Ok(1)
    }
    fn clear_if_unchanged(&self, _token: i64) {}
}

fn ctx(dir: &Path, auth: Arc<dyn Authenticator>, board: Arc<Board>) -> Ctx {
    let keys_dir = dir.join("keys");
    let keys: KeyStores = Arc::new(move || Box::new(FileKeyStore::new(keys_dir.clone())));
    Ctx::new(
        dir.join("data"),
        keys,
        Arc::new(SystemClock),
        "keyharbor".into(),
        board,
        auth,
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

/// An active item, a draft and a pending request; returns the request's id.
fn seed(dir: &Path) -> String {
    let store = || SqliteStore::open(&dir.join("data")).unwrap();
    let vault = Vault::init(
        store(),
        Box::new(FileKeyStore::new(dir.join("keys"))),
        Arc::new(SystemClock),
    )
    .unwrap();
    vault.add(login("github", ItemStatus::Active)).unwrap();
    vault.add(login("shop", ItemStatus::Draft)).unwrap();
    let ask = request::Ask {
        kind: ItemKind::Card,
        name: ItemName::parse("visa").unwrap(),
        reason: Description::parse("checkout").unwrap(),
        origin: None,
    };
    request::ask(&store(), &SystemClock, "codex", ask)
        .unwrap()
        .to_string()
}

fn refused_for(result: anyhow::Result<impl std::fmt::Debug>, scope: AuthScope) {
    let error = result.expect_err("a refused challenge stops the command");
    assert_eq!(error.downcast_ref::<Refused>(), Some(&Refused(scope)));
}

#[test]
fn a_refused_challenge_stops_every_protected_command_before_it_does_anything() {
    use AuthScope::*;
    let tmp = tempfile::tempdir().unwrap();
    let id = seed(tmp.path());
    let auth = Arc::new(RefuseAll::default());
    let board = Arc::new(Board::default());
    let c = ctx(tmp.path(), auth.clone(), board.clone());

    refused_for(commands::open_app(&c), OpenApp);
    refused_for(commands::reveal(&c, "github", "password"), Reveal);
    refused_for(commands::copy_value(&c, "github", "password"), Reveal);
    let new_item = NewItemForm {
        name: "new".into(),
        kind: "secret".into(),
        origins: vec![],
        values: [("value".to_owned(), "v-SECRET".to_owned())].into(),
        generate_password: false,
    };
    refused_for(commands::add_item(&c, new_item), Edit);
    let change = ChangeForm {
        values: [("password".to_owned(), "changed-1234".to_owned())].into(),
        origins: None,
    };
    refused_for(commands::edit_item(&c, "github", change, true), Edit);
    refused_for(commands::rename_item(&c, "github", "gh"), Edit);
    refused_for(commands::remove_item(&c, "github"), Edit);
    refused_for(commands::confirm_draft(&c, "shop"), Edit);
    refused_for(commands::discard_draft(&c, "shop"), Edit);
    let typed = [("number".to_owned(), "4111-SECRET".to_owned())].into();
    refused_for(commands::fulfill_request(&c, &id, typed), Edit);
    refused_for(commands::fulfill_with_item(&c, &id, "github"), Edit);
    refused_for(commands::decline_request(&c, &id), Edit);
    refused_for(
        commands::set_setting(&c, "audit_enabled", "false", true),
        PolicyChange,
    );
    let allow = commands::RuleForm {
        agent: "codex".into(),
        resource: "item".into(),
        target: "github".into(),
        effect: "allow".into(),
    };
    refused_for(commands::set_policy(&c, allow, true), PolicyChange);
    refused_for(
        commands::remove_policy(&c, "codex", "item", "github", true),
        PolicyChange,
    );

    assert_eq!(
        *auth.0.lock().unwrap(),
        [
            OpenApp,
            Reveal,
            Reveal,
            Edit,
            Edit,
            Edit,
            Edit,
            Edit,
            Edit,
            Edit,
            Edit,
            Edit,
            PolicyChange,
            PolicyChange,
            PolicyChange
        ],
        "each command asked once, for its own scope"
    );

    // Nothing happened: read back with no authentication.
    let open = ctx(tmp.path(), Arc::new(NoneAuth), Arc::default());
    assert!(board.0.lock().unwrap().is_empty(), "nothing was copied");
    let items: Vec<(String, &str)> = commands::items(&open)
        .unwrap()
        .into_iter()
        .map(|i| (i.name, i.status))
        .collect();
    assert_eq!(
        items,
        [("github".into(), "active"), ("shop".into(), "draft")]
    );
    assert_eq!(
        commands::reveal(&open, "github", "password").unwrap(),
        "pw-SECRET-1"
    );
    let reveals = commands::audit(&open, 50)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "value_revealed")
        .count();
    assert_eq!(reveals, 1, "only the reveal after the refusals");
    let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
    let request_id = keyharbor_domain::RequestId::parse(&id).unwrap();
    assert_eq!(
        request::status(&store, &SystemClock, request_id)
            .unwrap()
            .status,
        VaultRequestStatus::Pending
    );
    assert!(commands::policies(&open).unwrap().is_empty());
    let audit = commands::settings(&open)
        .unwrap()
        .into_iter()
        .find(|s| s.key == "audit_enabled")
        .unwrap();
    assert_eq!(audit.value, "true");
}

/// Refuses the first challenge, then lets the person through.
#[derive(Default)]
struct RefuseFirst(Mutex<usize>);
impl Authenticator for RefuseFirst {
    fn id(&self) -> &'static str {
        "refuse-first"
    }
    fn challenge(&self, scope: AuthScope) -> Result<(), Refused> {
        let mut asked = self.0.lock().unwrap();
        *asked += 1;
        if *asked == 1 {
            Err(Refused(scope))
        } else {
            Ok(())
        }
    }
}

/// A main window that remembers what was done to it.
#[derive(Default)]
struct Window(Mutex<Vec<&'static str>>);
impl MainWindow for Window {
    fn show(&self) -> anyhow::Result<()> {
        self.0.lock().unwrap().push("show");
        Ok(())
    }
    fn focus(&self) -> anyhow::Result<()> {
        self.0.lock().unwrap().push("focus");
        Ok(())
    }
}

#[test]
fn a_refused_open_at_launch_leaves_the_window_hidden_and_the_menu_bar_can_ask_again() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path(), Arc::new(RefuseFirst::default()), Arc::default());
    let window = Window::default();

    // At launch: refused, so the window stays hidden and nothing fails the app.
    refused_for(open_main_window(&c, &window), AuthScope::OpenApp);
    assert!(
        window.0.lock().unwrap().is_empty(),
        "the window stays hidden"
    );

    // The menu bar's "열기" takes the same path and asks again.
    open_main_window(&c, &window).unwrap();
    assert_eq!(*window.0.lock().unwrap(), ["show", "focus"]);
}

#[test]
fn a_window_never_shows_while_every_open_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let auth = Arc::new(RefuseAll::default());
    let c = ctx(tmp.path(), auth.clone(), Arc::default());
    let window = Window::default();
    for _ in 0..2 {
        refused_for(open_main_window(&c, &window), AuthScope::OpenApp);
    }
    assert!(window.0.lock().unwrap().is_empty());
    assert_eq!(
        *auth.0.lock().unwrap(),
        [AuthScope::OpenApp, AuthScope::OpenApp]
    );
}

#[test]
fn without_authentication_the_app_opens_and_says_which_method_it_uses() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path(), Arc::new(NoneAuth), Arc::default());
    commands::open_app(&c).unwrap();
    assert_eq!(commands::overview(&c).unwrap().auth, "none");
    let refusing = ctx(tmp.path(), Arc::new(RefuseAll::default()), Arc::default());
    assert_eq!(
        commands::overview(&refusing).unwrap().auth,
        "refuse-all",
        "the screens show the authenticator's own id"
    );
}

#[test]
fn lifting_a_session_lock_asks_the_authenticator_first() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path());
    let id = "11111111-1111-4111-8111-111111111111";
    let auth = Arc::new(RefuseAll::default());
    let refusing = ctx(tmp.path(), auth.clone(), Arc::default());
    refused_for(
        commands::unlock_session(&refusing, id),
        AuthScope::PolicyChange,
    );
    assert_eq!(*auth.0.lock().unwrap(), [AuthScope::PolicyChange]);
    // Past the challenge, a session with no supervisor has nothing to lift.
    let open = ctx(tmp.path(), Arc::new(NoneAuth), Arc::default());
    let error = commands::unlock_session(&open, id).unwrap_err();
    assert!(error.to_string().contains("not running"), "{error}");
}
