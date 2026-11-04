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

/// How long a client waits for one answer (a `wait` or `navigate` can take a while). Fixed:
/// the `browser_call_seconds` setting's range stops below it, so a stuck browser call fails
/// in the supervisor with its own reason before the client stops listening (J-272, checked
/// by `the_answer_wait_outlasts_the_longest_browser_wait`).
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

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::settings::{BROWSER_CALL_SECONDS, SettingSpec};

    /// The longest CDP call the setting allows, plus the longest page wait a request makes,
    /// still answers before the client gives up (J-272).
    #[test]
    fn the_answer_wait_outlasts_the_longest_browser_wait() {
        let call = SettingSpec::find(BROWSER_CALL_SECONDS).unwrap().range();
        let longest_call = Duration::from_secs(*call.unwrap().end());
        assert!(longest_call + keyharbor_broker::WAIT_TIMEOUT < ANSWER_TIMEOUT);
    }
}
