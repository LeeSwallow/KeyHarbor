//! Frames: where the keyboard focus is, and which origin a frame has, as the browser (not the
//! page) reports it.

use super::CdpBrowser;
use anyhow::{Context, Result, bail};
use keyharbor_domain::{ports::browser::Focus, vault::Origin};
use serde_json::json;

/// Iframes nested deeper than this are refused for substitution.
const MAX_FRAME_DEPTH: usize = 16;

impl CdpBrowser {
    /// Follows `document.activeElement` down through same-process iframes, evaluating in an
    /// isolated world in each frame so page scripts cannot redefine what it returns.
    pub(super) async fn focused(&self, session: &str) -> Result<Focus> {
        let tree = self.call("Page.getFrameTree", json!({}), session).await?;
        let mut frame = tree["frameTree"]["frame"]["id"]
            .as_str()
            .context("page has no main frame")?
            .to_owned();
        for _ in 0..MAX_FRAME_DEPTH {
            let world = self
                .call(
                    "Page.createIsolatedWorld",
                    json!({"frameId": frame, "worldName": "keyharbor"}),
                    session,
                )
                .await?;
            let context = world["executionContextId"]
                .as_i64()
                .context("no isolated world")?;
            let active = self
                .call(
                    "Runtime.evaluate",
                    json!({"expression": "document.activeElement", "contextId": context}),
                    session,
                )
                .await?;
            let object = active["result"]["objectId"]
                .as_str()
                .context("no element has focus")?
                .to_owned();
            let described = self
                .call("DOM.describeNode", json!({"objectId": object}), session)
                .await;
            let _ = self
                .call(
                    "Runtime.releaseObject",
                    json!({"objectId": object}),
                    session,
                )
                .await;
            let node = described?["node"].take();
            let local = node["localName"].as_str().unwrap_or_default();
            match node["frameId"].as_str() {
                Some(child) if matches!(local, "iframe" | "frame") && child != frame => {
                    frame = child.to_owned();
                }
                _ => {
                    return Ok(Focus {
                        frame,
                        node: node["backendNodeId"]
                            .as_u64()
                            .context("focused element has no backend node id")?,
                    });
                }
            }
        }
        bail!("frames are nested too deeply")
    }

    /// The security origin the browser reports for `frame`, if it is an origin KeyHarbor can
    /// bind items to (HTTPS, or HTTP on localhost).
    pub(super) async fn origin_of(&self, session: &str, frame: &str) -> Option<Origin> {
        let tree = self
            .call("Page.getFrameTree", json!({}), session)
            .await
            .ok()?;
        let mut stack = vec![&tree["frameTree"]];
        while let Some(node) = stack.pop() {
            if node["frame"]["id"] == frame {
                return Origin::parse(node["frame"]["securityOrigin"].as_str()?).ok();
            }
            if let Some(children) = node["childFrames"].as_array() {
                stack.extend(children);
            }
        }
        None
    }
}
