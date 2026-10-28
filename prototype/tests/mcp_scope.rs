use keyharbor::Store;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

fn send(input: &mut impl Write, output: &mut impl BufRead, request: Value) -> Value {
    writeln!(input, "{request}").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn scoped_mcp_cannot_discover_or_mutate_other_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    let owned = store.create("permitted").unwrap();
    let other = store.create("other-private").unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&root)
        .args(["mcp", "--allow-cdp", "--session", &owned.id])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = process.stdin.take().unwrap();
    let mut output = BufReader::new(process.stdout.take().unwrap());
    let response = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"scope-test","version":"1"}}}),
    );
    assert!(response.get("result").is_some());
    let duplicate = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&root)
        .args(["mcp", "--session", &owned.id])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let list = send(
        &mut input,
        &mut output,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"sessions_list","arguments":{}}}),
    );
    assert!(list.to_string().contains("permitted"));
    assert!(!list.to_string().contains("other-private"));
    for (number, name, args) in [
        (3, "session_create", json!({"name":"bypass"})),
        (4, "session_open", json!({"id":other.id})),
        (5, "session_wait_auth", json!({"id":other.id})),
        (6, "session_account", json!({"id":other.id})),
        (
            7,
            "session_wait_for_resume",
            json!({"id":other.id,"request_id":"none","timeout_seconds":1}),
        ),
        (8, "session_stop", json!({"id":other.id})),
        (9, "session_endpoint", json!({"id":other.id})),
    ] {
        let result = send(
            &mut input,
            &mut output,
            json!({"jsonrpc":"2.0","id":number,"method":"tools/call","params":{"name":name,"arguments":args}}),
        );
        assert_eq!(result["result"]["isError"], true, "{result}");
    }
    assert_eq!(store.get(&other.id).unwrap().state, "stopped");
    assert_eq!(store.list().unwrap().len(), 2);
    drop(input);
    assert!(process.wait().unwrap().success());
    assert!(store.lock_mcp_session(&owned.id).is_ok());
}
