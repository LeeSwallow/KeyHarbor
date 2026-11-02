//! A session in a real ego lite through the relay (ADR-0018): the same browser tools, the
//! same substitution and masking as over Chrome. Runs only when KEYHARBOR_TEST_EGO is set and
//! ego lite is running.
#![cfg(unix)]

mod common;

use common::{harness::Harness, serve_pages};
use serde_json::json;
use std::path::Path;

#[test]
fn ego_session_navigates_and_snapshots() {
    if !common::ego() {
        return;
    }
    let site = serve_pages();
    let mut h = Harness::new();
    h.open(Path::new("ego-lite"));
    let listed = h.ok(&["ls"], None);
    assert!(listed.contains("\tego-lite"), "{listed}");
    h.ok_call(json!({"op": "navigate", "url": format!("{}/find", site.base)}));
    h.ok_call(json!({"op": "wait", "selector": "#q"}));
    let snapshot = h.snapshot();
    assert!(snapshot.contains("Find"), "{snapshot}");
    let tab = h.ok_call(json!({"op": "new_tab", "url": format!("{}/", site.base)}));
    assert!(tab["id"].as_str().is_some(), "{tab}");
    let tabs = h.ok_call(json!({"op": "tabs"}));
    assert_eq!(tabs.as_array().unwrap().len(), 2, "{tabs}");
}

#[test]
fn ego_fill_substitutes_and_masks() {
    if !common::ego() {
        return;
    }
    let shop = serve_pages();
    let mut h = Harness::new();
    let (user, password) = (h.synth("user"), h.synth("password"));
    h.add(
        "site",
        "login",
        &[&shop.base],
        json!({"username": user, "password": password}),
    );
    h.open(Path::new("ego-lite"));
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    h.ok_call(json!({"op": "wait", "selector": "#user"}));
    h.fill("#user", "{{secret:site.username}}").unwrap();
    h.fill("#pw", "{{secret:site.password}}").unwrap();
    let snapshot = h.snapshot();
    assert!(!snapshot.contains(&user), "{snapshot}");
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    {
        let posted = shop.posted.lock().unwrap();
        assert!(
            posted.contains(&("user".into(), user.clone())),
            "{posted:?}"
        );
        assert!(posted.contains(&("pw".into(), password.clone())));
    }
    // The page shows the user name back; the caller sees the placeholder.
    let after = h.snapshot();
    assert!(after.contains("{{secret:site.username}}"), "{after}");
    h.assert_absent(&[&user, &password]);
}

#[test]
fn ego_pages_outside_the_grants_are_replaced_after_they_load() {
    if !common::ego() {
        return;
    }
    let elsewhere = serve_pages();
    let away = format!("{}/", elsewhere.base);
    let home = common::serve_fn(move |_| {
        format!("<!doctype html><title>Home</title><a id=away href=\"{away}\">away</a>")
    });
    let mut h = Harness::new();
    h.open(Path::new("ego-lite"));
    h.confirmed(&["grant", "add", &h.id.clone(), &home]);
    h.ok_call(json!({"op": "navigate", "url": format!("{home}/")}));
    h.ok_call(json!({"op": "wait", "selector": "#away"}));
    h.ok_call(json!({"op": "click", "target": {"selector": "#away"}}));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let tabs = h.ok_call(json!({"op": "tabs"}));
        let url = tabs[0]["url"].as_str().unwrap_or_default().to_owned();
        if url == "about:blank" {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "still at {url}");
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(h.ok(&["audit"], None).contains("navigation_denied"));
}
