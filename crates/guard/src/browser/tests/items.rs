//! Item rules on the way into a page: origins by kind and expiry.

use super::*;

#[test]
fn api_keys_need_an_origin_and_personal_details_note_their_first_origin() {
    use crate::test_support::field;
    use keyharbor_domain::vault::{ItemKind, ItemName};
    let e = env();
    let v = init(&e);
    let mut key = login("openai", "");
    key.kind = ItemKind::Secret;
    key.fields = vec![field("value", "KH-SYNTH-key-86")];
    key.origins = vec![];
    v.add(key).unwrap();
    let mut me = login("me", "");
    me.kind = ItemKind::Identity;
    me.fields = vec![field("email", "kh-synth-86@example.com")];
    me.origins = vec![];
    v.add(me).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:openai.value}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: openai (no-origin)");
    // Listing an origin lets it go there, and only there.
    v.edit(
        &ItemName::parse("openai").unwrap(),
        keyharbor_core::ItemChange {
            origins: Some(vec![Origin::parse("https://platform.openai.com").unwrap()]),
            ..Default::default()
        },
    )
    .unwrap();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:openai.value}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: openai (origin)");
    v.edit(
        &ItemName::parse("openai").unwrap(),
        keyharbor_core::ItemChange {
            origins: Some(vec![Origin::parse("https://github.com").unwrap()]),
            ..Default::default()
        },
    )
    .unwrap();
    block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:openai.value}}"),
    )
    .unwrap();
    // An identity goes anywhere; its first use at an origin is recorded once.
    for _ in 0..2 {
        block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:me.email}}"),
        )
        .unwrap();
    }
    let kinds = probe.kinds();
    assert_eq!(
        kinds
            .iter()
            .filter(|(k, _)| *k == AuditKind::FirstUseOrigin)
            .count(),
        1,
        "{kinds:?}"
    );
    assert_eq!(
        fake.typed,
        vec![
            "KH-SYNTH-key-86",
            "kh-synth-86@example.com",
            "kh-synth-86@example.com"
        ]
    );
}

#[test]
fn expired_items_are_not_typed() {
    let e = env();
    let mut item = login("github", "KH-SYNTH-pw-87");
    item.details.expiry = keyharbor_core::ExpiryChoice::On(
        keyharbor_domain::date::Expiry::parse("2020-01-01").unwrap(),
    );
    init(&e).add(item).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (expired)");
    assert!(fake.typed.is_empty());
    assert_eq!(
        probe.kinds(),
        vec![(AuditKind::SecretDenied, "expired".into())]
    );
}
