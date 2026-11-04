//! Substitution locks (focus moved, vault values in a request) and a person lifting them.

use super::*;

#[test]
fn focus_moving_during_input_locks_substitution_for_the_session() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-82")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake {
        focus_after_insert: Some(8),
        ..Fake::default()
    };
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("substitution locked"), "{err}");
    assert!(guard.is_locked());
    // Every later substitution is refused, for every agent; plain text still works.
    fake.focus_after_insert = None;
    fake.focused = 7;
    let err = block(
        guard
            .browser(&mut fake, "codex")
            .fill(&Target::Ref(7), "{{secret:github.username}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (locked)");
    block(guard.browser(&mut fake, "codex").type_text("plain")).unwrap();
    assert_eq!(fake.typed, vec!["KH-SYNTH-pw-82", "plain"]);
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretUsed, "ok".into()),
            (AuditKind::SubstitutionLocked, "focus_moved".into()),
            (AuditKind::SecretDenied, "locked".into()),
        ]
    );
}

#[test]
fn requests_carrying_vault_values_are_refused_and_lock_the_session() {
    let e = env();
    let v = init(&e);
    v.add(login("github", "KH-SYNTH-pw-83")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    // A guess list holding the value, raw or encoded, gets no answer either way.
    for url in [
        "https://x.example/?q=1111.KH-SYNTH-pw-83.2222",
        "https://x.example/#KH-SYNTH-pw-83",
    ] {
        let err = block(guard.browser(&mut fake, "claude").navigate(url)).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("denied: the request contains a vault value"),
            "{err}"
        );
        assert!(!err.to_string().contains("github"));
    }
    // Harmless requests still work until the limit.
    block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://x.example/"),
    )
    .unwrap();
    assert!(!guard.is_locked());
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("KH-SYNTH-pw-83"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("vault value"));
    assert!(guard.is_locked());
    // After the third probe nothing carrying text is answered, and nothing substitutes.
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://x.example/"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("accepts no more text"), "{err}");
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("accepts no more text"), "{err}");
    block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert!(fake.typed.is_empty());
    let kinds = probe.kinds();
    assert_eq!(
        kinds
            .iter()
            .filter(|(k, _)| *k == AuditKind::MaskProbe)
            .count(),
        3
    );
    assert!(kinds.contains(&(AuditKind::SubstitutionLocked, "mask_probe".into())));
}

#[test]
fn a_person_lifting_a_probe_lock_lets_the_session_take_text_and_substitute_again() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-87")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    for _ in 0..crate::guard::MAX_PROBES {
        let err = block(
            guard
                .browser(&mut fake, "claude")
                .type_text("KH-SYNTH-pw-87"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("vault value"), "{err}");
    }
    assert_eq!(guard.lock_reason(), Some(LockReason::MaskProbe));
    assert_eq!(guard.unlock(LOCAL_AGENT), Some(LockReason::MaskProbe));
    assert_eq!(guard.lock_reason(), None);
    // The count starts over: plain text goes through and placeholders resolve again.
    block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://x.example/"),
    )
    .unwrap();
    block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap();
    assert_eq!(fake.typed, vec!["KH-SYNTH-pw-87"]);
    // Lifting it again changes nothing and records nothing.
    assert_eq!(guard.unlock(LOCAL_AGENT), None);
    let kinds = probe.kinds();
    let locks: Vec<&(AuditKind, String)> = kinds
        .iter()
        .filter(|(k, _)| *k == AuditKind::SubstitutionLocked)
        .collect();
    assert_eq!(
        locks,
        [
            &(AuditKind::SubstitutionLocked, "mask_probe".into()),
            &(AuditKind::SubstitutionLocked, UNLOCKED.into()),
        ]
    );
    // The record names who lifted it and holds no value.
    let store = guard.store().lock().unwrap();
    let lifted = store
        .list_audit(None)
        .unwrap()
        .into_iter()
        .find(|r| r.event.result == UNLOCKED)
        .unwrap();
    assert_eq!(lifted.event.agent, LOCAL_AGENT);
    assert_eq!(lifted.event.item_name, None);
}

#[test]
fn a_person_lifting_a_focus_lock_lets_placeholders_resolve_again() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-88")).unwrap();
    let (guard, _probe) = guard(&e);
    let mut fake = Fake {
        focus_after_insert: Some(8),
        ..Fake::default()
    };
    block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(guard.lock_reason(), Some(LockReason::FocusMoved));
    assert_eq!(guard.unlock(LOCAL_AGENT), Some(LockReason::FocusMoved));
    fake.focus_after_insert = None;
    fake.focused = 7;
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.username}}"),
    )
    .unwrap();
}
