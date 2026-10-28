//! `snapshot`: the accessibility tree as `[ref] role "name"` lines, same-process iframes
//! included under `[frame] "<origin>"`. Password inputs never show their value; elements the
//! guard marked (they received a vault value) show `[redacted]` for their value and for the
//! text of everything inside them.

use super::CdpBrowser;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};

const MAX_SNAPSHOT_LINES: usize = 2000;
const REDACTED: &str = "[redacted]";

impl CdpBrowser {
    pub(super) async fn take_snapshot(&mut self, redacted: &BTreeSet<u64>) -> Result<Value> {
        let session = self.active_page().await?;
        let tree = self
            .call("Accessibility.getFullAXTree", json!({}), &session)
            .await?;
        let hidden = self.password_inputs(&session).await?;
        let mut text = render_ax_tree(
            tree["nodes"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default(),
            &hidden,
            redacted,
        );
        // Iframes in the page's process have their own trees; refs work across them.
        let frames = self.call("Page.getFrameTree", json!({}), &session).await?;
        let mut stack: Vec<&Value> = frames["frameTree"]["childFrames"]
            .as_array()
            .map(|c| c.iter().rev().collect())
            .unwrap_or_default();
        while let Some(frame) = stack.pop() {
            let origin = frame["frame"]["securityOrigin"].as_str().unwrap_or("?");
            text.push_str(&format!("\n[frame] {}", serde_json::to_string(origin)?));
            match self
                .call(
                    "Accessibility.getFullAXTree",
                    json!({"frameId": frame["frame"]["id"]}),
                    &session,
                )
                .await
            {
                Ok(tree) => {
                    let nodes = tree["nodes"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    for line in render_ax_tree(nodes, &hidden, redacted).lines() {
                        text.push_str("\n  ");
                        text.push_str(line);
                    }
                }
                Err(_) => text.push_str(" (not accessible)"),
            }
            if let Some(children) = frame["childFrames"].as_array() {
                stack.extend(children.iter().rev());
            }
        }
        let (url, title) = self.page_info(&session).await?;
        Ok(json!({"url": url, "title": title, "text": text}))
    }

    /// Backend ids of every `<input type=password>` in the page and its same-process frames.
    /// Their values never appear in a snapshot, whether or not they came from the vault.
    async fn password_inputs(&self, session: &str) -> Result<HashSet<u64>> {
        let document = self
            .call(
                "DOM.getDocument",
                json!({"depth": -1, "pierce": true}),
                session,
            )
            .await?;
        let mut found = HashSet::new();
        let mut stack = vec![&document["root"]];
        while let Some(node) = stack.pop() {
            if node["localName"] == "input" {
                let attributes = node["attributes"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let password = attributes.chunks(2).any(|pair| {
                    pair[0] == "type"
                        && pair
                            .get(1)
                            .and_then(Value::as_str)
                            .is_some_and(|t| t.trim().eq_ignore_ascii_case("password"))
                });
                if let (true, Some(id)) = (password, node["backendNodeId"].as_u64()) {
                    found.insert(id);
                }
            }
            for key in ["children", "shadowRoots"] {
                if let Some(children) = node[key].as_array() {
                    stack.extend(children);
                }
            }
            if node["contentDocument"].is_object() {
                stack.push(&node["contentDocument"]);
            }
        }
        Ok(found)
    }
}

/// Renders the accessibility tree as indented lines `[ref] role "name" value="…"`, where ref is
/// the backend DOM node id. Ignored nodes, inline text boxes and unnamed generic containers are
/// left out (their children move up a level).
/// Nodes in `hidden` are shown without their value; nodes in `redacted` with the value
/// `[redacted]` and every node below them with the name `[redacted]` and no value.
pub(crate) fn render_ax_tree(
    nodes: &[Value],
    hidden: &HashSet<u64>,
    redacted: &BTreeSet<u64>,
) -> String {
    let by_id: HashMap<&str, &Value> = nodes
        .iter()
        .filter_map(|n| n["nodeId"].as_str().map(|id| (id, n)))
        .collect();
    // (node, depth, inside a redacted element)
    let mut stack: Vec<(&Value, usize, bool)> = nodes
        .iter()
        .filter(|n| {
            n["parentId"]
                .as_str()
                .is_none_or(|parent| !by_id.contains_key(parent))
        })
        .rev()
        .map(|n| (n, 0, false))
        .collect();
    let mut lines = Vec::new();
    while let Some((node, depth, inside)) = stack.pop() {
        if lines.len() >= MAX_SNAPSHOT_LINES {
            lines.push(format!("… truncated at {MAX_SNAPSHOT_LINES} lines"));
            break;
        }
        let role = node["role"]["value"].as_str().unwrap_or("");
        let backend = node["backendDOMNodeId"].as_u64();
        let marked = backend.is_some_and(|id| redacted.contains(&id));
        let name = match node["name"]["value"].as_str().unwrap_or("") {
            "" => "",
            _ if inside => REDACTED,
            name => name,
        };
        let skip = node["ignored"] == true
            || role == "InlineTextBox"
            || (matches!(role, "generic" | "none" | "") && name.is_empty());
        let child_depth = if skip {
            depth
        } else {
            let reference = node["backendDOMNodeId"]
                .as_u64()
                .map_or_else(|| "-".to_owned(), |r| r.to_string());
            let mut line = format!(
                "{}[{reference}] {role} {}",
                "  ".repeat(depth),
                serde_json::to_string(name).unwrap_or_default()
            );
            let secret = backend.is_some_and(|id| hidden.contains(&id));
            if let Some(value) = node["value"]["value"]
                .as_str()
                .filter(|v| !v.is_empty() && !secret)
            {
                let value = if marked || inside { REDACTED } else { value };
                line.push_str(&format!(
                    " value={}",
                    serde_json::to_string(value).unwrap_or_default()
                ));
            }
            lines.push(line);
            depth + 1
        };
        if let Some(children) = node["childIds"].as_array() {
            for child in children.iter().rev() {
                if let Some(child) = child.as_str().and_then(|c| by_id.get(c)) {
                    stack.push((child, child_depth, inside || marked));
                }
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ax_tree_renders_refs_roles_and_skips_noise() {
        let nodes = vec![
            json!({"nodeId": "1", "role": {"value": "RootWebArea"}, "name": {"value": "Login"}, "childIds": ["2"], "backendDOMNodeId": 10}),
            json!({"nodeId": "2", "parentId": "1", "role": {"value": "generic"}, "name": {"value": ""}, "childIds": ["3", "4", "5"], "backendDOMNodeId": 11}),
            json!({"nodeId": "3", "parentId": "2", "role": {"value": "textbox"}, "name": {"value": "User"}, "value": {"value": "alice"}, "childIds": [], "backendDOMNodeId": 12}),
            json!({"nodeId": "4", "parentId": "2", "ignored": true, "role": {"value": "none"}, "childIds": ["6"]}),
            json!({"nodeId": "6", "parentId": "4", "role": {"value": "button"}, "name": {"value": "로그인"}, "childIds": ["7"], "backendDOMNodeId": 13}),
            json!({"nodeId": "7", "parentId": "6", "role": {"value": "InlineTextBox"}, "name": {"value": "로그인"}, "childIds": []}),
            json!({"nodeId": "5", "parentId": "2", "role": {"value": "StaticText"}, "name": {"value": "say \"hi\""}, "childIds": []}),
        ];
        assert_eq!(
            render_ax_tree(&nodes, &HashSet::new(), &BTreeSet::new()),
            "[10] RootWebArea \"Login\"\n  [12] textbox \"User\" value=\"alice\"\n  [13] button \"로그인\"\n  [-] StaticText \"say \\\"hi\\\"\""
        );
        let hidden = render_ax_tree(&nodes, &HashSet::from([12]), &BTreeSet::new());
        assert!(hidden.contains("[12] textbox \"User\"\n"), "{hidden}");
        assert!(!hidden.contains("alice"));
    }

    #[test]
    fn redacted_elements_hide_their_value_and_everything_inside() {
        let nodes = vec![
            json!({"nodeId": "1", "role": {"value": "RootWebArea"}, "name": {"value": "Login"}, "childIds": ["2", "3", "5"], "backendDOMNodeId": 10}),
            json!({"nodeId": "2", "parentId": "1", "role": {"value": "textbox"}, "name": {"value": "User"}, "value": {"value": "KH-SYNTH-pw-edited"}, "childIds": [], "backendDOMNodeId": 12}),
            json!({"nodeId": "3", "parentId": "1", "role": {"value": "textbox"}, "name": {"value": "Note"}, "childIds": ["4"], "backendDOMNodeId": 13}),
            json!({"nodeId": "4", "parentId": "3", "role": {"value": "StaticText"}, "name": {"value": "KH-SYNTH-note-edited"}, "childIds": [], "backendDOMNodeId": 14}),
            json!({"nodeId": "5", "parentId": "1", "role": {"value": "textbox"}, "name": {"value": "Other"}, "value": {"value": "visible"}, "childIds": [], "backendDOMNodeId": 15}),
        ];
        let text = render_ax_tree(&nodes, &HashSet::new(), &BTreeSet::from([12, 13]));
        assert_eq!(
            text,
            "[10] RootWebArea \"Login\"\n  [12] textbox \"User\" value=\"[redacted]\"\n  [13] textbox \"Note\"\n    [14] StaticText \"[redacted]\"\n  [15] textbox \"Other\" value=\"visible\""
        );
        // A password input shows no value at all, marked or not.
        let text = render_ax_tree(&nodes, &HashSet::from([12]), &BTreeSet::from([12]));
        assert!(text.contains("[12] textbox \"User\"\n"), "{text}");
    }

    #[test]
    fn ax_tree_is_capped() {
        let mut nodes = vec![
            json!({"nodeId": "0", "role": {"value": "RootWebArea"}, "name": {"value": "r"}, "childIds": (1..3000).map(|i| i.to_string()).collect::<Vec<_>>()}),
        ];
        for i in 1..3000 {
            nodes.push(json!({"nodeId": i.to_string(), "parentId": "0", "role": {"value": "link"}, "name": {"value": "x"}}));
        }
        let text = render_ax_tree(&nodes, &HashSet::new(), &BTreeSet::new());
        assert_eq!(text.lines().count(), MAX_SNAPSHOT_LINES + 1);
        assert!(text.ends_with("truncated at 2000 lines"));
    }
}
