//! Placeholder substitution against real Chrome: policy, origin binding per frame, and no
//! plaintext in anything the caller sees. Runs only when KEYHARBOR_TEST_CHROME is set.
#![cfg(unix)]

mod common;

use common::{chrome, serve_pages};
use keyharbor_broker::client;
use keyharbor_domain::SessionId;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

pub struct Harness {
    pub tmp: tempfile::TempDir,
    pub data: PathBuf,
    pub id: String,
    /// Everything the caller saw: CLI output, op results and errors.
    pub transcript: RefCell<String>,
}
impl Harness {
    fn new() -> Self {
        let tmp = common::short_tempdir();
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

#[test]
fn placeholders_fill_only_allowed_items_into_bound_frames() {
    let Some(browser) = chrome() else { return };
    let shop = serve_pages();
    let other = serve_pages();
    let mut h = Harness::new();
    let (user, password) = (h.synth("user"), h.synth("password"));
    let (gh_password, inner_user, note) = (h.synth("gh"), h.synth("inner"), h.synth("note"));
    h.add(
        "site",
        "login",
        &[&shop.base],
        json!({"username": user, "password": password}),
    );
    h.add(
        "gh",
        "login",
        &["https://github.com"],
        json!({"username": "octo", "password": gh_password}),
    );
    h.add(
        "inner",
        "login",
        &[&other.base],
        json!({"username": inner_user}),
    );
    h.add("free", "secret", &[], json!({"value": note}));
    h.open(&browser);

    // The bound origin matches: both fields are typed, the response holds no value.
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    assert_eq!(
        h.fill("#user", "{{secret:site.username}}").unwrap(),
        json!({})
    );
    h.fill("#pw", "{{secret:site.password}}").unwrap();
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    {
        let posted = shop.posted.lock().unwrap();
        assert!(
            posted.contains(&("user".into(), user.clone())),
            "user not typed"
        );
        assert!(
            posted.contains(&("pw".into(), password.clone())),
            "password not typed"
        );
    }

    // Review focus 1: a phishing origin gets nothing and the field stays empty.
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    let error = h.denied(
        json!({"op": "fill", "target": {"selector": "#user"}, "text": "{{secret:gh.password}}"}),
    );
    assert_eq!(error, "denied: gh (origin)");
    let snapshot = h.snapshot();
    let line = snapshot
        .lines()
        .find(|l| l.contains("textbox \"User\""))
        .unwrap();
    assert!(!line.contains("value="), "{line}");
    assert!(
        h.call(json!({"op": "type", "text": "{{secret:gh.password}}"}))
            .is_err()
    );

    // Text around a placeholder is kept; several references resolve together.
    h.fill("#user", "{{secret:site.username}}-{{secret:free.value}}")
        .unwrap();

    // Missing items or fields, malformed references and policy are refused by name.
    assert_eq!(
        h.fill("#user", "{{secret:nope.password}}")
            .unwrap_err()
            .to_string(),
        "denied: nope (not_found)"
    );
    assert_eq!(
        h.fill("#user", "{{secret:site.note}}")
            .unwrap_err()
            .to_string(),
        "denied: site (not_found)"
    );
    assert!(h.fill("#user", "{{secret:site}}").is_err());
    h.ok(
        &[
            "policy",
            "deny",
            "--agent",
            "cli",
            "--resource",
            "item",
            "--target",
            "site",
        ],
        None,
    );
    assert_eq!(
        h.fill("#pw", "{{secret:site.password}}")
            .unwrap_err()
            .to_string(),
        "denied: site (policy)"
    );
    h.ok(
        &[
            "policy",
            "allow",
            "--agent",
            "cli",
            "--resource",
            "item",
            "--target",
            "site",
        ],
        None,
    );
    // An unbound item may go anywhere; `type` checks the focused element's frame.
    h.ok_call(json!({"op": "click", "target": {"selector": "#pw"}}));
    h.ok_call(json!({"op": "type", "text": "{{secret:site.password}}"}));

    // Review focus 2: the top document is allowed, the cross-origin iframe is not.
    h.ok_call(json!({
        "op": "navigate",
        "url": format!("{}/embed?port={}", shop.base, other.base.rsplit(':').next().unwrap()),
    }));
    h.ok_call(json!({"op": "wait", "ms": 300}));
    let snapshot = h.snapshot();
    let framed = reference(&snapshot, "[frame]", "textbox \"User\"");
    let error = h.denied(
        json!({"op": "fill", "target": {"ref": framed}, "text": "{{secret:site.username}}"}),
    );
    assert_eq!(error, "denied: site (origin)");
    h.ok_call(
        json!({"op": "fill", "target": {"ref": framed}, "text": "{{secret:inner.username}}"}),
    );
    h.ok_call(json!({"op": "fill", "target": {"ref": framed}, "text": "{{secret:free.value}}"}));
    h.fill("#top", "{{secret:site.username}}").unwrap();
    assert_eq!(
        h.fill("#top", "{{secret:inner.username}}")
            .unwrap_err()
            .to_string(),
        "denied: inner (origin)"
    );

    // Every use and refusal is in the usage record, without values.
    let audit: Value = serde_json::from_str(&h.ok(&["audit", "--json"], None)).unwrap();
    let events = audit.as_array().unwrap();
    let has = |kind: &str, item: &str, result: &str| {
        events.iter().any(|e| {
            e["kind"] == kind
                && e["item_name"] == item
                && e["result"] == result
                && e["agent"] == "cli"
        })
    };
    assert!(has("secret_used", "site", "ok"));
    assert!(has("secret_used", "inner", "ok"));
    assert!(has("secret_denied", "gh", "origin"));
    assert!(has("secret_denied", "site", "policy"));
    assert!(has("secret_denied", "nope", "not_found"));
    let used = events
        .iter()
        .find(|e| e["kind"] == "secret_used" && e["item_name"] == "inner")
        .unwrap();
    assert_eq!(used["origin"], other.base.as_str());
    assert_eq!(used["session_id"], h.id.as_str());
    h.assert_absent(&[&user, &password, &gh_password, &inner_user, &note]);
}

/// Sixteen digits derived from the test directory name, so every run uses a new number.
fn card_number(seed: &str) -> String {
    let mut n: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.bytes() {
        n = (n ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
    format!("4{:015}", n % 1_000_000_000_000_000)
}

#[test]
fn values_shown_back_by_the_page_are_masked() {
    let Some(browser) = chrome() else { return };
    let shop = serve_pages();
    let mut h = Harness::new();
    let number = card_number(&h.synth("card"));
    let spaced = h.synth("p@ss w&rd");
    let token = h.synth("token");
    h.add(
        "visa",
        "card",
        &[&shop.base],
        json!({"number": number, "cvc": "123", "exp_month": "12"}),
    );
    h.add("api", "secret", &[], json!({"value": spaced}));
    h.add("other", "secret", &[], json!({"value": token}));
    h.open(&browser);

    // Review focus 3: the page shows the typed card number as is and in groups of four.
    h.ok_call(json!({"op": "navigate", "url": format!("{}/", shop.base)}));
    h.fill("#user", "{{secret:visa.number}}").unwrap();
    h.fill("#pw", "{{secret:api.value}}").unwrap();
    let before = h.snapshot();
    assert!(
        before.contains("value=\"{{secret:visa.number}}\""),
        "{before}"
    );
    let password = before
        .lines()
        .find(|l| l.contains("textbox \"Password\""))
        .unwrap();
    assert!(!password.contains("value="), "{password}");
    h.ok_call(json!({"op": "click", "target": {"selector": "#go"}}));
    h.ok_call(json!({"op": "wait", "selector": "#done"}));
    assert!(
        shop.posted
            .lock()
            .unwrap()
            .contains(&("user".into(), number.clone()))
    );
    let after = h.snapshot();
    assert_eq!(
        after.matches("{{secret:visa.number}}").count(),
        2,
        "{after}"
    );
    assert!(!after.contains(&number[..8]), "{after}");

    // Values in URLs, raw or percent-encoded, are masked in every result.
    let encoded: String = spaced
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let page = h.ok_call(json!({
        "op": "navigate",
        "url": format!("{}/missing?a={encoded}&b={token}", shop.base),
    }));
    assert_eq!(
        page["url"],
        format!(
            "{}/missing?a={{{{secret:api.value}}}}&b={{{{secret:other.value}}}}",
            shop.base
        )
    );
    let tabs = h.ok_call(json!({"op": "tabs"}));
    assert!(
        tabs.to_string().contains("{{secret:other.value}}"),
        "{tabs}"
    );

    // A vault change is picked up by the next response without restarting the session.
    let late = h.synth("late");
    h.add("late", "secret", &[], json!({"value": late}));
    let page = h.ok_call(json!({
        "op": "navigate",
        "url": format!("{}/missing?c={late}", shop.base),
    }));
    assert!(
        page["url"]
            .as_str()
            .unwrap()
            .ends_with("c={{secret:late.value}}"),
        "{page}"
    );
    h.assert_absent(&[&number, &spaced, &encoded, &token, &late]);
}
