//! `keyharbor audit` and `keyharbor settings`: the value-free usage record.
#![cfg(unix)]

mod common;

use common::{FAKE_BROWSER, short_tempdir};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Cli {
    tmp: tempfile::TempDir,
    data: PathBuf,
}
impl Cli {
    fn new() -> Self {
        let tmp = short_tempdir();
        let data = tmp.path().join("data");
        Self { tmp, data }
    }
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", self.tmp.path().join("keys"))
            .env("TEST_DIR", self.tmp.path())
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
    fn events(&self) -> Vec<Value> {
        let json: Value =
            serde_json::from_str(&self.ok(&["audit", "--json"], None)).expect("audit --json");
        json.as_array().unwrap().clone()
    }
    fn kinds(&self) -> Vec<String> {
        self.events()
            .iter()
            .map(|e| e["kind"].as_str().unwrap().to_owned())
            .collect()
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn vault_and_policy_changes_are_recorded_without_values() {
    let c = Cli::new();
    let secret = format!(
        "KH-SYNTH-{}",
        c.tmp.path().file_name().unwrap().to_string_lossy()
    );
    c.ok(&["vault", "init"], None);
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
        Some(&format!(r#"{{"password":"{secret}"}}"#)),
    );
    c.ok(
        &["vault", "edit", "github", "--stdin"],
        Some(&format!(r#"{{"password":"{secret}-2"}}"#)),
    );
    c.ok(&["vault", "rm", "github"], None);
    c.ok(
        &[
            "policy",
            "deny",
            "--agent",
            "codex",
            "--resource",
            "domain",
            "--target",
            "payment",
        ],
        None,
    );
    c.ok(
        &[
            "policy",
            "rm",
            "--agent",
            "codex",
            "--resource",
            "domain",
            "--target",
            "payment",
        ],
        None,
    );
    let events = c.events();
    let summary: Vec<(String, String, String)> = events
        .iter()
        .map(|e| {
            (
                e["kind"].as_str().unwrap().to_owned(),
                e["item_name"].as_str().unwrap_or("").to_owned(),
                e["result"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("vault_item_changed".into(), "github".into(), "added".into()),
            (
                "vault_item_changed".into(),
                "github".into(),
                "updated".into()
            ),
            (
                "vault_item_changed".into(),
                "github".into(),
                "removed".into()
            ),
            (
                "policy_changed".into(),
                "".into(),
                "deny codex domain payment".into()
            ),
            (
                "policy_changed".into(),
                "".into(),
                "removed codex domain payment".into()
            ),
        ]
    );
    assert!(events.iter().all(|e| e["agent"] == "cli"));
    assert!(events[0]["seq"].as_i64().unwrap() >= 1);
    assert!(events[0]["at"].as_str().unwrap().ends_with('Z'));
    let text = c.ok(&["audit"], None);
    assert_eq!(text.lines().count(), 5, "{text}");
    assert!(text.contains("\tvault_item_changed\t"), "{text}");
    assert_eq!(c.ok(&["audit", "--limit", "2"], None).lines().count(), 2);
    assert!(!text.contains(&secret));

    // Turning the record off keeps everything out, denials included (J-23).
    assert_eq!(c.ok(&["settings", "get", "audit_enabled"], None), "true\n");
    c.ok(&["settings", "set", "audit_enabled", "false"], None);
    assert_eq!(c.ok(&["settings", "get", "audit_enabled"], None), "false\n");
    c.ok(
        &["vault", "add", "api", "--kind", "secret", "--stdin"],
        Some(&format!(r#"{{"value":"{secret}-api"}}"#)),
    );
    c.ok(
        &["policy", "deny", "--agent", "*", "--resource", "raw_cdp"],
        None,
    );
    assert_eq!(c.events().len(), 5);
    c.ok(&["settings", "set", "audit_enabled", "true"], None);
    c.ok(&["vault", "rm", "api"], None);
    assert_eq!(c.events().len(), 6);
    assert!(
        !c.run(&["settings", "set", "audit_enabled", "maybe"], None)
            .status
            .success()
    );
    assert!(
        !c.run(&["settings", "set", "color", "blue"], None)
            .status
            .success()
    );

    for file in ["keyharbor.sqlite3", "keyharbor.sqlite3-wal"] {
        if let Ok(bytes) = fs::read(c.data.join(file)) {
            assert!(!contains(&bytes, secret.as_bytes()), "{file} leaks");
        }
    }
}

#[test]
fn session_lifecycle_is_recorded() {
    let c = Cli::new();
    let browser = c.tmp.path().join("browser");
    fs::write(&browser, FAKE_BROWSER).unwrap();
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
    let id = c
        .ok(&["create", "work"], None)
        .split('\t')
        .next()
        .unwrap()
        .to_owned();
    c.ok(&["open", &id, "--browser", browser.to_str().unwrap()], None);
    let request = c
        .ok(&["wait-auth", &id, "--origin", "https://example.com"], None)
        .trim_end()
        .rsplit('\t')
        .next()
        .unwrap()
        .to_owned();
    c.ok(&["resume", &id, &request], None);
    c.ok(&["stop", &id], None);
    assert_eq!(
        c.kinds(),
        [
            "session_opened",
            "handoff_requested",
            "handoff_resumed",
            "session_stopped"
        ]
    );
    let events = c.events();
    assert!(events.iter().all(|e| e["session_id"] == id.as_str()));
    assert_eq!(events[1]["origin"], "https://example.com");

    // A browser that exits on its own is recorded by the supervisor.
    fs::remove_file(c.tmp.path().join("ended")).ok();
    c.ok(&["open", &id, "--browser", browser.to_str().unwrap()], None);
    fs::write(c.tmp.path().join("close"), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while c.kinds().len() < 6 {
        assert!(Instant::now() < deadline, "{:?}", c.kinds());
        thread::sleep(Duration::from_millis(50));
    }
    let last = c.events().pop().unwrap();
    assert_eq!(last["kind"], "session_stopped");
    assert_eq!(last["agent"], "browser");
    assert_eq!(last["result"], "stopped");
}
