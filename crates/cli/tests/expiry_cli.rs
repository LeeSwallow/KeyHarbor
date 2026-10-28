//! Item expiry (R-37): never by default, refused after the last day, values kept, and only a
//! person may extend or remove it.
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
    fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args, None);
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
    fn add_env(&self, name: &str, extra: &[&str]) {
        let mut args = vec!["vault", "add", name, "--kind", "env", "--stdin"];
        args.extend_from_slice(extra);
        self.ok(
            &args,
            Some(&json!({"API_KEY": format!("KH-SYNTH-{name}")}).to_string()),
        );
    }
}

/// The binary uses the system clock, so the test reads it too.
fn days_from_today(days: u32) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    keyharbor_domain::date::Date::from_unix_seconds(now)
        .plus_days(days)
        .to_string()
}

#[test]
fn items_never_expire_by_default_and_are_refused_after_their_last_day() {
    let c = Cli::new();
    c.add_env("forever", &[]);
    assert_eq!(c.item("forever")["expires_at"], Value::Null);
    assert_eq!(c.item("forever")["expired"], false);
    c.add_env("soon", &["--expires-in", "30d"]);
    assert_eq!(c.item("soon")["expires_at"], days_from_today(30));
    let line = c.ok(&["vault", "list"], None);
    assert!(
        line.contains(&format!("\texpires {}", days_from_today(30))),
        "{line}"
    );
    c.ok(&["run", "--env", "soon", "--", "true"], None);

    // An expiry in the past may be set (shortening never asks) and stops every use.
    c.ok(&["vault", "edit", "soon", "--expires", "2020-01-31"], None);
    assert_eq!(c.item("soon")["expired"], true);
    assert!(c.ok(&["vault", "list"], None).contains("\texpired"));
    let error = c.fails(&["run", "--env", "soon", "--", "true"]);
    assert!(error.contains("denied: soon (expired)"), "{error}");
    let audit = c.ok(&["audit", "--json"], None);
    assert!(audit.contains("\"result\":\"expired\""), "{audit}");

    // Extending or removing it needs a person; the value was kept all along.
    for args in [
        &["vault", "edit", "soon", "--expires-in", "10d"][..],
        &["vault", "edit", "soon", "--no-expiry"],
    ] {
        let error = c.fails(args);
        assert!(
            error.contains("needs confirmation from a person"),
            "{args:?}: {error}"
        );
    }
    assert_eq!(c.item("soon")["expires_at"], "2020-01-31");
}

#[test]
fn a_person_extends_an_expiry_and_new_items_follow_the_default_ttl() {
    let c = Cli::new();
    c.add_env("forever", &[]);
    c.ok(
        &["vault", "edit", "forever", "--expires", "9999-12-31"],
        None,
    );
    c.add_env("soon", &["--expires", "2020-01-31"]);
    let extended = common::on_terminal(
        &c.data,
        &c.tmp.path().join("keys"),
        &["vault", "edit", "soon", "--expires-in", "10d"],
        "yes",
    );
    assert!(extended.success, "{}", extended.output);
    assert!(
        extended
            .output
            .contains("extending the expiry of soon from 2020-01-31"),
        "{}",
        extended.output
    );
    c.ok(&["run", "--env", "soon", "--", "true"], None);

    assert_eq!(
        c.ok(&["settings", "get", "default_item_ttl"], None),
        "none\n"
    );
    c.ok(&["settings", "set", "default_item_ttl", "90d"], None);
    assert!(
        c.run(&["settings", "set", "default_item_ttl", "90"], None)
            .status
            .code()
            != Some(0)
    );
    c.add_env("dated", &[]);
    assert_eq!(c.item("dated")["expires_at"], days_from_today(90));
    c.add_env("undated", &["--no-expiry"]);
    assert_eq!(c.item("undated")["expires_at"], Value::Null);
}
