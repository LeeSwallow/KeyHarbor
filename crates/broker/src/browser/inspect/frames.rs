//! Frames: where the keyboard focus is, and which origin a frame has, as the browser (not the
//! page) reports it.

use crate::browser::{CdpBrowser, dom::element_of};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::browser::{Element, Focus},
    vault::Origin,
};
use serde_json::{Value, json};

/// Iframes nested deeper than this are refused for substitution. Fixed: it only bounds the
/// walk (one isolated world per level); sign-in forms are not nested anywhere near this deep.
const MAX_FRAME_DEPTH: usize = 16;

/// The focused element of the document, followed through open shadow roots.
const ACTIVE_ELEMENT: &str = "(() => { let e = document.activeElement; \
    while (e && e.shadowRoot && e.shadowRoot.activeElement) { e = e.shadowRoot.activeElement; } \
    return e; })()";

/// Elements that host a document or plugin; typing "into" one types into what it hosts.
const FRAME_HOSTS: &[&str] = &[
    "iframe",
    "frame",
    "object",
    "embed",
    "fencedframe",
    "portal",
];

impl CdpBrowser {
    /// Follows `document.activeElement` down through open shadow roots, and into every
    /// element that hosts a document — whatever its tag (iframe, frame, object, embed,
    /// fencedframe) — as the DOM agent reports it (`frameId`). Script runs in an isolated
    /// world in each frame so page scripts cannot redefine what it returns. A hosted
    /// document this page session cannot enter (another process), a frame host without a
    /// document, or an element that holds a shadow root it cannot see into is an error.
    pub(crate) async fn focused(&self, session: &str) -> Result<Focus> {
        let tree = self.call("Page.getFrameTree", json!({}), session).await?;
        let mut frame = tree["frameTree"]["frame"]["id"]
            .as_str()
            .context("page has no main frame")?
            .to_owned();
        for _ in 0..MAX_FRAME_DEPTH {
            let node = self.active_in(session, &frame).await?;
            let local = node["localName"].as_str().unwrap_or_default();
            match node["frameId"].as_str() {
                Some(child) if child != frame => {
                    frame = child.to_owned();
                }
                _ if FRAME_HOSTS.contains(&local) => bail!("the focused frame cannot be entered"),
                _ if node["shadowRoots"]
                    .as_array()
                    .is_some_and(|roots| roots.iter().any(|r| r["shadowRootType"] == "closed")) =>
                {
                    bail!("the focus is inside a closed shadow root")
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

    /// The `DOM.Node` (depth 0, shadow roots pierced) of `frame`'s focused element, found by
    /// script in an isolated world of that frame.
    async fn active_in(&self, session: &str, frame: &str) -> Result<Value> {
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
                json!({"expression": ACTIVE_ELEMENT, "contextId": context}),
                session,
            )
            .await?;
        let object = active["result"]["objectId"]
            .as_str()
            .context("no element has focus")?
            .to_owned();
        let described = self
            .call(
                "DOM.describeNode",
                json!({"objectId": object, "depth": 0, "pierce": true}),
                session,
            )
            .await;
        let _ = self
            .call(
                "Runtime.releaseObject",
                json!({"objectId": object}),
                session,
            )
            .await;
        Ok(described?["node"].take())
    }

    /// The element's tag and attributes as the DOM agent reports them (page script cannot
    /// change what this returns, only the attributes themselves).
    pub(crate) async fn describe_element(&self, session: &str, node: u64) -> Result<Element> {
        let described = self
            .call("DOM.describeNode", json!({"backendNodeId": node}), session)
            .await?;
        Ok(element_of(&described["node"]))
    }

    /// The security origin the browser reports for `frame`, if it is an origin KeyHarbor can
    /// bind items to (HTTPS, or HTTP on localhost).
    pub(crate) async fn origin_of(&self, session: &str, frame: &str) -> Option<Origin> {
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
