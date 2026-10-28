#![cfg(unix)]

use keyharbor::Store;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
fn concurrent_cli_open_and_user_auth_handoff() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    let session = store.create("work").unwrap();
    let browser = tmp.path().join("browser");
    let close = tmp.path().join("close");
    fs::write(
        &browser,
        "#!/bin/sh\nwhile [ ! -f \"$KEYHARBOR_TEST_CLOSE\" ]; do sleep 0.1; done\n",
    )
    .unwrap();
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
    let cmd = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_keyharbor"));
        command
            .arg("--data-dir")
            .arg(&root)
            .env("KEYHARBOR_TEST_CLOSE", &close);
        command
    };
    let start = || {
        cmd()
            .args(["open", &session.id, "--browser"])
            .arg(&browser)
            .spawn()
            .unwrap()
    };
    let mut a = start();
    let mut b = start();
    let results = [a.wait().unwrap().success(), b.wait().unwrap().success()];
    assert_eq!(results.iter().filter(|success| **success).count(), 1);
    assert!(
        !cmd()
            .args(["recover", &session.id, "--confirm-browser-closed"])
            .status()
            .unwrap()
            .success()
    );
    let handoff = cmd().args(["wait-auth", &session.id]).output().unwrap();
    assert!(handoff.status.success());
    let handoff = String::from_utf8(handoff.stdout).unwrap();
    let request_id = handoff.trim().split('\t').nth(2).unwrap();
    let current = cmd().args(["handoff", &session.id]).output().unwrap();
    assert!(current.status.success());
    assert_eq!(
        String::from_utf8(current.stdout).unwrap(),
        format!("{}\torigin not provided\n", handoff.trim())
    );
    assert_eq!(store.get(&session.id).unwrap().state, "awaiting_user");
    assert!(
        cmd()
            .args(["resume", &session.id, request_id])
            .status()
            .unwrap()
            .success()
    );
    let second = cmd().args(["wait-auth", &session.id]).output().unwrap();
    assert!(second.status.success());
    let second = String::from_utf8(second.stdout).unwrap();
    let second_request_id = second.trim().split('\t').nth(2).unwrap();
    assert_ne!(request_id, second_request_id);
    assert!(
        !cmd()
            .args(["resume", &session.id, request_id])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(store.get(&session.id).unwrap().state, "awaiting_user");
    assert!(
        cmd()
            .args(["resume", &session.id, second_request_id])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        cmd()
            .args(["stop", &session.id])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(store.get(&session.id).unwrap().state, "stopping");
    fs::write(&close, "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while store.get(&session.id).unwrap().state != "stopped" {
        assert!(
            Instant::now() < deadline,
            "supervisor did not observe browser exit"
        );
        thread::sleep(Duration::from_millis(50));
    }
    assert!(store.reserve(&session.id).is_ok());
}

#[test]
fn crashed_supervisor_does_not_release_the_session() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    let session = store.create("crash").unwrap();
    let browser = tmp.path().join("browser");
    let close = tmp.path().join("close");
    let pid = tmp.path().join("supervisor-pid");
    let ended = tmp.path().join("ended");
    fs::write(&browser, "#!/bin/sh\nprintf '%s' \"$PPID\" > \"$TEST_PID\"\nwhile [ ! -f \"$TEST_CLOSE\" ]; do sleep 0.1; done\ntouch \"$TEST_ENDED\"\n").unwrap();
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_keyharbor"));
        command
            .arg("--data-dir")
            .arg(&root)
            .env("TEST_PID", &pid)
            .env("TEST_CLOSE", &close)
            .env("TEST_ENDED", &ended);
        command
    };
    assert!(
        command()
            .args(["open", &session.id, "--browser"])
            .arg(&browser)
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !pid.exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    let supervisor_pid = fs::read_to_string(&pid).unwrap();
    assert!(
        Command::new("kill")
            .args(["-9", supervisor_pid.trim()])
            .status()
            .unwrap()
            .success()
    );
    while store.supervisor_active(&session.id).unwrap() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    let listed = command().arg("list").output().unwrap();
    assert!(
        String::from_utf8(listed.stdout)
            .unwrap()
            .contains("\trecovery_required")
    );
    assert!(
        !command()
            .args(["wait-auth", &session.id])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        !command()
            .args(["stop", &session.id])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        !command()
            .args(["open", &session.id, "--browser"])
            .arg(&browser)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        !command()
            .args(["recover", &session.id])
            .status()
            .unwrap()
            .success()
    );
    fs::write(&close, "").unwrap();
    while !ended.exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        command()
            .args(["recover", &session.id, "--confirm-browser-closed"])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(store.get(&session.id).unwrap().state, "stopped");
}

#[test]
fn startup_failure_preserves_existing_profile_path() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    let session = store.create("startup").unwrap();
    let profile = store.profile(&session.id).unwrap();
    fs::create_dir_all(profile.parent().unwrap()).unwrap();
    fs::write(&profile, b"keep-existing-data").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&root)
        .args(["open", &session.id, "--browser", "/bin/sh"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(store.get(&session.id).unwrap().state, "failed");
    assert_eq!(fs::read(&profile).unwrap(), b"keep-existing-data");
    assert!(store.reserve(&session.id).is_err());
    store.recover(&session.id).unwrap();
    assert_eq!(fs::read(&profile).unwrap(), b"keep-existing-data");
}

#[test]
fn profile_symlinks_are_rejected_without_launching_browser() {
    for linked_parent in [true, false] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let store = Store::open(&root).unwrap();
        let session = store.create("symlink").unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"preserve").unwrap();
        let profiles = root.join("profiles");
        if linked_parent {
            symlink(&outside, &profiles).unwrap();
        } else {
            fs::create_dir(&profiles).unwrap();
            symlink(&outside, profiles.join(&session.id)).unwrap();
        }
        let launched = tmp.path().join("launched");
        let browser = tmp.path().join("browser");
        fs::write(&browser, "#!/bin/sh\ntouch \"$TEST_LAUNCHED\"\n").unwrap();
        fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
            .arg("--data-dir")
            .arg(&root)
            .env("TEST_LAUNCHED", &launched)
            .args(["open", &session.id, "--browser"])
            .arg(&browser)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert_eq!(store.get(&session.id).unwrap().state, "failed");
        assert!(!launched.exists());
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"preserve");
    }
}

#[test]
fn backup_command_creates_a_readable_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    let session = store.create("snapshot").unwrap();
    let destination = tmp.path().join("snapshot.sqlite3");
    let status = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&root)
        .arg("backup")
        .arg(&destination)
        .status()
        .unwrap();
    assert!(status.success());
    let snapshot = rusqlite::Connection::open(destination).unwrap();
    let name: String = snapshot
        .query_row(
            "SELECT name FROM sessions WHERE id=?1",
            [&session.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "snapshot");
}

#[test]
fn supervisor_keeps_lock_through_temporary_database_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    let session = store.create("database-failure").unwrap();
    let browser = tmp.path().join("browser");
    let close = tmp.path().join("close");
    struct CloseOnDrop(std::path::PathBuf);
    impl Drop for CloseOnDrop {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, "");
        }
    }
    let _close_on_drop = CloseOnDrop(close.clone());
    fs::write(
        &browser,
        "#!/bin/sh\nwhile [ ! -f \"$TEST_CLOSE\" ]; do sleep 0.1; done\n",
    )
    .unwrap();
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&root)
        .env("TEST_CLOSE", &close)
        .args(["open", &session.id, "--browser"])
        .arg(&browser)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let conn = rusqlite::Connection::open(root.join("keyharbor.sqlite3")).unwrap();
    conn.execute_batch("ALTER TABLE sessions RENAME TO unavailable_sessions")
        .unwrap();
    thread::sleep(Duration::from_millis(800));
    assert!(store.supervisor_active(&session.id).unwrap());
    assert!(store.lock_session(&session.id).is_err());
    conn.execute_batch("ALTER TABLE unavailable_sessions RENAME TO sessions")
        .unwrap();
    fs::write(&close, "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while store.get(&session.id).unwrap().state != "stopped" {
        assert!(
            Instant::now() < deadline,
            "supervisor did not record browser exit"
        );
        thread::sleep(Duration::from_millis(50));
    }
}
