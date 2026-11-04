//! A credential's fields the person named (R-48, #205) against real Chrome: a secret one is
//! typed through its placeholder and masked in what comes back, a plain one is typed and left
//! as it is. Runs only when KEYHARBOR_TEST_CHROME is set.
#![cfg(unix)]

mod common;

use common::{chrome, harness::Harness, serve_pages};
use serde_json::json;

#[test]
fn a_named_secret_field_is_typed_and_masked_and_a_plain_one_is_not_masked() {
    let Some(browser) = chrome() else { return };
    let bank = serve_pages();
    let mut h = Harness::new();
    let pin = h.synth("pin");
    let member = "M-plain-20261104";
    let fields = json!({"second_password": pin, "member_no": member}).to_string();
    h.ok(
        &[
            "vault",
            "add",
            "hana",
            "--kind",
            "credential",
            "--origin",
            &bank.base,
            "--plain",
            "member_no",
            "--stdin",
        ],
        Some(&fields),
    );
    h.open(&browser);

    h.ok_call(json!({"op": "navigate", "url": format!("{}/", bank.base)}));
    h.fill("#user", "{{secret:hana.second_password}}").unwrap();
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    assert!(
        bank.posted
            .lock()
            .unwrap()
            .contains(&("user".into(), pin.clone())),
        "the secret field was not typed"
    );
    // The page shows the typed value back; the caller sees the placeholder instead.
    let shown = h.snapshot();
    assert!(shown.contains("{{secret:hana.second_password}}"), "{shown}");

    h.ok_call(json!({"op": "navigate", "url": format!("{}/", bank.base)}));
    h.fill("#user", "{{secret:hana.member_no}}").unwrap();
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    let shown = h.snapshot();
    assert!(
        shown.contains(member),
        "a plain field is not masked: {shown}"
    );

    // No origin, no page (R-36, J-280): the same credential elsewhere is refused.
    let elsewhere = serve_pages();
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", elsewhere.base)}));
    let error = h.denied(
        json!({"op": "fill", "target": {"selector": "#user"}, "text": "{{secret:hana.second_password}}"}),
    );
    assert_eq!(error, "denied: hana (origin)");
    h.assert_absent(&[&pin]);
}
