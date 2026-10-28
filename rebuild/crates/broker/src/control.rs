//! The control socket: one JSON request per line, one JSON response per line.
//!
//! Request `{"op":"...", ...}` → `{"ok":true,"result":...}` or `{"ok":false,"error":"..."}`.

use crate::{paths, supervisor::Shared};
use anyhow::{Context, Result, bail};
use keyharbor_domain::SessionId;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// Requests larger than this are refused.
const MAX_REQUEST: usize = 1 << 20;

pub(crate) async fn serve_connection(stream: tokio::net::UnixStream, shared: Arc<Shared>) {
    let (read, mut write) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(read);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => (),
        }
        let response = if line.len() > MAX_REQUEST {
            json!({"ok": false, "error": "request too large"})
        } else {
            match serde_json::from_str::<Value>(&line) {
                Err(_) => json!({"ok": false, "error": "invalid request"}),
                Ok(request) => {
                    let mut ops = shared.ops.lock().await;
                    match ops.execute(&shared, &request).await {
                        Ok(result) => json!({"ok": true, "result": result}),
                        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
                    }
                }
            }
        };
        let mut out = response.to_string();
        out.push('\n');
        if write.write_all(out.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// Sends one request to the session's supervisor and returns its `result`.
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
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
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
    let mut response: Value =
        serde_json::from_str(&response).context("invalid supervisor response")?;
    if response["ok"] == true {
        Ok(response["result"].take())
    } else {
        bail!(
            "{}",
            response["error"].as_str().unwrap_or("supervisor error")
        )
    }
}
