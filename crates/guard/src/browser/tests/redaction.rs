//! What leaves the page: snapshots and screenshots hide filled elements, or are withheld.

use super::*;

#[test]
fn substituted_elements_stay_redacted_in_snapshots() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-80")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert!(fake.hidden.is_empty());
    // Plain text taints nothing; a substitution taints the element for the session,
    // whatever later happens to its value.
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "plain"),
    )
    .unwrap();
    block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert!(fake.hidden.is_empty());
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap();
    block(guard.browser(&mut fake, "codex").snapshot()).unwrap();
    assert_eq!(fake.hidden, BTreeSet::from([7]));
}

#[test]
fn screenshots_hide_the_elements_that_received_a_value() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-82")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    block(guard.browser(&mut fake, "claude").screenshot()).unwrap();
    assert_eq!(fake.shot_hidden, Some(BTreeSet::new()));
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap();
    block(guard.browser(&mut fake, "codex").screenshot()).unwrap();
    assert_eq!(fake.shot_hidden, Some(BTreeSet::from([7])));
}

#[test]
fn no_screenshot_when_filled_elements_cannot_be_hidden() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-83")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = Fake {
        conceal_fails: true,
        ..Fake::default()
    };
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap();
    let err = block(guard.browser(&mut fake, "claude").screenshot()).unwrap_err();
    assert!(err.to_string().contains("no screenshot taken"), "{err}");
}

#[test]
fn a_masker_that_cannot_be_built_withholds_without_panicking() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-90")).unwrap();
    let (guard, _probe) = guard(&e);
    let mut fake = Fake {
        echo: "KH-SYNTH-pw-90".into(),
        ..Fake::default()
    };
    fake.typed.push("KH-SYNTH-pw-90".into());
    // A response: withheld, never sent unmasked.
    crate::pep::mask::fail::next_build();
    let err = block(guard.browser(&mut fake, "claude").snapshot())
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "response withheld: vault values could not be loaded for masking"
    );
    // A request with agent text: refused, since it cannot be checked for values.
    crate::pep::mask::fail::next_build();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://github.com"),
    )
    .unwrap_err()
    .to_string();
    assert_eq!(
        err,
        "request withheld: vault values could not be loaded for checking"
    );
    // Nothing was poisoned: the next request builds the masker and masks.
    let snap = block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert_eq!(snap, json!({"text": "{{secret:github.password}}"}));
}
