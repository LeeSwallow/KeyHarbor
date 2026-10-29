//! Agents add vault items only as drafts that never replace an existing item (08 §8.2–§8.3,
//! phase 4 Task 2).
#![cfg(unix)]

mod common;

use common::{chrome, mcp::Mcp, short_tempdir};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn vault(data: &Path, keys: &Path, args: &[&str], stdin: &str) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .env("KEYHARBOR_TEST_KEYSTORE_DIR", keys)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success(), "{args:?}");
}

fn listed<'a>(items: &'a Value, name: &str) -> &'a Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == name)
        .unwrap_or_else(|| panic!("{name} not in {items}"))
}

#[test]
fn a_put_never_replaces_an_existing_item_and_returns_no_value() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    vault(&data, &keys, &["vault", "init"], "");
    vault(
        &data,
        &keys,
        &[
            "vault",
            "add",
            "github",
            "--kind",
            "login",
            "--origin",
            "https://github.com",
            "--stdin",
        ],
        r#"{"password":"original-SECRET"}"#,
    );
    let mut mcp = Mcp::start(&data, tmp.path(), &["--agent", "claude"]);
    let (error, text) = mcp.call(
        "vault_put",
        json!({"name": "github", "kind": "login", "fields": {"password": "injected-SECRET"}, "origins": ["https://github.com"]}),
    );
    assert!(!error, "{text}");
    assert!(!text.contains("injected-SECRET"), "{text}");
    let put: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(put["name"], "github~draft-1");
    assert_eq!(put["draft"], true);
    assert_eq!(
        put["placeholders"],
        json!(["{{secret:github~draft-1.password}}"])
    );

    let items = mcp.ok("vault_list", json!({}));
    assert!(!items.to_string().contains("SECRET"));
    assert_eq!(listed(&items, "github")["status"], "active");
    assert_eq!(listed(&items, "github")["source"], "user");
    assert_eq!(listed(&items, "github~draft-1")["status"], "draft");
    assert_eq!(listed(&items, "github~draft-1")["source"], "agent");

    for bad in [
        json!({"name": "github~draft-9", "kind": "login", "fields": {"password": "x"}}),
        json!({"name": "x", "kind": "nope", "fields": {"password": "x"}}),
        json!({"name": "x", "kind": "login", "fields": {}}),
        json!({"name": "x", "kind": "login", "fields": {"pin": "x"}}),
        json!({"name": "x", "kind": "login", "fields": {"password": "x"}, "origins": ["github.com"]}),
    ] {
        let error = mcp.err("vault_put", bad.clone());
        assert!(!error.contains("\"x\""), "{bad}: {error}");
    }
    mcp.finish();
}

#[test]
fn a_generated_password_is_never_shown_and_follows_the_rule() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    vault(&data, &keys, &["vault", "init"], "");
    let mut mcp = Mcp::start(&data, tmp.path(), &["--agent", "claude"]);
    let generated = mcp.ok(
        "vault_generate",
        json!({"name": "shop", "origin": "https://shop.example", "username": "me", "password_rule": {"length": 32, "symbols": false}}),
    );
    assert_eq!(generated["name"], "shop");
    assert_eq!(
        generated["placeholders"],
        json!(["{{secret:shop.username}}", "{{secret:shop.password}}"])
    );
    let item = mcp.ok("vault_list", json!({}));
    assert_eq!(listed(&item, "shop")["source"], "generated");
    assert_eq!(listed(&item, "shop")["status"], "draft");
    for rule in [
        json!({"length": 4}),
        json!({"length": 500}),
        json!({"symbols": false, "forbid": "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"}),
    ] {
        mcp.err(
            "vault_generate",
            json!({"name": "other", "origin": "https://shop.example", "password_rule": rule}),
        );
    }
    mcp.err(
        "vault_generate",
        json!({"name": "other", "origin": "shop.example"}),
    );
    mcp.finish();
}

/// A draft can be typed by placeholder at once (Review Focus 5); the page gets the value,
/// the agent never does.
#[test]
fn a_generated_draft_is_typed_by_placeholder_with_chrome() {
    let Some(chrome) = chrome() else { return };
    let site = common::serve_pages();
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    vault(&data, &keys, &["vault", "init"], "");
    let mut mcp = Mcp::start(
        &data,
        tmp.path(),
        &["--browser", chrome.to_str().unwrap(), "--agent", "claude"],
    );
    mcp.ok(
        "vault_generate",
        json!({"name": "site", "origin": site.base, "password_rule": {"length": 24, "symbols": false}}),
    );
    let id = mcp.ok("session_create", json!({"name": "c"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.ok("session_open", json!({"id": id}));
    mcp.ok(
        "browser_navigate",
        json!({"session": id, "url": format!("{}/", site.base)}),
    );
    mcp.ok(
        "browser_fill",
        json!({"session": id, "selector": "#user", "text": "me"}),
    );
    let (error, filled) = mcp.call(
        "browser_fill",
        json!({"session": id, "selector": "#pw", "text": "{{secret:site.password}}"}),
    );
    assert!(!error, "{filled}");
    mcp.ok("browser_click", json!({"session": id, "selector": "#go"}));
    mcp.ok("browser_wait", json!({"session": id, "selector": "#done"}));
    let posted = site.posted.lock().unwrap().clone();
    let password = &posted.iter().find(|(k, _)| k == "pw").unwrap().1;
    assert_eq!(password.len(), 24);
    assert!(!filled.contains(password.as_str()));
    mcp.ok("session_stop", json!({"id": id}));
    mcp.finish();
}
