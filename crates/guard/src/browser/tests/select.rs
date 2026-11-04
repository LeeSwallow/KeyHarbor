//! `browser_select`: options chosen by label, placeholders resolved and masked.

use super::*;

fn select_fake(options: &[(&str, &str)]) -> Fake {
    Fake {
        element: Element {
            local_name: "select".into(),
            ..Element::default()
        },
        options: options
            .iter()
            .map(|(v, l)| ((*v).to_owned(), (*l).to_owned()))
            .collect(),
        ..Fake::default()
    }
}
fn identity(military: &str) -> keyharbor_core::NewItem {
    keyharbor_core::NewItem {
        name: keyharbor_domain::vault::ItemName::parse("me").unwrap(),
        kind: keyharbor_domain::vault::ItemKind::Identity,
        fields: vec![crate::test_support::field("military_service", military)],
        origins: vec![],
        origin_binding: None,
        status: keyharbor_core::ItemStatus::Active,
        source: keyharbor_core::ItemSource::User,
        details: Default::default(),
    }
}

#[test]
fn select_by_label() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = select_fake(&[("1", "Served"), ("2", "Exempt")]);
    let out = block(
        guard
            .browser(&mut fake, "claude")
            .select(&Target::Ref(7), &Choice::Label("Exempt".into())),
    )
    .unwrap();
    assert_eq!(out, json!({"selected": {"index": 1}}));
}

#[test]
fn select_needs_a_select_element() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .select(&Target::Ref(7), &Choice::Index(0)),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "browser_select needs a <select> element");
    assert!(fake.asked.is_empty());
}

#[test]
fn select_placeholder_is_resolved_and_masked() {
    let e = env();
    init(&e).add(identity("KH-SYNTH-served")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = select_fake(&[("a", "KH-SYNTH-exempt"), ("b", "KH-SYNTH-served")]);
    let out = block(async {
        let mut browser = guard.browser(&mut fake, "claude");
        let out = browser
            .select(
                &Target::Ref(7),
                &Choice::Label("{{secret:me.military_service}}".into()),
            )
            .await
            .unwrap();
        browser.snapshot().await.unwrap();
        out
    });
    assert_eq!(out, json!({"selected": {"index": 1}}));
    assert_eq!(fake.asked, vec![Choice::Label("KH-SYNTH-served".into())]);
    assert!(
        fake.hidden.contains(&7),
        "the chosen select is redacted after"
    );
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretUsed, "ok".into()),
            (AuditKind::FirstUseOrigin, "ok".into())
        ]
    );
}

#[test]
fn select_refuses_a_password_field_value() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-90")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = select_fake(&[("x", "KH-SYNTH-pw-90")]);
    let err = block(guard.browser(&mut fake, "claude").select(
        &Target::Ref(7),
        &Choice::Value("{{secret:github.password}}".into()),
    ))
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (field-kind)");
    assert!(fake.asked.is_empty());
}

#[test]
fn unmatched_placeholder_does_not_echo_value() {
    let e = env();
    init(&e).add(identity("KH-SYNTH-other")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = select_fake(&[("1", "Served")]);
    let err = block(guard.browser(&mut fake, "claude").select(
        &Target::Ref(7),
        &Choice::Label("{{secret:me.military_service}}".into()),
    ))
    .unwrap_err()
    .to_string();
    assert_eq!(err, "no option matches");
    assert!(!fake.hidden.contains(&7));
}
