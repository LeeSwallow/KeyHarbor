//! Navigation tracking (I4): which frames of each attached page are navigating, and a count
//! of navigation events per page, from the `Page.*` events CDP sends for every attached page.

use super::CdpBrowser;
use anyhow::Result;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;

/// How long `settle` waits for navigations to finish.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(3);
/// How long after a click or key press a navigation it caused may take to be reported.
const INPUT_QUIET: Duration = Duration::from_millis(250);
const POLL: Duration = Duration::from_millis(50);

#[derive(Default)]
pub(super) struct PageNavigation {
    /// Navigation events seen in any frame of the page.
    epoch: u64,
    /// Frames that requested or started a navigation that has not stopped loading.
    pending: HashSet<String>,
    /// The main frame, once a navigation of it was seen.
    main: Option<String>,
}

/// Per CDP page session.
pub(super) type Navigations = Arc<Mutex<HashMap<String, PageNavigation>>>;

fn frame_id(event: &Value) -> Option<&str> {
    event["params"]["frameId"]
        .as_str()
        .or_else(|| event["params"]["frame"]["id"].as_str())
}

/// Applies one CDP event.
pub(super) fn apply(navigations: &Navigations, event: &Value) {
    let Some(session) = event["sessionId"].as_str() else {
        return;
    };
    let Some(method) = event["method"].as_str() else {
        return;
    };
    let Ok(mut all) = navigations.lock() else {
        return;
    };
    let page = all.entry(session.to_owned()).or_default();
    match (method, frame_id(event)) {
        (
            "Page.frameRequestedNavigation"
            | "Page.frameStartedNavigating"
            | "Page.frameStartedLoading"
            | "Page.frameScheduledNavigation",
            Some(frame),
        ) => {
            page.epoch += 1;
            page.pending.insert(frame.to_owned());
        }
        ("Page.frameNavigated", Some(frame)) => {
            page.epoch += 1;
            if event["params"]["frame"]["parentId"].is_null() {
                page.main = Some(frame.to_owned());
            }
        }
        ("Page.navigatedWithinDocument", _) => page.epoch += 1,
        // Not `frameClearedScheduledNavigation`: Chrome sends it as a scheduled navigation
        // starts for real, long before the new document is there.
        ("Page.frameStoppedLoading" | "Page.frameDetached", Some(frame)) => {
            page.pending.remove(frame);
        }
        _ => {}
    }
}

/// Follows `events` until the browser connection closes. If events were dropped, every page
/// counts as having navigated.
pub(super) async fn track(mut events: broadcast::Receiver<Value>, navigations: Navigations) {
    loop {
        match events.recv().await {
            Ok(event) => apply(&navigations, &event),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                if let Ok(mut all) = navigations.lock() {
                    for page in all.values_mut() {
                        page.epoch += 1;
                    }
                }
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

impl CdpBrowser {
    pub(super) fn note_input(&self) {
        if let Ok(mut at) = self.last_input.lock() {
            *at = Some(Instant::now());
        }
    }

    /// No click or key press just happened, and neither the main frame nor `frame` is
    /// navigating (any frame, while the main frame is not known yet).
    fn quiet(&self, session: &str, frame: Option<&str>) -> bool {
        let after_input = self
            .last_input
            .lock()
            .ok()
            .and_then(|at| *at)
            .is_none_or(|at| at.elapsed() >= INPUT_QUIET);
        let pending = self.navigations.lock().is_ok_and(|all| {
            all.get(session).is_some_and(|page| match &page.main {
                Some(main) => {
                    page.pending.contains(main) || frame.is_some_and(|f| page.pending.contains(f))
                }
                None => !page.pending.is_empty(),
            })
        });
        after_input && !pending
    }

    /// Waits for `quiet`. It asks the browser nothing: page commands wait while a
    /// navigation is pending, so asking would only wait for the new document.
    pub(super) async fn settle_frames(&self, session: &str, frame: Option<&str>) -> Result<bool> {
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        loop {
            if self.quiet(session, frame) {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    pub(super) fn epoch(&self, session: &str) -> u64 {
        self.navigations
            .lock()
            .ok()
            .and_then(|all| all.get(session).map(|page| page.epoch))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(method: &str, frame: &str) -> Value {
        json!({"sessionId": "s", "method": method, "params": {"frameId": frame}})
    }

    #[test]
    fn navigations_are_pending_until_they_stop_loading() {
        let navigations = Navigations::default();
        let state = |n: &Navigations| {
            let all = n.lock().unwrap();
            let page = &all["s"];
            (page.epoch, page.pending.len())
        };
        apply(
            &navigations,
            &event("Page.frameRequestedNavigation", "main"),
        );
        assert_eq!(state(&navigations), (1, 1));
        apply(
            &navigations,
            &json!({"sessionId": "s", "method": "Page.frameNavigated", "params": {"frame": {"id": "main"}}}),
        );
        assert_eq!(state(&navigations), (2, 1));
        assert_eq!(
            navigations.lock().unwrap()["s"].main.as_deref(),
            Some("main")
        );
        apply(&navigations, &event("Page.frameStoppedLoading", "main"));
        assert_eq!(state(&navigations), (2, 0));
        // Events of other sessions and without a session do not count here.
        apply(
            &navigations,
            &json!({"method": "Page.frameStartedLoading", "params": {"frameId": "x"}}),
        );
        assert_eq!(state(&navigations), (2, 0));
    }
}
