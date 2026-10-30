//! Session lifecycle through the CLI with a fake browser: a shell script that drains the CDP
//! pipe (fd 3) and exits when the pipe closes or the test asks it to.
#![cfg(unix)]

mod common;

use common::FAKE_BROWSER;

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Env {
    tmp: tempfile::TempDir,
    data: PathBuf,
    browser: PathBuf,
}
impl Env {
    fn new() -> Self {
        // Unix socket paths are limited to ~104 bytes; macOS $TMPDIR is too deep.
        let tmp = tempfile::Builder::new()
            .prefix("kh")
            .tempdir_in("/tmp")
            .unwrap();
        let browser = tmp.path().join("browser");
        fs::write(&browser, FAKE_BROWSER).unwrap();
        fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            data: tmp.path().join("data"),
            browser,
            tmp,
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }
    fn cmd(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_keyharbor"));
        command
            .arg("--data-dir")
            .arg(&self.data)
            .env("TEST_DIR", self.tmp.path())
            .stdin(Stdio::null());
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.cmd().args(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?} should fail");
        String::from_utf8(out.stderr).unwrap()
    }
    fn create(&self, name: &str) -> String {
        self.ok(&["create", name])
            .split('\t')
            .next()
            .unwrap()
            .to_owned()
    }
    fn open(&self, id: &str) -> Output {
        self.cmd()
            .args(["open", id, "--browser"])
            .arg(&self.browser)
            .output()
            .unwrap()
    }
    fn state(&self, id: &str) -> String {
        self.ok(&["list"])
            .lines()
            .find(|line| line.starts_with(id))
            .unwrap()
            .split('\t')
            .nth(2)
            .unwrap()
            .to_owned()
    }
    fn wait_for_state(&self, id: &str, state: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.state(id) != state {
            assert!(Instant::now() < deadline, "never reached {state}");
            thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::write(self.path("close"), "");
    }
}
fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn open_list_stop() {
    let env = Env::new();
    let id = env.create("work");
    assert_eq!(env.state(&id), "stopped");
    let out = env.open(&id);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(env.state(&id), "running");
    wait_for(&env.path("args"));
    let args = fs::read_to_string(env.path("args")).unwrap();
    assert!(
        args.lines().any(|l| l == "--remote-debugging-pipe"),
        "{args}"
    );
    assert!(args.lines().any(|l| l.starts_with("--user-data-dir=")));
    assert!(
        !args
            .lines()
            .any(|l| l.starts_with("--remote-debugging-port"))
    );
    let socket = env.data.join("run").join(format!("{id}.sock"));
    assert!(socket.exists());
    assert_eq!(
        fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(env.data.join("profiles").join(&id))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    env.ok(&["stop", &id]);
    assert_eq!(env.state(&id), "stopped");
    wait_for(&env.path("ended"));
    assert!(!socket.exists());
    // It can be opened again.
    fs::remove_file(env.path("ended")).unwrap();
    assert!(env.open(&id).status.success());
    env.ok(&["stop", &id]);
}

#[test]
fn second_open_is_refused_and_first_keeps_running() {
    let env = Env::new();
    let id = env.create("twice");
    let spawn = || {
        env.cmd()
            .args(["open", &id, "--browser"])
            .arg(&env.browser)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let (mut a, mut b) = (spawn(), spawn());
    let results = [a.wait().unwrap().success(), b.wait().unwrap().success()];
    assert_eq!(results.iter().filter(|ok| **ok).count(), 1, "{results:?}");
    wait_for(&env.path("supervisor-pid"));
    let supervisor = fs::read_to_string(env.path("supervisor-pid")).unwrap();
    env.fails(&["open", &id, "--browser", "/bin/sh"]);
    assert_eq!(env.state(&id), "running");
    assert!(
        Command::new("kill")
            .args(["-0", supervisor.trim()])
            .status()
            .unwrap()
            .success()
    );
    assert!(!env.path("ended").exists());
    env.ok(&["stop", &id]);
}

#[test]
fn killed_supervisor_requires_recovery() {
    let env = Env::new();
    let id = env.create("crash");
    assert!(env.open(&id).status.success());
    let request = env.ok(&["wait-auth", &id]);
    let request = request.trim().split('\t').nth(2).unwrap().to_owned();
    wait_for(&env.path("supervisor-pid"));
    let pid = fs::read_to_string(env.path("supervisor-pid")).unwrap();
    assert!(
        Command::new("kill")
            .args(["-9", pid.trim()])
            .status()
            .unwrap()
            .success()
    );
    env.wait_for_state(&id, "recovery_required");
    assert!(
        env.fails(&["open", &id, "--browser", "/bin/sh"])
            .contains("recover")
    );
    env.fails(&["resume", &id, &request]);
    env.fails(&["wait-auth", &id]);
    env.fails(&["stop", &id]);
    env.fails(&["recover", &id]);
    assert_eq!(env.state(&id), "recovery_required");
    wait_for(&env.path("ended"));
    assert_eq!(
        env.ok(&["recover", &id, "--confirm-browser-closed"]),
        format!("{id}\tstopped\n")
    );
    assert_eq!(env.state(&id), "stopped");
    env.fails(&["recover", &id, "--confirm-browser-closed"]);
    fs::remove_file(env.path("ended")).unwrap();
    assert!(env.open(&id).status.success());
    env.ok(&["stop", &id]);
}

#[test]
fn browser_exit_records_stopped_and_cleans_up() {
    let env = Env::new();
    let id = env.create("closed");
    assert!(env.open(&id).status.success());
    fs::write(env.path("close"), "").unwrap();
    env.wait_for_state(&id, "stopped");
    let socket = env.data.join("run").join(format!("{id}.sock"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while socket.exists() {
        assert!(Instant::now() < deadline, "socket was not removed");
        thread::sleep(Duration::from_millis(20));
    }
    // The lock is free again: recovery is refused only because nothing needs recovering.
    assert!(
        env.fails(&["recover", &id, "--confirm-browser-closed"])
            .contains("already stopped")
    );
}

#[test]
fn handoff_wait_resume() {
    let env = Env::new();
    let id = env.create("auth");
    env.fails(&["wait-auth", &id]);
    assert!(env.open(&id).status.success());
    env.fails(&["handoff", &id]);
    env.fails(&["wait-auth", &id, "--origin", "https://example.com/login"]);
    let first = env.ok(&["wait-auth", &id, "--origin", "https://example.com"]);
    let fields: Vec<_> = first.trim().split('\t').collect();
    assert_eq!(fields[..2], [id.as_str(), "awaiting_user"]);
    let first_request = fields[2].to_owned();
    assert_eq!(
        env.ok(&["handoff", &id]),
        format!("{id}\tawaiting_user\t{first_request}\thttps://example.com\n")
    );
    assert_eq!(env.state(&id), "awaiting_user");
    let second = env.ok(&["wait-auth", &id]);
    let second_request = second.trim().split('\t').nth(2).unwrap().to_owned();
    assert_ne!(first_request, second_request);
    assert_eq!(
        env.ok(&["handoff", &id]),
        format!("{id}\tawaiting_user\t{second_request}\torigin not provided\n")
    );
    env.fails(&["resume", &id, &first_request]);
    env.fails(&["resume", &id, "not-a-request"]);
    assert_eq!(env.state(&id), "awaiting_user");
    assert_eq!(
        env.ok(&["resume", &id, &second_request]),
        format!("{id}\trunning\n")
    );
    assert_eq!(env.state(&id), "running");
    env.fails(&["resume", &id, &second_request]);
    // Stop works from awaiting_user too.
    env.ok(&["wait-auth", &id]);
    env.ok(&["stop", &id]);
    assert_eq!(env.state(&id), "stopped");
}

#[test]
fn unknown_sessions_and_bad_urls_are_rejected() {
    let env = Env::new();
    let id = env.create("bad");
    env.fails(&[
        "open",
        "00000000-0000-4000-8000-000000000000",
        "--browser",
        "/bin/sh",
    ]);
    env.fails(&["open", "not-an-id"]);
    let out = env
        .cmd()
        .args(["open", &id, "--url", "http://example.com", "--browser"])
        .arg(&env.browser)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(env.state(&id), "stopped");
    env.fails(&["open", &id, "--browser", "/nonexistent/browser"]);
    assert_eq!(env.state(&id), "stopped");
}

#[test]
fn profile_symlinks_and_files_are_not_followed() {
    for case in ["linked-parent", "linked-profile", "file-profile"] {
        let env = Env::new();
        let id = env.create("symlink");
        let outside = env.path("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"preserve").unwrap();
        fs::create_dir_all(&env.data).unwrap();
        let profiles = env.data.join("profiles");
        match case {
            "linked-parent" => std::os::unix::fs::symlink(&outside, &profiles).unwrap(),
            "linked-profile" => {
                fs::create_dir(&profiles).unwrap();
                std::os::unix::fs::symlink(&outside, profiles.join(&id)).unwrap();
            }
            _ => {
                fs::create_dir(&profiles).unwrap();
                fs::write(profiles.join(&id), b"keep").unwrap();
            }
        }
        assert!(!env.open(&id).status.success(), "{case}");
        assert_eq!(env.state(&id), "failed", "{case}");
        assert!(!env.path("args").exists(), "{case}: browser was launched");
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"preserve");
        if case == "file-profile" {
            assert_eq!(fs::read(profiles.join(&id)).unwrap(), b"keep");
        }
        env.ok(&["recover", &id, "--confirm-browser-closed"]);
        assert_eq!(env.state(&id), "stopped");
    }
}

#[test]
fn endpoint_only_for_cdp_sessions() {
    let env = Env::new();
    let id = env.create("raw");
    env.fails(&["endpoint", &id]);
    assert!(env.open(&id).status.success());
    assert!(env.fails(&["endpoint", &id]).contains("--cdp"));
    assert!(!env.ok(&["list"]).contains("\tcdp"));
    env.ok(&["stop", &id]);
    fs::remove_file(env.path("ended")).unwrap();
    let out = env
        .cmd()
        .args(["open", &id, "--cdp", "--browser"])
        .arg(&env.browser)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("warning"));
    let args = fs::read_to_string(env.path("args")).unwrap();
    assert!(
        args.lines().any(|l| l == "--remote-debugging-port=0"),
        "{args}"
    );
    assert!(args.lines().any(|l| l == "--remote-debugging-pipe"));
    assert!(env.ok(&["list"]).contains("\trunning\tcdp"));
    assert_eq!(
        env.ok(&["endpoint", &id]),
        "ws://127.0.0.1:12345/devtools/browser/11111111-1111-4111-8111-111111111111\n"
    );
    // Not while the user is authenticating.
    let request = env.ok(&["wait-auth", &id]);
    env.fails(&["endpoint", &id]);
    let request = request.trim().split('\t').nth(2).unwrap().to_owned();
    env.ok(&["resume", &id, &request]);
    env.ok(&["endpoint", &id]);
    env.ok(&["stop", &id]);
    env.fails(&["endpoint", &id]);
}

#[test]
fn receive_waits_for_a_site_handed_over_and_ignores_grants_a_person_typed() {
    use keyharbor_domain::{
        SessionId,
        grant::{Grant, GrantId},
        ports::GrantStore,
        vault::Origin,
    };
    let env = Env::new();
    let id = env.create("work");
    let session = SessionId::parse(&id).unwrap();
    let grant = |created_by: &str, origin: &str| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        keyharbor_sqlite::SqliteStore::open(&env.data)
            .unwrap()
            .grant_add(&Grant {
                id: GrantId::from_bytes(rand_bytes(created_by, origin)),
                session,
                origin: Origin::parse(origin).unwrap(),
                until: Some(now + 600),
                created_by: created_by.into(),
                reason: None,
                created_at: now,
                revoked_at: None,
                cleared: false,
            })
            .unwrap();
    };
    let nothing = env.fails(&["receive", &id, "--timeout-seconds", "1"]);
    assert!(nothing.contains("no site was handed over"), "{nothing}");

    let waiting = env
        .cmd()
        .args([
            "receive",
            &id,
            "--origin",
            "https://github.com",
            "--timeout-seconds",
            "10",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(1500));
    grant("cli", "https://github.com");
    grant("handoff", "https://bank.example");
    thread::sleep(Duration::from_millis(1500));
    grant("handoff", "https://github.com");
    let out = waiting.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("{id}\treceived\thttps://github.com\n")
    );
}

/// Distinct ids for the test's grants (the store wants them unique, not random).
fn rand_bytes(a: &str, b: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, byte) in a.bytes().chain(b.bytes()).enumerate() {
        out[i % 16] ^= byte.wrapping_add(i as u8);
    }
    out[6] = (out[6] & 0x0f) | 0x40;
    out[8] = (out[8] & 0x3f) | 0x80;
    out
}
