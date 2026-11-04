//! `navigate`: load a URL in the active tab and wait for its load event.

use crate::{browser::CdpBrowser, cdp::CONNECTION_CLOSED};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

/// How long `navigate` waits for the page's load event.
const LOAD_TIMEOUT: Duration = Duration::from_secs(15);

impl CdpBrowser {
    pub(crate) async fn navigate_to(&mut self, url: &str) -> Result<Value> {
        let url = keyharbor_domain::validate_navigation_url(url).map_err(|e| anyhow!("{e}"))?;
        let session = self.active_page().await?;
        let mut events = self.cdp.events();
        let navigated = self
            .call("Page.navigate", json!({"url": url}), &session)
            .await?;
        if let Some(error) = navigated["errorText"].as_str().filter(|e| !e.is_empty()) {
            bail!("navigation failed: {error}");
        }
        // Count only the load of this navigation: a page still loading from before (the start
        // page) fires its own load event on the same session. A navigation that commits says
        // so with the loader `Page.navigate` returned; without one, any load will do.
        let loader = navigated["loaderId"].as_str().map(str::to_owned);
        let mut committed = loader.is_none();
        let deadline = Instant::now() + LOAD_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, events.recv())
                .await
                .map_err(|_| {
                    anyhow!(
                        "page did not finish loading within {} seconds",
                        LOAD_TIMEOUT.as_secs()
                    )
                })?;
            match event {
                Ok(event) if event["sessionId"] != session.as_str() => (),
                Ok(event)
                    if event["method"] == "Page.frameNavigated"
                        && event["params"]["frame"]["parentId"].is_null()
                        && event["params"]["frame"]["loaderId"].as_str() == loader.as_deref() =>
                {
                    committed = true;
                }
                Ok(event) if committed && event["method"] == "Page.loadEventFired" => break,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
                Err(_) => bail!(CONNECTION_CLOSED),
            }
        }
        let (url, title) = self.page_info(&session).await?;
        Ok(json!({"url": url, "title": title}))
    }
}
