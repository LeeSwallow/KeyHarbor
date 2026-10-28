//! Shared by the real-Chrome tests.
#![allow(dead_code)]

pub mod mcp;

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
};

/// A fake browser: drains the CDP pipe (fd 3) and exits when the pipe closes, when
/// `$TEST_DIR/close` appears, or when `$TEST_DIR` is gone. Records its arguments and its
/// parent (the supervisor) pid, and writes `DevToolsActivePort` for `--remote-debugging-port=0`.
pub const FAKE_BROWSER: &str = r#"#!/bin/sh
printf '%s' "$PPID" > "$TEST_DIR/supervisor-pid"
printf '%s\n' "$@" > "$TEST_DIR/args"
for arg in "$@"; do case "$arg" in --user-data-dir=*) profile=${arg#--user-data-dir=};; --remote-debugging-port=0) cdp=1;; esac; done
if [ "$cdp" = 1 ]; then printf '12345\n/devtools/browser/11111111-1111-4111-8111-111111111111\n' > "$profile/DevToolsActivePort"; fi
cat <&3 >/dev/null &
reader=$!
while [ -d "$TEST_DIR" ] && [ ! -f "$TEST_DIR/close" ] && kill -0 "$reader" 2>/dev/null; do sleep 0.05; done
kill "$reader" 2>/dev/null
[ -d "$TEST_DIR" ] && touch "$TEST_DIR/ended"
exit 0
"#;

/// Unix socket paths are limited to ~104 bytes; macOS $TMPDIR is too deep for them.
pub fn short_tempdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("kh")
        .tempdir_in("/tmp")
        .unwrap()
}

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
