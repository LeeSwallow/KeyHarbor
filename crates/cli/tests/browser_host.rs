//! The Native Messaging host and its registration (R-42, phase 4 Task 8).
#![cfg(unix)]

mod common;

use common::{chrome, mcp::Mcp, person::Person, short_tempdir};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

const SECRET: &str = "KH-COOKIE-7d1f";

fn cli(data: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// Runs the host as a browser would, with one framed message; returns its reply and stderr.
fn host(data: &Path, message: &[u8]) -> (Value, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args([
            "browser",
            "host",
            "chrome-extension://fimjcgdlhnkdmjnhcccnkpcnmcecgjoc/",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut framed = (message.len() as u32).to_ne_bytes().to_vec();
    framed.extend_from_slice(message);
    child.stdin.take().unwrap().write_all(&framed).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let length = u32::from_ne_bytes(out.stdout[..4].try_into().unwrap()) as usize;
    assert_eq!(
        out.stdout.len(),
        4 + length,
        "stdout carries only the reply"
    );
    let reply = serde_json::from_slice(&out.stdout[4..]).unwrap();
    (reply, String::from_utf8(out.stderr).unwrap())
}

fn receive_site(session: &str, origin: &str, cookie_url: &str) -> Vec<u8> {
    json!({
        "v": 1, "type": "receive_site", "origin": origin, "session": session, "ttl_seconds": 3600,
        "cookies": [{"name": "sid", "value": SECRET, "url": cookie_url, "path": "/",
                     "secure": false, "httpOnly": true, "sameSite": "Lax"}],
    })
    .to_string()
    .into_bytes()
}

fn code(reply: &Value) -> &str {
    reply["code"].as_str().unwrap_or_default()
}

#[test]
fn install_host_registers_only_our_extension() {
    let tmp = short_tempdir();
    let (home, data) = (tmp.path().join("home"), tmp.path().join("data"));
    let out = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args(["browser", "install-host"])
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let written = String::from_utf8(out.stdout).unwrap();
    assert_eq!(written.lines().count(), 3, "{written}");
    for path in written.lines() {
        assert!(path.starts_with(home.to_str().unwrap()), "{path}");
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(manifest["name"], "dev.keyharbor.host");
        assert_eq!(manifest["path"], data.join("native-host").to_str().unwrap());
        let allowed = manifest
            .get("allowed_origins")
            .or(manifest.get("allowed_extensions"));
        assert_eq!(allowed.unwrap().as_array().unwrap().len(), 1, "{manifest}");
    }
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(data.join("native-host"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn the_host_refuses_bad_messages_unknown_or_stopped_sessions_and_other_sites_cookies() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    assert!(cli(&data, &["create", "work"]).status.success());
    let site = "https://github.com";
    let cases: [(&[u8], &str); 5] = [
        (b"not json", "bad_request"),
        (br#"{"v":2,"type":"receive_site"}"#, "bad_request"),
        (
            &receive_site("nobody", site, "https://github.com/"),
            "no_session",
        ),
        (
            &receive_site("work", site, "https://github.com/"),
            "not_running",
        ),
        (
            &receive_site("work", site, "https://evil.example/"),
            "refused",
        ),
    ];
    for (message, expected) in cases {
        let (reply, stderr) = host(&data, message);
        assert_eq!(
            (reply["type"].as_str(), code(&reply)),
            (Some("error"), expected),
            "{reply}"
        );
        assert!(!reply.to_string().contains(SECRET) && !stderr.contains(SECRET));
    }
}

#[test]
fn a_handed_site_arrives_with_its_cookies_and_the_session_stays_on_it() {
    let Some(chrome) = chrome() else { return };
    let site = common::serve_pages();
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let mut mcp = Mcp::start(
        &data,
        tmp.path(),
        &["--browser", chrome.to_str().unwrap(), "--allow-cdp"],
    );
    let id = mcp.ok("session_create", json!({"name": "work"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.ok("session_open", json!({"id": id, "cdp": true}));
    let endpoint = mcp.ok("session_endpoint", json!({"id": id}))["endpoint"]
        .as_str()
        .unwrap()
        .to_owned();

    let (reply, stderr) = host(
        &data,
        &receive_site("work", &site.base, &format!("{}/", site.base)),
    );
    assert_eq!(
        reply,
        json!({"v": 1, "type": "received", "session": "work", "cookies": 1})
    );
    assert!(!stderr.contains(SECRET));

    let mut person = Person::connect(&endpoint);
    let cookies = person.browser("Storage.getCookies", json!({}));
    assert!(
        cookies["cookies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["value"] == SECRET)
    );
    let tabs = mcp.ok("browser_tabs", json!({"session": id}));
    assert!(tabs.to_string().contains(&site.base), "{tabs}");
    let grants = String::from_utf8(cli(&data, &["grant", "list", &id]).stdout).unwrap();
    assert!(
        grants.contains(&format!("\t{}\tactive\t", site.base)) && grants.contains("handoff"),
        "{grants}"
    );
    let refused = mcp.err(
        "browser_navigate",
        json!({"session": id, "url": "https://example.com/"}),
    );
    assert!(refused.contains("out-of-grant"), "{refused}");

    let audit = String::from_utf8(cli(&data, &["audit", "--json"]).stdout).unwrap();
    assert!(audit.contains("site_handed_off"), "{audit}");
    assert!(!audit.contains(SECRET));
    for entry in std::fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            let found = bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes());
            assert!(!found, "{} holds the cookie value", path.display());
        }
    }
    mcp.ok("session_stop", json!({"id": id}));
    mcp.finish();
}

const PASSWORD: &str = "KH-PW-autofill-41";

/// The host with a vault (plaintext test key store) and a test answer for Touch ID.
fn host_with_vault(data: &Path, presence: &str, message: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args([
            "browser",
            "host",
            "chrome-extension://fimjcgdlhnkdmjnhcccnkpcnmcecgjoc/",
        ])
        .env("KEYHARBOR_TEST_KEYSTORE_DIR", data.join("keys"))
        .env("KEYHARBOR_TEST_PRESENCE", presence)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let body = message.to_string().into_bytes();
    let mut framed = (body.len() as u32).to_ne_bytes().to_vec();
    framed.extend_from_slice(&body);
    child.stdin.take().unwrap().write_all(&framed).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains(PASSWORD), "stderr holds no value");
    serde_json::from_slice(&out.stdout[4..]).unwrap()
}

fn vault_with_github(data: &Path) {
    let run = |args: &[&str], stdin: Option<&str>| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", data.join("keys"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        if let Some(text) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        }
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success(), "{args:?}");
    };
    run(&["vault", "init"], None);
    run(
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
        Some(&format!(r#"{{"username":"amy","password":"{PASSWORD}"}}"#)),
    );
}

#[test]
fn autofill_lists_by_name_fills_only_after_touch_id_and_saves_drafts() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    vault_with_github(&data);
    let list = |origin: &str| {
        host_with_vault(
            &data,
            "yes",
            json!({"v": 1, "type": "autofill_list", "origin": origin}),
        )
    };
    assert_eq!(
        list("https://github.com"),
        json!({"v": 1, "type": "logins", "items": ["github"]})
    );
    assert_eq!(list("https://github.com.evil.example")["items"], json!([]));

    let fill =
        json!({"v": 1, "type": "autofill_fill", "origin": "https://github.com", "item": "github"});
    let refused = host_with_vault(&data, "no", fill.clone());
    assert_eq!(
        refused,
        json!({"v": 1, "type": "error", "code": "not_confirmed"})
    );
    let filled = host_with_vault(&data, "yes", fill);
    assert_eq!(
        filled,
        json!({"v": 1, "type": "filled", "username": "amy", "password": PASSWORD})
    );
    let elsewhere = host_with_vault(
        &data,
        "yes",
        json!({"v": 1, "type": "autofill_fill", "origin": "https://evil.example", "item": "github"}),
    );
    assert_eq!(code(&elsewhere), "refused");
    assert!(!elsewhere.to_string().contains(PASSWORD));

    let save = |password: &str| {
        host_with_vault(
            &data,
            "no",
            json!({"v": 1, "type": "save_login", "origin": "https://github.com", "username": "amy", "password": password}),
        )
    };
    assert_eq!(
        save(PASSWORD),
        json!({"v": 1, "type": "known", "item": "github"})
    );
    let saved = save("KH-PW-new-42");
    assert_eq!(saved["type"], "saved", "{saved}");
    assert!(saved["item"].as_str().unwrap().starts_with("github~draft-"));
    assert!(!saved.to_string().contains("KH-PW-new-42"));
}

/// Reads one length-prefixed message from the host's stdout.
fn read_framed(out: &mut impl std::io::Read) -> Value {
    let mut length = [0u8; 4];
    out.read_exact(&mut length).unwrap();
    let mut body = vec![0u8; u32::from_ne_bytes(length) as usize];
    out.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}
fn write_framed(input: &mut impl Write, message: &Value) {
    let body = message.to_string().into_bytes();
    input.write_all(&(body.len() as u32).to_ne_bytes()).unwrap();
    input.write_all(&body).unwrap();
    input.flush().unwrap();
}

#[test]
fn the_bridge_carries_the_apps_ask_to_the_extension_and_its_answer_back() {
    use keyharbor_supervisor::bridge::{SiteAnswer, SiteAsk, ask, bridge_socket};
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args([
            "browser",
            "host",
            "chrome-extension://fimjcgdlhnkdmjnhcccnkpcnmcecgjoc/",
        ])
        .env("KEYHARBOR_TEST_PRESENCE", "yes")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (mut to_host, mut from_host) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    write_framed(&mut to_host, &json!({"v": 1, "type": "bridge"}));
    assert_eq!(
        read_framed(&mut from_host),
        json!({"v": 1, "type": "bridge_ready"})
    );

    let not_a_site = ask(
        &data,
        &SiteAsk {
            session: "work".into(),
            url: "file:///etc/passwd".into(),
        },
    )
    .unwrap();
    assert_eq!(
        not_a_site,
        SiteAnswer::BadSite,
        "refused before reaching the extension"
    );

    let asking = {
        let data = data.clone();
        std::thread::spawn(move || {
            ask(
                &data,
                &SiteAsk {
                    session: "work".into(),
                    url: "https://github.com/settings".into(),
                },
            )
        })
    };
    let request = read_framed(&mut from_host);
    assert_eq!(
        (
            request["type"].as_str(),
            request["session"].as_str(),
            request["origin"].as_str()
        ),
        (
            Some("request_site"),
            Some("work"),
            Some("https://github.com")
        )
    );
    write_framed(
        &mut to_host,
        &json!({"v": 1, "type": "request_answer", "id": request["id"], "outcome": "handed_off", "cookies": 3}),
    );
    assert_eq!(
        asking.join().unwrap().unwrap(),
        SiteAnswer::HandedOff { cookies: 3 }
    );

    drop(to_host);
    assert!(child.wait().unwrap().success());
    assert!(
        !bridge_socket(&data).exists(),
        "the socket goes with the extension"
    );
}

#[test]
fn a_bridge_started_from_a_shell_is_refused() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args(["browser", "host", "x"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    write_framed(
        &mut child.stdin.take().unwrap(),
        &json!({"v": 1, "type": "bridge"}),
    );
    let out = child.wait_with_output().unwrap();
    let reply: Value = serde_json::from_slice(&out.stdout[4..]).unwrap();
    assert_eq!(
        reply,
        json!({"v": 1, "type": "error", "code": "not_browser"})
    );
    assert!(!keyharbor_supervisor::bridge::bridge_socket(&data).exists());
}
