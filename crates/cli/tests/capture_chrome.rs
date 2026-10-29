//! Capturing a sign-in the user types during a handoff (08 §8.2 ②, phase 4 Task 4), with
//! real Chrome. The test plays the person at the browser over the raw CDP endpoint, whose
//! input is trusted like a person's; the agent goes through MCP.
#![cfg(unix)]

mod common;

use common::{chrome, mcp::Mcp, on_terminal, short_tempdir};
use serde_json::{Value, json};
use std::{
    io::Write,
    net::TcpStream,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

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

/// The person at the browser: a raw CDP connection attached to the first page.
struct Person {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    session: String,
    next: u64,
}

impl Person {
    fn connect(endpoint: &str) -> Self {
        let (socket, _) = tungstenite::connect(endpoint).unwrap();
        let mut person = Self {
            socket,
            session: String::new(),
            next: 0,
        };
        let targets = person.call("Target.getTargets", json!({}), false);
        let page = targets["targetInfos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["type"] == "page")
            .unwrap()["targetId"]
            .clone();
        let attached = person.call(
            "Target.attachToTarget",
            json!({"targetId": page, "flatten": true}),
            false,
        );
        person.session = attached["sessionId"].as_str().unwrap().to_owned();
        person
    }
    fn call(&mut self, method: &str, params: Value, in_page: bool) -> Value {
        self.next += 1;
        let mut request = json!({"id": self.next, "method": method, "params": params});
        if in_page {
            request["sessionId"] = json!(self.session);
        }
        self.socket
            .send(Message::text(request.to_string()))
            .unwrap();
        loop {
            let Message::Text(text) = self.socket.read().unwrap() else {
                continue;
            };
            let reply: Value = serde_json::from_str(&text).unwrap();
            if reply["id"] == self.next {
                assert!(reply.get("error").is_none(), "{method}: {reply}");
                return reply["result"].clone();
            }
        }
    }
    /// Focuses `selector` and types `text` as keyboard input.
    fn type_into(&mut self, selector: &str, text: &str) {
        let focus = format!("document.querySelector('{selector}').focus()");
        self.call("Runtime.evaluate", json!({"expression": focus}), true);
        self.call("Input.insertText", json!({"text": text}), true);
    }
    fn press_enter(&mut self) {
        for kind in ["keyDown", "keyUp"] {
            let key = json!({"type": kind, "key": "Enter", "code": "Enter", "windowsVirtualKeyCode": 13, "text": "\r"});
            self.call("Input.dispatchKeyEvent", key, true);
        }
    }
}

fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(100));
    }
}

fn item<'a>(items: &'a Value, name: &str) -> Option<&'a Value> {
    items.as_array().unwrap().iter().find(|i| i["name"] == name)
}

#[test]
fn a_password_the_user_types_during_a_handoff_becomes_a_draft() {
    let Some(chrome) = chrome() else { return };
    let site = common::serve_pages();
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    cli(&data, &keys, &["vault", "init"], "");
    cli(
        &data,
        &keys,
        &[
            "vault",
            "add",
            "site",
            "--kind",
            "login",
            "--origin",
            &site.base,
            "--set",
            "username=me",
            "--stdin",
        ],
        r#"{"password":"old-password"}"#,
    );
    let enabled = on_terminal(
        &data,
        &keys,
        &["settings", "set", "capture_enabled", "true"],
        "yes",
    );
    assert!(enabled.success, "{}", enabled.output);

    let mut mcp = Mcp::start(
        &data,
        tmp.path(),
        &[
            "--browser",
            chrome.to_str().unwrap(),
            "--agent",
            "claude",
            "--allow-cdp",
        ],
    );
    let id = mcp.ok("session_create", json!({"name": "c"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.ok("session_open", json!({"id": id, "cdp": true}));
    let login_page = json!({"session": id, "url": format!("{}/", site.base)});
    mcp.ok("browser_navigate", login_page.clone());

    // What the agent types is never captured: it is not a handoff.
    mcp.ok(
        "browser_fill",
        json!({"session": id, "selector": "#user", "text": "me"}),
    );
    mcp.ok(
        "browser_fill",
        json!({"session": id, "selector": "#pw", "text": "agent-typed"}),
    );
    mcp.ok("browser_press", json!({"session": id, "key": "Enter"}));
    mcp.ok("browser_wait", json!({"session": id, "selector": "#done"}));
    mcp.ok("browser_navigate", login_page);

    let endpoint = mcp.ok("session_endpoint", json!({"id": id}))["endpoint"]
        .as_str()
        .unwrap()
        .to_owned();
    let request = mcp.ok("session_wait_auth", json!({"id": id}))["request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let refused = mcp.err(
        "browser_fill",
        json!({"session": id, "selector": "#user", "text": "x"}),
    );
    assert!(refused.contains("with the user"), "{refused}");

    let mut person = Person::connect(&endpoint);
    person.type_into("#user", "me");
    person.type_into("#pw", "new-password");
    person.press_enter();
    wait_until(|| {
        site.posted
            .lock()
            .unwrap()
            .iter()
            .any(|(k, v)| k == "pw" && v == "new-password")
    });
    wait_until(|| item(&mcp.ok("vault_list", json!({})), "site~draft-1").is_some());
    let items = mcp.ok("vault_list", json!({}));
    assert_eq!(item(&items, "site~draft-1").unwrap()["source"], "captured");
    assert_eq!(item(&items, "site").unwrap()["source"], "user");
    assert_eq!(items.as_array().unwrap().len(), 2, "{items}");
    assert!(!items.to_string().contains("new-password"));

    cli(&data, &keys, &["resume", &id, &request], "");
    let set = mcp.ok(
        "site_state_set",
        json!({"session": id, "origin": site.base, "state": "logged_in"}),
    );
    assert_eq!(set["confirmed"], "site");
    let items = mcp.ok("vault_list", json!({}));
    assert_eq!(item(&items, "site").unwrap()["source"], "captured");
    let audit = cli(&data, &keys, &["audit", "--json"], "");
    assert!(audit.contains("credential_captured"), "{audit}");
    assert!(!audit.contains("new-password") && !audit.contains("agent-typed"));
    mcp.ok("session_stop", json!({"id": id}));
    mcp.finish();
}
