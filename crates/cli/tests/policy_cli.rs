//! `keyharbor policy …`.

mod common;

use std::process::{Command, Output, Stdio};

fn run(data: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
fn ok(data: &std::path::Path, args: &[&str]) -> String {
    let out = run(data, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
/// A weakening change, confirmed by a person at a terminal.
fn confirmed(data: &std::path::Path, args: &[&str]) {
    let out = common::on_terminal(data, &data.join("keys"), args, "yes");
    assert!(out.success, "{args:?}: {}", out.output);
}

#[test]
fn policy_rules_can_be_added_listed_changed_and_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    assert_eq!(ok(&data, &["policy", "list"]), "");
    assert_eq!(
        ok(
            &data,
            &[
                "policy",
                "deny",
                "--agent",
                "codex",
                "--resource",
                "domain",
                "--target",
                "payment"
            ]
        ),
        "deny\tcodex\tdomain\tpayment\n"
    );
    confirmed(
        &data,
        &[
            "policy",
            "allow",
            "--agent",
            "*",
            "--resource",
            "item",
            "--target",
            "github",
        ],
    );
    ok(
        &data,
        &["policy", "deny", "--agent", "*", "--resource", "raw_cdp"],
    );
    assert_eq!(
        ok(&data, &["policy", "list"]),
        "*\titem\tgithub\tallow\n*\traw_cdp\t*\tdeny\ncodex\tdomain\tpayment\tdeny\n"
    );
    confirmed(
        &data,
        &[
            "policy",
            "allow",
            "--agent",
            "codex",
            "--resource",
            "domain",
            "--target",
            "payment",
        ],
    );
    let json: serde_json::Value =
        serde_json::from_str(&ok(&data, &["policy", "list", "--json"])).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 3);
    assert!(
        json.as_array()
            .unwrap()
            .iter()
            .any(|r| r["agent"] == "codex"
                && r["resource"] == "domain"
                && r["target"] == "payment"
                && r["effect"] == "allow")
    );
    assert_eq!(
        ok(
            &data,
            &[
                "policy",
                "rm",
                "--agent",
                "codex",
                "--resource",
                "domain",
                "--target",
                "payment"
            ]
        ),
        "removed\tcodex\tdomain\tpayment\n"
    );
    for bad in [
        &[
            "policy",
            "rm",
            "--agent",
            "codex",
            "--resource",
            "domain",
            "--target",
            "payment",
        ][..],
        &["policy", "deny", "--agent", "codex", "--resource", "money"],
        &["policy", "deny", "--agent", "a b", "--resource", "item"],
        &[
            "policy",
            "deny",
            "--agent",
            "codex",
            "--resource",
            "session",
            "--target",
            "x",
        ],
        &[
            "policy",
            "deny",
            "--agent",
            "codex",
            "--resource",
            "create_session",
            "--target",
            "x",
        ],
    ] {
        assert!(!run(&data, bad).status.success(), "{bad:?}");
    }
    assert_eq!(ok(&data, &["policy", "list"]).lines().count(), 2);
}
