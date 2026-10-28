//! The control socket's client side: what the MCP server, `ProcessRuntime` and tests call.

use super::wire::{ControlEnvelope, ControlResponse};
use crate::paths;
use anyhow::{Context, Result, bail};
use keyharbor_domain::SessionId;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    time::Duration,
};

/// How long a client waits for one answer (a `wait` or `navigate` can take a while).
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

/// Sends one request to the session's supervisor and returns its `result`.
pub fn request(data: &Path, id: &SessionId, request: &ControlEnvelope) -> Result<Value> {
    call(data, id, &serde_json::to_value(request)?)
}

/// Sends one already-shaped JSON request. Clients use [`request`]; this is for tests that
/// probe how the server handles malformed requests.
pub fn call(data: &Path, id: &SessionId, request: &Value) -> Result<Value> {
    let socket = paths::socket_path(data, id);
    let stream = match std::os::unix::net::UnixStream::connect(&socket) {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            bail!("supervisor is not running")
        }
        Err(error) => return Err(error).context("connect to supervisor"),
    };
    stream.set_read_timeout(Some(ANSWER_TIMEOUT))?;
    let mut line = request.to_string();
    line.push('\n');
    (&stream).write_all(line.as_bytes())?;
    let mut response = String::new();
    BufReader::new(&stream)
        .read_line(&mut response)
        .context("read supervisor response")?;
    if response.is_empty() {
        bail!("supervisor closed the connection");
    }
    match serde_json::from_str(&response).context("invalid supervisor response")? {
        ControlResponse::Ok(result) => Ok(result),
        ControlResponse::Err(error) => bail!("{error}"),
    }
}
