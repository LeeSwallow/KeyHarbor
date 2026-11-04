//! Reading the DOM agent's view of a page: a node's attributes, an `Element` from a node, the
//! whole document with every frame's document in it, and a walk over it (J-155).

use super::CdpBrowser;
use anyhow::Result;
use keyharbor_domain::ports::browser::Element;
use serde_json::{Value, json};

/// Which hosted documents (`contentDocument`) a [`walk`] enters.
#[derive(Clone, Copy)]
pub(super) enum Frames {
    /// Only frames of the top document's origin (sign-in inputs, R-39).
    SameOrigin,
    /// Every frame the document holds (password inputs hidden from a snapshot).
    All,
}

/// The value of attribute `name` (any case) of a `DOM.Node`, whose `attributes` is
/// `[name, value, …]`. An attribute without a value reads as `""`.
pub(super) fn attribute<'a>(node: &'a Value, name: &str) -> Option<&'a str> {
    node["attributes"]
        .as_array()?
        .chunks(2)
        .find(|pair| {
            pair[0]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .map(|pair| pair.get(1).and_then(Value::as_str).unwrap_or_default())
}

/// An `Element` from a `DOM.Node`.
pub(super) fn element_of(node: &Value) -> Element {
    let attribute = |name| attribute(node, name);
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

/// `scheme://host:port` of a URL, or the URL itself if it has no path.
fn origin_of(url: &str) -> &str {
    let after_scheme = url.find("://").map_or(0, |i| i + 3);
    match url[after_scheme..].find('/') {
        Some(i) => &url[..after_scheme + i],
        None => url,
    }
}

fn document_origin(document: &Value) -> &str {
    origin_of(document["documentURL"].as_str().unwrap_or_default())
}

/// Visits every node under `root` in document order: children, then shadow roots, then the
/// document an element hosts if `frames` lets the walk enter it.
pub(super) fn walk<'a>(root: &'a Value, frames: Frames, mut visit: impl FnMut(&'a Value)) {
    let origin = document_origin(root);
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        visit(node);
        let mut next: Vec<&Value> = Vec::new();
        for key in ["children", "shadowRoots"] {
            if let Some(children) = node[key].as_array() {
                next.extend(children);
            }
        }
        let document = &node["contentDocument"];
        let enters = match frames {
            Frames::SameOrigin => document_origin(document) == origin,
            Frames::All => true,
        };
        if document.is_object() && enters {
            next.push(document);
        }
        stack.extend(next.into_iter().rev());
    }
}

impl CdpBrowser {
    /// The page's document root with every node, through shadow roots and into the documents
    /// of same-process frames.
    pub(super) async fn full_document(&self, session: &str) -> Result<Value> {
        let mut document = self
            .call(
                "DOM.getDocument",
                json!({"depth": -1, "pierce": true}),
                session,
            )
            .await?;
        Ok(document["root"].take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_are_read_in_name_value_pairs() {
        let node = json!({"attributes": ["id", "user", "STYLE", "color: red", "hidden"]});
        assert_eq!(attribute(&node, "style"), Some("color: red"));
        assert_eq!(attribute(&node, "hidden"), Some(""));
        assert_eq!(attribute(&node, "class"), None);
    }

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

    fn names(root: &Value, frames: Frames) -> Vec<&str> {
        let mut seen = Vec::new();
        walk(root, frames, |node| {
            if let Some(name) = node["localName"].as_str() {
                seen.push(name);
            }
        });
        seen
    }

    #[test]
    fn the_walk_keeps_document_order_and_enters_frames_as_asked() {
        let page = json!({"documentURL": "https://a.example/", "children": [
            {"localName": "a", "children": [{"localName": "b"}], "shadowRoots": [{"localName": "c"}]},
            {"localName": "same", "contentDocument": {"documentURL": "https://a.example/f", "children": [{"localName": "d"}]}},
            {"localName": "other", "contentDocument": {"documentURL": "https://evil.example/", "children": [{"localName": "e"}]}},
        ]});
        assert_eq!(
            names(&page, Frames::SameOrigin),
            ["a", "b", "c", "same", "d", "other"]
        );
        assert_eq!(
            names(&page, Frames::All),
            ["a", "b", "c", "same", "d", "other", "e"]
        );
    }
}
