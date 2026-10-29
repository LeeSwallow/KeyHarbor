//! Per-session site sign-in states through the CLI and MCP (02 §2.7, phase 4 Task 1).
#![cfg(unix)]

mod common;

use common::{mcp::Mcp, short_tempdir};
use serde_json::json;
use std::{
    path::Path,
    process::{Command, Output, Stdio},
};

fn cli(data: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
fn create(data: &Path, name: &str) -> String {
    let out = cli(data, &["create", name]);
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .split('\t')
        .next()
        .unwrap()
        .to_owned()
}
fn stdout(out: Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn an_agent_records_and_reads_a_site_state_and_the_user_lists_it() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let id = create(&data, "work");
    let mut mcp = Mcp::start(&data, tmp.path(), &["--agent", "claude"]);
    let github = json!({"session": id, "origin": "https://github.com"});
    assert_eq!(mcp.ok("site_state_get", github.clone())["state"], "unknown");
    let set = mcp.ok(
        "site_state_set",
        json!({"session": id, "origin": "https://github.com", "state": "logged_in", "item": "github"}),
    );
    assert_eq!(set["state"], "logged_in");
    assert_eq!(mcp.ok("site_state_get", github)["state"], "logged_in");
    mcp.finish();

    let listed = stdout(cli(&data, &["site", "list", &id]));
    let fields: Vec<&str> = listed.trim_end().split('\t').collect();
    assert_eq!(fields[..3], ["https://github.com", "logged_in", "github"]);
    assert_eq!(fields[4], "claude");

    stdout(cli(
        &data,
        &["site", "set", &id, "https://github.com", "expired"],
    ));
    let listed = stdout(cli(&data, &["site", "list", &id]));
    assert!(
        listed.starts_with("https://github.com\texpired\t-\t"),
        "{listed}"
    );
    assert!(listed.trim_end().ends_with("\tcli"), "{listed}");

    let audit = stdout(cli(&data, &["audit", "--json"]));
    assert_eq!(audit.matches("site_state_changed").count(), 2, "{audit}");
}

#[test]
fn malformed_origins_states_and_items_are_refused() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let id = create(&data, "work");
    let mut mcp = Mcp::start(&data, tmp.path(), &[]);
    for origin in [
        "github.com",
        "https://github.com/login",
        "http://github.com",
        "https://user@github.com",
    ] {
        let error = mcp.err(
            "site_state_set",
            json!({"session": id, "origin": origin, "state": "logged_in"}),
        );
        assert!(error.contains("origin"), "{origin}: {error}");
        mcp.err("site_state_get", json!({"session": id, "origin": origin}));
    }
    mcp.err(
        "site_state_set",
        json!({"session": id, "origin": "https://github.com", "state": "signed_in"}),
    );
    mcp.err(
        "site_state_set",
        json!({"session": id, "origin": "https://github.com", "state": "logged_in", "item": "a b"}),
    );
    mcp.err(
        "site_state_get",
        json!({"session": "00000000-0000-4000-8000-000000000000", "origin": "https://github.com"}),
    );
    mcp.finish();
    assert!(
        !cli(&data, &["site", "set", &id, "github.com", "expired"])
            .status
            .success()
    );
    assert_eq!(stdout(cli(&data, &["site", "list", &id])), "");
}

#[test]
fn site_states_follow_session_scope_and_policy() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let mine = create(&data, "mine");
    let other = create(&data, "other");
    let mut scoped = Mcp::start(&data, tmp.path(), &["--session", &mine]);
    let error = scoped.err(
        "site_state_get",
        json!({"session": other, "origin": "https://github.com"}),
    );
    assert!(error.contains("outside this server's scope"), "{error}");
    scoped.finish();

    let rule = [
        "policy",
        "deny",
        "--agent",
        "codex",
        "--resource",
        "session",
        "--target",
        "*",
    ];
    assert!(cli(&data, &rule).status.success());
    let mut codex = Mcp::start(&data, tmp.path(), &["--agent", "codex"]);
    let denied = codex.err(
        "site_state_set",
        json!({"session": mine, "origin": "https://github.com", "state": "logged_in"}),
    );
    assert!(denied.contains("denied by policy"), "{denied}");
    codex.finish();
}
