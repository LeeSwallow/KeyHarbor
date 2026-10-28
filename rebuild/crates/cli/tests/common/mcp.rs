//! A JSON-RPC client for `keyharbor mcp` over stdio.

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

pub struct Mcp {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}
impl Mcp {
    pub fn start(data: &Path, test_dir: &Path, args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(data)
            .arg("mcp")
            .args(args)
            .env("TEST_DIR", test_dir)
            .env("KEYHARBOR_BROWSER_HEADLESS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut mcp = Self {
            child,
            input,
            output,
            next: 1,
        };
        let init = mcp.request(
            "initialize",
            json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}),
        );
        assert!(init.get("result").is_some(), "{init}");
        writeln!(
            mcp.input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        mcp
    }
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).expect("one JSON-RPC response per line")
    }
    pub fn tools(&mut self) -> Vec<String> {
        self.request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }
    /// Calls a tool; returns (is_error, text). An unknown tool counts as an error.
    pub fn call(&mut self, name: &str, arguments: Value) -> (bool, String) {
        let response = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        if let Some(message) = response["error"]["message"].as_str() {
            return (true, message.to_owned());
        }
        let result = &response["result"];
        assert!(result.is_object(), "{name}: {response}");
        (
            result["isError"] == true,
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        )
    }
    pub fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let (error, text) = self.call(name, arguments);
        assert!(!error, "{name}: {text}");
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    }
    pub fn err(&mut self, name: &str, arguments: Value) -> String {
        let (error, text) = self.call(name, arguments);
        assert!(error, "{name} should fail: {text}");
        text
    }
    pub fn finish(mut self) {
        drop(self.input);
        assert!(self.child.wait().unwrap().success());
    }
}
