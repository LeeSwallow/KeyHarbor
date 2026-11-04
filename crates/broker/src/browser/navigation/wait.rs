//! `wait`: for a selector to match, or for a fixed time.

use crate::browser::CdpBrowser;
use anyhow::{Result, bail};
use keyharbor_domain::ports::browser::WaitFor;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

/// The longest `wait` waits, for a selector or a fixed time.
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// How often `wait` asks the page for the selector (each poll is a browser call).
const WAIT_POLL: Duration = Duration::from_millis(100);

impl CdpBrowser {
    pub(crate) async fn wait_for(&mut self, until: &WaitFor) -> Result<Value> {
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
            tokio::time::sleep(WAIT_POLL).await;
        }
    }
}
