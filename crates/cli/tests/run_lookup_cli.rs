//! `keyharbor run`, channel ④ (J-122): the child asks for values by name over a socket only it
//! knows; refusals carry no value, its output stays masked, and the socket goes with the run.
#![cfg(unix)]

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::Receiver,
    time::Duration,
};

struct Cli {
    tmp: tempfile::TempDir,
    data: PathBuf,
}

impl Cli {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let c = Self { tmp, data };
        c.ok(&["vault", "init"], None);
        c
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_keyharbor"));
        command
            .arg("--data-dir")
            .arg(&self.data)
            .args(args)
            .env("KEYHARBOR_TEST_KEYSTORE_DIR", self.tmp.path().join("keys"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn ok(&self, args: &[&str], stdin: Option<&str>) -> String {
        let mut child = self.command(args).stdin(Stdio::piped()).spawn().unwrap();
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
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn synth(&self, tag: &str) -> String {
        format!(
            "KH-SYNTH-{}-{tag}",
            self.tmp.path().file_name().unwrap().to_string_lossy()
        )
    }
    fn add_env(&self, item: &str, var: &str, value: &str) {
        self.ok(
            &["vault", "add", item, "--kind", "env", "--stdin"],
            Some(&serde_json::json!({ var: value }).to_string()),
        );
    }
    fn events(&self) -> Vec<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(&self.ok(&["audit", "--json"], None))
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    }
}

/// Reads `out` on its own thread, chunk by chunk, until it closes.
fn reader(mut out: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
    let (sender, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 256];
        while let Ok(n) = out.read(&mut buffer) {
            if n == 0 || sender.send(buffer[..n].to_vec()).is_err() {
                return;
            }
        }
    });
    received
}

/// What `chunks` gives until `want` appears, 10 seconds pass, or the stream closes.
fn read_until(chunks: &Receiver<Vec<u8>>, want: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut taken = Vec::new();
    while !String::from_utf8_lossy(&taken).contains(want) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match chunks.recv_timeout(left) {
            Ok(chunk) => taken.extend(chunk),
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&taken).into_owned()
}

/// The child prints `socket=<path>` and `token=<token>` first; returns them.
fn socket_and_token(out: &Receiver<Vec<u8>>) -> (PathBuf, String) {
    let text = read_until(out, "end-of-vars\n");
    let value = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap_or_else(|| panic!("{key} missing in {text:?}"))
            .to_owned()
    };
    (PathBuf::from(value("socket=")), value("token="))
}

const PRINT_VARS: &str = "echo \"socket=$KEYHARBOR_LOOKUP_SOCKET\"; echo \"token=$KEYHARBOR_LOOKUP_TOKEN\"; echo end-of-vars; ";

fn ask(socket: &Path, token: &str, item: &str, field: &str) -> serde_json::Value {
    let mut stream = UnixStream::connect(socket).unwrap();
    let request = serde_json::json!({"token": token, "item": item, "field": field});
    stream.write_all(format!("{request}\n").as_bytes()).unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    serde_json::from_str(&reply).unwrap()
}

#[test]
fn the_child_asks_by_name_within_the_run_and_its_output_stays_masked() {
    let c = Cli::new();
    let (key, db, prod, locked) = (
        c.synth("key"),
        c.synth("db"),
        c.synth("prod"),
        c.synth("locked"),
    );
    c.add_env("app", "API_KEY", &key);
    c.add_env("myapp/dev/db", "PASSWORD", &db);
    c.add_env("myapp/dev/locked", "PASSWORD", &locked);
    c.add_env("myapp/prod/db", "PASSWORD", &prod);
    c.ok(
        &[
            "policy",
            "deny",
            "--agent",
            "claude",
            "--resource",
            "item",
            "--target",
            "myapp/dev/locked",
        ],
        None,
    );
    let script = format!("{PRINT_VARS} read -r v; echo \"got $v\"; read -r _");
    let mut run = c
        .command(&[
            "run",
            "--agent",
            "claude",
            "--env",
            "app",
            "--allow",
            "myapp/dev/*",
            "--",
            "sh",
            "-c",
            &script,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let out = reader(run.stdout.take().unwrap());
    let (socket, token) = socket_and_token(&out);
    let dir = socket.parent().unwrap().to_owned();

    // The run's own item, and one --allow covers.
    assert_eq!(ask(&socket, &token, "app", "API_KEY")["value"], *key);
    assert_eq!(
        ask(&socket, &token, "myapp/dev/db", "PASSWORD")["value"],
        *db
    );
    // No token, a wrong token, outside the run, refused by a policy: no value.
    let refusals = [
        (ask(&socket, "", "app", "API_KEY"), "denied (bad-token)"),
        (
            ask(&socket, &"0".repeat(64), "app", "API_KEY"),
            "denied (bad-token)",
        ),
        (
            ask(&socket, &token, "myapp/prod/db", "PASSWORD"),
            "denied: myapp/prod/db (outside-run)",
        ),
        (
            ask(&socket, &token, "myapp/dev/locked", "PASSWORD"),
            "denied: myapp/dev/locked (policy)",
        ),
    ];
    for (reply, error) in &refusals {
        assert_eq!(reply, &serde_json::json!({ "error": error }));
    }

    // What the child prints of a looked-up value is masked like any other.
    let mut stdin = run.stdin.take().unwrap();
    writeln!(stdin, "{db}").unwrap();
    // Waits the full 10 seconds only if the value comes out unmasked.
    let shown = read_until(&out, "PASSWORD}}\n");
    assert!(
        shown.contains("got {{secret:myapp/dev/db.PASSWORD}}"),
        "{shown:?}"
    );
    writeln!(stdin, "done").unwrap();
    let mut err = String::new();
    run.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert!(run.wait().unwrap().success(), "{err}");
    assert!(
        !socket.exists() && !dir.exists(),
        "the socket outlived the run"
    );
    for value in [&key, &db, &prod, &locked] {
        assert!(!shown.contains(value.as_str()) && !err.contains(value.as_str()));
    }

    let events = c.events();
    let lookups = events
        .iter()
        .filter(|e| e["kind"] == "env_injected" && e["result"] == "lookup")
        .count();
    assert_eq!(lookups, 2);
    for reason in ["bad-token", "outside-run", "policy"] {
        assert!(
            events
                .iter()
                .any(|e| e["kind"] == "secret_denied" && e["result"] == reason),
            "{reason}"
        );
    }
}

fn start_waiting(c: &Cli) -> (Child, Receiver<Vec<u8>>, PathBuf) {
    c.add_env("app", "API_KEY", &c.synth("key"));
    let script =
        format!("trap 'exit 5' INT; trap 'exit 7' TERM; {PRINT_VARS} while :; do sleep 0.1; done");
    let mut run = c
        .command(&["run", "--env", "app", "--", "sh", "-c", &script])
        .stdin(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let out = reader(run.stdout.take().unwrap());
    let (socket, _) = socket_and_token(&out);
    assert!(socket.exists());
    (run, out, socket)
}

/// Ctrl-C reaches the whole foreground group; `run` outlives the child and cleans up.
#[test]
fn an_interrupt_ends_the_run_without_leaving_the_socket() {
    let c = Cli::new();
    let (mut run, _out, socket) = start_waiting(&c);
    let group = run.id() as i32;
    // SAFETY: killpg only sends a signal to the process group we started.
    unsafe { libc::killpg(group, libc::SIGINT) };
    assert_eq!(run.wait().unwrap().code(), Some(5));
    assert!(
        !socket.parent().unwrap().exists(),
        "the socket outlived the run"
    );
}

#[test]
fn a_terminate_signal_ends_the_run_without_leaving_the_socket() {
    let c = Cli::new();
    let (mut run, _out, socket) = start_waiting(&c);
    // SAFETY: kill only sends a signal to the process we started.
    unsafe { libc::kill(run.id() as i32, libc::SIGTERM) };
    assert_eq!(run.wait().unwrap().code(), Some(7));
    assert!(
        !socket.parent().unwrap().exists(),
        "the socket outlived the run"
    );
}

#[test]
fn only_allow_is_enough_and_patterns_are_checked() {
    let c = Cli::new();
    c.add_env("myapp/dev/db", "PASSWORD", &c.synth("db"));
    let out = c.ok(
        &[
            "run",
            "--allow",
            "myapp/dev/*",
            "--",
            "sh",
            "-c",
            "test -S \"$KEYHARBOR_LOOKUP_SOCKET\" && echo socket",
        ],
        None,
    );
    assert_eq!(out, "socket\n");
    let bad = c
        .command(&["run", "--allow", "my*app", "--", "true"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("--allow my*app"));
}
