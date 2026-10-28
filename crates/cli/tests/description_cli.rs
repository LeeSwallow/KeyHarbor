//! Descriptions (R-38): what an item, a field or a session is for, shown to agents; never a
//! vault value.
#![cfg(unix)]

mod common;

use common::mcp::Mcp;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Cli {
    tmp: tempfile::TempDir,
    data: PathBuf,
}
impl Cli {
    fn new() -> Self {
        let tmp = common::short_tempdir();
        let data = tmp.path().join("data");
        let c = Self { tmp, data };
        c.ok(&["vault", "init"], None);
        c
    }
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", self.tmp.path().join("keys"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        drop(child.stdin.take());
        child.wait_with_output().unwrap()
    }
    fn ok(&self, args: &[&str], stdin: Option<&str>) -> String {
        let out = self.run(args, stdin);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn fails(&self, args: &[&str], stdin: Option<&str>) -> String {
        let out = self.run(args, stdin);
        assert!(!out.status.success(), "{args:?} succeeded");
        String::from_utf8_lossy(&out.stderr).into_owned()
    }
    fn item(&self, name: &str) -> Value {
        let list: Value =
            serde_json::from_str(&self.ok(&["vault", "list", "--json"], None)).unwrap();
        list.as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == name)
            .cloned()
            .unwrap()
    }
}

#[test]
fn item_and_field_descriptions_round_trip() {
    let c = Cli::new();
    c.ok(
        &[
            "vault",
            "add",
            "openai",
            "--kind",
            "env",
            "--stdin",
            "--description",
            "OpenAI for the docs bot",
            "--field-description",
            "OPENAI_API_KEY=Project docs-bot key",
        ],
        Some(&json!({"OPENAI_API_KEY": "KH-SYNTH-key", "OPENAI_ORG": "KH-SYNTH-org"}).to_string()),
    );
    let item = c.item("openai");
    assert_eq!(item["description"], "OpenAI for the docs bot");
    assert_eq!(
        item["field_descriptions"]["OPENAI_API_KEY"],
        "Project docs-bot key"
    );
    // A field without its own description shows the kind's.
    assert_eq!(
        item["field_descriptions"]["OPENAI_ORG"],
        "Environment variable"
    );

    c.ok(
        &[
            "vault",
            "edit",
            "openai",
            "--description",
            "OpenAI, staging",
            "--field-description",
            "OPENAI_API_KEY=",
            "--field-description",
            "OPENAI_ORG=Organization id",
        ],
        None,
    );
    let item = c.item("openai");
    assert_eq!(item["description"], "OpenAI, staging");
    assert_eq!(
        item["field_descriptions"]["OPENAI_API_KEY"],
        "Environment variable"
    );
    assert_eq!(item["field_descriptions"]["OPENAI_ORG"], "Organization id");
    c.ok(&["vault", "edit", "openai", "--no-description"], None);
    assert_eq!(c.item("openai")["description"], Value::Null);
    // Removing a field drops its description.
    c.ok(&["vault", "edit", "openai", "--unset", "OPENAI_ORG"], None);
    assert!(
        c.item("openai")["field_descriptions"]
            .get("OPENAI_ORG")
            .is_none()
    );
    assert!(c.item("openai")["tampered"] == false);

    // Too long, a field the item does not have, and control characters are refused.
    let long = "x".repeat(501);
    let error = c.fails(&["vault", "edit", "openai", "--description", &long], None);
    assert!(error.contains("1–500 characters"), "{error}");
    let error = c.fails(
        &["vault", "edit", "openai", "--field-description", "NOPE=x"],
        None,
    );
    assert!(error.contains("not part of the item"), "{error}");
    assert!(
        c.fails(
            &["vault", "edit", "openai", "--description", "a\u{1b}b"],
            None
        )
        .contains("control")
    );
}

#[test]
fn a_description_holding_a_vault_value_is_refused() {
    let c = Cli::new();
    let error = c.fails(
        &[
            "vault",
            "add",
            "api",
            "--kind",
            "secret",
            "--stdin",
            "--description",
            "key KH-SYNTH-own-89",
        ],
        Some(&json!({"value": "KH-SYNTH-own-89"}).to_string()),
    );
    assert!(error.contains("must not contain a vault value"), "{error}");
    assert!(!error.contains("KH-SYNTH-own-89"));
    c.ok(
        &["vault", "add", "api", "--kind", "secret", "--stdin"],
        Some(&json!({"value": "KH-SYNTH-own-89"}).to_string()),
    );
    let error = c.fails(
        &[
            "vault",
            "add",
            "other",
            "--kind",
            "secret",
            "--stdin",
            "--description",
            "same as KH-SYNTH-own-89",
        ],
        Some(&json!({"value": "KH-SYNTH-other-89"}).to_string()),
    );
    assert!(error.contains("must not contain a vault value"), "{error}");
    let error = c.fails(
        &[
            "vault",
            "edit",
            "api",
            "--field-description",
            "value=KH-SYNTH-own-89",
        ],
        None,
    );
    assert!(error.contains("must not contain a vault value"), "{error}");
    assert_eq!(
        c.item("api")["field_descriptions"]["value"],
        "The secret value, e.g. an API key or token"
    );
}

#[test]
fn agents_see_descriptions_of_items_and_sessions() {
    let c = Cli::new();
    c.ok(
        &[
            "vault",
            "add",
            "api",
            "--kind",
            "secret",
            "--stdin",
            "--description",
            "Billing API, read-only",
        ],
        Some(&json!({"value": "KH-SYNTH-api-90"}).to_string()),
    );
    let id = c
        .ok(
            &["create", "work", "--description", "Work Gmail and GitHub"],
            None,
        )
        .split('\t')
        .next()
        .unwrap()
        .to_owned();
    assert!(c.ok(&["list"], None).contains("\tWork Gmail and GitHub"));
    let mut mcp = Mcp::start(&c.data, c.tmp.path(), &["--agent", "claude"]);
    let items = mcp.ok("vault_list", json!({}));
    assert_eq!(items[0]["description"], "Billing API, read-only");
    assert_eq!(
        items[0]["field_descriptions"]["value"],
        "The secret value, e.g. an API key or token"
    );
    let sessions = mcp.ok("session_list", json!({}));
    assert_eq!(sessions[0]["description"], "Work Gmail and GitHub");
    let created = mcp.ok(
        "session_create",
        json!({"name": "agent", "description": "Scratch session"}),
    );
    assert!(created["id"].is_string());
    mcp.finish();
    c.ok(&["edit", &id, "--no-description"], None);
    assert!(!c.ok(&["list"], None).contains("Work Gmail"));
    assert!(c.ok(&["list"], None).contains("\tScratch session"));
}
