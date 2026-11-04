//! Fields the person names on any kind, each secret or plain, and credential templates (R-48,
//! #205), through the app's commands. No DTO carries a secret value.

use keyharbor_app::{
    Ctx, KeyStores, SystemClock,
    commands::{self, ChangeForm, NewItemForm, TemplateDto, TemplateFieldDto},
    confirm::Outcome,
};
use keyharbor_core::Vault;
use keyharbor_keystore::FileKeyStore;
use keyharbor_sqlite::SqliteStore;
use std::{path::Path, sync::Arc};

#[derive(Default)]
struct Board;
impl keyharbor_app::clipboard::Clipboard for Board {
    fn copy(&self, _text: &str, _concealed: bool) -> anyhow::Result<i64> {
        Ok(1)
    }
    fn clear_if_unchanged(&self, _token: i64) {}
}

fn ctx(dir: &Path) -> Ctx {
    let keys_dir = dir.join("keys");
    let keys: KeyStores = Arc::new(move || Box::new(FileKeyStore::new(keys_dir.clone())));
    Ctx::new(
        dir.join("data"),
        keys,
        Arc::new(SystemClock),
        "keyharbor".into(),
        Arc::new(Board),
        Arc::new(keyharbor_app::auth::NoneAuth),
    )
}

fn init_vault(dir: &Path) -> Vault {
    Vault::init(
        SqliteStore::open(&dir.join("data")).unwrap(),
        Box::new(FileKeyStore::new(dir.join("keys"))),
        Arc::new(SystemClock),
    )
    .unwrap()
}

fn field(name: &str, secret: bool) -> TemplateFieldDto {
    TemplateFieldDto {
        name: name.into(),
        secret,
    }
}

fn bank() -> TemplateDto {
    TemplateDto {
        name: "bank".into(),
        fields: vec![field("second_password", true), field("member_no", false)],
    }
}

/// The items as the window gets them, and the same as JSON text to search for values.
fn shown(c: &Ctx) -> (commands::ItemDto, String) {
    let items = commands::items(c).unwrap();
    let text = serde_json::to_string(&items).unwrap();
    (items.into_iter().next().unwrap(), text)
}

#[test]
fn templates_are_added_once_listed_and_removed_without_a_question() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    init_vault(tmp.path());
    commands::add_template(&c, bank()).unwrap();
    assert!(commands::add_template(&c, bank()).is_err(), "a name once");
    let clash = TemplateDto {
        name: "x".into(),
        fields: vec![field("note", true)],
    };
    assert!(
        commands::add_template(&c, clash).is_err(),
        "every credential has a note"
    );
    assert_eq!(commands::templates(&c).unwrap(), [bank()]);
    commands::remove_template(&c, "bank").unwrap();
    assert!(commands::templates(&c).unwrap().is_empty());
    assert!(commands::remove_template(&c, "bank").is_err());
}

#[test]
fn a_credential_from_a_template_shows_its_plain_field_and_never_its_secret_one() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    init_vault(tmp.path());
    commands::add_template(&c, bank()).unwrap();
    // The window fills a credential from the template and sends its plain fields along.
    let form = NewItemForm {
        name: "hana".into(),
        kind: "credential".into(),
        origins: vec!["https://bank.example".into()],
        values: [
            ("second_password".to_owned(), "2nd-SECRET-4821".to_owned()),
            ("member_no".to_owned(), "M-20261104".to_owned()),
        ]
        .into(),
        generate_password: false,
        plain: vec!["member_no".into()],
    };
    let saved = commands::add_item(&c, form).unwrap();
    assert!(!serde_json::to_string(&saved).unwrap().contains("SECRET"));
    commands::remove_template(&c, "bank").unwrap();
    let (item, text) = shown(&c);
    assert_eq!(item.kind, "credential");
    assert_eq!(item.fields, ["member_no", "second_password"]);
    assert_eq!(item.plain_values["member_no"], "M-20261104");
    assert!(!text.contains("2nd-SECRET"), "{text}");
}

#[test]
fn adding_a_field_plain_asks_nothing_and_exposing_a_secret_one_asks_first() {
    let tmp = tempfile::tempdir().unwrap();
    let c = ctx(tmp.path());
    init_vault(tmp.path());
    let form = NewItemForm {
        name: "github".into(),
        kind: "login".into(),
        origins: vec!["https://github.com".into()],
        values: [
            ("password".to_owned(), "pw-SECRET-1234".to_owned()),
            ("recovery_code".to_owned(), "rc-SECRET-9876".to_owned()),
        ]
        .into(),
        generate_password: false,
        plain: vec![],
    };
    commands::add_item(&c, form).unwrap();
    let add_plain = ChangeForm {
        values: [("member_no".to_owned(), "M-1234".to_owned())].into(),
        plain: vec!["member_no".into()],
        ..Default::default()
    };
    let edited = commands::edit_item(&c, "github", add_plain, false).unwrap();
    assert_eq!(edited.outcome, Outcome::Done);
    let expose = ChangeForm {
        plain: vec!["recovery_code".into()],
        ..Default::default()
    };
    let asked = commands::edit_item(&c, "github", expose, false).unwrap();
    assert!(matches!(asked.outcome, Outcome::Ask { .. }));
    assert!(!serde_json::to_string(&asked).unwrap().contains("SECRET"));
    let (item, text) = shown(&c);
    assert!(
        !text.contains("SECRET"),
        "nothing changed before the person confirmed: {text}"
    );
    assert_eq!(item.plain_values.keys().collect::<Vec<_>>(), ["member_no"]);
    let tidy = ChangeForm {
        remove: vec!["recovery_code".into()],
        secret: vec!["member_no".into()],
        ..Default::default()
    };
    commands::edit_item(&c, "github", tidy, false).unwrap();
    let (item, _) = shown(&c);
    assert_eq!(item.fields, ["member_no", "password"]);
    assert!(item.plain_values.is_empty(), "{:?}", item.plain_values);
}
