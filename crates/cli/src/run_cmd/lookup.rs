//! Channel ④ (developer secrets spec §3, J-122): a Unix socket in a private folder made for
//! this run, which the child finds through [`SOCKET_VAR`] and must show [`TOKEN_VAR`] to.
//! This module only carries requests; the guard (`guard::run::Lookup`) decides.
//!
//! One request per connection, one JSON line each way:
//! `{"token": "…", "item": "myapp/dev/db", "field": "password"}` →
//! `{"value": "…"}` or `{"error": "denied: myapp/dev/db (policy)"}`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use zeroize::Zeroizing;

/// The child's variable holding the socket's path.
pub(super) const SOCKET_VAR: &str = "KEYHARBOR_LOOKUP_SOCKET";
/// The child's variable holding the run's token.
pub(super) const TOKEN_VAR: &str = "KEYHARBOR_LOOKUP_TOKEN";

/// The longest request line read. Names are at most 64 characters and the token 64, so a
/// real request is far shorter; a longer one is refused unread.
const MAX_REQUEST: u64 = 4096;
/// How long one connection may take to send its request or read the answer. Requests are
/// answered one at a time, so a client that stalls holds the others up this long at most.
const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// One request as it arrives on the socket.
#[derive(Deserialize)]
pub(super) struct LookupRequest {
    pub token: String,
    pub item: String,
    pub field: String,
}

/// One answer: a value or why there is none.
#[derive(Serialize)]
pub(super) struct LookupResponse<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

/// What the server does with a request: the value, or the refusal's text.
pub(super) type Answer = std::result::Result<Zeroizing<String>, String>;

/// The run's socket; dropping it removes the socket and its folder.
pub(super) struct LookupSocket {
    listener: UnixListener,
    path: PathBuf,
    /// Held only so that dropping the socket removes the folder.
    _dir: tempfile::TempDir,
}

impl LookupSocket {
    pub(super) fn open() -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("keyharbor-lookup-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .context("could not make a private folder for the lookup socket")?;
        let path = dir.path().join("s");
        let listener = UnixListener::bind(&path).context("could not open the lookup socket")?;
        Ok(Self {
            listener,
            path,
            _dir: dir,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Answers requests one at a time until `stop` is set and [`Self::wake`] is called.
    pub(super) fn serve(
        &self,
        stop: &AtomicBool,
        mut answer: impl FnMut(&LookupRequest) -> Answer,
    ) {
        for stream in self.listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            if let Ok(stream) = stream {
                let _ = handle(stream, &mut answer);
            }
        }
    }

    /// Unblocks [`Self::serve`] so that it sees `stop`.
    pub(super) fn wake(&self) {
        let _ = UnixStream::connect(&self.path);
    }
}

fn handle(stream: UnixStream, answer: &mut impl FnMut(&LookupRequest) -> Answer) -> Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut line = Zeroizing::new(Vec::new());
    BufReader::new((&stream).take(MAX_REQUEST)).read_until(b'\n', &mut line)?;
    let reply = match serde_json::from_slice::<LookupRequest>(&line) {
        Ok(request) => answer(&request),
        Err(_) => Err("bad request".to_owned()),
    };
    let mut out = Zeroizing::new(response(&reply)?);
    out.push(b'\n');
    (&stream).write_all(&out)?;
    Ok(())
}

/// The JSON line for `reply` (it may hold a value, so the caller zeroizes it).
fn response(reply: &Answer) -> Result<Vec<u8>> {
    let wire = match reply {
        Ok(value) => LookupResponse {
            value: Some(value.as_str()),
            error: None,
        },
        Err(error) => LookupResponse {
            value: None,
            error: Some(error),
        },
    };
    Ok(serde_json::to_vec(&wire)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(path: &Path, line: &str) -> serde_json::Value {
        let mut stream = UnixStream::connect(path).unwrap();
        stream.write_all(line.as_bytes()).unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap()
    }

    #[test]
    fn requests_are_carried_and_the_folder_goes_with_the_socket() {
        let socket = LookupSocket::open().unwrap();
        let (path, dir) = (
            socket.path().to_owned(),
            socket.path().parent().unwrap().to_owned(),
        );
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                socket.serve(&stop, |r| {
                    if r.token == "t" {
                        Ok(Zeroizing::new(format!("{}.{}", r.item, r.field)))
                    } else {
                        Err("denied (bad-token)".to_owned())
                    }
                })
            });
            let ok = ask(
                &path,
                "{\"token\":\"t\",\"item\":\"a/b\",\"field\":\"F\"}\n",
            );
            assert_eq!(ok, serde_json::json!({"value": "a/b.F"}));
            let no = ask(&path, "{\"token\":\"x\",\"item\":\"a\",\"field\":\"F\"}\n");
            assert_eq!(no, serde_json::json!({"error": "denied (bad-token)"}));
            let bad = ask(&path, "not json\n");
            assert_eq!(bad, serde_json::json!({"error": "bad request"}));
            stop.store(true, Ordering::SeqCst);
            socket.wake();
        });
        drop(socket);
        assert!(!path.exists() && !dir.exists());
    }
}
