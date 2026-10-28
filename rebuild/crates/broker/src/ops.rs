//! Browser operations. Every op's input text and every response passes through here.

use crate::supervisor::Shared;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::unix::pipe;

pub(crate) struct Ops {
    writer: Option<pipe::Sender>,
}

impl Ops {
    pub fn new(writer: pipe::Sender, mut reader: pipe::Receiver) -> Self {
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut reader, &mut tokio::io::sink()).await;
        });
        Self {
            writer: Some(writer),
        }
    }

    pub async fn execute(&mut self, shared: &Shared, request: &Value) -> Result<Value> {
        match request.get("op").and_then(Value::as_str) {
            Some("status") => {
                let store = shared
                    .store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
                let row = store.get(shared.id)?;
                Ok(json!({
                    "state": row.as_ref().map(|r| r.state.as_str()),
                    "generation": shared.generation.to_string(),
                }))
            }
            Some("close") => {
                if let Some(mut writer) = self.writer.take() {
                    let _ = writer
                        .write_all(b"{\"id\":1,\"method\":\"Browser.close\"}\0")
                        .await;
                }
                shared.request_close();
                Ok(json!({}))
            }
            Some(_) => bail!("unknown operation"),
            None => bail!("request has no op"),
        }
    }
}
