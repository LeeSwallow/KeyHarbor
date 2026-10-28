//! Placeholder substitution against real Chrome: policy, origin binding per frame, and no
//! plaintext in anything the caller sees. Runs only when KEYHARBOR_TEST_CHROME is set.
#![cfg(unix)]

mod common;

use common::{
    chrome,
    harness::{Harness, reference},
    serve_pages,
};
use serde_json::{Value, json};

#[test]
fn placeholders_fill_only_allowed_items_into_bound_frames() {
    let Some(browser) = chrome() else { return };
    let shop = serve_pages();
    let other = serve_pages();
    let mut h = Harness::new();
    let (user, password) = (h.synth("user"), h.synth("password"));
    let (gh_password, inner_user, note) = (h.synth("gh"), h.synth("inner"), h.synth("note"));
    h.add(
        "site",
        "login",
        &[&shop.base],
        json!({"username": user, "password": password}),
    );
    h.add(
        "gh",
        "login",
        &["https://github.com"],
        json!({"username": "octo", "password": gh_password}),
    );
    h.add(
        "inner",
        "login",
        &[&other.base],
        json!({"username": inner_user}),
    );
    h.add("free", "secret", &[], json!({"value": note}));
    h.open(&browser);

    // The bound origin matches: both fields are typed, the response holds no value.
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    assert_eq!(
        h.fill("#user", "{{secret:site.username}}").unwrap(),
        json!({})
    );
    h.fill("#pw", "{{secret:site.password}}").unwrap();
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    {
        let posted = shop.posted.lock().unwrap();
        assert!(
            posted.contains(&("user".into(), user.clone())),
            "user not typed"
        );
        assert!(
            posted.contains(&("pw".into(), password.clone())),
            "password not typed"
        );
    }

    // Review focus 1: a phishing origin gets nothing and the field stays empty.
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    let error = h.denied(
        json!({"op": "fill", "target": {"selector": "#user"}, "text": "{{secret:gh.password}}"}),
    );
    assert_eq!(error, "denied: gh (origin)");
    let snapshot = h.snapshot();
    let line = snapshot
        .lines()
        .find(|l| l.contains("textbox \"User\""))
        .unwrap();
    assert!(!line.contains("value="), "{line}");
    assert!(
        h.call(json!({"op": "type", "text": "{{secret:gh.password}}"}))
            .is_err()
    );

    // Text around a placeholder is kept; several references resolve together.
    h.fill("#user", "{{secret:site.username}}-{{secret:free.value}}")
        .unwrap();

    // Missing items or fields, malformed references and policy are refused by name.
    assert_eq!(
        h.fill("#user", "{{secret:nope.password}}")
            .unwrap_err()
            .to_string(),
        "denied: nope (not_found)"
    );
    assert_eq!(
        h.fill("#user", "{{secret:site.note}}")
            .unwrap_err()
            .to_string(),
        "denied: site (not_found)"
    );
    assert!(h.fill("#user", "{{secret:site}}").is_err());
    h.ok(
        &[
            "policy",
            "deny",
            "--agent",
            "cli",
            "--resource",
            "item",
            "--target",
            "site",
        ],
        None,
    );
    assert_eq!(
        h.fill("#pw", "{{secret:site.password}}")
            .unwrap_err()
            .to_string(),
        "denied: site (policy)"
    );
    h.confirmed(&[
        "policy",
        "allow",
        "--agent",
        "cli",
        "--resource",
        "item",
        "--target",
        "site",
    ]);
    // An unbound item may go anywhere; `type` checks the focused element's frame.
    h.ok_call(json!({"op": "click", "target": {"selector": "#pw"}}));
    h.ok_call(json!({"op": "type", "text": "{{secret:site.password}}"}));

    // Review focus 2: the top document is allowed, the cross-origin iframe is not.
    h.ok_call(json!({
        "op": "navigate",
        "url": format!("{}/embed?port={}", shop.base, other.base.rsplit(':').next().unwrap()),
    }));
    h.ok_call(json!({"op": "wait", "ms": 300}));
    let snapshot = h.snapshot();
    let framed = reference(&snapshot, "[frame]", "textbox \"User\"");
    let error = h.denied(
        json!({"op": "fill", "target": {"ref": framed}, "text": "{{secret:site.username}}"}),
    );
    assert_eq!(error, "denied: site (origin)");
    h.ok_call(
        json!({"op": "fill", "target": {"ref": framed}, "text": "{{secret:inner.username}}"}),
    );
    h.ok_call(json!({"op": "fill", "target": {"ref": framed}, "text": "{{secret:free.value}}"}));
    h.fill("#top", "{{secret:site.username}}").unwrap();
    assert_eq!(
        h.fill("#top", "{{secret:inner.username}}")
            .unwrap_err()
            .to_string(),
        "denied: inner (origin)"
    );

    // Every use and refusal is in the usage record, without values.
    let audit: Value = serde_json::from_str(&h.ok(&["audit", "--json"], None)).unwrap();
    let events = audit.as_array().unwrap();
    let has = |kind: &str, item: &str, result: &str| {
        events.iter().any(|e| {
            e["kind"] == kind
                && e["item_name"] == item
                && e["result"] == result
                && e["agent"] == "cli"
        })
    };
    assert!(has("secret_used", "site", "ok"));
    assert!(has("secret_used", "inner", "ok"));
    assert!(has("secret_denied", "gh", "origin"));
    assert!(has("secret_denied", "site", "policy"));
    assert!(has("secret_denied", "nope", "not_found"));
    let used = events
        .iter()
        .find(|e| e["kind"] == "secret_used" && e["item_name"] == "inner")
        .unwrap();
    assert_eq!(used["origin"], other.base.as_str());
    assert_eq!(used["session_id"], h.id.as_str());
    h.assert_absent(&[&user, &password, &gh_password, &inner_user, &note]);
}

/// Sixteen digits derived from the test directory name, so every run uses a new number.
fn card_number(seed: &str) -> String {
    let mut n: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.bytes() {
        n = (n ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
    format!("4{:015}", n % 1_000_000_000_000_000)
}

#[test]
fn values_shown_back_by_the_page_are_masked() {
    let Some(browser) = chrome() else { return };
    let shop = serve_pages();
    let mut h = Harness::new();
    let number = card_number(&h.synth("card"));
    let spaced = h.synth("p@ss w&rd");
    let token = h.synth("token");
    h.add(
        "visa",
        "card",
        &[&shop.base],
        json!({"number": number, "cvc": "123", "exp_month": "12"}),
    );
    h.add("api", "secret", &[], json!({"value": spaced}));
    h.add("other", "secret", &[], json!({"value": token}));
    h.open(&browser);

    // Review focus 3: the page shows the typed card number as is and in groups of four.
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    h.fill("#user", "{{secret:visa.number}}").unwrap();
    h.fill("#pw", "{{secret:api.value}}").unwrap();
    let before = h.snapshot();
    // The input that received the value stays redacted for the session (C1).
    assert!(before.contains("value=\"[redacted]\""), "{before}");
    let password = before
        .lines()
        .find(|l| l.contains("textbox \"Password\""))
        .unwrap();
    assert!(!password.contains("value="), "{password}");
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    assert!(
        shop.posted
            .lock()
            .unwrap()
            .contains(&("user".into(), number.clone()))
    );
    let after = h.snapshot();
    assert_eq!(
        after.matches("{{secret:visa.number}}").count(),
        2,
        "{after}"
    );
    assert!(!after.contains(&number[..8]), "{after}");

    // A request that carries a value is refused (I1), so values reach URLs only through
    // the page: a form sends them form-urlencoded, and every result masks that form (I5).
    let encoded: String = spaced
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let error = h.denied(json!({
        "op": "navigate",
        "url": format!("{}/missing?a={encoded}&b={token}", shop.base),
    }));
    assert!(
        error.starts_with("denied: the request contains a vault value"),
        "{error}"
    );
    h.ok_call(json!({"op": "navigate", "url": format!("{}/find", shop.base)}));
    h.fill("#q", "{{secret:api.value}}").unwrap();
    h.ok_call(json!({"op": "click", "target": {"selector": "#find"}}));
    h.ok_call(json!({"op": "wait", "ms": 500}));
    let tabs = h.ok_call(json!({"op": "tabs"}));
    assert!(
        tabs.to_string().contains("/missing?q={{secret:api.value}}"),
        "{tabs}"
    );

    // A vault change is picked up by the next response without restarting the session.
    let late = h.synth("late");
    h.add("late", "secret", &[&shop.base], json!({"value": late}));
    h.ok_call(json!({"op": "navigate", "url": format!("{}/find", shop.base)}));
    h.fill("#q", "{{secret:late.value}}").unwrap();
    h.ok_call(json!({"op": "click", "target": {"selector": "#find"}}));
    h.ok_call(json!({"op": "wait", "ms": 500}));
    let tabs = h.ok_call(json!({"op": "tabs"}));
    assert!(
        tabs.to_string()
            .contains("/missing?q={{secret:late.value}}"),
        "{tabs}"
    );
    h.assert_absent(&[&number, &spaced, &encoded, &token, &late]);
}
