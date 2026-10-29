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

impl Cli {
    /// Starts `keyharbor run --env app -- sh -c <script>` without waiting for it.
    fn start(&self, script: &str) -> std::process::Child {
        Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&self.data)
            .args(["run", "--env", "app", "--", "sh", "-c", script])
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", self.tmp.path().join("keys"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }
    fn with_key(&self) -> String {
        let key = self.synth("key");
        self.ok(
            &["vault", "add", "app", "--kind", "env", "--stdin"],
            Some(&serde_json::json!({"API_KEY": key}).to_string()),
        );
        key
    }
}

/// Reads `out` on its own thread, chunk by chunk, until it closes.
fn reader(mut out: impl std::io::Read + Send + 'static) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (sender, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 256];
        while let Ok(n) = out.read(&mut buffer) {
            if n == 0 || sender.send(buffer[..n].to_vec()).is_err() {
                return;
            }
        }
    });
    received
}

/// What `chunks` gives until `want` appears, `limit` passes, or the stream closes.
fn read_until(
    chunks: &std::sync::mpsc::Receiver<Vec<u8>>,
    want: &str,
    limit: std::time::Duration,
) -> String {
    let deadline = std::time::Instant::now() + limit;
    let mut taken = Vec::new();
    while !String::from_utf8_lossy(&taken).contains(want) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match chunks.recv_timeout(left) {
            Ok(chunk) => taken.extend(chunk),
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&taken).into_owned()
}

/// J-83: a prompt without a newline shows while the child still runs, and a value written a
/// byte at a time is still masked.
#[test]
fn output_without_a_newline_shows_while_the_child_runs_and_stays_masked() {
    let c = Cli::new();
    let key = c.with_key();
    let mut child = c.start(
        "printf 'Ready> '; printf '%s\\n' \"$API_KEY\" | fold -w1 | while read -r ch; do printf '%s' \"$ch\"; sleep 0.01; done; sleep 30",
    );
    let out = reader(child.stdout.take().unwrap());
    let shown = read_until(
        &out,
        "{{secret:app.API_KEY}}",
        std::time::Duration::from_secs(10),
    );
    assert!(shown.starts_with("Ready> "), "{shown:?}");
    assert!(shown.contains("{{secret:app.API_KEY}}"), "{shown:?}");
    assert!(!shown.contains(&key[..6]), "{shown:?}");
    child.kill().unwrap();
    child.wait().unwrap();
}

/// J-83: SIGTERM to `keyharbor run` reaches the child, which can clean up; its exit code is
/// passed on.
#[test]
fn a_terminate_signal_is_passed_to_the_child() {
    let c = Cli::new();
    c.with_key();
    let mut child =
        c.start("trap 'echo cleaned up; exit 7' TERM; echo started; while :; do sleep 0.1; done");
    let out = reader(child.stdout.take().unwrap());
    assert!(read_until(&out, "started", std::time::Duration::from_secs(10)).contains("started"));
    // SAFETY: kill only sends a signal to the process we started.
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let status = child.wait().unwrap();
    let rest = read_until(&out, "cleaned up", std::time::Duration::from_secs(5));
    assert_eq!(status.code(), Some(7), "{rest:?}");
    assert!(rest.contains("cleaned up"), "{rest:?}");
}

/// Channel ②: a field as a private file, gone after the run; its contents are still masked
/// if the child prints them.
#[test]
fn a_field_can_be_handed_over_as_a_private_file() {
    let c = Cli::new();
    c.with_key();
    let out = c.run(
        &[
            "run", "--file", "CRED=app.API_KEY", "--", "sh", "-c",
            "echo \"$CRED\"; cat \"$CRED\"; echo; mode=$(stat -c %a \"$CRED\" 2>/dev/null || stat -f %Lp \"$CRED\"); echo \"mode=$mode\"; d=$(dirname \"$CRED\"); dmode=$(stat -c %a \"$d\" 2>/dev/null || stat -f %Lp \"$d\"); echo \"dir=$dmode\"",
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    let mut lines = text.lines();
    let path = lines.next().unwrap().to_owned();
    assert_eq!(lines.next(), Some("{{secret:app.API_KEY}}"), "{text}");
    assert_eq!(lines.next(), Some("mode=600"), "{text}");
    assert_eq!(lines.next(), Some("dir=700"), "{text}");
    assert!(!std::path::Path::new(&path).exists(), "{path} left behind");
}

/// Channel ③: a filled copy of a template file, and placeholders in arguments (with a
/// warning that arguments are visible to other processes).
#[test]
fn templates_and_arguments_get_their_placeholders_filled() {
    let c = Cli::new();
    let key = c.with_key();
    let template = c.tmp.path().join("app.yml");
    std::fs::write(&template, "token: {{secret:app.API_KEY}}\n").unwrap();
    let out = c.run(
        &[
            "run",
            "--template",
            &format!("CONF={}", template.display()),
            "--",
            "sh",
            "-c",
            "cat \"$CONF\"; test \"$1\" = \"$2\" && echo same",
            "_",
            "{{secret:app.API_KEY}}",
            &key,
        ],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "token: {{secret:app.API_KEY}}\nsame\n");
    assert!(
        stderr(&out).contains("arguments can be read"),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        std::fs::read_to_string(&template).unwrap(),
        "token: {{secret:app.API_KEY}}\n",
        "the template itself is not changed"
    );
}

/// Every channel goes through the same checks: a login item (not `run_allowed`) is refused
/// whether it is asked for as a file, in a template or in an argument, and nothing starts.
#[test]
fn every_channel_refuses_items_run_may_not_use() {
    let c = Cli::new();
    c.ok(
        &[
            "vault",
            "add",
            "site",
            "--kind",
            "login",
            "--origin",
            "https://example.com",
            "--stdin",
        ],
        Some(r#"{"password":"pw-SECRET-1234"}"#),
    );
    let template = c.tmp.path().join("t.txt");
    std::fs::write(&template, "{{secret:site.password}}").unwrap();
    let marker = c.tmp.path().join("started");
    let touch = format!("touch {}", marker.display());
    for channel in [
        vec!["--file", "P=site.password"],
        vec!["--template", &*format!("T={}", template.display()).leak()],
    ] {
        let mut args = vec!["run"];
        args.extend(channel);
        args.extend(["--", "sh", "-c", &touch]);
        let out = c.run(&args, None);
        assert!(!out.status.success(), "{args:?}");
        assert!(stderr(&out).contains("denied: site"), "{}", stderr(&out));
    }
    let out = c.run(
        &[
            "run",
            "--",
            "sh",
            "-c",
            &touch,
            "_",
            "{{secret:site.password}}",
        ],
        None,
    );
    assert!(stderr(&out).contains("denied: site"), "{}", stderr(&out));
    assert!(!marker.exists());
}
