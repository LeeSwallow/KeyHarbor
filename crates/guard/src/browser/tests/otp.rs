//! One-time codes on the way into a page (#206): the setting, the item's switch, the origin
//! binding, then the code of the moment, masked in what comes back and never recorded.

use super::*;
use crate::test_support::field;
use keyharbor_crypto::otp::OtpSeed;
use keyharbor_domain::{
    ports::Clock,
    settings::AGENT_OTP,
    vault::{ItemKind, ItemName},
};

/// RFC 6238's SHA-1 seed, in base32.
const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

/// A login for https://github.com with a password and a seed.
fn bank() -> keyharbor_core::NewItem {
    let mut item = login("bank", "KH-SYNTH-pw-206");
    item.fields.push(field("totp_seed", SEED));
    item
}

fn agent_otp(probe: &SharedStoreProbe, on: bool) {
    probe
        .0
        .lock()
        .unwrap()
        .setting_set(AGENT_OTP, if on { "true" } else { "false" })
        .unwrap();
}

fn type_code(guard: &Guard, fake: &mut Fake) -> anyhow::Result<()> {
    block(guard.browser(fake, "claude").type_text("{{otp:bank}}")).map(|_| ())
}

fn change(v: &keyharbor_core::Vault, change: keyharbor_core::ItemChange) {
    v.edit(&ItemName::parse("bank").unwrap(), change).unwrap();
}

#[test]
fn codes_are_refused_while_agent_otp_is_off() {
    let e = env();
    init(&e).add(bank()).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let err = type_code(&guard, &mut fake).unwrap_err();
    assert_eq!(err.to_string(), "denied: bank (otp-off)");
    assert!(fake.typed.is_empty());
    assert_eq!(
        probe.kinds(),
        vec![(AuditKind::SecretDenied, "otp-off".to_owned())]
    );
    // The seed field itself is still a stored secret: nothing about it changed.
    agent_otp(&probe, false);
    assert!(type_code(&guard, &mut fake).is_err());
}

#[test]
fn an_item_with_codes_turned_off_is_refused() {
    let e = env();
    let v = init(&e);
    v.add(bank()).unwrap();
    change(
        &v,
        keyharbor_core::ItemChange {
            otp_allowed: Some(false),
            ..Default::default()
        },
    );
    let (guard, probe) = guard(&e);
    agent_otp(&probe, true);
    let mut fake = Fake::default();
    let err = type_code(&guard, &mut fake).unwrap_err();
    assert_eq!(err.to_string(), "denied: bank (otp-not-allowed)");
    assert!(fake.typed.is_empty());
}

#[test]
fn codes_go_only_to_the_items_own_bound_origins() {
    let e = env();
    let v = init(&e);
    v.add(bank()).unwrap();
    let (guard, probe) = guard(&e);
    agent_otp(&probe, true);
    let mut fake = Fake::default();
    // Bound elsewhere: the page (github.com) is not one of its origins.
    change(
        &v,
        keyharbor_core::ItemChange {
            origins: Some(vec![Origin::parse("https://bank.example").unwrap()]),
            ..Default::default()
        },
    );
    let err = type_code(&guard, &mut fake).unwrap_err();
    assert_eq!(err.to_string(), "denied: bank (origin)");
    // Not bound at all, even listing the page: a code needs a binding.
    change(
        &v,
        keyharbor_core::ItemChange {
            origins: Some(vec![Origin::parse("https://github.com").unwrap()]),
            origin_binding: Some(false),
            ..Default::default()
        },
    );
    let err = type_code(&guard, &mut fake).unwrap_err();
    assert_eq!(err.to_string(), "denied: bank (no-origin)");
    // Bound with no origin at all.
    change(
        &v,
        keyharbor_core::ItemChange {
            origins: Some(vec![]),
            origin_binding: Some(true),
            ..Default::default()
        },
    );
    let err = type_code(&guard, &mut fake).unwrap_err();
    assert_eq!(err.to_string(), "denied: bank (no-origin)");
    assert!(fake.typed.is_empty());
}

#[test]
fn an_item_without_a_seed_has_no_code() {
    let e = env();
    let v = init(&e);
    v.add(login("bank", "KH-SYNTH-pw-206")).unwrap();
    let mut card = login("visa", "");
    card.kind = ItemKind::Card;
    card.fields = vec![field("number", "4000056655665556")];
    v.add(card).unwrap();
    let (guard, probe) = guard(&e);
    agent_otp(&probe, true);
    let mut fake = Fake::default();
    let err = type_code(&guard, &mut fake).unwrap_err();
    assert_eq!(err.to_string(), "denied: bank (not_found)");
    let err = block(guard.browser(&mut fake, "claude").type_text("{{otp:visa}}")).unwrap_err();
    assert_eq!(err.to_string(), "denied: visa (not_found)");
}

#[test]
fn the_code_of_the_moment_is_typed_masked_and_never_recorded() {
    let e = env();
    init(&e).add(bank()).unwrap();
    let (guard, probe) = guard(&e);
    agent_otp(&probe, true);
    let seed = OtpSeed::parse(SEED).unwrap();
    let before = CLOCK.unix_seconds();
    let mut fake = Fake::default();
    type_code(&guard, &mut fake).unwrap();
    let after = CLOCK.unix_seconds();
    let typed = fake.typed.join("");
    let expected = [seed.code_at(before).to_string(), seed.code_at(after).to_string()];
    assert!(expected.contains(&typed), "typed the code of now");
    assert_eq!(
        probe.kinds(),
        vec![(AuditKind::OtpUsed, "ok".to_owned())],
        "recorded as otp_used, value-free"
    );
    // What the page shows back is masked: a snapshot holding the code, a title, an error.
    let snapshot = block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert_eq!(snapshot["text"], "{{otp:bank}}");
    fake.echo = format!("code {typed}");
    let title = block(guard.browser(&mut fake, "claude").navigate("https://github.com"));
    assert_eq!(title.unwrap()["title"], "code {{otp:bank}}");
    let err = block(guard.browser(&mut fake, "claude").click(&Target::Ref(1))).unwrap_err();
    assert_eq!(err.to_string(), "no element code {{otp:bank}}");
    // Neither the code nor the seed is in the usage record.
    let rows = probe.0.lock().unwrap().list_audit(None).unwrap();
    for row in rows {
        let text = format!("{:?}", row.event);
        assert!(!text.contains(&typed) && !text.contains(SEED), "{text}");
    }
}

#[test]
fn a_password_and_a_code_in_one_text_are_both_checked() {
    let e = env();
    init(&e).add(bank()).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:bank.password}} {{otp:bank}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: bank (otp-off)");
    assert!(fake.typed.is_empty(), "one refusal stops everything");
    agent_otp(&probe, true);
    block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:bank.password}} {{otp:bank}}"),
    )
    .unwrap();
    assert!(fake.typed[0].starts_with("KH-SYNTH-pw-206 "));
}
