//! The raw browser adapter: `BrowserPort`, `PageInput` and `FrameInspector` over CDP.
//!
//! Crate-private. It types exactly the text it is given and returns results unmasked; the
//! supervisor only ever reaches it through `keyharbor_guard::GuardedBrowser`.

mod frames;
mod input;
mod navigate;
mod screenshot;
mod snapshot;
mod tabs;
mod wait;

use crate::cdp::Cdp;
use anyhow::{Context, Result};
use keyharbor_domain::{
    SessionId,
    ports::{
        BrowserPort, FrameInspector, PageInput,
        browser::{Element, Focus, Page, WaitFor},
    },
    vault::Origin,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

pub(crate) const LOAD_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const POLL: Duration = Duration::from_millis(100);

pub(crate) struct CdpBrowser {
    cdp: Arc<Cdp>,
    /// Target the page ops act on.
    active: Option<String>,
    /// targetId → flattened CDP sessionId.
    attached: HashMap<String, String>,
    /// The data directory and session, for screenshot files.
    data: PathBuf,
    session: SessionId,
}

struct Tab {
    id: String,
    url: String,
    title: String,
}

impl CdpBrowser {
    pub fn new(cdp: Arc<Cdp>, data: PathBuf, session: SessionId) -> Self {
        Self {
            cdp,
            active: None,
            attached: HashMap::new(),
            data,
            session,
        }
    }

    async fn call(&self, method: &str, params: Value, session: &str) -> Result<Value> {
        self.cdp.call(method, params, Some(session)).await
    }

    async fn page_tabs(&self) -> Result<Vec<Tab>> {
        let targets = self.cdp.call("Target.getTargets", json!({}), None).await?;
        Ok(targets["targetInfos"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|t| t["type"] == "page")
            .map(|t| Tab {
                id: t["targetId"].as_str().unwrap_or_default().to_owned(),
                url: t["url"].as_str().unwrap_or_default().to_owned(),
                title: t["title"].as_str().unwrap_or_default().to_owned(),
            })
            .collect())
    }

    /// The CDP session of the active tab, attaching on first use. Falls back to the first tab
    /// if the active one was closed.
    async fn active_page(&mut self) -> Result<String> {
        let tabs = self.page_tabs().await?;
        self.attached
            .retain(|target, _| tabs.iter().any(|t| &t.id == target));
        let active = match &self.active {
            Some(id) if tabs.iter().any(|t| &t.id == id) => id.clone(),
            _ => tabs
                .first()
                .context("the browser has no open tab")?
                .id
                .clone(),
        };
        self.active = Some(active.clone());
        self.attach(&active).await
    }

    async fn attach(&mut self, target: &str) -> Result<String> {
        if let Some(session) = self.attached.get(target) {
            return Ok(session.clone());
        }
        let attached = self
            .cdp
            .call(
                "Target.attachToTarget",
                json!({"targetId": target, "flatten": true}),
                None,
            )
            .await?;
        let session = attached["sessionId"]
            .as_str()
            .context("attach returned no session")?
            .to_owned();
        self.call("Page.enable", json!({}), &session).await?;
        self.attached.insert(target.to_owned(), session.clone());
        Ok(session)
    }

    async fn page_info(&self, session: &str) -> Result<(String, String)> {
        let value = self
            .call(
                "Runtime.evaluate",
                json!({"expression": "[location.href, document.title]", "returnByValue": true}),
                session,
            )
            .await?;
        let pair = &value["result"]["value"];
        Ok((
            pair[0].as_str().unwrap_or_default().to_owned(),
            pair[1].as_str().unwrap_or_default().to_owned(),
        ))
    }
}

impl BrowserPort for CdpBrowser {
    async fn navigate(&mut self, url: &str) -> Result<Value> {
        self.navigate_to(url).await
    }
    async fn snapshot(&mut self) -> Result<Value> {
        self.take_snapshot(&BTreeSet::new()).await
    }
    async fn screenshot(&mut self) -> Result<Value> {
        self.save_screenshot().await
    }
    async fn click(&mut self, target: &Value) -> Result<Value> {
        self.click_target(target).await
    }
    async fn fill(&mut self, target: &Value, text: &str) -> Result<Value> {
        let page = self.page().await?;
        let node = self.focus_element(&page, target).await?;
        self.clear_element(&page, node).await?;
        self.insert_text(&page, text).await?;
        Ok(json!({}))
    }
    async fn type_text(&mut self, text: &str) -> Result<Value> {
        let page = self.page().await?;
        self.insert_text(&page, text).await?;
        Ok(json!({}))
    }
    async fn press(&mut self, key: &str) -> Result<Value> {
        self.press_key(key).await
    }
    async fn tabs(&mut self) -> Result<Value> {
        self.list_tabs().await
    }
    async fn new_tab(&mut self, url: Option<&str>) -> Result<Value> {
        self.open_tab(url).await
    }
    async fn switch_tab(&mut self, id: &str) -> Result<Value> {
        self.switch_to(id).await
    }
    async fn wait(&mut self, until: &WaitFor) -> Result<Value> {
        self.wait_for(until).await
    }
    fn close(&mut self) -> Result<Value> {
        self.cdp.send_and_close("Browser.close", json!({}));
        Ok(json!({}))
    }
}

impl PageInput for CdpBrowser {
    async fn page(&mut self) -> Result<Page> {
        Ok(Page(self.active_page().await?))
    }
    async fn focus_element(&mut self, page: &Page, target: &Value) -> Result<u64> {
        self.focus_target(&page.0, target).await
    }
    async fn clear_element(&mut self, page: &Page, node: u64) -> Result<()> {
        self.clear_node(&page.0, node).await
    }
    async fn insert_text(&mut self, page: &Page, text: &str) -> Result<()> {
        self.insert(&page.0, text).await
    }
    async fn redacted_snapshot(&mut self, hidden: &BTreeSet<u64>) -> Result<Value> {
        self.take_snapshot(hidden).await
    }
}

impl FrameInspector for CdpBrowser {
    async fn focus(&mut self, page: &Page) -> Result<Focus> {
        self.focused(&page.0).await
    }
    async fn frame_origin(&mut self, page: &Page, frame: &str) -> Option<Origin> {
        self.origin_of(&page.0, frame).await
    }
    async fn element(&mut self, page: &Page, node: u64) -> Result<Element> {
        self.describe_element(&page.0, node).await
    }
}
