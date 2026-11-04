//! `browser_login`: both fields through the checked path, or nothing.

use super::*;

#[test]
fn login_fills_both_fields_through_the_checked_path_or_nothing() {
    let e = env();
    let v = init(&e);
    v.add(login("github", "KH-SYNTH-pw-91")).unwrap();
    let mut card = login("visa", "");
    card.kind = keyharbor_domain::vault::ItemKind::Card;
    card.fields = vec![crate::test_support::field("number", "4242424242424242")];
    v.add(card).unwrap();
    let (guard, probe) = guard(&e);
    let github = ItemName::parse("github").unwrap();
    // A page with no password input gets nothing.
    let mut fake = Fake {
        login: LoginFields {
            username: Some(7),
            password: None,
        },
        ..Fake::default()
    };
    let err = block(guard.browser(&mut fake, "claude").login(&github, true)).unwrap_err();
    assert_eq!(err.to_string(), "not found: password field");
    let mut fake = Fake {
        login: LoginFields {
            username: None,
            password: Some(7),
        },
        ..Fake::default()
    };
    let err = block(guard.browser(&mut fake, "claude").login(&github, true)).unwrap_err();
    assert_eq!(err.to_string(), "not found: username field");
    assert!(fake.typed.is_empty());
    // Only login items, and the same checks as fill.
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .login(&ItemName::parse("visa").unwrap(), true),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "visa is not a login item");
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .login(&ItemName::parse("nope").unwrap(), true),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: nope (not_found)");
    let mut fake = Fake::default();
    let out = block(guard.browser(&mut fake, "claude").login(&github, true)).unwrap();
    assert_eq!(
        out,
        crate::LoginOutcome {
            filled: vec!["username", "password"],
            submitted: true,
            url: "https://github.com/".into()
        }
    );
    assert_eq!(fake.typed, vec!["me@x.com", "KH-SYNTH-pw-91"]);
    assert_eq!(fake.pressed, vec!["Enter"]);
    let used = probe
        .kinds()
        .into_iter()
        .filter(|(k, _)| *k == AuditKind::SecretUsed)
        .count();
    assert_eq!(used, 2);
    // The result is masked like any other.
    let mut fake = Fake {
        echo: "?pw=KH-SYNTH-pw-91".into(),
        ..Fake::default()
    };
    let out = block(guard.browser(&mut fake, "claude").login(&github, false)).unwrap();
    assert_eq!(out.url, "https://github.com/?pw={{secret:github.password}}");
    assert!(fake.pressed.is_empty());
}
