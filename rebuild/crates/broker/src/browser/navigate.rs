//! `navigate`: load a URL in the active tab and wait for its load event.

use super::{CdpBrowser, LOAD_TIMEOUT};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use tokio::time::Instant;

impl CdpBrowser {
    pub(super) async fn navigate_to(&mut self, url: &str) -> Result<Value> {
        let url = keyharbor_domain::validate_navigation_url(url).map_err(|e| anyhow!("{e}"))?;
        let session = self.active_page().await?;
        let mut events = self.cdp.events();
        let navigated = self
            .call("Page.navigate", json!({"url": url}), &session)
            .await?;
        if let Some(error) = navigated["errorText"].as_str().filter(|e| !e.is_empty()) {
            bail!("navigation failed: {error}");
        }
        let deadline = Instant::now() + LOAD_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, events.recv())
                .await
                .map_err(|_| anyhow!("page did not finish loading within 15 seconds"))?;
            match event {
                Ok(event)
                    if event["method"] == "Page.loadEventFired"
                        && event["sessionId"] == session.as_str() =>
                {
                    break;
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
                Err(_) => bail!("browser connection closed"),
            }
        }
        let (url, title) = self.page_info(&session).await?;
        Ok(json!({"url": url, "title": title}))
    }
}
