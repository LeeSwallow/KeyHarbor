//! A scoped MCP server (`--session ID`) sees and touches only its own session.
#![cfg(unix)]

mod common;

use common::mcp::Mcp;
use serde_json::json;
use std::process::{Command, Stdio};

#[test]
fn scoped_server_cannot_discover_or_change_other_sessions() {
    let tmp = tempfile::Builder::new()
        .prefix("kh")
        .tempdir_in("/tmp")
        .unwrap();
    let data = tmp.path().join("data");
    let create = |name: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&data)
            .args(["create", name])
            .output()
            .unwrap();
        String::from_utf8(out.stdout)
            .unwrap()
            .split('\t')
            .next()
            .unwrap()
            .to_owned()
    };
    let owned = create("permitted");
    let other = create("other-private");
    let unknown = "00000000-0000-4000-8000-000000000000";
    let refused = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args(["mcp", "--session", unknown])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!refused.status.success());

    let mut mcp = Mcp::start(&data, tmp.path(), &["--allow-cdp", "--session", &owned]);
    let duplicate = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args(["mcp", "--session", &owned])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    let list = mcp.ok("session_list", json!({}));
    assert!(list.to_string().contains("permitted"));
    assert!(!list.to_string().contains("other-private"));
    assert!(!list.to_string().contains(&other));
    for (name, args) in [
        ("session_create", json!({"name": "bypass"})),
        ("session_open", json!({"id": other})),
        ("session_stop", json!({"id": other})),
        ("session_wait_auth", json!({"id": other})),
        (
            "session_wait_for_resume",
            json!({"id": other, "request_id": "00000000-0000-4000-8000-000000000000", "timeout_seconds": 1}),
        ),
        ("session_endpoint", json!({"id": other})),
        (
            "browser_navigate",
            json!({"session": other, "url": "https://example.com/"}),
        ),
        ("browser_snapshot", json!({"session": other})),
        ("browser_tabs", json!({"session": other})),
    ] {
        let text = mcp.err(name, args);
        assert!(!text.contains("other-private"), "{name}: {text}");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .arg("list")
        .output()
        .unwrap();
    let listed = String::from_utf8(out.stdout).unwrap();
    assert_eq!(listed.lines().count(), 2, "{listed}");
    assert!(listed.contains(&format!("{other}\tother-private\tstopped")));
    mcp.finish();
    // The scope lock is released when the server exits.
    let again = Mcp::start(&data, tmp.path(), &["--session", &owned]);
    again.finish();
}
