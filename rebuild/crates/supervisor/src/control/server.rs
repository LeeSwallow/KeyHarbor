//! The control socket's server side: reads one request line, answers one response line.

use super::wire::ControlResponse;
use crate::process::Shared;
use serde_json::Value;
use std::sync::Arc;
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
            ControlResponse::Err("request too large".into())
        } else {
            match serde_json::from_str::<Value>(&line) {
                Err(_) => ControlResponse::Err("invalid request".into()),
                Ok(request) => {
                    let mut ops = shared.ops.lock().await;
                    match ops.execute(&shared, &request).await {
                        Ok(result) => ControlResponse::Ok(result),
                        Err(error) => ControlResponse::Err(format!("{error:#}")),
                    }
                }
            }
        };
        let Ok(mut out) = serde_json::to_string(&response) else {
            return;
        };
        out.push('\n');
        if write.write_all(out.as_bytes()).await.is_err() {
            return;
        }
    }
}
