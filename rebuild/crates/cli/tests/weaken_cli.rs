//! Operations that weaken a protection need a person at a terminal (R-35), and `keyharbor
//! run` injects only items that allow it (R-35, R-36).
#![cfg(unix)]

mod common;

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
    fn synth(&self, tag: &str) -> String {
        format!(
            "KH-SYNTH-{}-{tag}",
            self.tmp.path().file_name().unwrap().to_string_lossy()
        )
    }
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .current_dir(self.tmp.path())
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
    fn refused(&self, args: &[&str]) -> String {
        let out = self.run(args, None);
        assert!(!out.status.success(), "{args:?} was not refused");
        String::from_utf8_lossy(&out.stderr).into_owned()
    }
    fn terminal(&self, args: &[&str], answer: &str) -> common::Terminal {
        common::on_terminal(&self.data, &self.tmp.path().join("keys"), args, answer)
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

const NEEDS_PERSON: &str = "needs confirmation from a person at a terminal (or the admin app)";

#[test]
fn weakening_operations_are_refused_without_a_terminal() {
    let c = Cli::new();
    c.ok(
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
        Some(&json!({"password": c.synth("pw")}).to_string()),
    );
    c.ok(
        &[
            "policy",
            "deny",
            "--agent",
            "codex",
            "--resource",
            "item",
            "--target",
            "github",
        ],
        None,
    );
    let before = c.ok(&["vault", "list", "--json"], None);
    let policies = c.ok(&["policy", "list"], None);
    for args in [
        &["vault", "edit", "github", "--origin-binding", "off"][..],
        &[
            "vault",
            "edit",
            "github",
            "--origin",
            "https://github.com",
            "--origin",
            "https://evil.example",
        ],
        &["vault", "edit", "github", "--run-allowed", "on"],
        &[
            "policy",
            "allow",
            "--agent",
            "codex",
            "--resource",
            "item",
            "--target",
            "github",
        ],
        &["policy", "allow", "--agent", "*", "--resource", "raw_cdp"],
        &[
            "policy",
            "rm",
            "--agent",
            "codex",
            "--resource",
            "item",
            "--target",
            "github",
        ],
        &["settings", "set", "audit_enabled", "false"],
        &["settings", "set", "capture_enabled", "true"],
        &["settings", "set", "capture_enabled", "false"],
    ] {
        let error = c.refused(args);
        assert!(error.contains(NEEDS_PERSON), "{args:?}: {error}");
    }
    // Nothing changed.
    assert_eq!(c.ok(&["vault", "list", "--json"], None), before);
    assert_eq!(c.ok(&["policy", "list"], None), policies);
    assert_eq!(c.ok(&["settings", "get", "audit_enabled"], None), "true\n");
    assert_eq!(
        c.ok(&["settings", "get", "capture_enabled"], None),
        "false\n"
    );

    // Making things stricter never asks.
    c.ok(
        &["vault", "edit", "github", "--origin", "https://github.com"],
        None,
    );
    c.ok(&["vault", "edit", "github", "--run-allowed", "off"], None);
    c.ok(
        &["policy", "deny", "--agent", "*", "--resource", "raw_cdp"],
        None,
    );
    c.ok(&["settings", "set", "audit_enabled", "true"], None);
}

#[test]
fn a_person_at_the_terminal_can_confirm_or_decline() {
    let c = Cli::new();
    c.ok(
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
        Some(&json!({"password": c.synth("pw")}).to_string()),
    );
    let declined = c.terminal(
        &["vault", "edit", "github", "--origin-binding", "off"],
        "no",
    );
    assert!(!declined.success, "{}", declined.output);
    assert!(
        declined.output.contains("not confirmed"),
        "{}",
        declined.output
    );
    assert_eq!(c.item("github")["origin_binding"], true);
    let confirmed = c.terminal(
        &[
            "vault",
            "edit",
            "github",
            "--origin-binding",
            "off",
            "--run-allowed",
            "on",
        ],
        "yes",
    );
    assert!(confirmed.success, "{}", confirmed.output);
    assert!(
        confirmed
            .output
            .contains("turning origin binding off for github; letting keyharbor run inject github"),
        "{}",
        confirmed.output
    );
    assert_eq!(c.item("github")["origin_binding"], false);
    assert_eq!(c.item("github")["run_allowed"], true);
}

#[test]
fn run_injects_only_items_that_allow_it() {
    let c = Cli::new();
    let (key, token, name) = (c.synth("key"), c.synth("token"), c.synth("name"));
    c.ok(
        &["vault", "add", "openai", "--kind", "env", "--stdin"],
        Some(&json!({"OPENAI_API_KEY": key}).to_string()),
    );
    c.ok(
        &["vault", "add", "api", "--kind", "secret", "--stdin"],
        Some(&json!({"value": token}).to_string()),
    );
    c.ok(
        &["vault", "add", "me", "--kind", "identity", "--stdin"],
        Some(&json!({"full_name": name}).to_string()),
    );
    // Defaults (R-36): env and secret items may be injected, the others may not.
    assert_eq!(c.item("openai")["run_allowed"], true);
    assert_eq!(c.item("api")["run_allowed"], true);
    assert_eq!(c.item("me")["run_allowed"], false);
    let out = c.run(
        &[
            "run",
            "--env",
            "openai",
            "--",
            "sh",
            "-c",
            "echo \"$OPENAI_API_KEY\"",
        ],
        None,
    );
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "{{secret:openai.OPENAI_API_KEY}}\n"
    );
    std::fs::write(c.tmp.path().join("api.env"), "TOKEN={{secret:api.value}}\n").unwrap();
    c.ok(&["run", "--env-file", "api.env", "--", "true"], None);

    std::fs::write(
        c.tmp.path().join("me.env"),
        "NAME={{secret:me.full_name}}\n",
    )
    .unwrap();
    let marker = c.tmp.path().join("ran");
    let touch = format!("touch {}", marker.display());
    let error = c.refused(&["run", "--env-file", "me.env", "--", "sh", "-c", &touch]);
    assert!(error.contains("denied: me (run-not-allowed)"), "{error}");
    c.ok(&["vault", "edit", "openai", "--run-allowed", "off"], None);
    let error = c.refused(&["run", "--env", "openai", "--", "sh", "-c", &touch]);
    assert!(
        error.contains("denied: openai (run-not-allowed)"),
        "{error}"
    );
    assert!(!marker.exists());
    let audit = c.ok(&["audit", "--json"], None);
    assert!(audit.contains("\"result\":\"run-not-allowed\""), "{audit}");
    for secret in [&key, &token, &name] {
        assert!(!audit.contains(secret.as_str()));
    }
}
