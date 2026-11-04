//! The real-Chrome test harness: a data directory with a vault, one session's supervisor and
//! a transcript of everything the caller saw.

use keyharbor_domain::SessionId;
use keyharbor_supervisor::control::client;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

/// How long `call_until` waits for a page to settle. Generous: it only bounds a failing test.
const PAGE_SETTLE_LIMIT: Duration = Duration::from_secs(20);
const PAGE_SETTLE_POLL: Duration = Duration::from_millis(100);

pub struct Harness {
    pub tmp: tempfile::TempDir,
    pub data: PathBuf,
    pub id: String,
    /// Everything the caller saw: CLI output, op results and errors.
    pub transcript: RefCell<String>,
}
impl Harness {
    pub fn new() -> Self {
        let tmp = super::short_tempdir();
        let data = tmp.path().join("data");
        let h = Self {
            tmp,
            data,
            id: String::new(),
            transcript: RefCell::default(),
        };
        h.ok(&["vault", "init"], None);
        h
    }
    pub fn synth(&self, tag: &str) -> String {
        format!(
            "KH-SYNTH-{}-{tag}",
            self.tmp.path().file_name().unwrap().to_string_lossy()
        )
    }
    pub fn run(&self, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", self.tmp.path().join("keys"))
            .env("KEYHARBOR_BROWSER_HEADLESS", "1")
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
    pub fn ok(&self, args: &[&str], stdin: Option<&str>) -> String {
        let out = self.run(args, stdin);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    /// A weakening command, confirmed by a person at a terminal.
    pub fn confirmed(&self, args: &[&str]) {
        let out = super::on_terminal(&self.data, &self.tmp.path().join("keys"), args, "yes");
        self.transcript.borrow_mut().push_str(&out.output);
        assert!(out.success, "{args:?}: {}", out.output);
    }
    pub fn add(&self, name: &str, kind: &str, origins: &[&str], fields: Value) {
        let mut args = vec!["vault", "add", name, "--kind", kind, "--stdin"];
        for origin in origins {
            args.push("--origin");
            args.push(origin);
        }
        self.ok(&args, Some(&fields.to_string()));
    }
    pub fn open(&mut self, browser: &Path) {
        self.id = self
            .ok(&["create", "chrome"], None)
            .split('\t')
            .next()
            .unwrap()
            .to_owned();
        let id = self.id.clone();
        self.ok(&["open", &id, "--browser", browser.to_str().unwrap()], None);
    }
    pub fn call(&self, request: Value) -> anyhow::Result<Value> {
        let result = client::call(&self.data, &SessionId::parse(&self.id).unwrap(), &request);
        let mut t = self.transcript.borrow_mut();
        match &result {
            Ok(value) => t.push_str(&value.to_string()),
            Err(error) => t.push_str(&format!("{error:#}")),
        }
        t.push('\n');
        result
    }
    pub fn ok_call(&self, request: Value) -> Value {
        let op = request["op"].clone();
        self.call(request).unwrap_or_else(|e| panic!("{op}: {e:#}"))
    }
    /// Repeats `request` until `ready` accepts its result or `PAGE_SETTLE_LIMIT` passes, and
    /// returns the last result either way, so the caller's assertion reports what it saw.
    /// For waiting on what a page does after a click (a navigation, a frame loading) instead
    /// of sleeping a fixed time, which a loaded CI machine outruns. Every result still goes
    /// into the transcript, so `assert_absent` checks the intermediate ones too.
    pub fn call_until(&self, request: Value, ready: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + PAGE_SETTLE_LIMIT;
        loop {
            let result = self.ok_call(request.clone());
            if ready(&result) || Instant::now() >= deadline {
                return result;
            }
            thread::sleep(PAGE_SETTLE_POLL);
        }
    }
    /// `tabs`, once some tab's URL contains `fragment`.
    pub fn tabs_once_at(&self, fragment: &str) -> Value {
        self.call_until(json!({"op": "tabs"}), |tabs| {
            tabs.as_array().is_some_and(|tabs| {
                tabs.iter().any(|tab| {
                    tab["url"]
                        .as_str()
                        .is_some_and(|url| url.contains(fragment))
                })
            })
        })
    }
    /// The snapshot text, once `ready` accepts it.
    pub fn snapshot_once(&self, ready: impl Fn(&str) -> bool) -> String {
        let result = self.call_until(json!({"op": "snapshot"}), |snapshot| {
            snapshot["text"].as_str().is_some_and(&ready)
        });
        result["text"].as_str().unwrap().to_owned()
    }
    pub fn denied(&self, request: Value) -> String {
        let error = self.call(request).expect_err("should be denied");
        format!("{error:#}")
    }
    pub fn fill(&self, selector: &str, text: &str) -> anyhow::Result<Value> {
        self.call(json!({"op": "fill", "target": {"selector": selector}, "text": text}))
    }
    pub fn snapshot(&self) -> String {
        self.ok_call(json!({"op": "snapshot"}))["text"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    pub fn assert_absent(&self, secrets: &[&str]) {
        let t = self.transcript.borrow();
        for secret in secrets {
            assert!(!t.contains(secret), "plaintext reached the caller:\n{t}");
        }
        for file in ["keyharbor.sqlite3", "keyharbor.sqlite3-wal"] {
            if let Ok(bytes) = std::fs::read(self.data.join(file)) {
                for secret in secrets {
                    assert!(
                        !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                        "{file} holds plaintext"
                    );
                }
            }
        }
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        if !self.id.is_empty() {
            let _ = self.run(&["stop", &self.id.clone()], None);
        }
    }
}

/// The ref of the first snapshot line containing `needle`, after the line containing `after`.
pub fn reference(snapshot: &str, after: &str, needle: &str) -> u64 {
    let line = snapshot
        .lines()
        .skip_while(|line| !line.contains(after))
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{needle} not in snapshot:\n{snapshot}"));
    let start = line.find('[').unwrap() + 1;
    let end = line.find(']').unwrap();
    line[start..end].parse().unwrap()
}
