//! The bridge to the browser extension (drag to hand off, spec 2026-10-31): while the extension
//! keeps a native connection open, the host listens on `run/extension.sock`, and the admin app
//! asks it to hand a site to a session. An ask carries a session name and a URL; an answer a
//! code and a count. No value crosses here: the extension hands the site over through the usual
//! `receive_site`, after the person presses [넘기기] in their browser.

use crate::paths;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::Duration,
};

/// How long the app waits: the person has to notice the browser notification and press it.
pub const BRIDGE_WAIT: Duration = Duration::from_secs(130);

pub fn bridge_socket(data: &Path) -> PathBuf {
    paths::run_dir(data).join("extension.sock")
}

/// What the app asks for: hand the site of `url` to the session named `session`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteAsk {
    pub session: String,
    pub url: String,
}

/// How an ask ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SiteAnswer {
    /// The person pressed [넘기기]; `cookies` went over.
    HandedOff {
        cookies: u64,
    },
    /// The person dismissed the notification, or it timed out in the browser.
    Declined,
    /// The extension has no access to that site yet: allow it once from the popup.
    NoPermission,
    /// No running session has that name.
    NoSession,
    /// The URL is not an http(s) site.
    BadSite,
    /// No answer came in time.
    Timeout,
    Failed,
}

/// Opens the bridge socket: `run/` private (0700), a stale socket replaced, the socket 0600.
pub fn listen(data: &Path) -> Result<std::os::unix::net::UnixListener> {
    std::fs::create_dir_all(data).context("create the data directory")?;
    paths::private_dir(&paths::run_dir(data))?;
    let socket = bridge_socket(data);
    let _ = std::fs::remove_file(&socket);
    let listener =
        std::os::unix::net::UnixListener::bind(&socket).context("open the extension bridge")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Sends one ask to the bridge and waits for its answer.
pub fn ask(data: &Path, ask: &SiteAsk) -> Result<SiteAnswer> {
    let socket = bridge_socket(data);
    let stream = match std::os::unix::net::UnixStream::connect(&socket) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            bail!(
                "the KeyHarbor extension is not connected; open Chrome with the extension installed"
            )
        }
        Err(error) => return Err(error).context("connect to the extension bridge"),
    };
    stream.set_read_timeout(Some(BRIDGE_WAIT))?;
    let mut line = serde_json::to_string(ask)?;
    line.push('\n');
    (&stream).write_all(line.as_bytes())?;
    let mut answer = String::new();
    BufReader::new(&stream)
        .read_line(&mut answer)
        .context("no answer from the extension bridge")?;
    serde_json::from_str(answer.trim()).context("the extension bridge answered something else")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn an_ask_goes_out_as_one_line_and_the_answer_comes_back() {
        let tmp = tempfile::Builder::new()
            .prefix("kh")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::create_dir_all(paths::run_dir(tmp.path())).unwrap();
        let listener = UnixListener::bind(bridge_socket(tmp.path())).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            (&stream)
                .write_all(b"{\"outcome\":\"handed_off\",\"cookies\":3}\n")
                .unwrap();
            line
        });
        let sent = SiteAsk {
            session: "work".into(),
            url: "https://github.com/x".into(),
        };
        assert_eq!(
            ask(tmp.path(), &sent).unwrap(),
            SiteAnswer::HandedOff { cookies: 3 }
        );
        let line = server.join().unwrap();
        assert_eq!(serde_json::from_str::<SiteAsk>(line.trim()).unwrap(), sent);
    }

    #[test]
    fn without_the_extension_the_ask_says_so() {
        let tmp = tempfile::Builder::new()
            .prefix("kh")
            .tempdir_in("/tmp")
            .unwrap();
        let err = ask(
            tmp.path(),
            &SiteAsk {
                session: "w".into(),
                url: "https://a.example".into(),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("extension is not connected"), "{err}");
    }

    #[test]
    fn answers_read_as_their_codes() {
        for (text, answer) in [
            (r#"{"outcome":"declined"}"#, SiteAnswer::Declined),
            (r#"{"outcome":"no_permission"}"#, SiteAnswer::NoPermission),
            (r#"{"outcome":"timeout"}"#, SiteAnswer::Timeout),
        ] {
            assert_eq!(serde_json::from_str::<SiteAnswer>(text).unwrap(), answer);
            assert_eq!(serde_json::to_string(&answer).unwrap(), text);
        }
    }
}
