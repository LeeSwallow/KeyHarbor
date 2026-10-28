//! Shared by the real-Chrome tests.
#![allow(dead_code)]

pub mod harness;
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

/// A site that records what forms post to it (in this process only, never printed).
pub struct Site {
    pub base: String,
    pub posted: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

fn form_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `/` is a login form posting to `/session`; `/embed?port=N` holds a top-level input and an
/// iframe showing `http://127.0.0.1:N/`. `/session` records the fields and shows `user` back
/// as written (`#raw`) and in groups of four separated by spaces (`#grouped`), as payment
/// pages show card numbers.
pub fn serve_pages() -> Site {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let posted = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = posted.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(&stream);
            let mut request = String::new();
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                    break;
                }
                if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            if std::io::Read::read_exact(&mut reader, &mut body).is_err() {
                continue;
            }
            let path = request.split(' ').nth(1).unwrap_or("/").to_owned();
            let page = if request.starts_with("POST /session") {
                let fields: Vec<(String, String)> = String::from_utf8_lossy(&body)
                    .split('&')
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(k, v)| (form_decode(k), form_decode(v)))
                    .collect();
                let user = fields
                    .iter()
                    .find(|(k, _)| k == "user")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                let grouped: Vec<String> = user
                    .chars()
                    .collect::<Vec<_>>()
                    .chunks(4)
                    .map(|c| c.iter().collect())
                    .collect();
                *record.lock().unwrap() = fields;
                format!(
                    "<!doctype html><title>Signed in</title><h1 id=done>signed in</h1><p id=raw>{}</p><p id=grouped>{}</p>",
                    html_escape(&user),
                    html_escape(&grouped.join(" "))
                )
            } else if path == "/" {
                "<!doctype html><title>Login</title><form action=/session method=post>\
                 <label>User <input id=user name=user></label>\
                 <label>Password <input id=pw name=pw type=password></label>\
                 <button id=go>Sign in</button></form>"
                    .to_owned()
            } else if let Some(port) = path.strip_prefix("/embed?port=") {
                format!(
                    "<!doctype html><title>Shop</title><label>Top <input id=top name=top></label>\
                     <iframe id=f src=\"http://127.0.0.1:{port}/\"></iframe>"
                )
            } else {
                "<!doctype html><title>Elsewhere</title><p>elsewhere</p>".to_owned()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{page}",
                page.len()
            );
        }
    });
    Site { base, posted }
}

/// A tiny HTTP server answering every GET with `page(path)` as HTML. A path starting with
/// `/slow` is answered after five seconds.
pub fn serve_fn(page: impl Fn(&str) -> String + Send + Sync + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let page = std::sync::Arc::new(page);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let page = page.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(&stream);
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    return;
                }
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                        break;
                    }
                }
                let path = request.split(' ').nth(1).unwrap_or("/").to_owned();
                if path.starts_with("/slow") {
                    thread::sleep(std::time::Duration::from_secs(5));
                }
                let body = page(&path);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    base
}

/// What `keyharbor` printed on a pseudo-terminal and whether it succeeded.
pub struct Terminal {
    pub success: bool,
    pub output: String,
}

/// Runs `keyharbor --data-dir <data> <args>` on a pseudo-terminal (`script(1)`), as a person
/// at a terminal would, typing `answer` when it asks for confirmation. Test runs have no
/// terminal, so this is the only way they reach the confirmed path of a weakening operation;
/// it also shows that the terminal check is a weak boundary (03 §3.12).
pub fn on_terminal(
    data: &std::path::Path,
    keys: &std::path::Path,
    args: &[&str],
    answer: &str,
) -> Terminal {
    use std::{
        io::Read,
        process::{Command, Stdio},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    let mut child = Command::new("script")
        .args([
            "-q",
            "/dev/null",
            env!("CARGO_BIN_EXE_keyharbor"),
            "--data-dir",
        ])
        .arg(data)
        .args(args)
        .env("KEYHARBOR_TEST_KEYSTORE_DIR", keys)
        .env("KEYHARBOR_BROWSER_HEADLESS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script(1)");
    let output = Arc::new(Mutex::new(Vec::new()));
    let mut stdout = child.stdout.take().unwrap();
    let sink = output.clone();
    let reader = thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        while let Ok(n) = stdout.read(&mut buffer) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buffer[..n]);
        }
    });
    let mut stdin = child.stdin.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let asked = String::from_utf8_lossy(&output.lock().unwrap()).contains("Type yes");
        if asked {
            stdin.write_all(format!("{answer}\n").as_bytes()).unwrap();
            break;
        }
        if child.try_wait().unwrap().is_some() || Instant::now() > deadline {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let status = child.wait().unwrap();
    drop(stdin);
    reader.join().unwrap();
    let output = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
    Terminal {
        success: status.success(),
        output,
    }
}
