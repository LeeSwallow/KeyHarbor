#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
#[test]
fn stdio_initialization_and_session_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let browser = tmp.path().join("browser");
    let close = tmp.path().join("close");
    fs::write(&browser, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TEST_DIR/args\"\nfor arg in \"$@\"; do case \"$arg\" in --user-data-dir=*) profile=${arg#--user-data-dir=};; --remote-debugging-port=0) cdp=1;; esac; done\nif [ \"$cdp\" = 1 ]; then printf '12345\\n/devtools/browser/11111111-1111-4111-8111-111111111111\\n' > \"$profile/DevToolsActivePort\"; fi\nwhile [ -d \"$TEST_DIR\" ] && [ ! -f \"$TEST_DIR/close\" ]; do sleep 0.1; done\n").unwrap();
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
    let mut server = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .arg("mcp")
        .arg("--browser")
        .arg(&browser)
        .env("TEST_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = server.stdin.take().unwrap();
    let mut output = BufReader::new(server.stdout.take().unwrap());
    fn send(input: &mut impl Write, output: &mut impl BufRead, request: Value) -> Value {
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).expect("expected one MCP JSON response on stdout")
    }
    let result = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    assert!(result.get("result").is_some(), "{result}");
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let tools = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    );
    let names: Vec<_> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"session_create"));
    assert!(!names.contains(&"session_resume"));
    assert!(
        !names
            .iter()
            .any(|name| name.contains("recover") || name.contains("secret"))
    );
    let created = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"session_create","arguments":{"name":"--help"}}}),
    );
    assert_ne!(created["result"]["isError"], true);
    assert!(created.to_string().contains("--help"));
    let id = created["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .split('\t')
        .next()
        .unwrap();
    let denied = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"session_open","arguments":{"id":id,"cdp":true}}}),
    );
    assert_eq!(denied["result"]["isError"], true);
    assert!(
        Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(tmp.path().join("data"))
            .args(["bind-account", id, "Demo account"])
            .status()
            .unwrap()
            .success()
    );
    let account = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"session_account","arguments":{"id":id}}}),
    );
    assert!(account.to_string().contains("Demo account"));
    let bad_url = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"session_open","arguments":{"id":id,"url":"http://example.com"}}}),
    );
    assert_eq!(bad_url["result"]["isError"], true);
    let open = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"session_open","arguments":{"id":id,"url":"https://example.com/login"}}}),
    );
    assert_ne!(open["result"]["isError"], true, "{open}");
    let endpoint = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"session_endpoint","arguments":{"id":id}}}),
    );
    assert_eq!(endpoint["result"]["isError"], true);
    let args = tmp.path().join("args");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !args.exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        fs::read_to_string(args)
            .unwrap()
            .lines()
            .any(|line| line == "https://example.com/login")
    );
    let duplicate = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"session_open","arguments":{"id":id}}}),
    );
    assert_eq!(duplicate["result"]["isError"], true);
    let handoff = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"session_wait_auth","arguments":{"id":id,"origin":"https://example.com"}}}),
    );
    assert_ne!(handoff["result"]["isError"], true);
    let request_id = handoff["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .split('\t')
        .nth(2)
        .unwrap();
    let inspected = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["handoff", id])
        .output()
        .unwrap();
    assert!(inspected.status.success());
    assert!(
        String::from_utf8(inspected.stdout)
            .unwrap()
            .contains("\thttps://example.com")
    );
    let timed_out = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"session_wait_for_resume","arguments":{"id":id,"request_id":request_id,"timeout_seconds":1}}}),
    );
    assert_eq!(timed_out["result"]["isError"], true);
    let resume_id = id.to_owned();
    let resume_request_id = request_id.to_owned();
    let data = tmp.path().join("data");
    let resume = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(data)
            .args(["resume", &resume_id, &resume_request_id])
            .status()
            .unwrap()
            .success()
    });
    let waited = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"session_wait_for_resume","arguments":{"id":id,"request_id":request_id,"timeout_seconds":5}}}),
    );
    assert!(resume.join().unwrap());
    assert_ne!(waited["result"]["isError"], true, "{waited}");
    let late = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":19,"method":"tools/call","params":{"name":"session_wait_for_resume","arguments":{"id":id,"request_id":request_id,"timeout_seconds":1}}}),
    );
    assert_ne!(late["result"]["isError"], true);
    let again = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"session_wait_auth","arguments":{"id":id}}}),
    );
    assert_ne!(again["result"]["isError"], true);
    let second_request = again["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .split('\t')
        .nth(2)
        .unwrap();
    let old = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":18,"method":"tools/call","params":{"name":"session_wait_for_resume","arguments":{"id":id,"request_id":request_id,"timeout_seconds":1}}}),
    );
    assert_eq!(old["result"]["isError"], true);
    let stop_id = id.to_owned();
    let data = tmp.path().join("data");
    let stop = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(data)
            .args(["stop", &stop_id])
            .status()
            .unwrap()
            .success()
    });
    let interrupted = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":17,"method":"tools/call","params":{"name":"session_wait_for_resume","arguments":{"id":id,"request_id":second_request,"timeout_seconds":5}}}),
    );
    assert!(stop.join().unwrap());
    assert_eq!(interrupted["result"]["isError"], true);
    fs::write(&close, "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let store = keyharbor::Store::open(&tmp.path().join("data")).unwrap();
    while store.get(id).unwrap().state != "stopped" {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(50));
    }
    let list = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"sessions_list","arguments":{}}}),
    );
    assert!(list.to_string().contains("--help"));
    let bad = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"session_wait_auth","arguments":{"id":"not-a-session"}}}),
    );
    assert_eq!(bad["result"]["isError"], true);
    drop(input);
    assert!(server.wait().unwrap().success());

    fs::remove_file(&close).unwrap();
    let mut server = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(tmp.path().join("data"))
        .args(["mcp", "--allow-cdp", "--browser"])
        .arg(&browser)
        .env("TEST_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = server.stdin.take().unwrap();
    let mut output = BufReader::new(server.stdout.take().unwrap());
    let _ = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":20,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let without_cdp = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"session_open","arguments":{"id":id}}}),
    );
    assert_ne!(without_cdp["result"]["isError"], true);
    let no_endpoint = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":24,"method":"tools/call","params":{"name":"session_endpoint","arguments":{"id":id}}}),
    );
    assert_eq!(no_endpoint["result"]["isError"], true);
    fs::write(&close, "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while store.get(id).unwrap().state != "stopped" {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(50));
    }
    fs::remove_file(&close).unwrap();
    let open = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"session_open","arguments":{"id":id,"cdp":true}}}),
    );
    assert_ne!(open["result"]["isError"], true, "{open}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !tmp
        .path()
        .join("data")
        .join("profiles")
        .join(id)
        .join("DevToolsActivePort")
        .exists()
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    let endpoint = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"session_endpoint","arguments":{"id":id}}}),
    );
    assert!(
        endpoint
            .to_string()
            .contains("ws://127.0.0.1:12345/devtools/browser/"),
        "{endpoint}"
    );
    fs::write(&close, "").unwrap();
    while store.get(id).unwrap().state != "stopped" {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(50));
    }
    drop(input);
    assert!(server.wait().unwrap().success());
}
