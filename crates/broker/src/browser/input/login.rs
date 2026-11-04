//! Finding a page's sign-in inputs for `browser_login` (R-39): in the top document and in
//! frames of the same origin, from the DOM agent's view (attributes, not page script).

use crate::browser::{
    CdpBrowser,
    dom::{Frames, attribute, element_of, walk},
};
use anyhow::Result;
use keyharbor_domain::ports::browser::{Element, LoginFields};
use serde_json::Value;

/// How well an input looks like a username field (higher is better), if at all.
fn username_rank(node: &Value, element: &Element) -> Option<u8> {
    if element.local_name != "input" || element.is_password() || !element.takes_text() {
        return None;
    }
    if element.autocomplete.iter().any(|t| t == "username") {
        return Some(3);
    }
    if element.input_type.as_deref() == Some("email") {
        return Some(2);
    }
    let named = [attribute(node, "name"), attribute(node, "id")]
        .into_iter()
        .flatten()
        .any(|v| {
            let v = v.to_ascii_lowercase();
            ["user", "email", "login"]
                .iter()
                .any(|word| v.contains(word))
        });
    named.then_some(1)
}

fn pick(root: &Value) -> LoginFields {
    // Inputs of the top document and its same-origin frames, in document order.
    let mut found = Vec::new();
    walk(root, Frames::SameOrigin, |node| {
        if node["localName"] == "input" {
            found.push(node);
        }
    });
    let mut fields = LoginFields::default();
    let mut best = 0;
    for node in found {
        let element = element_of(node);
        let Some(id) = node["backendNodeId"].as_u64() else {
            continue;
        };
        if element.is_password() && fields.password.is_none() {
            fields.password = Some(id);
        } else if let Some(rank) = username_rank(node, &element).filter(|r| *r > best) {
            best = rank;
            fields.username = Some(id);
        }
    }
    fields
}

impl CdpBrowser {
    pub(crate) async fn find_login_fields(&self, session: &str) -> Result<LoginFields> {
        Ok(pick(&self.full_document(session).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(id: u64, attributes: &[&str]) -> Value {
        json!({"localName": "input", "backendNodeId": id, "attributes": attributes})
    }

    #[test]
    fn sign_in_inputs_are_found_by_their_attributes() {
        let page = json!({"documentURL": "https://a.example/login", "children": [
            input(1, &["name", "q", "type", "search"]),
            input(2, &["id", "login-name"]),
            input(3, &["type", "email"]),
            input(4, &["type", "password"]),
            input(5, &["type", "password"]),
        ]});
        assert_eq!(
            pick(&page),
            LoginFields {
                username: Some(3),
                password: Some(4)
            }
        );
        let declared = json!({"documentURL": "https://a.example/", "children": [
            input(1, &["type", "email"]),
            input(2, &["autocomplete", "username"]),
            input(3, &["autocomplete", "current-password"]),
        ]});
        assert_eq!(
            pick(&declared),
            LoginFields {
                username: Some(2),
                password: Some(3)
            }
        );
    }

    #[test]
    fn only_same_origin_frames_are_searched() {
        let framed = |url: &str| {
            json!({"documentURL": "https://a.example/", "children": [
                {"localName": "iframe", "contentDocument": {"documentURL": url, "children": [
                    input(7, &["name", "user"]),
                    input(8, &["type", "password"]),
                ]}}
            ]})
        };
        assert_eq!(
            pick(&framed("https://a.example/frame")),
            LoginFields {
                username: Some(7),
                password: Some(8)
            }
        );
        assert_eq!(
            pick(&framed("https://evil.example/")),
            LoginFields::default()
        );
    }
}
