//! Browser operations against real Chrome. Runs only when KEYHARBOR_TEST_CHROME names a
//! Chrome executable; otherwise each test returns early (and says so on stderr).
#![cfg(unix)]

use keyharbor_broker::client;
use keyharbor_domain::SessionId;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
};

pub fn chrome() -> Option<PathBuf> {
    match std::env::var_os("KEYHARBOR_TEST_CHROME") {
        Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => {
            eprintln!("skipped: set KEYHARBOR_TEST_CHROME to a Chrome executable to run");
            None
        }
    }
}

/// A tiny HTTP server: `/` is a login form, `/done?user=…` the page after it.
pub fn serve_site() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = String::new();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                    break;
                }
            }
            let path = request.split(' ').nth(1).unwrap_or("/").to_owned();
            let body = if let Some(query) = path.strip_prefix("/done?") {
                let user = query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("user="))
                    .unwrap_or("");
                format!("<!doctype html><title>Signed in</title><h1 id=done>hello {user}</h1>")
            } else if path == "/" {
                "<!doctype html><title>Login</title><form action=/done method=get>\
                 <label>User <input id=user name=user></label>\
                 <label>Password <input id=pw name=pw type=password></label>\
                 <button id=go>로그인</button></form>"
                    .to_owned()
            } else {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                );
                continue;
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    base
}

pub struct Session {
    pub tmp: tempfile::TempDir,
    pub data: PathBuf,
    pub id: String,
}
impl Session {
    pub fn open(browser: &Path, extra: &[&str]) -> Self {
        let tmp = tempfile::Builder::new()
            .prefix("kh")
            .tempdir_in("/tmp")
            .unwrap();
        let data = tmp.path().join("data");
        let mut session = Self {
            tmp,
            data,
            id: String::new(),
        };
        let created = session.ok(&["create", "chrome"]);
        session.id = created.split('\t').next().unwrap().to_owned();
        let id = session.id.clone();
        let browser = browser.to_str().unwrap();
        let mut args = vec!["open", id.as_str(), "--browser", browser];
        args.extend_from_slice(extra);
        session.ok(&args);
        session
    }
    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .env("KEYHARBOR_BROWSER_HEADLESS", "1")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
    pub fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    pub fn call(&self, request: Value) -> anyhow::Result<Value> {
        client::call(&self.data, &SessionId::parse(&self.id).unwrap(), &request)
    }
    pub fn ok_call(&self, request: Value) -> Value {
        let op = request["op"].clone();
        self.call(request).unwrap_or_else(|e| panic!("{op}: {e:#}"))
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.run(&["stop", &self.id]);
    }
}

fn reference(snapshot: &str, needle: &str) -> u64 {
    let line = snapshot
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{needle} not in snapshot:\n{snapshot}"));
    let start = line.find('[').unwrap() + 1;
    let end = line.find(']').unwrap();
    line[start..end].parse().unwrap()
}

#[test]
fn operations_drive_a_real_browser() {
    let Some(browser) = chrome() else { return };
    let base = serve_site();
    let session = Session::open(&browser, &[]);

    let page = session.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));
    assert_eq!(page["title"], "Login", "{page}");
    assert_eq!(page["url"], format!("{base}/"));

    let snapshot = session.ok_call(json!({"op": "snapshot"}));
    let text = snapshot["text"].as_str().unwrap();
    assert!(text.contains("textbox"), "{text}");
    assert!(text.contains("button \"로그인\""), "{text}");
    assert_eq!(snapshot["title"], "Login");
    let button = reference(text, "button \"로그인\"");

    session.ok_call(json!({"op": "fill", "target": {"selector": "#user"}, "text": "alice"}));
    session.ok_call(json!({"op": "fill", "target": {"selector": "#pw"}, "text": "not-a-real-pw"}));
    session.ok_call(json!({"op": "click", "target": {"ref": button}}));
    session.ok_call(json!({"op": "wait", "selector": "#done"}));
    let after = session.ok_call(json!({"op": "snapshot"}));
    assert_eq!(after["title"], "Signed in", "{after}");
    assert!(after["text"].as_str().unwrap().contains("hello alice"));

    // fill replaces the old value; type and press work on the focused element.
    session.ok_call(json!({"op": "navigate", "url": format!("{base}/")}));
    session.ok_call(json!({"op": "fill", "target": {"selector": "#user"}, "text": "zzz"}));
    session.ok_call(json!({"op": "fill", "target": {"selector": "#user"}, "text": "bo"}));
    session.ok_call(json!({"op": "type", "text": "b"}));
    session.ok_call(json!({"op": "press", "key": "Enter"}));
    session.ok_call(json!({"op": "wait", "selector": "#done"}));
    let after = session.ok_call(json!({"op": "snapshot"}));
    assert!(
        after["text"].as_str().unwrap().contains("hello bob"),
        "{after}"
    );
    assert!(
        session
            .call(json!({"op": "press", "key": "NotAKey"}))
            .is_err()
    );

    let tabs = session.ok_call(json!({"op": "tabs"}));
    let tabs = tabs.as_array().unwrap();
    assert_eq!(tabs.len(), 1, "{tabs:?}");
    assert_eq!(tabs[0]["active"], true);
    let first = tabs[0]["id"].as_str().unwrap().to_owned();
    let created = session.ok_call(json!({"op": "new_tab", "url": format!("{base}/")}));
    let second = created["id"].as_str().unwrap().to_owned();
    let tabs = session.ok_call(json!({"op": "tabs"}));
    assert_eq!(tabs.as_array().unwrap().len(), 2);
    assert!(
        tabs.as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == second && t["active"] == true)
    );
    session.ok_call(json!({"op": "wait", "selector": "#user"}));
    assert_eq!(session.ok_call(json!({"op": "snapshot"}))["title"], "Login");
    session.ok_call(json!({"op": "switch_tab", "id": first}));
    let tabs = session.ok_call(json!({"op": "tabs"}));
    assert!(
        tabs.as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == first && t["active"] == true)
    );
    assert_eq!(
        session.ok_call(json!({"op": "snapshot"}))["title"],
        "Signed in"
    );
    assert!(
        session
            .call(json!({"op": "switch_tab", "id": "nope"}))
            .is_err()
    );

    let shot = session.ok_call(json!({"op": "screenshot"}));
    let path = PathBuf::from(shot["path"].as_str().unwrap());
    assert!(path.starts_with(session.data.join("run").join("shots")));
    assert_eq!(&fs::read(&path).unwrap()[..4], b"\x89PNG");
    session.ok_call(json!({"op": "wait", "ms": 10}));

    // Failures are answered; the supervisor stays up.
    assert!(
        session
            .call(json!({"op": "click", "target": {"ref": 987654321}}))
            .is_err()
    );
    assert!(
        session
            .call(json!({"op": "click", "target": {"selector": "#missing"}}))
            .is_err()
    );
    assert!(
        session
            .call(json!({"op": "navigate", "url": "http://127.0.0.1:9/"}))
            .is_err()
    );
    assert!(
        session
            .call(json!({"op": "navigate", "url": "file:///etc/hosts"}))
            .is_err()
    );
    assert!(session.call(json!({"op": "fly"})).is_err());
    let status = session.ok_call(json!({"op": "status"}));
    assert_eq!(status["state"], "running", "{status}");

    assert_eq!(
        session.ok(&["stop", &session.id]),
        format!("{}\tstopped\n", session.id)
    );
    let _ = session.tmp.path();
}
