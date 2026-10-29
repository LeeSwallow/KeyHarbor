//! The document gate (R-43): every document request of a page (top level and frames) pauses
//! until the supervisor's rule allows it; a refused one fails as blocked. Subresources are
//! not paused.

use crate::cdp::Cdp;
use serde_json::{Value, json};
use std::sync::Arc;

/// Decides whether a document URL may load. Called off the async runtime (it may read the
/// store); an error refuses.
pub type DocumentRule = Arc<dyn Fn(&str) -> anyhow::Result<bool> + Send + Sync>;

pub(super) async fn enable(cdp: &Cdp, session: &str) -> anyhow::Result<()> {
    let patterns =
        json!([{"urlPattern": "*", "resourceType": "Document", "requestStage": "Request"}]);
    cdp.call("Fetch.enable", json!({"patterns": patterns}), Some(session))
        .await?;
    Ok(())
}

/// Answers one `Fetch.requestPaused` event: continue if `rule` allows the URL, else fail it.
pub(super) async fn answer(cdp: Arc<Cdp>, rule: DocumentRule, session: String, params: Value) {
    let Some(request) = params["requestId"].as_str().map(str::to_owned) else {
        return;
    };
    let url = params["request"]["url"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let allowed = tokio::task::spawn_blocking(move || rule(&url))
        .await
        .is_ok_and(|decision| decision.unwrap_or(false));
    let (method, body) = if allowed {
        ("Fetch.continueRequest", json!({"requestId": request}))
    } else {
        (
            "Fetch.failRequest",
            json!({"requestId": request, "errorReason": "BlockedByClient"}),
        )
    };
    let _ = cdp.call(method, body, Some(&session)).await;
}
