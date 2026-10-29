//! What the supervisor does to every page the browser opens: capture sign-ins the user types
//! ([`capture`], 08 §8.2 ②) and hold document loads to the session's grants ([`gate`],
//! R-43). Pages are attached as they appear (and those already open).

mod capture;
mod cookies;
mod gate;

pub use gate::DocumentRule;

use crate::{cdp::Cdp, chrome::CdpLink};
use keyharbor_domain::ports::browser::CapturedLogin;
use serde_json::json;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{broadcast, mpsc};

/// Where page reports go.
pub struct PageHooks {
    pub captured: mpsc::Sender<CapturedLogin>,
    pub documents: DocumentRule,
}

impl CdpLink {
    /// Instruments every page and handles its events until the browser closes.
    pub async fn watch_pages(&self, hooks: PageHooks) {
        let cdp = self.cdp();
        let mut events = cdp.events();
        let attach = json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true});
        if cdp
            .call("Target.setAutoAttach", attach, None)
            .await
            .is_err()
        {
            return;
        }
        let mut ours = HashSet::new();
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            let session = event["sessionId"].as_str().unwrap_or_default();
            match event["method"].as_str() {
                Some("Target.attachedToTarget") => {
                    let params = &event["params"];
                    let Some(page) = params["sessionId"].as_str() else {
                        continue;
                    };
                    if params["targetInfo"]["type"] == "page" {
                        ours.insert(page.to_owned());
                        tokio::spawn(instrument(cdp.clone(), page.to_owned()));
                    } else {
                        tokio::spawn(resume(cdp.clone(), page.to_owned()));
                    }
                }
                Some("Fetch.requestPaused") if ours.contains(session) => {
                    let (rule, params) = (hooks.documents.clone(), event["params"].clone());
                    tokio::spawn(gate::answer(cdp.clone(), rule, session.to_owned(), params));
                }
                Some("Runtime.bindingCalled")
                    if ours.contains(session) && event["params"]["name"] == capture::BINDING =>
                {
                    if let Some(login) = capture::parse(&event["params"]["payload"])
                        && hooks.captured.send(login).await.is_err()
                    {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Gates and instruments a new page before it loads anything, then lets it run. A page that
/// cannot be gated is closed rather than left ungated.
async fn instrument(cdp: Arc<Cdp>, session: String) {
    let ready = async {
        gate::enable(&cdp, &session).await?;
        capture::instrument(&cdp, &session).await
    }
    .await;
    if ready.is_err() {
        let _ = cdp.call("Page.close", json!({}), Some(&session)).await;
    }
    resume(cdp, session).await;
}

/// Lets a target paused at start (`waitForDebuggerOnStart`) run.
async fn resume(cdp: Arc<Cdp>, session: String) {
    let _ = cdp
        .call("Runtime.runIfWaitingForDebugger", json!({}), Some(&session))
        .await;
}
