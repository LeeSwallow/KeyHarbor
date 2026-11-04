//! Fields the person names on every kind, each secret or plain, the `credential` kind and its
//! templates (R-48, #205): secret values come only from a hidden prompt or stdin, and no
//! command, listing or MCP answer carries one.
#![cfg(unix)]

mod common;

use common::{mcp::Mcp, on_terminal_answering, short_tempdir};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

const SECOND: &str = "2nd-SECRET-4821";

fn run(data: &Path, keys: &Path, args: &[&str], stdin: &str) -> Output {
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

/// Runs a command that must succeed and print no secret value, and returns its stdout.
fn cli(data: &Path, keys: &Path, args: &[&str], stdin: &str) -> String {
    let out = run(data, keys, args, stdin);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{args:?}: {stderr}");
    assert!(
        !stdout.contains("SECRET") && !stderr.contains("SECRET"),
        "{args:?}: {stdout}{stderr}"
    );
    stdout.into_owned()
}

/// Runs a command that must fail, and returns its stderr (which names no value either).
fn refused(data: &Path, keys: &Path, args: &[&str], stdin: &str) -> String {
    let out = run(data, keys, args, stdin);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!out.status.success(), "{args:?} should fail");
    assert!(!stderr.contains("SECRET"), "{stderr}");
    stderr
}

fn listed(data: &Path, keys: &Path) -> Value {
    serde_json::from_str(&cli(data, keys, &["vault", "list", "--json"], "")).unwrap()
}

#[test]
fn a_credential_from_a_template_asks_its_secret_field_hidden_and_shows_its_plain_one() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    cli(&data, &keys, &["vault", "init"], "");
    let template = [
        "vault",
        "template",
        "add",
        "bank",
        "--field",
        "second_password",
        "--field",
        "member_no",
        "--plain",
        "member_no",
    ];
    assert_eq!(cli(&data, &keys, &template, ""), "added\tbank\n");
    assert!(refused(&data, &keys, &template, "").contains("already exists"));
    let clash = ["vault", "template", "add", "x", "--field", "note"];
    assert!(refused(&data, &keys, &clash, "").contains("every credential has"));
    let stray = [
        "vault", "template", "add", "x", "--field", "a", "--plain", "b",
    ];
    assert!(refused(&data, &keys, &stray, "").contains("--plain b"));
    assert_eq!(
        cli(&data, &keys, &["vault", "template", "ls"], ""),
        "bank\tsecond_password,member_no\tplain member_no\n"
    );
    let as_json: Value = serde_json::from_str(&cli(
        &data,
        &keys,
        &["vault", "template", "ls", "--json"],
        "",
    ))
    .unwrap();
    assert_eq!(
        as_json,
        json!([{"name": "bank", "fields": [
            {"name": "second_password", "secret": true},
            {"name": "member_no", "secret": false},
        ]}])
    );
    let not_credential = [
        "vault",
        "add",
        "x",
        "--kind",
        "login",
        "--template",
        "bank",
        "--stdin",
    ];
    assert!(refused(&data, &keys, &not_credential, "{}").contains("credential"));

    // The template's plain field comes from --set; its secret one from a hidden prompt.
    let added = on_terminal_answering(
        &data,
        &keys,
        &[
            "vault",
            "add",
            "hana",
            "--kind",
            "credential",
            "--template",
            "bank",
            "--set",
            "member_no=M-20261104",
            "--origin",
            "https://bank.example",
        ],
        &[
            ("note (empty to skip): ", ""),
            ("second_password (empty to skip): ", SECOND),
        ],
    );
    assert!(added.success, "{}", added.output);
    assert!(!added.output.contains("SECRET"), "{}", added.output);
    cli(&data, &keys, &["vault", "template", "rm", "bank"], "");
    assert!(cli(&data, &keys, &["vault", "template", "ls"], "").is_empty());

    let item = &listed(&data, &keys)[0];
    assert_eq!(item["kind"], "credential");
    assert_eq!(item["domain"], "credentials");
    assert_eq!(item["fields"], json!(["member_no", "second_password"]));
    assert_eq!(item["plain_values"], json!({"member_no": "M-20261104"}));
    assert_eq!(item["origin_binding"], false);
    assert_eq!(item["run_allowed"], false);

    let mut mcp = Mcp::start(&data, tmp.path(), &[]);
    let seen = mcp.ok("vault_list", json!({}));
    assert!(!seen.to_string().contains("SECRET"), "{seen}");
    assert_eq!(seen[0]["plain_values"], json!({"member_no": "M-20261104"}));
    let placeholders = seen[0]["placeholders"].to_string();
    assert!(
        placeholders.contains("hana.second_password"),
        "{placeholders}"
    );
    mcp.finish();
}

#[test]
fn a_field_of_ones_own_stays_off_the_command_line_unless_marked_plain() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    cli(&data, &keys, &["vault", "init"], "");
    // A secret custom field never arrives as an argument, not even refused with its value.
    let secret_set = [
        "vault",
        "add",
        "gh",
        "--kind",
        "login",
        "--set",
        "recovery=rc-SECRET-1",
    ];
    assert!(refused(&data, &keys, &secret_set, "").contains("field recovery is secret"));
    // A name some kind declares secret stays secret on the command line, even marked plain.
    let declared = [
        "vault",
        "add",
        "c",
        "--kind",
        "credential",
        "--plain",
        "password",
        "--set",
        "password=pw-SECRET-1",
    ];
    assert!(refused(&data, &keys, &declared, "").contains("field password is secret"));
    cli(
        &data,
        &keys,
        &[
            "vault",
            "add",
            "gh",
            "--kind",
            "login",
            "--origin",
            "https://github.com",
            "--plain",
            "member_no",
            "--set",
            "member_no=M-1",
            "--stdin",
        ],
        r#"{"password":"pw-SECRET-1234","recovery":"rc-SECRET-9876"}"#,
    );
    let item = &listed(&data, &keys)[0];
    assert_eq!(item["fields"], json!(["member_no", "password", "recovery"]));
    assert_eq!(item["plain_values"], json!({"member_no": "M-1"}));

    // Edit: a new plain field by --set asks nothing; marking a secret one plain asks a person.
    cli(
        &data,
        &keys,
        &[
            "vault",
            "edit",
            "gh",
            "--plain",
            "branch",
            "--set",
            "branch=Gangnam",
        ],
        "",
    );
    let expose = ["vault", "edit", "gh", "--plain", "recovery"];
    assert!(refused(&data, &keys, &expose, "").contains("needs confirmation"));
    cli(
        &data,
        &keys,
        &[
            "vault",
            "edit",
            "gh",
            "--unset",
            "branch",
            "--secret",
            "member_no",
        ],
        "",
    );
    let again = ["vault", "edit", "gh", "--set", "member_no=M-2"];
    assert!(refused(&data, &keys, &again, "").contains("field member_no is secret"));
    let item = &listed(&data, &keys)[0];
    assert_eq!(item["fields"], json!(["member_no", "password", "recovery"]));
    assert_eq!(item["plain_values"], json!({}));

    // A field added at a hidden prompt.
    let typed = on_terminal_answering(
        &data,
        &keys,
        &["vault", "edit", "gh", "--field", "pin"],
        &[("pin (empty to skip): ", "pin-SECRET-0000")],
    );
    assert!(typed.success, "{}", typed.output);
    assert!(!typed.output.contains("SECRET"), "{}", typed.output);
    assert!(
        listed(&data, &keys)[0]["fields"]
            .to_string()
            .contains("pin")
    );
}

#[test]
fn agents_put_and_request_named_fields_and_see_only_placeholders() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    cli(&data, &keys, &["vault", "init"], "");
    let mut mcp = Mcp::start(&data, tmp.path(), &[]);
    let put = mcp.ok(
        "vault_put",
        json!({"name": "portal", "kind": "credential", "origins": ["https://portal.example"],
               "fields": {"student_no": "S-SECRET-2026", "pin": "pin-SECRET-4321"}}),
    );
    assert!(!put.to_string().contains("SECRET"), "{put}");
    let placeholders = put["placeholders"].to_string();
    assert!(placeholders.contains("portal.student_no"), "{placeholders}");
    let seen = mcp.ok("vault_list", json!({}));
    assert!(!seen.to_string().contains("SECRET"), "{seen}");
    assert_eq!(
        seen[0]["plain_values"],
        json!({}),
        "an agent's fields are secret"
    );

    let asked = mcp.ok(
        "vault_request",
        json!({"kind": "credential", "name": "bank", "reason": "second password for transfers"}),
    );
    let id = asked["request_id"].as_str().unwrap().to_owned();
    mcp.finish();
    cli(
        &data,
        &keys,
        &["vault", "fulfill", &id, "--stdin"],
        &format!(r#"{{"second_password":"{SECOND}"}}"#),
    );
    let bank = listed(&data, &keys)
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "bank")
        .cloned()
        .unwrap();
    assert_eq!(bank["kind"], "credential");
    assert_eq!(bank["fields"], json!(["second_password"]));
}
