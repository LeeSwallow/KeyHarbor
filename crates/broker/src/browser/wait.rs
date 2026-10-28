//! `wait`: for a selector to match, or for a fixed time.

use super::{CdpBrowser, POLL, WAIT_TIMEOUT};
use anyhow::{Result, bail};
use keyharbor_domain::ports::browser::WaitFor;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

impl CdpBrowser {
    pub(super) async fn wait_for(&mut self, until: &WaitFor) -> Result<Value> {
        let ms = match until {
            WaitFor::Selector(selector) => return self.wait_selector(selector).await,
            WaitFor::Millis(ms) => *ms,
        };
        tokio::time::sleep(Duration::from_millis(ms).min(WAIT_TIMEOUT)).await;
        Ok(json!({}))
    }

    async fn wait_selector(&mut self, selector: &str) -> Result<Value> {
        let expression = format!(
            "document.querySelector({}) !== null",
            serde_json::to_string(selector)?
        );
        let deadline = Instant::now() + WAIT_TIMEOUT;
        loop {
            // The page may be between documents; errors mean "not yet".
            if let Ok(session) = self.active_page().await {
                let found = self
                    .call(
                        "Runtime.evaluate",
                        json!({"expression": expression, "returnByValue": true}),
                        &session,
                    )
                    .await;
                if found.is_ok_and(|v| v["result"]["value"] == true) {
                    return Ok(json!({}));
                }
            }
            if Instant::now() >= deadline {
                bail!("timed out waiting for the selector");
            }
            tokio::time::sleep(POLL).await;
        }
    }
}
