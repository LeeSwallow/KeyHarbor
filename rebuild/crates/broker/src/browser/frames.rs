//! Frames: where the keyboard focus is, and which origin a frame has, as the browser (not the
//! page) reports it.

use super::CdpBrowser;
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::browser::{Element, Focus},
    vault::Origin,
};
use serde_json::{Value, json};

/// Iframes nested deeper than this are refused for substitution.
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
            let node = described?["node"].take();
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

    /// The element's tag and attributes as the DOM agent reports them (page script cannot
    /// change what this returns, only the attributes themselves).
    pub(super) async fn describe_element(&self, session: &str, node: u64) -> Result<Element> {
        let described = self
            .call("DOM.describeNode", json!({"backendNodeId": node}), session)
            .await?;
        Ok(element_of(&described["node"]))
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

/// An `Element` from a `DOM.Node`.
pub(super) fn element_of(node: &Value) -> Element {
    let attributes = node["attributes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let attribute = |name: &str| {
        attributes
            .chunks(2)
            .find(|pair| {
                pair[0]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .map(|pair| pair.get(1).and_then(Value::as_str).unwrap_or_default())
    };
    Element {
        local_name: node["localName"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase(),
        input_type: attribute("type").map(|t| t.trim().to_ascii_lowercase()),
        autocomplete: attribute("autocomplete")
            .map(|a| {
                a.split_ascii_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect()
            })
            .unwrap_or_default(),
        content_editable: attribute("contenteditable")
            .is_some_and(|v| !v.trim().eq_ignore_ascii_case("false")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elements_are_read_from_dom_attributes() {
        let node = json!({"localName": "input", "attributes": ["id", "pw", "TYPE", " Password ", "autocomplete", "section-a Current-Password"]});
        let element = element_of(&node);
        assert_eq!(element.input_type.as_deref(), Some("password"));
        assert_eq!(element.autocomplete, vec!["section-a", "current-password"]);
        assert!(element.is_password() && element.takes_text());
        let div = element_of(&json!({"localName": "div", "attributes": ["contenteditable", ""]}));
        assert!(div.content_editable && div.takes_text() && !div.is_password());
        let off =
            element_of(&json!({"localName": "div", "attributes": ["contenteditable", "false"]}));
        assert!(!off.takes_text());
        let check = element_of(&json!({"localName": "input", "attributes": ["type", "checkbox"]}));
        assert!(!check.takes_text());
        assert!(element_of(&json!({"localName": "input"})).takes_text());
    }
}
