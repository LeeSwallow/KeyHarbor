//! Typing through the checked path: placeholders, refusals, field kinds, navigations and
//! the refusal counter.

use super::*;

#[test]
fn placeholders_are_resolved_into_the_page_and_masked_on_the_way_out() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-77")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    block(async {
        let mut browser = guard.browser(&mut fake, "claude");
        let target = Target::Ref(7);
        let out = browser
            .fill(&target, "{{secret:github.password}}")
            .await
            .unwrap();
        assert_eq!(out, FillOutcome::default());
        // The page has the value; the agent sees only the placeholder.
        let snap = browser.snapshot().await.unwrap();
        assert_eq!(snap, json!({"text": "{{secret:github.password}}"}));
    });
    assert_eq!(fake.typed, vec!["KH-SYNTH-pw-77".to_owned()]);
    assert_eq!(probe.kinds(), vec![(AuditKind::SecretUsed, "ok".into())]);
    // Errors are masked too.
    fake.echo = "KH-SYNTH-pw-77".into();
    let err = block(guard.browser(&mut fake, "claude").click(&Target::Ref(1)))
        .unwrap_err()
        .to_string();
    assert_eq!(err, "no element {{secret:github.password}}");
    assert!(open_vault(&e).is_ok());
}

#[test]
fn refusals_type_nothing_and_are_recorded() {
    let e = env();
    let v = init(&e);
    v.add(login("github", "KH-SYNTH-pw-78")).unwrap();
    let mut bound_elsewhere = login("bank", "KH-SYNTH-bank");
    bound_elsewhere.origins = vec![Origin::parse("https://bank.example").unwrap()];
    v.add(bound_elsewhere).unwrap();
    SqliteStore::open(&e.data)
        .unwrap()
        .policy_set(&Rule::new("codex", Resource::Item, "github", Effect::Deny).unwrap())
        .unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let cases = [
        (
            "codex",
            "{{secret:github.password}}",
            "denied: github (policy)",
        ),
        (
            "claude",
            "{{secret:bank.password}}",
            "denied: bank (origin)",
        ),
        (
            "claude",
            "{{secret:nope.password}}",
            "denied: nope (not_found)",
        ),
        (
            "claude",
            "{{secret:github.cvc}}",
            "denied: github (not_found)",
        ),
    ];
    for (agent, text, expected) in cases {
        let err = block(guard.browser(&mut fake, agent).type_text(text)).unwrap_err();
        assert_eq!(err.to_string(), expected);
    }
    assert!(fake.typed.is_empty());
    let kinds = probe.kinds();
    assert_eq!(kinds.len(), 4);
    assert!(kinds.iter().all(|(k, _)| *k == AuditKind::SecretDenied));
    // Text without placeholders passes through untouched.
    block(guard.browser(&mut fake, "codex").type_text("hello")).unwrap();
    assert_eq!(fake.typed, vec!["hello".to_owned()]);
}

#[test]
fn password_fields_go_only_into_password_inputs() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-79")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake {
        element: Element {
            local_name: "input".into(),
            input_type: Some("text".into()),
            ..Element::default()
        },
        ..Fake::default()
    };
    let target = Target::Ref(7);
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .fill(&target, "{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (field-kind)");
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (field-kind)");
    assert!(fake.typed.is_empty());
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretDenied, "field-kind".into()),
            (AuditKind::SecretDenied, "field-kind".into())
        ]
    );
    // The username may go into a text input; a text input that declares itself a
    // password field takes the password.
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&target, "{{secret:github.username}}"),
    )
    .unwrap();
    fake.element.autocomplete = vec!["current-password".into()];
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&target, "{{secret:github.password}}"),
    )
    .unwrap();
    assert_eq!(fake.typed, vec!["me@x.com", "KH-SYNTH-pw-79"]);
}

#[test]
fn text_goes_only_into_elements_that_take_text() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-81")).unwrap();
    let (guard, probe) = guard(&e);
    for local_name in ["div", "iframe", "object", "button", "body"] {
        let mut fake = Fake {
            element: Element {
                local_name: local_name.into(),
                ..Element::default()
            },
            ..Fake::default()
        };
        let err = block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:github.username}}"),
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "denied: github (focus)", "{local_name}");
        assert!(fake.typed.is_empty());
    }
    assert_eq!(probe.kinds().len(), 5);
}

#[test]
fn navigations_under_way_or_during_the_checks_stop_substitution() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-84")).unwrap();
    let (guard, probe) = guard(&e);
    // A click started a navigation that has not finished.
    let mut fake = Fake {
        settled: false,
        ..Fake::default()
    };
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (navigating)");
    // Page code navigates between the checks and the input.
    let mut fake = Fake {
        navigate_on_clear: 1,
        ..Fake::default()
    };
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (navigating)");
    assert!(fake.typed.is_empty());
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretDenied, "navigating".into()),
            (AuditKind::SecretDenied, "navigating".into())
        ]
    );
}

#[test]
fn a_flood_of_refusals_is_recorded_as_one_counter_per_second() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-85")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    for _ in 0..25 {
        assert!(
            block(
                guard
                    .browser(&mut fake, "claude")
                    .type_text("{{secret:nope.x}}")
            )
            .is_err()
        );
    }
    CLOCK.advance(std::time::Duration::from_secs(1));
    assert!(
        block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:nope.x}}")
        )
        .is_err()
    );
    let kinds = probe.kinds();
    let denied = kinds
        .iter()
        .filter(|(k, _)| *k == AuditKind::SecretDenied)
        .count();
    assert_eq!(denied, crate::guard::MAX_DENIALS_PER_SECOND as usize + 1);
    assert!(
        kinds.contains(&(AuditKind::DeniedSuppressed, "15".into())),
        "{kinds:?}"
    );
    // Whatever is left when the session ends is counted too.
    for _ in 0..12 {
        let _ = block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:nope.x}}"),
        );
    }
    drop(guard);
    assert!(
        probe
            .kinds()
            .contains(&(AuditKind::DeniedSuppressed, "3".into()))
    );
}
