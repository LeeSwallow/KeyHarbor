//! Phase 3 hardening against real Chrome: the attacks of the independent security review
//! (C1–C3, I1–I5) must not get a value or a fragment of one. Runs only when
//! KEYHARBOR_TEST_CHROME is set.
#![cfg(unix)]

mod common;

use common::{chrome, harness::Harness, serve_fn};
use serde_json::json;

/// A login form whose password input a button switches between `password` and `text`.
fn login_page() -> String {
    "<!doctype html><title>Login</title><form action=/next method=get>\
     <label>User <input id=user name=user></label>\
     <label>Password <input id=pw name=pw type=password></label>\
     <button type=button id=show onclick=\"pw.type = pw.type == 'password' ? 'text' : 'password'\">Show</button>\
     <button id=go>Sign in</button></form>"
        .to_owned()
}

/// Every prefix and suffix of `value` at least `min` characters long, and `value` itself.
fn fragments(value: &str, min: usize) -> Vec<String> {
    let mut out = Vec::new();
    for n in min..=value.len() {
        out.push(value[..n].to_owned());
        out.push(value[value.len() - n..].to_owned());
    }
    out
}

#[test]
fn edited_values_stay_hidden_and_passwords_go_only_into_password_inputs() {
    let Some(browser) = chrome() else { return };
    let base = serve_fn(|_| login_page());
    let mut h = Harness::new();
    let (user, password) = (h.synth("user"), h.synth("password"));
    h.add(
        "site",
        "login",
        &[&base],
        json!({"username": user, "password": password}),
    );
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));

    // C1: the password does not go into a text input, where it would show in snapshots.
    assert_eq!(
        h.fill("#user", "{{secret:site.password}}")
            .unwrap_err()
            .to_string(),
        "denied: site (field-kind)"
    );
    h.ok_call(json!({"op": "click", "target": {"selector": "#user"}}));
    assert!(
        h.call(json!({"op": "type", "text": "{{secret:site.password}}"}))
            .is_err()
    );

    // Editing a substituted value no longer lets the masker miss it: the element stays
    // redacted for the whole session.
    h.fill("#user", "{{secret:site.username}}").unwrap();
    h.ok_call(json!({"op": "press", "key": "Backspace"}));
    let snapshot = h.snapshot();
    let line = snapshot
        .lines()
        .find(|l| l.contains("textbox \"User\""))
        .unwrap();
    assert!(line.ends_with("value=\"[redacted]\""), "{line}");

    // A password input switched to text by the page is still redacted, after any edit.
    h.fill("#pw", "{{secret:site.password}}").unwrap();
    h.ok_call(json!({"op": "press", "key": "Backspace"}));
    h.ok_call(json!({"op": "click", "target": {"selector": "#show"}}));
    let snapshot = h.snapshot();
    assert!(snapshot.contains("value=\"[redacted]\""), "{snapshot}");
    h.ok_call(json!({"op": "click", "target": {"selector": "#pw"}}));
    h.ok_call(json!({"op": "press", "key": "Home"}));
    h.ok_call(json!({"op": "press", "key": "Delete"}));
    h.snapshot();

    let mut secrets = fragments(&user, user.len() - 4);
    secrets.extend(fragments(&password, password.len() - 4));
    let refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
    h.assert_absent(&refs);
}
