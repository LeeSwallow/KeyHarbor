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
    // An API key goes into a browser only at its listed origins (R-36).
    h.add(
        "free",
        "secret",
        &[&shop.base, &other.base],
        json!({"value": note}),
    );
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
    // The frame loads after the top document; wait for its form, not for a fixed time.
    let snapshot = h.snapshot_once(|text| {
        text.lines()
            .skip_while(|line| !line.contains("[frame]"))
            .any(|line| line.contains("textbox \"User\""))
    });
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
    h.add("api", "secret", &[&shop.base], json!({"value": spaced}));
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
    let tabs = h.tabs_once_at("/missing?");
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
    let tabs = h.tabs_once_at("/missing?");
    assert!(
        tabs.to_string()
            .contains("/missing?q={{secret:late.value}}"),
        "{tabs}"
    );
    h.assert_absent(&[&number, &spaced, &encoded, &token, &late]);
}

#[test]
fn select_chooses_an_option_from_a_placeholder_without_showing_it() {
    let Some(browser) = chrome() else { return };
    let base = common::serve_fn(|_| {
        "<!doctype html><title>Apply</title>\
         <label>Service <select id=mil><option value=''>Choose</option>\
         <option value=done>Served</option><option value=exempt>Exempt</option></select></label>\
         <p id=picked></p>\
         <script>document.getElementById('mil').addEventListener('change', e => \
         document.getElementById('picked').textContent = 'picked ' + e.target.selectedIndex)</script>"
            .to_owned()
    });
    let mut h = Harness::new();
    h.add("me", "identity", &[], json!({"military_service": "Exempt"}));
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));
    let chosen = h.ok_call(json!({
        "op": "select", "target": {"selector": "#mil"}, "label": "{{secret:me.military_service}}"
    }));
    assert_eq!(chosen, json!({"selected": {"index": 2}}));
    h.ok_call(json!({"op": "wait", "selector": "#picked"}));
    let snapshot = h.snapshot();
    assert!(snapshot.contains("picked 2"), "change fired: {snapshot}");
    let plain = h.ok_call(json!({"op": "select", "target": {"selector": "#mil"}, "value": "done"}));
    assert_eq!(plain, json!({"selected": {"index": 1}}));
    let missing = h
        .call(json!({"op": "select", "target": {"selector": "#mil"}, "label": "Nope"}))
        .unwrap_err()
        .to_string();
    assert_eq!(missing, "no option matches");
}

#[test]
fn upload_puts_only_allowed_files_into_a_file_input() {
    let Some(browser) = chrome() else { return };
    let base = common::serve_fn(|_| {
        "<!doctype html><title>Apply</title>\
         <label>Resume <input id=cv type=file></label><p id=got></p>\
         <script>document.getElementById('cv').addEventListener('change', e => \
         document.getElementById('got').textContent = 'got ' + e.target.files[0].name + ' ' + e.target.files[0].size)</script>"
            .to_owned()
    });
    let mut h = Harness::new();
    let folder = common::short_tempdir();
    let resume = folder.path().join("resume.pdf");
    std::fs::write(&resume, "%PDF-1.4 test").unwrap();
    let outside = common::short_tempdir();
    let secret = outside.path().join("id_rsa");
    std::fs::write(&secret, "private").unwrap();
    let resume = resume.to_str().unwrap().to_owned();
    let secret = secret.to_str().unwrap().to_owned();
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));
    let refused = h
        .call(json!({"op": "upload", "target": {"selector": "#cv"}, "files": [resume]}))
        .unwrap_err()
        .to_string();
    assert!(refused.starts_with("upload_dirs is not set"), "{refused}");
    h.confirmed(&[
        "settings",
        "set",
        "upload_dirs",
        folder.path().to_str().unwrap(),
    ]);
    let out = h.ok_call(json!({"op": "upload", "target": {"selector": "#cv"}, "files": [resume]}));
    assert_eq!(out, json!({"uploaded": ["resume.pdf"]}));
    h.ok_call(json!({"op": "wait", "selector": "#got"}));
    assert!(h.snapshot().contains("got resume.pdf 13"));
    let outside_refused = h
        .call(json!({"op": "upload", "target": {"selector": "#cv"}, "files": [secret]}))
        .unwrap_err()
        .to_string();
    assert!(
        outside_refused.ends_with("is outside upload_dirs"),
        "{outside_refused}"
    );
}

#[test]
fn screenshots_draw_filled_values_as_discs() {
    let Some(browser) = chrome() else { return };
    let shop = serve_pages();
    let mut h = Harness::new();
    let (first, second) = (
        card_number(&h.synth("shot-a")),
        card_number(&h.synth("shot-b")),
    );
    assert_ne!(first, second);
    h.add("visa", "card", &[&shop.base], json!({"number": first}));
    h.add("amex", "card", &[&shop.base], json!({"number": second}));
    h.open(&browser);

    // Same length, different digits: if the pixels held the digits, the shots would differ.
    let shot = |text: &str| -> Vec<u8> {
        h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
        h.fill("#user", text).unwrap();
        // Focus leaves the input so no caret is drawn.
        h.ok_call(json!({"op": "press", "key": "Tab"}));
        h.ok_call(json!({"op": "press", "key": "Tab"}));
        let path = h.ok_call(json!({"op": "screenshot"}))["path"]
            .as_str()
            .unwrap()
            .to_owned();
        std::fs::read(path).unwrap()
    };
    let visa = shot("{{secret:visa.number}}");
    let amex = shot("{{secret:amex.number}}");
    let plain = shot("4000000000000002");
    assert!(visa == amex, "filled card numbers show in the screenshot");
    assert!(
        visa != plain,
        "the comparison cannot tell digits from discs"
    );
    h.assert_absent(&[&first, &second]);
}

/// One-time codes (#206): refused while `agent_otp` is off; once a person turns it on, the page
/// of the item's origin receives the code of the moment, and nothing the caller sees holds it.
#[test]
fn a_one_time_code_reaches_only_the_bound_page_once_turned_on() {
    let Some(browser) = chrome() else { return };
    let shop = serve_pages();
    let mut h = Harness::new();
    const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    h.add(
        "bank",
        "login",
        &[&shop.base],
        json!({"username": h.synth("user"), "totp_seed": SEED}),
    );
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    let error =
        h.denied(json!({"op": "fill", "target": {"selector": "#user"}, "text": "{{otp:bank}}"}));
    assert_eq!(error, "denied: bank (otp-off)");

    h.confirmed(&["settings", "set", "agent_otp", "true"]);
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    };
    let before = now();
    assert_eq!(h.fill("#user", "{{otp:bank}}").unwrap(), json!({}));
    let after = now();
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    let seed = keyharbor_crypto::otp::OtpSeed::parse(SEED).unwrap();
    let expected = [seed.code_at(before).to_string(), seed.code_at(after).to_string()];
    let typed = {
        let posted = shop.posted.lock().unwrap();
        posted
            .iter()
            .find(|(k, _)| k == "user")
            .map(|(_, v)| v.clone())
            .expect("the page received a user value")
    };
    assert!(expected.contains(&typed), "the page received the code of now");

    let audit: Value = serde_json::from_str(&h.ok(&["audit", "--json"], None)).unwrap();
    let events = audit.as_array().unwrap();
    assert!(events.iter().any(|e| e["kind"] == "otp_used"
        && e["item_name"] == "bank"
        && e["result"] == "ok"
        && e["origin"] == shop.base.as_str()));
    assert!(events.iter().any(|e| e["kind"] == "secret_denied" && e["result"] == "otp-off"));
    for event in events {
        for key in ["item_name", "origin", "result"] {
            let text = event[key].as_str().unwrap_or_default();
            assert!(!text.contains(&typed), "{event}");
        }
    }
    h.assert_absent(&[SEED]);
}
