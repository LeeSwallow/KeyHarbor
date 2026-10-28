//! Tabs: listing, opening and switching the active tab.

use super::CdpBrowser;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

impl CdpBrowser {
    pub(super) async fn list_tabs(&mut self) -> Result<Value> {
        let tabs = self.page_tabs().await?;
        let active = match &self.active {
            Some(id) if tabs.iter().any(|t| &t.id == id) => Some(id.clone()),
            _ => tabs.first().map(|t| t.id.clone()),
        };
        Ok(Value::Array(
            tabs.into_iter()
                .map(|t| {
                    let is_active = Some(&t.id) == active.as_ref();
                    json!({"id": t.id, "url": t.url, "title": t.title, "active": is_active})
                })
                .collect(),
        ))
    }

    pub(super) async fn open_tab(&mut self, url: Option<&str>) -> Result<Value> {
        let url = match url {
            Some(url) => {
                keyharbor_domain::validate_navigation_url(url).map_err(|e| anyhow!("{e}"))?
            }
            None => "about:blank".to_owned(),
        };
        let created = self
            .cdp
            .call("Target.createTarget", json!({"url": url}), None)
            .await?;
        let id = created["targetId"]
            .as_str()
            .context("no target id")?
            .to_owned();
        self.activate(&id).await?;
        Ok(json!({"id": id}))
    }

    pub(super) async fn switch_to(&mut self, id: &str) -> Result<Value> {
        if !self.page_tabs().await?.iter().any(|t| t.id == id) {
            bail!("no such tab");
        }
        self.activate(id).await?;
        Ok(json!({}))
    }

    async fn activate(&mut self, id: &str) -> Result<()> {
        self.cdp
            .call("Target.activateTarget", json!({"targetId": id}), None)
            .await?;
        self.attach(id).await?;
        self.active = Some(id.to_owned());
        Ok(())
    }
}
