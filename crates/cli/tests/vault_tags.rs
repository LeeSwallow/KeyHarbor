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
