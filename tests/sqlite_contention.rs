use keyharbor::Store;
use std::{
    collections::HashSet,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn concurrent_processes_create_unique_sessions_and_reject_duplicate_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    let mut children = Vec::new();
    for index in 0..24 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_keyharbor"))
                .arg("--data-dir")
                .arg(&root)
                .args(["create", &format!("task-{index}")])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    let mut ids = HashSet::new();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(ids.insert(stdout.split('\t').next().unwrap().to_owned()));
    }
    assert_eq!(store.list().unwrap().len(), 24);

    let mut contenders = Vec::new();
    for _ in 0..12 {
        contenders.push(
            Command::new(env!("CARGO_BIN_EXE_keyharbor"))
                .arg("--data-dir")
                .arg(&root)
                .args(["create", "shared-name"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    let successes = contenders
        .into_iter()
        .map(|mut child| child.wait().unwrap().success())
        .filter(|ok| *ok)
        .count();
    assert_eq!(successes, 1);
    assert_eq!(store.list().unwrap().len(), 25);
}

#[test]
fn writer_contention_fails_without_partial_change_and_can_retry() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("data");
    let store = Store::open(&root).unwrap();
    store.create("existing").unwrap();
    let writer = rusqlite::Connection::open(root.join("keyharbor.sqlite3")).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&root)
        .args(["create", "contended"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(start.elapsed() < Duration::from_secs(20));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .to_lowercase()
            .contains("locked")
    );
    assert_eq!(store.list().unwrap().len(), 1);
    writer.execute_batch("ROLLBACK").unwrap();
    let retry = Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(&root)
        .args(["create", "contended"])
        .output()
        .unwrap();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert_eq!(store.list().unwrap().len(), 2);
}
