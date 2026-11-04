//! What the supervisor does to every page the browser opens: capture sign-ins the user types
//! ([`capture`], 08 §8.2 ②) and hold document loads to the session's grants ([`gate`],
//! R-43). Pages are attached as they appear (and those already open).

mod capture;
mod cookies;
mod gate;

pub use gate::DocumentRule;

use crate::{cdp::Cdp, chrome::CdpLink};
use keyharbor_domain::ports::browser::CapturedLogin;
use serde_json::{Value, json};
use std::{collections::HashSet, ops::ControlFlow, sync::Arc};
use tokio::sync::{broadcast, mpsc};

/// How documents outside the session's grants are kept out (R-43, ADR-0018).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// Every document request waits until the rule allows it (Chrome).
    Intercept,
    /// A document the rule refuses is replaced by `about:blank` once it is reported
    /// committed. ego lite runs one command per page at a time, so a paused request could
    /// never be answered while the navigation that made it waits.
    AfterCommit,
}

/// Where page reports go.
pub struct PageHooks {
    pub captured: mpsc::Sender<CapturedLogin>,
    pub documents: DocumentRule,
    pub gate: Gate,
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
        let mut watch = Watch {
            cdp: cdp.clone(),
            hooks,
            ours: HashSet::new(),
        };
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                // Events past `EVENT_BACKLOG` were dropped. That fails closed: a missed
                // `Fetch.requestPaused` is never answered, so its page waits on that request
                // instead of loading it, and a missed `Target.attachedToTarget` leaves its
                // target paused at start. A missed sign-in report is only not offered.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            if watch.handle(&event).await.is_break() {
                return;
            }
        }
    }
}

/// `watch_pages`'s state: where reports go and which sessions are pages it instrumented.
struct Watch {
    cdp: Arc<Cdp>,
    hooks: PageHooks,
    ours: HashSet<String>,
}

impl Watch {
    /// Handles one event; breaks once captured sign-ins have nowhere to go.
    async fn handle(&mut self, event: &Value) -> ControlFlow<()> {
        let session = event["sessionId"].as_str().unwrap_or_default();
        let params = &event["params"];
        let ours = self.ours.contains(session);
        match event["method"].as_str() {
            Some("Target.attachedToTarget") => self.attached(params),
            Some("Page.frameNavigated") if ours && self.hooks.gate == Gate::AfterCommit => {
                self.navigated(session, params);
            }
            Some("Fetch.requestPaused") if ours => self.paused(session, params),
            Some("Runtime.bindingCalled") if ours && params["name"] == capture::BINDING => {
                return self.signed_in(params).await;
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    /// A new target: a page is gated and instrumented, anything else just resumed.
    fn attached(&mut self, params: &Value) {
        let Some(target) = params["sessionId"].as_str() else {
            return;
        };
        if params["targetInfo"]["type"] == "page" {
            self.ours.insert(target.to_owned());
            tokio::spawn(instrument(
                self.cdp.clone(),
                target.to_owned(),
                self.hooks.gate,
            ));
        } else {
            tokio::spawn(resume(self.cdp.clone(), target.to_owned()));
        }
    }

    /// A committed document, checked against the grants after the fact (`Gate::AfterCommit`).
    fn navigated(&self, session: &str, params: &Value) {
        let url = params["frame"]["url"].as_str().unwrap_or_default();
        tokio::spawn(gate::after_commit(
            self.cdp.clone(),
            self.hooks.documents.clone(),
            session.to_owned(),
            url.to_owned(),
        ));
    }

    /// A held document request, answered by the rule (`Gate::Intercept`).
    fn paused(&self, session: &str, params: &Value) {
        tokio::spawn(gate::answer(
            self.cdp.clone(),
            self.hooks.documents.clone(),
            session.to_owned(),
            params.clone(),
        ));
    }

    /// A sign-in the user submitted.
    async fn signed_in(&self, params: &Value) -> ControlFlow<()> {
        if let Some(login) = capture::parse(&params["payload"])
            && self.hooks.captured.send(login).await.is_err()
        {
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }
}

/// Gates and instruments a new page before it loads anything, then lets it run. A page that
/// cannot be gated is closed rather than left ungated.
async fn instrument(cdp: Arc<Cdp>, session: String, mode: Gate) {
    let ready = async {
        if mode == Gate::Intercept {
            gate::enable(&cdp, &session).await?;
        }
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
