//! Is ego lite here and answering? `auto` asks this before every open (#120): ego lite's
//! `ego-browser` command must exist and run a trivial script within the probe time.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// The command ego lite installs.
pub const EGO_COMMAND: &str = "ego-browser";
/// Points at another `ego-browser` (tests, or an install outside `PATH`).
pub const EGO_COMMAND_ENV: &str = "KEYHARBOR_EGO_BROWSER";
/// How often the probe looks whether the command finished.
const PROBE_POLL: Duration = Duration::from_millis(50);

/// Where `ego-browser` is: the override, else `PATH`, else ego lite's usual `~/.local/bin`.
pub fn locate() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(EGO_COMMAND_ENV) {
        return Some(PathBuf::from(path));
    }
    let home_bin = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/bin"));
    std::env::var_os("PATH")
        .iter()
        .flat_map(std::env::split_paths)
        .chain(home_bin)
        .map(|dir| dir.join(EGO_COMMAND))
        .find(|candidate| candidate.is_file())
}

/// Whether `command` runs a trivial script successfully within `timeout`. A command that
/// takes longer is killed.
pub fn answers(command: &Path, timeout: Duration) -> bool {
    let Ok(mut child) = Command::new(command)
        .args(["nodejs", "-e", "process.exit(0)"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => thread::sleep(PROBE_POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The `ego-browser` command, if ego lite is here and answers within `timeout`.
pub fn find(timeout: Duration) -> Option<PathBuf> {
    locate().filter(|command| answers(command, timeout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("ego-browser");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn a_command_that_runs_answers() {
        let tmp = super::super::test_dir("probe");
        assert!(answers(&script(&tmp, "exit 0"), Duration::from_secs(5)));
        assert!(!answers(&script(&tmp, "exit 3"), Duration::from_secs(5)));
        assert!(!answers(&tmp.join("missing"), Duration::from_secs(5)));
    }

    #[test]
    fn auto_falls_back_when_probe_times_out() {
        let tmp = super::super::test_dir("slow");
        let slow = script(&tmp, "sleep 10");
        let started = Instant::now();
        assert!(!answers(&slow, Duration::from_millis(300)));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the probe gave up in time"
        );
    }
}
