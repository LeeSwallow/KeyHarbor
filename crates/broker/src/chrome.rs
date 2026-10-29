//! Starting Chrome with the CDP pipe on fds 3/4, and what Chrome publishes about itself
//! (`DevToolsActivePort` for a `--remote-debugging-port` browser).

use crate::{browser::CdpBrowser, cdp::Cdp};
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::{path::PathBuf, process::Stdio, sync::Arc};
use tokio::process::Child;

/// The browser `open` uses when none is given.
pub const DEFAULT_BROWSER: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/// The file in the profile where Chrome writes its debugging port and browser path.
pub const DEVTOOLS_ACTIVE_PORT: &str = "DevToolsActivePort";

/// How to start one browser. The paths are prepared by the caller.
pub struct ChromeLaunch {
    pub executable: PathBuf,
    /// Chrome's `--user-data-dir`, a private directory.
    pub profile: PathBuf,
    /// Already validated by `validate_start_url`.
    pub url: Option<String>,
    /// Also open a `--remote-debugging-port` (raw CDP, `open --cdp`).
    pub debug_port: bool,
    pub headless: bool,
}

/// A started browser: its process and its CDP pipe.
pub struct Chrome {
    pub process: Child,
    pub link: CdpLink,
}

/// The CDP pipe of one browser. Cloning shares the pipe.
#[derive(Clone)]
pub struct CdpLink(Arc<Cdp>);

impl CdpLink {
    pub(crate) fn cdp(&self) -> Arc<Cdp> {
        self.0.clone()
    }

    /// The raw browser adapter over this pipe. Must be called inside a Tokio runtime.
    /// Screenshots are written into `shots`, a private directory that already exists, as
    /// `<prefix>-<n>.png`.
    pub fn browser(&self, shots: PathBuf, prefix: String) -> CdpBrowser {
        CdpBrowser::new(self.0.clone(), shots, prefix)
    }

    /// Returns `true` once the browser's last page has closed, after asking the browser to
    /// exit (on macOS Chrome keeps running with no windows). Returns `false` if the pipe ends
    /// or target discovery fails first.
    pub async fn last_page_closed(&self) -> bool {
        let cdp = &self.0;
        let mut events = cdp.events();
        if cdp
            .call("Target.setDiscoverTargets", json!({"discover": true}), None)
            .await
            .is_err()
        {
            return false;
        }
        loop {
            match events.recv().await {
                Ok(event) if event["method"] == "Target.targetDestroyed" => {
                    let Ok(targets) = cdp.call("Target.getTargets", json!({}), None).await else {
                        return false;
                    };
                    let pages = targets["targetInfos"]
                        .as_array()
                        .is_some_and(|t| t.iter().any(|t| t["type"] == "page"));
                    if !pages {
                        cdp.send_and_close("Browser.close", json!({}));
                        return true;
                    }
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
                Err(_) => return false,
            }
        }
    }
}

/// Starts Chrome with the CDP pipe on fds 3 (to Chrome) and 4 (from Chrome). Must be called
/// inside a Tokio runtime.
pub fn spawn(launch: &ChromeLaunch) -> Result<Chrome> {
    use std::os::fd::{AsRawFd, OwnedFd};
    let (to_chrome_read, to_chrome_write) = std::io::pipe()?;
    let (from_chrome_read, from_chrome_write) = std::io::pipe()?;
    let mut command = tokio::process::Command::new(&launch.executable);
    command
        .arg(format!("--user-data-dir={}", launch.profile.display()))
        .arg("--remote-debugging-pipe")
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if launch.headless {
        command.arg("--headless=new");
    }
    if launch.debug_port {
        command.arg("--remote-debugging-port=0");
    }
    if let Some(url) = &launch.url {
        command.arg(url);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let (read_fd, write_fd) = (to_chrome_read.as_raw_fd(), from_chrome_write.as_raw_fd());
    // SAFETY: only async-signal-safe calls (fcntl, dup2) run between fork and exec.
    unsafe {
        command.pre_exec(move || {
            // Copy above 4 first so dup2 never sees source == target (which would keep CLOEXEC).
            let read_copy = libc::fcntl(read_fd, libc::F_DUPFD_CLOEXEC, 10);
            let write_copy = libc::fcntl(write_fd, libc::F_DUPFD_CLOEXEC, 10);
            if read_copy < 0
                || write_copy < 0
                || libc::dup2(read_copy, 3) < 0
                || libc::dup2(write_copy, 4) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let process = command
        .spawn()
        .with_context(|| format!("start browser {}", launch.executable.display()))?;
    drop((to_chrome_read, from_chrome_write));
    let sender = tokio::net::unix::pipe::Sender::from_owned_fd(OwnedFd::from(to_chrome_write))?;
    let receiver =
        tokio::net::unix::pipe::Receiver::from_owned_fd(OwnedFd::from(from_chrome_read))?;
    Ok(Chrome {
        process,
        link: CdpLink(Arc::new(Cdp::new(sender, receiver))),
    })
}

/// The raw CDP WebSocket address in a `DevToolsActivePort` file, checked for shape.
pub fn parse_devtools_active_port(text: &str) -> Result<String> {
    let mut lines = text.lines();
    let port: u16 = lines
        .next()
        .and_then(|p| p.parse().ok())
        .filter(|p| *p != 0)
        .context("invalid debug port")?;
    let path = lines.next().context("missing browser endpoint")?;
    let browser = path
        .strip_prefix("/devtools/browser/")
        .context("invalid browser endpoint")?;
    uuid_like(browser)?;
    if lines.next().is_some() {
        bail!("invalid browser endpoint");
    }
    Ok(format!("ws://127.0.0.1:{port}{path}"))
}

fn uuid_like(value: &str) -> Result<()> {
    let ok = value.len() == 36
        && value.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    if !ok {
        bail!("invalid browser endpoint");
    }
    Ok(())
}
