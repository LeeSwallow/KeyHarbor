//! An agent asks the user for information and waits; the user answers from the CLI
//! (08 §8.2 ③, phase 4 Task 3).
#![cfg(unix)]

mod common;

use common::{mcp::Mcp, short_tempdir};
use serde_json::json;
use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn cli(data: &Path, keys: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .env("KEYHARBOR_TEST_KEYSTORE_DIR", keys)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn ok(out: Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_user_answers_a_request_and_the_agent_gets_placeholders_only() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    ok(cli(&data, &keys, &["vault", "init"], ""));
    let mut mcp = Mcp::start(&data, tmp.path(), &["--agent", "codex"]);
    let asked = mcp.ok(
        "vault_request",
        json!({"kind": "card", "name": "my-visa", "reason": "pay at the shop", "origin": "https://shop.example"}),
    );
    let id = asked["request_id"].as_str().unwrap().to_owned();
    assert_eq!(asked["status"], "pending");

    // A wait that runs out ends the wait only (Review Focus 3).
    let waited = mcp.ok(
        "vault_wait",
        json!({"request_id": id, "timeout_seconds": 1}),
    );
    assert_eq!(waited["status"], "pending");
    let listed = ok(cli(&data, &keys, &["vault", "requests"], ""));
    assert_eq!(
        listed.trim_end(),
        format!("{id}\tcodex\tcard\tmy-visa\thttps://shop.example\tpay at the shop")
    );

    let answer = r#"{"number":"4111111111111111","cvc":"987"}"#;
    let fulfilled = ok(cli(
        &data,
        &keys,
        &["vault", "fulfill", &id, "--stdin"],
        answer,
    ));
    assert_eq!(fulfilled.trim_end(), format!("{id}\tfulfilled\tmy-visa"));
    let (error, text) = mcp.call(
        "vault_wait",
        json!({"request_id": id, "timeout_seconds": 1}),
    );
    assert!(!error, "{text}");
    assert!(
        !text.contains("4111111111111111") && !text.contains("987"),
        "{text}"
    );
    let done: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(done["status"], "fulfilled");
    assert_eq!(done["name"], "my-visa");
    let placeholders = done["placeholders"].as_array().unwrap();
    assert!(
        placeholders.contains(&json!("{{secret:my-visa.number}}")),
        "{done}"
    );
    let items = mcp.ok("vault_list", json!({}));
    assert_eq!(items[0]["status"], "active");
    assert_eq!(items[0]["source"], "requested");

    assert_eq!(ok(cli(&data, &keys, &["vault", "requests"], "")), "");
    assert!(
        !cli(&data, &keys, &["vault", "decline", &id], "")
            .status
            .success()
    );
    mcp.finish();
}

#[test]
fn a_request_can_be_declined_or_answered_with_an_existing_item() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    ok(cli(&data, &keys, &["vault", "init"], ""));
    ok(cli(
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
        r#"{"password":"pw-SECRET"}"#,
    ));
    let mut mcp = Mcp::start(&data, tmp.path(), &["--agent", "codex"]);
    let ask = |mcp: &mut Mcp| {
        mcp.ok(
            "vault_request",
            json!({"kind": "login", "name": "gh", "reason": "push the fix"}),
        )["request_id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let declined = ask(&mut mcp);
    ok(cli(&data, &keys, &["vault", "decline", &declined], ""));
    let answer = mcp.ok(
        "vault_wait",
        json!({"request_id": declined, "timeout_seconds": 1}),
    );
    assert_eq!(
        answer,
        json!({"request_id": declined, "status": "declined"})
    );

    let reused = ask(&mut mcp);
    let wrong = cli(
        &data,
        &keys,
        &["vault", "fulfill", &reused, "--use", "nope"],
        "",
    );
    assert!(!wrong.status.success());
    ok(cli(
        &data,
        &keys,
        &["vault", "fulfill", &reused, "--use", "github"],
        "",
    ));
    let answer = mcp.ok(
        "vault_wait",
        json!({"request_id": reused, "timeout_seconds": 1}),
    );
    assert_eq!(answer["name"], "github");
    assert!(!answer.to_string().contains("pw-SECRET"));

    for bad in [
        json!({"kind": "card", "name": "x~draft-1", "reason": "r"}),
        json!({"kind": "nope", "name": "x", "reason": "r"}),
        json!({"kind": "card", "name": "x", "reason": ""}),
        json!({"kind": "card", "name": "x", "reason": "r", "origin": "shop.example"}),
    ] {
        mcp.err("vault_request", bad);
    }
    mcp.err("vault_wait", json!({"request_id": "not-an-id"}));
    mcp.err(
        "vault_wait",
        json!({"request_id": reused, "timeout_seconds": 0}),
    );
    mcp.err(
        "vault_wait",
        json!({"request_id": "00000000-0000-4000-8000-000000000000", "timeout_seconds": 1}),
    );
    mcp.finish();
}
