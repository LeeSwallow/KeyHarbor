//! `keyharbor run`: env items go to the child only, its output comes back masked.
#![cfg(unix)]

use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Cli {
    tmp: tempfile::TempDir,
    data: PathBuf,
    transcript: std::cell::RefCell<String>,
}
impl Cli {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let c = Self {
            tmp,
            data,
            transcript: Default::default(),
        };
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
        let out = child.wait_with_output().unwrap();
        let mut t = self.transcript.borrow_mut();
        t.push_str(&String::from_utf8_lossy(&out.stdout));
        t.push_str(&String::from_utf8_lossy(&out.stderr));
        out
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
    fn events(&self) -> Vec<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(&self.ok(&["audit", "--json"], None))
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    }
}
fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}
fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn env_items_reach_the_child_and_its_output_is_masked() {
    let c = Cli::new();
    let (key, url) = (c.synth("key"), c.synth("db url&x"));
    c.ok(
        &["vault", "add", "app", "--kind", "env", "--stdin"],
        Some(&serde_json::json!({"API_KEY": key, "DB_URL": url}).to_string()),
    );

    // Review focus 4: the child prints the variable on both streams and exits 3.
    let out = c.run(
        &[
            "run", "--env", "app", "--", "sh", "-c",
            "echo \"key=$API_KEY\"; echo \"$API_KEY\" >&2; printf 'no newline %s' \"$DB_URL\"; exit 3",
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "key={{secret:app.API_KEY}}\nno newline {{secret:app.DB_URL}}"
    );
    assert!(
        stderr(&out).contains("{{secret:app.API_KEY}}\n"),
        "{}",
        stderr(&out)
    );

    // A template names variables with placeholders; plain values pass through.
    std::fs::write(
        c.tmp.path().join(".env.keyharbor"),
        "# committed, no secrets\nTOKEN={{secret:app.DB_URL}}\nexport PLAIN=hello\nBOTH=\"{{secret:app.API_KEY}}:{{secret:app.DB_URL}}\"\n",
    )
    .unwrap();
    let out = c.run(
        &[
            "run", "--env-file", ".env.keyharbor", "--", "sh", "-c",
            "echo \"$TOKEN $PLAIN\"; [ \"$BOTH\" = \"$API_KEY:$TOKEN\" ] || echo \"$BOTH\"; [ -z \"$API_KEY\" ] && echo only-listed",
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "{{secret:app.DB_URL}} hello\n{{secret:app.API_KEY}}:{{secret:app.DB_URL}}\nonly-listed\n"
    );

    // Core dumps are off for the child; a signal is reported as 128 + its number.
    let out = c.run(
        &["run", "--env", "app", "--", "sh", "-c", "ulimit -c"],
        None,
    );
    assert_eq!(stdout(&out), "0\n");
    let out = c.run(
        &["run", "--env", "app", "--", "sh", "-c", "kill -9 $$"],
        None,
    );
    assert_eq!(out.status.code(), Some(137));
    let out = c.run(&["run", "--env", "app", "--", "/no/such/program"], None);
    assert!(!out.status.success());

    let injected = c
        .events()
        .into_iter()
        .filter(|e| e["kind"] == "env_injected")
        .count();
    assert_eq!(injected, 4);
    assert!(!c.transcript.borrow().contains(&key));
    assert!(!c.transcript.borrow().contains(&url));
    assert!(!c.transcript.borrow().contains(&url.replace(' ', "%20")));
}

#[test]
fn refused_items_never_start_the_child() {
    let c = Cli::new();
    let key = c.synth("key");
    c.ok(
        &["vault", "add", "app", "--kind", "env", "--stdin"],
        Some(&serde_json::json!({"API_KEY": key}).to_string()),
    );
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
        Some(&serde_json::json!({"password": c.synth("pw")}).to_string()),
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
            "app",
        ],
        None,
    );
    let marker = c.tmp.path().join("ran");
    let touch = format!("touch {}", marker.display());
    let refused = |args: &[&str], reason: &str| {
        let mut full = vec!["run"];
        full.extend_from_slice(args);
        full.extend_from_slice(&["--", "sh", "-c", &touch]);
        let out = c.run(&full, None);
        assert!(!out.status.success(), "{args:?}");
        assert!(stderr(&out).contains(reason), "{args:?}: {}", stderr(&out));
        assert!(!marker.exists(), "{args:?} started the child");
    };
    // Review focus 4: a policy refusal does not run the command.
    refused(
        &["--agent", "codex", "--env", "app"],
        "denied: app (policy)",
    );
    refused(&["--env", "nope"], "denied: nope (not_found)");
    refused(&["--env", "github"], "not an env item");
    std::fs::write(
        c.tmp.path().join("t.env"),
        "PW={{secret:github.password}}\n",
    )
    .unwrap();
    refused(&["--env-file", "t.env"], "denied: github (origin)");
    std::fs::write(c.tmp.path().join("u.env"), "K={{secret:app.MISSING}}\n").unwrap();
    refused(&["--env-file", "u.env"], "denied: app (not_found)");
    std::fs::write(c.tmp.path().join("v.env"), "API_KEY=x\n").unwrap();
    refused(
        &["--env", "app", "--env-file", "v.env"],
        "API_KEY given twice",
    );
    refused(&["--agent", "*", "--env", "app"], "reserved");
    assert!(!c.run(&["run", "--", "true"], None).status.success());

    // Another agent is unaffected by codex's rule.
    c.ok(
        &[
            "run", "--agent", "claude", "--env", "app", "--", "sh", "-c", &touch,
        ],
        None,
    );
    assert!(marker.exists());
    let events = c.events();
    assert!(events.iter().any(|e| e["kind"] == "secret_denied"
        && e["agent"] == "codex"
        && e["item_name"] == "app"
        && e["result"] == "policy"));
    assert!(
        events.iter().any(|e| e["kind"] == "env_injected"
            && e["agent"] == "claude"
            && e["item_name"] == "app")
    );
    assert!(!c.transcript.borrow().contains(&key));
}
