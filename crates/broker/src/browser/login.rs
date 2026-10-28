//! Finding a page's sign-in inputs for `browser_login` (R-39): in the top document and in
//! frames of the same origin, from the DOM agent's view (attributes, not page script).

use super::{CdpBrowser, frames::element_of};
use anyhow::Result;
use keyharbor_domain::ports::browser::{Element, LoginFields};
use serde_json::{Value, json};

/// `scheme://host:port` of a URL, or the URL itself if it has no path.
fn origin_of(url: &str) -> &str {
    let after_scheme = url.find("://").map_or(0, |i| i + 3);
    match url[after_scheme..].find('/') {
        Some(i) => &url[..after_scheme + i],
        None => url,
    }
}

fn attribute<'a>(node: &'a Value, name: &str) -> &'a str {
    node["attributes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .chunks(2)
        .find(|pair| {
            pair[0]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|pair| pair.get(1).and_then(Value::as_str))
        .unwrap_or_default()
}

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
        .iter()
        .any(|v| {
            let v = v.to_ascii_lowercase();
            ["user", "email", "login"]
                .iter()
                .any(|word| v.contains(word))
        });
    named.then_some(1)
}

/// Every input of the top document and its same-origin frames, in document order.
fn inputs<'a>(root: &'a Value, out: &mut Vec<&'a Value>) {
    let origin = origin_of(root["documentURL"].as_str().unwrap_or_default());
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node["localName"] == "input" {
            out.push(node);
        }
        let mut next: Vec<&Value> = Vec::new();
        for key in ["children", "shadowRoots"] {
            if let Some(children) = node[key].as_array() {
                next.extend(children);
            }
        }
        let document = &node["contentDocument"];
        let same_origin = document["documentURL"]
            .as_str()
            .is_some_and(|url| origin_of(url) == origin);
        if document.is_object() && same_origin {
            next.push(document);
        }
        stack.extend(next.into_iter().rev());
    }
}

pub(super) fn pick(root: &Value) -> LoginFields {
    let mut found = Vec::new();
    inputs(root, &mut found);
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
    pub(super) async fn find_login_fields(&self, session: &str) -> Result<LoginFields> {
        let document = self
            .call(
                "DOM.getDocument",
                json!({"depth": -1, "pierce": true}),
                session,
            )
            .await?;
        Ok(pick(&document["root"]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
