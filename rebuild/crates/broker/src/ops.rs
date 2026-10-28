//! Browser operations. Every op's input text and every response passes through here.

use crate::{cdp::Cdp, supervisor::Shared};
use anyhow::{Result, bail};
use serde_json::{Value, json};

pub(crate) struct Ops {
    cdp: Cdp,
}

impl Ops {
    pub fn new(cdp: Cdp) -> Self {
        Self { cdp }
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
                self.cdp.send_and_close("Browser.close", json!({}));
                shared.request_close();
                Ok(json!({}))
            }
            Some(_) => bail!("unknown operation"),
            None => bail!("request has no op"),
        }
    }
}
