//! Path-like names and tags (developer secrets spec §2): set, change, filter, back up.
#![cfg(unix)]

mod common;

use common::{mcp::Mcp, short_tempdir};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn cli(data: &Path, keys: &Path, args: &[&str], stdin: &str) -> String {
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
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn names(listing: &Value) -> Vec<&str> {
    listing
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect()
}

#[test]
fn items_are_named_by_path_tagged_filtered_and_backed_up_with_their_tags() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    cli(&data, &keys, &["vault", "init"], "");
    let add = |name: &str, tags: &[&str]| {
        let mut args = vec!["vault", "add", name, "--kind", "secret", "--stdin"];
        for tag in tags {
            args.extend(["--tag", tag]);
        }
        cli(&data, &keys, &args, r#"{"value":"v-SECRET-1234"}"#);
    };
    add("myapp/prod/db", &["prod", "db"]);
    add("myapp/dev/db", &["dev", "db"]);
    add("other", &[]);

    let list = |extra: &[&str]| -> Value {
        let mut args = vec!["vault", "list", "--json"];
        args.extend(extra);
        serde_json::from_str(&cli(&data, &keys, &args, "")).unwrap()
    };
    assert_eq!(
        names(&list(&["--prefix", "myapp/"])),
        ["myapp/dev/db", "myapp/prod/db"]
    );
    assert_eq!(names(&list(&["--tag", "prod"])), ["myapp/prod/db"]);
    assert_eq!(
        names(&list(&["--prefix", "myapp/", "--tag", "dev"])),
        ["myapp/dev/db"]
    );
    let text = cli(
        &data,
        &keys,
        &["vault", "list", "--prefix", "myapp/prod"],
        "",
    );
    assert!(text.trim_end().ends_with("\ttags db,prod"), "{text}");

    cli(
        &data,
        &keys,
        &[
            "vault",
            "edit",
            "myapp/prod/db",
            "--untag",
            "db",
            "--tag",
            "team-a",
        ],
        "",
    );
    let prod = &list(&["--prefix", "myapp/prod"])[0];
    assert_eq!(prod["tags"], json!(["prod", "team-a"]));
    assert_eq!(prod["tampered"], false);

    let mut mcp = Mcp::start(&data, tmp.path(), &[]);
    let from_agent = mcp.ok("vault_list", json!({"tag": "team-a"}));
    assert_eq!(names(&from_agent), ["myapp/prod/db"]);
    assert_eq!(
        from_agent[0]["placeholders"],
        json!(["{{secret:myapp/prod/db.value}}"])
    );
    assert_eq!(
        names(&mcp.ok("vault_list", json!({"prefix": "myapp/dev"}))),
        ["myapp/dev/db"]
    );
    mcp.err("vault_list", json!({"tag": "Not A Tag"}));
    mcp.finish();

    let export = tmp.path().join("out.age");
    cli(
        &data,
        &keys,
        &[
            "vault",
            "export",
            export.to_str().unwrap(),
            "--passphrase-stdin",
        ],
        "pass phrase\n",
    );
    let (other, other_keys) = (tmp.path().join("other"), tmp.path().join("other-keys"));
    cli(&other, &other_keys, &["vault", "init"], "");
    cli(
        &other,
        &other_keys,
        &[
            "vault",
            "import",
            export.to_str().unwrap(),
            "--passphrase-stdin",
        ],
        "pass phrase\n",
    );
    let imported: Value = serde_json::from_str(&cli(
        &other,
        &other_keys,
        &["vault", "list", "--json", "--tag", "team-a"],
        "",
    ))
    .unwrap();
    assert_eq!(names(&imported), ["myapp/prod/db"]);

    let bad = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args([
            "vault", "add", "x", "--kind", "secret", "--tag", "Bad Tag", "--stdin",
        ])
        .env("KEYHARBOR_TEST_KEYSTORE_DIR", &keys)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!bad.status.success());
}

/// Developer secrets spec §2: one previous value per field, swapped back by `vault revert`.
#[test]
fn a_changed_value_can_be_reverted_without_showing_it() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    cli(&data, &keys, &["vault", "init"], "");
    cli(
        &data,
        &keys,
        &["vault", "add", "api", "--kind", "secret", "--stdin"],
        r#"{"value":"value-ONE-1234"}"#,
    );
    cli(
        &data,
        &keys,
        &["vault", "edit", "api", "--stdin"],
        r#"{"value":"value-TWO-5678"}"#,
    );
    let which = || {
        cli(
            &data,
            &keys,
            &[
                "run",
                "--file",
                "F=api.value",
                "--",
                "sh",
                "-c",
                "if test \"$(cat \"$F\")\" = \"$1\"; then echo one; else echo two; fi",
                "_",
                "value-ONE-1234",
            ],
            "",
        )
    };
    assert_eq!(which(), "two\n");
    let reverted = cli(&data, &keys, &["vault", "revert", "api"], "");
    assert_eq!(reverted, "reverted\tapi\tvalue\n");
    assert_eq!(which(), "one\n");
    let listed = cli(&data, &keys, &["vault", "list", "--json"], "");
    assert!(
        !listed.contains("value-") && listed.contains("\"tampered\":false"),
        "{listed}"
    );
}

/// Developer secrets spec §2: a field marked not secret shows in listings and is not masked;
/// marking a secret field plain needs a person at a terminal.
#[test]
fn plain_fields_show_their_values_and_marking_one_needs_a_person() {
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    cli(&data, &keys, &["vault", "init"], "");
    cli(
        &data,
        &keys,
        &[
            "vault", "add", "app", "--kind", "env", "--plain", "PORT", "--stdin",
        ],
        r#"{"API_KEY":"key-SECRET-1234","PORT":"8081"}"#,
    );
    let listed: Value =
        serde_json::from_str(&cli(&data, &keys, &["vault", "list", "--json"], "")).unwrap();
    assert_eq!(listed[0]["plain_values"], json!({"PORT": "8081"}));
    assert!(!listed.to_string().contains("key-SECRET"));
    let out = cli(
        &data,
        &keys,
        &[
            "run",
            "--env",
            "app",
            "--",
            "sh",
            "-c",
            "echo $PORT $API_KEY",
        ],
        "",
    );
    assert_eq!(out, "8081 {{secret:app.API_KEY}}\n");

    let refused = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args(["vault", "edit", "app", "--plain", "API_KEY"])
        .env("KEYHARBOR_TEST_KEYSTORE_DIR", &keys)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("needs confirmation"));
    let confirmed = common::on_terminal(
        &data,
        &keys,
        &["vault", "edit", "app", "--plain", "API_KEY"],
        "yes",
    );
    assert!(confirmed.success, "{}", confirmed.output);
    let mut mcp = Mcp::start(&data, tmp.path(), &[]);
    let seen = mcp.ok("vault_list", json!({}));
    assert_eq!(seen[0]["plain_values"]["API_KEY"], "key-SECRET-1234");
    mcp.finish();
    cli(
        &data,
        &keys,
        &["vault", "edit", "app", "--secret", "API_KEY"],
        "",
    );
    let listed: Value =
        serde_json::from_str(&cli(&data, &keys, &["vault", "list", "--json"], "")).unwrap();
    assert_eq!(listed[0]["plain_values"], json!({"PORT": "8081"}));
}
