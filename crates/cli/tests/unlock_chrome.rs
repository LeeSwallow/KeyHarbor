//! A locked session substitutes again only after a person lifts the lock (#190, J-220), with
//! real Chrome. Runs only when KEYHARBOR_TEST_CHROME is set.
#![cfg(unix)]

mod common;

use common::{chrome, harness::Harness, serve_fn};
use serde_json::json;

fn login_page() -> String {
    "<!doctype html><title>Login</title><form action=/next method=get>\
     <label>User <input id=user name=user></label>\
     <label>Password <input id=pw name=pw type=password></label></form>"
        .to_owned()
}

#[test]
fn a_locked_session_substitutes_again_only_after_a_person_lifts_the_lock() {
    let Some(browser) = chrome() else { return };
    let base = serve_fn(|_| login_page());
    let mut h = Harness::new();
    let password = h.synth("password");
    h.add("site", "login", &[&base], json!({"password": password}));
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));

    // Three requests carrying the value lock the session (the rule itself is unchanged).
    h.ok_call(json!({"op": "click", "target": {"selector": "#pw"}}));
    for _ in 0..3 {
        let error = h.denied(json!({"op": "type", "text": password}));
        assert!(error.contains("vault value"), "{error}");
    }
    let locked = h.fill("#pw", "{{secret:site.password}}").unwrap_err();
    assert!(
        locked.to_string().contains("accepts no more text"),
        "{locked}"
    );

    // An agent cannot lift it: not through the control socket under its name, nor through
    // the CLI without a person at a terminal.
    let error = h.denied(json!({"op": "unlock", "agent": "claude"}));
    assert!(error.contains("only a person"), "{error}");
    let out = h.run(&["unlock", &h.id.clone()], None);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("needs confirmation from a person at a terminal"),
        "{stderr}"
    );
    assert!(h.fill("#pw", "{{secret:site.password}}").is_err());

    // The person confirms; the session takes text and substitutes again.
    h.confirmed(&["unlock", &h.id.clone()]);
    h.fill("#pw", "{{secret:site.password}}").unwrap();
    assert!(!h.snapshot().contains(&password));

    let audit = h.ok(&["audit", "--json"], None);
    assert!(
        audit.contains("\"result\":\"mask_probe\""),
        "the lock was recorded: {audit}"
    );
    assert!(
        audit.contains("\"result\":\"unlocked\""),
        "the lifting was recorded: {audit}"
    );
    // Lifting it again finds nothing to lift and asks nothing.
    let out = h.run(&["unlock", &h.id.clone()], None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not locked"));
    h.assert_absent(&[&password]);
}
