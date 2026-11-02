//! ego lite (ADR-0018): finding it ([`probe`]) and driving it through a relay that runs inside
//! `ego-browser nodejs`. The supervisor listens on a private socket, starts the relay, and
//! hands the connection to the same CDP client Chrome's pipe uses, so everything above the
//! wire (`CdpBrowser`, the guard, page watching) is shared.

pub mod probe;

use crate::{
    cdp::Cdp,
    chrome::{CdpLink, Chrome},
};
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::{fs, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::net::UnixListener;

/// The relay script; [`script`] fills its configuration in.
const RELAY: &str = include_str!("relay.js");
/// How long ego lite may take to start the relay and connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// The relay looks for new and closed tabs once every this many event polls.
const RECONCILE_EVERY: u32 = 25;

/// How to start one relay. The paths are prepared by the caller.
pub struct EgoLaunch {
    /// The `ego-browser` command.
    pub command: PathBuf,
    /// The private socket the supervisor listens on (`run/<id>.ego`).
    pub socket: PathBuf,
    /// Where the relay's error output goes, for a person diagnosing a failed start.
    pub log: PathBuf,
    /// The ego TaskSpace name (one per session; ego resumes it by name).
    pub space: String,
    /// Already validated by `validate_start_url`.
    pub url: Option<String>,
    /// How often the relay takes page events (setting `ego_poll_ms`).
    pub poll: Duration,
}

/// The relay script with its configuration.
fn script(launch: &EgoLaunch) -> String {
    let config = json!({
        "socket": launch.socket,
        "space": launch.space,
        "pollMs": launch.poll.as_millis() as u64,
        "reconcileEvery": RECONCILE_EVERY,
    });
    RELAY.replace("__CONFIG__", &config.to_string())
}

/// Starts the relay and waits for it to connect. Must be called inside a Tokio runtime.
pub async fn spawn(launch: &EgoLaunch) -> Result<Chrome> {
    let _ = fs::remove_file(&launch.socket);
    let listener = UnixListener::bind(&launch.socket).context("listen for the ego relay")?;
    restrict(&launch.socket)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&launch.log)?;
    restrict(&launch.log)?;
    let mut process = tokio::process::Command::new(&launch.command)
        .args(["nodejs", "-e", &script(launch)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("start {}", launch.command.display()))?;
    let accepted = tokio::select! {
        accepted = listener.accept() => accepted.context("accept the ego relay")?,
        status = process.wait() => {
            let _ = fs::remove_file(&launch.socket);
            bail!("ego relay exited before it connected ({}); see {}", status?, launch.log.display());
        }
        () = tokio::time::sleep(CONNECT_TIMEOUT) => {
            let _ = process.start_kill();
            let _ = fs::remove_file(&launch.socket);
            bail!("ego lite did not start the relay in time; see {}", launch.log.display());
        }
    };
    // One connection only: nobody else may reach the relay through the socket.
    drop(listener);
    let _ = fs::remove_file(&launch.socket);
    let (reader, writer) = accepted.0.into_split();
    let link = CdpLink::from_cdp(Arc::new(Cdp::new(writer, reader)));
    if let Some(url) = &launch.url {
        start_at(&link, url).await?;
    }
    Ok(Chrome { process, link })
}

/// Opens the start page in the relay's first Page.
async fn start_at(link: &CdpLink, url: &str) -> Result<()> {
    let cdp = link.cdp();
    let targets = cdp.call("Target.getTargets", json!({}), None).await?;
    let first = targets["targetInfos"][0]["targetId"]
        .as_str()
        .context("ego lite gave no page")?;
    let attached = cdp
        .call(
            "Target.attachToTarget",
            json!({"targetId": first, "flatten": true}),
            None,
        )
        .await?;
    let session = attached["sessionId"]
        .as_str()
        .context("no ego page session")?;
    cdp.call("Page.navigate", json!({"url": url}), Some(session))
        .await?;
    Ok(())
}

/// Makes `path` readable and writable by its owner only.
fn restrict(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// A fresh private directory for a unit test (the crate takes no test-only dependencies).
#[cfg(test)]
pub(crate) fn test_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kh-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_gets_its_configuration_as_json() {
        let launch = EgoLaunch {
            command: "/x/ego-browser".into(),
            socket: "/d/run/a\"b.ego".into(),
            log: "/d/run/a.log".into(),
            space: "keyharbor work".into(),
            url: None,
            poll: Duration::from_millis(20),
        };
        let text = script(&launch);
        assert!(!text.contains("__CONFIG__"));
        let line = text
            .lines()
            .find(|l| l.starts_with("const CONFIG = "))
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(
            line.trim_start_matches("const CONFIG = ")
                .trim_end_matches(';'),
        )
        .unwrap();
        assert_eq!(config["socket"], "/d/run/a\"b.ego");
        assert_eq!(config["space"], "keyharbor work");
        assert_eq!(config["pollMs"], 20);
    }

    #[tokio::test]
    async fn relay_frames_round_trip() {
        // A stand-in relay: answers one call over the socket with the pipe's framing.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = test_dir("relay");
        let path = dir.join("r.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let client = tokio::spawn({
            let path = path.clone();
            async move {
                let mut stream = tokio::net::UnixStream::connect(path).await.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                let frame: serde_json::Value = serde_json::from_slice(&buf[..n - 1]).unwrap();
                let reply = json!({"id": frame["id"], "result": {"echo": frame["method"]}});
                stream
                    .write_all(format!("{reply}\0").as_bytes())
                    .await
                    .unwrap();
                stream.flush().await.unwrap();
                // Keep the connection open until the caller has the answer.
                let _ = stream.read(&mut buf).await;
            }
        });
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, writer) = stream.into_split();
        let cdp = Cdp::new(writer, reader);
        let out = cdp
            .call("Target.getTargets", json!({}), None)
            .await
            .unwrap();
        assert_eq!(out, json!({"echo": "Target.getTargets"}));
        drop(cdp);
        client.abort();
    }
}
