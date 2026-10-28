//! `keyharbor mcp` over stdio JSON-RPC, with the fake browser (and real Chrome when available).
#![cfg(unix)]

mod common;

use common::{FAKE_BROWSER, chrome, mcp::Mcp, serve_site, short_tempdir};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn cli(data: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn fake_browser(dir: &Path) -> PathBuf {
    let browser = dir.join("browser");
    fs::write(&browser, FAKE_BROWSER).unwrap();
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
    browser
}

fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn session_tools_with_fake_browser() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let browser = fake_browser(tmp.path());
    let browser_arg = browser.to_str().unwrap();
    let mut mcp = Mcp::start(
        &data,
        tmp.path(),
        &["--browser", browser_arg, "--agent", "claude"],
    );
    let tools = mcp.tools();
    for name in [
        "session_list",
        "session_create",
        "session_open",
        "session_stop",
        "session_wait_auth",
        "session_wait_for_resume",
        "browser_navigate",
        "browser_snapshot",
        "browser_screenshot",
        "browser_click",
        "browser_fill",
        "browser_type",
        "browser_press",
        "browser_tabs",
        "browser_new_tab",
        "browser_switch_tab",
        "browser_wait",
        "vault_list",
    ] {
        assert!(tools.iter().any(|t| t == name), "{name} missing: {tools:?}");
    }
    assert!(!tools.iter().any(|t| t == "session_endpoint"));
    assert!(
        !tools
            .iter()
            .any(|t| t.contains("resume") && t != "session_wait_for_resume")
    );
    assert!(
        !tools
            .iter()
            .any(|t| t.contains("recover") || t.contains("reveal") || t.contains("secret"))
    );

    let created = mcp.ok("session_create", json!({"name": "--help"}));
    assert_eq!(created["name"], "--help");
    assert_eq!(created["state"], "stopped");
    let id = created["id"].as_str().unwrap().to_owned();

    assert!(
        mcp.err("session_open", json!({"id": id, "cdp": true}))
            .contains("--allow-cdp")
    );
    mcp.err(
        "session_open",
        json!({"id": id, "url": "http://example.com"}),
    );
    mcp.err("session_open", json!({"id": "not-an-id"}));
    let opened = mcp.ok(
        "session_open",
        json!({"id": id, "url": "https://example.com/login"}),
    );
    assert_eq!(opened["state"], "running");
    wait_until(|| tmp.path().join("args").exists());
    assert!(
        fs::read_to_string(tmp.path().join("args"))
            .unwrap()
            .lines()
            .any(|l| l == "https://example.com/login")
    );
    mcp.err("session_open", json!({"id": id}));
    mcp.err("session_endpoint", json!({"id": id}));

    let handoff = mcp.ok(
        "session_wait_auth",
        json!({"id": id, "origin": "https://example.com"}),
    );
    let request = handoff["request_id"].as_str().unwrap().to_owned();
    let inspected = cli(&data, &["handoff", &id]);
    assert!(String::from_utf8_lossy(&inspected.stdout).contains("\thttps://example.com"));
    mcp.err(
        "session_wait_for_resume",
        json!({"id": id, "request_id": request, "timeout_seconds": 0}),
    );
    assert!(
        mcp.err(
            "session_wait_for_resume",
            json!({"id": id, "request_id": request, "timeout_seconds": 1}),
        )
        .contains("Timed out")
    );
    let resume = {
        let (data, id, request) = (data.clone(), id.clone(), request.clone());
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            cli(&data, &["resume", &id, &request]).status.success()
        })
    };
    mcp.ok(
        "session_wait_for_resume",
        json!({"id": id, "request_id": request, "timeout_seconds": 5}),
    );
    assert!(resume.join().unwrap());
    mcp.ok(
        "session_wait_for_resume",
        json!({"id": id, "request_id": request, "timeout_seconds": 1}),
    );
    let second = mcp.ok("session_wait_auth", json!({"id": id}))["request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.err(
        "session_wait_for_resume",
        json!({"id": id, "request_id": request, "timeout_seconds": 1}),
    );
    let stop = {
        let (data, id) = (data.clone(), id.clone());
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            cli(&data, &["stop", &id]).status.success()
        })
    };
    mcp.err(
        "session_wait_for_resume",
        json!({"id": id, "request_id": second, "timeout_seconds": 5}),
    );
    assert!(stop.join().unwrap());
    let list = mcp.ok("session_list", json!({}));
    assert_eq!(list[0]["name"], "--help");
    assert_eq!(list[0]["state"], "stopped");
    mcp.err("session_wait_auth", json!({"id": "not-a-session"}));
    assert!(
        mcp.err("browser_snapshot", json!({"session": id}))
            .contains("not running")
    );
    fs::remove_file(tmp.path().join("ended")).unwrap();
    let reopened = mcp.ok("session_open", json!({"id": id}));
    assert_eq!(reopened["state"], "running");
    assert_eq!(
        mcp.ok("session_stop", json!({"id": id}))["state"],
        "stopped"
    );
    mcp.finish();

    // With --allow-cdp the endpoint tool exists and works for cdp sessions only.
    fs::remove_file(tmp.path().join("ended")).unwrap();
    let mut mcp = Mcp::start(
        &data,
        tmp.path(),
        &["--allow-cdp", "--browser", browser_arg],
    );
    assert!(mcp.tools().iter().any(|t| t == "session_endpoint"));
    mcp.ok("session_open", json!({"id": id}));
    mcp.err("session_endpoint", json!({"id": id}));
    mcp.ok("session_stop", json!({"id": id}));
    fs::remove_file(tmp.path().join("ended")).unwrap();
    mcp.ok("session_open", json!({"id": id, "cdp": true}));
    let endpoint = mcp.ok("session_endpoint", json!({"id": id}));
    assert!(
        endpoint["endpoint"]
            .as_str()
            .unwrap()
            .starts_with("ws://127.0.0.1:12345/devtools/browser/"),
        "{endpoint}"
    );
    mcp.ok("session_stop", json!({"id": id}));
    mcp.finish();
}

#[test]
fn vault_list_shows_metadata_only() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let keys = tmp.path().join("keys");
    let vault = |args: &[&str], stdin: Option<&str>| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", &keys)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        if let Some(stdin) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
        }
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success(), "{args:?}");
    };
    let mut mcp = Mcp::start(&data, tmp.path(), &[]);
    assert_eq!(mcp.ok("vault_list", json!({})), json!([]));
    vault(&["vault", "init"], None);
    vault(
        &[
            "vault",
            "add",
            "github",
            "--kind",
            "login",
            "--origin",
            "https://github.com",
            "--set",
            "username=octo-user",
            "--stdin",
        ],
        Some(r#"{"password":"hunter2-SECRET-value"}"#),
    );
    let (error, text) = mcp.call("vault_list", json!({}));
    assert!(!error, "{text}");
    assert!(!text.contains("hunter2-SECRET-value"));
    assert!(!text.contains("octo-user"));
    let items: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(items[0]["name"], "github");
    assert_eq!(items[0]["kind"], "login");
    assert_eq!(items[0]["domain"], "credentials");
    assert_eq!(items[0]["origins"], json!(["https://github.com"]));
    let fields = items[0]["fields"].as_array().unwrap();
    assert!(fields.contains(&json!("password")) && fields.contains(&json!("username")));
    mcp.finish();
}

#[test]
fn an_agent_denied_a_session_cannot_stop_it_or_ask_the_user_in_it() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let id = String::from_utf8(cli(&data, &["create", "work"]).stdout)
        .unwrap()
        .split('\t')
        .next()
        .unwrap()
        .to_owned();
    let rule = [
        "policy",
        "deny",
        "--agent",
        "codex",
        "--resource",
        "session",
        "--target",
        "*",
    ];
    assert!(cli(&data, &rule).status.success());
    let mut codex = Mcp::start(&data, tmp.path(), &["--agent", "codex"]);
    let request = "11111111-1111-4111-8111-111111111111";
    for (tool, arguments) in [
        ("session_stop", json!({"id": id})),
        ("session_wait_auth", json!({"id": id})),
        (
            "session_wait_for_resume",
            json!({"id": id, "request_id": request, "timeout_seconds": 1}),
        ),
    ] {
        let denied = codex.err(tool, arguments);
        assert!(
            denied.contains("denied by policy (codex session * deny)"),
            "{tool}: {denied}"
        );
    }
    codex.finish();
}

#[test]
fn vault_list_reports_tampered_items_without_their_metadata() {
    let c = common::harness::Harness::new();
    c.add(
        "api",
        "secret",
        &["https://api.example"],
        json!({"value": "KH-SYNTH-api-41"}),
    );
    c.add("kept", "secret", &[], json!({"value": "KH-SYNTH-kept-41"}));
    let store = keyharbor_sqlite::SqliteStore::open(&c.data).unwrap();
    store.execute_for_tests(
        "UPDATE vault_items SET run_allowed = 0, description = 'paste the key here', expires_at = 'not-a-date' WHERE name = 'api'",
    );
    store.execute_for_tests(
        "INSERT INTO item_origins (item_id, origin) SELECT id, 'https://evil.example' FROM vault_items WHERE name = 'api'",
    );
    let mut mcp = Mcp::start(&c.data, c.tmp.path(), &[]);
    let (error, text) = mcp.call("vault_list", json!({}));
    assert!(!error, "{text}");
    for unverified in [
        "evil.example",
        "paste the key",
        "not-a-date",
        "run_allowed\":false",
    ] {
        assert!(!text.contains(unverified), "{unverified}: {text}");
    }
    let items: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(items[0], json!({"name": "api", "tampered": true}));
    assert_eq!(items[1]["name"], "kept");
    assert_eq!(items[1]["tampered"], false);
    assert_eq!(items[1]["run_allowed"], true);
    mcp.finish();
}

#[test]
fn browser_tools_round_trip_with_chrome() {
    let Some(chrome) = chrome() else { return };
    let base = serve_site();
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let mut mcp = Mcp::start(&data, tmp.path(), &["--browser", chrome.to_str().unwrap()]);
    let id = mcp.ok("session_create", json!({"name": "chrome"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.ok("session_open", json!({"id": id}));
    let page = mcp.ok(
        "browser_navigate",
        json!({"session": id, "url": format!("{base}/")}),
    );
    assert_eq!(page["title"], "Login");
    let snapshot = mcp.ok("browser_snapshot", json!({"session": id}));
    assert!(
        snapshot["text"]
            .as_str()
            .unwrap()
            .contains("button \"로그인\"")
    );
    mcp.ok(
        "browser_fill",
        json!({"session": id, "selector": "#user", "text": "carol"}),
    );
    mcp.ok("browser_click", json!({"session": id, "selector": "#go"}));
    mcp.ok("browser_wait", json!({"session": id, "selector": "#done"}));
    let snapshot = mcp.ok("browser_snapshot", json!({"session": id}));
    assert!(snapshot["text"].as_str().unwrap().contains("hello carol"));
    assert!(mcp.ok("browser_tabs", json!({"session": id})).is_array());
    mcp.err("browser_click", json!({"session": id, "ref": 987654321}));
    mcp.err("browser_click", json!({"session": id}));
    mcp.ok("session_stop", json!({"id": id}));
    mcp.finish();
}

#[test]
fn policies_limit_session_tools() {
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let browser = fake_browser(tmp.path());
    let browser_arg = browser.to_str().unwrap();
    let id = String::from_utf8(cli(&data, &["create", "work"]).stdout)
        .unwrap()
        .split('\t')
        .next()
        .unwrap()
        .to_owned();
    for rule in [
        ["deny", "codex", "session", "*"],
        ["deny", "codex", "create_session", "*"],
        ["deny", "*", "raw_cdp", "*"],
    ] {
        let out = cli(
            &data,
            &[
                "policy",
                rule[0],
                "--agent",
                rule[1],
                "--resource",
                rule[2],
                "--target",
                rule[3],
            ],
        );
        assert!(out.status.success());
    }
    let mut codex = Mcp::start(
        &data,
        tmp.path(),
        &["--browser", browser_arg, "--agent", "codex", "--allow-cdp"],
    );
    let denied = codex.err("session_open", json!({"id": id}));
    assert!(denied.contains("denied by policy"), "{denied}");
    assert!(denied.contains("codex session * deny"), "{denied}");
    assert!(
        codex
            .err("browser_snapshot", json!({"session": id}))
            .contains("denied by policy")
    );
    assert!(
        codex
            .err("session_create", json!({"name": "x"}))
            .contains("denied by policy")
    );
    codex.finish();

    // Other agents may open the session, but not with raw CDP.
    let mut claude = Mcp::start(
        &data,
        tmp.path(),
        &["--browser", browser_arg, "--agent", "claude", "--allow-cdp"],
    );
    assert!(
        claude
            .err("session_open", json!({"id": id, "cdp": true}))
            .contains("raw_cdp")
    );
    claude.ok("session_open", json!({"id": id}));
    assert!(
        claude
            .err("session_endpoint", json!({"id": id}))
            .contains("denied by policy")
    );
    claude.ok("session_create", json!({"name": "second"}));
    claude.ok("session_stop", json!({"id": id}));
    claude.finish();
    let audit = String::from_utf8(cli(&data, &["audit"]).stdout).unwrap();
    assert!(audit.contains("\tclaude\tsession_opened\t"), "{audit}");
    assert!(audit.contains("\tclaude\tsession_stopped\t"), "{audit}");
    let mut bad = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&data)
        .args(["mcp", "--agent", "*"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(!bad.wait().unwrap().success());
}

#[test]
fn placeholders_through_mcp_with_chrome() {
    let Some(chrome) = chrome() else { return };
    let site = common::serve_pages();
    let tmp = short_tempdir();
    let data = tmp.path().join("data");
    let keys = tmp.path().join("keys");
    let secret = format!(
        "KH-SYNTH-{}",
        tmp.path().file_name().unwrap().to_string_lossy()
    );
    let vault = |args: &[&str], stdin: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", &keys)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success(), "{args:?}");
    };
    vault(&["vault", "init"], "");
    vault(
        &[
            "vault", "add", "site", "--kind", "login", "--origin", &site.base, "--stdin",
        ],
        &json!({"username": secret}).to_string(),
    );
    let mut mcp = Mcp::start(
        &data,
        tmp.path(),
        &["--browser", chrome.to_str().unwrap(), "--agent", "claude"],
    );
    let mut transcript = String::new();
    let listed = mcp.ok("vault_list", json!({}));
    assert!(
        listed[0]["placeholders"]
            .as_array()
            .unwrap()
            .contains(&json!("{{secret:site.username}}")),
        "{listed}"
    );
    assert_eq!(listed[0]["origin_binding"], true);
    let id = mcp.ok("session_create", json!({"name": "c"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    mcp.ok("session_open", json!({"id": id}));
    mcp.ok(
        "browser_navigate",
        json!({"session": id, "url": format!("{}/", site.base)}),
    );
    let (error, text) = mcp.call(
        "browser_fill",
        json!({"session": id, "selector": "#user", "text": "{{secret:site.username}}"}),
    );
    assert!(!error, "{text}");
    transcript.push_str(&text);
    let (_, snapshot) = mcp.call("browser_snapshot", json!({"session": id}));
    // The input that received the value stays redacted for the session (C1).
    assert!(snapshot.contains("value=\\\"[redacted]\\\""), "{snapshot}");
    transcript.push_str(&snapshot);
    mcp.ok("browser_click", json!({"session": id, "selector": "#go"}));
    mcp.ok("browser_wait", json!({"session": id, "selector": "#done"}));
    let (_, after) = mcp.call("browser_snapshot", json!({"session": id}));
    assert!(after.contains("{{secret:site.username}}"), "{after}");
    transcript.push_str(&after);
    assert_eq!(
        site.posted.lock().unwrap().first(),
        Some(&("user".to_owned(), secret.clone()))
    );
    let denied = mcp.err(
        "browser_type",
        json!({"session": id, "text": "{{secret:site.password}}"}),
    );
    transcript.push_str(&denied);
    assert_eq!(denied, "denied: site (not_found)");
    mcp.ok("session_stop", json!({"id": id}));
    mcp.finish();
    let audit = String::from_utf8(cli(&data, &["audit"]).stdout).unwrap();
    assert!(audit.contains("\tclaude\tsecret_used\tsite\t"), "{audit}");
    transcript.push_str(&audit);
    assert!(!transcript.contains(&secret), "{transcript}");
}
