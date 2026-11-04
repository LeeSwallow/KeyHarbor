//! Input: resolving targets, clicking, focusing, clearing, inserting text and key presses.

use super::CdpBrowser;
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::ports::browser::Target;
use serde_json::{Value, json};

/// A key an agent may press: its name (also the CDP `code`), its virtual key code and the
/// text it types, if any.
struct KeySpec {
    name: &'static str,
    key_code: i64,
    text: Option<&'static str>,
}

const fn key(name: &'static str, key_code: i64) -> KeySpec {
    KeySpec {
        name,
        key_code,
        text: None,
    }
}

/// Every key `press_key` knows. The MCP `browser_press` schema lists the same names.
static KEYS: &[KeySpec] = &[
    KeySpec {
        text: Some("\r"),
        ..key("Enter", 13)
    },
    key("Tab", 9),
    key("Escape", 27),
    key("Backspace", 8),
    key("Delete", 46),
    KeySpec {
        text: Some(" "),
        ..key("Space", 32)
    },
    key("ArrowUp", 38),
    key("ArrowDown", 40),
    key("ArrowLeft", 37),
    key("ArrowRight", 39),
    key("Home", 36),
    key("End", 35),
    key("PageUp", 33),
    key("PageDown", 34),
];

/// The names of the keys an agent may press (`" "` also means `Space`).
pub fn key_names() -> impl Iterator<Item = &'static str> {
    KEYS.iter().map(|spec| spec.name)
}

impl CdpBrowser {
    /// The backend DOM node id a target names: a ref from a snapshot, or a CSS selector.
    async fn resolve(&mut self, session: &str, target: &Target) -> Result<u64> {
        let selector = match target {
            Target::Ref(reference) => return Ok(*reference),
            Target::Selector(selector) => selector,
        };
        let document = self
            .call("DOM.getDocument", json!({"depth": 0}), session)
            .await?;
        let found = self
            .call(
                "DOM.querySelector",
                json!({"nodeId": document["root"]["nodeId"], "selector": selector}),
                session,
            )
            .await?;
        let node = found["nodeId"].as_u64().unwrap_or(0);
        if node == 0 {
            bail!("no element matches the selector");
        }
        let described = self
            .call("DOM.describeNode", json!({"nodeId": node}), session)
            .await?;
        described["node"]["backendNodeId"]
            .as_u64()
            .context("element has no backend node id")
    }

    pub(super) async fn click_target(&mut self, target: &Target) -> Result<Value> {
        let session = self.active_page().await?;
        let node = self.resolve(&session, target).await?;
        let _ = self
            .call(
                "DOM.scrollIntoViewIfNeeded",
                json!({"backendNodeId": node}),
                &session,
            )
            .await;
        let model = self
            .call("DOM.getBoxModel", json!({"backendNodeId": node}), &session)
            .await
            .context("element is not visible")?;
        let quad: Vec<f64> = model["model"]["content"]
            .as_array()
            .context("element has no box")?
            .iter()
            .filter_map(Value::as_f64)
            .collect();
        if quad.len() != 8 {
            bail!("element has no box");
        }
        let x = (quad[0] + quad[2] + quad[4] + quad[6]) / 4.0;
        let y = (quad[1] + quad[3] + quad[5] + quad[7]) / 4.0;
        self.note_input();
        for kind in ["mouseMoved", "mousePressed", "mouseReleased"] {
            self.call(
                "Input.dispatchMouseEvent",
                json!({"type": kind, "x": x, "y": y, "button": "left", "clickCount": 1}),
                &session,
            )
            .await?;
        }
        Ok(json!({}))
    }

    /// Resolves the target and gives it keyboard focus.
    pub(super) async fn focus_target(&mut self, session: &str, target: &Target) -> Result<u64> {
        let node = self.resolve(session, target).await?;
        self.call("DOM.focus", json!({"backendNodeId": node}), session)
            .await?;
        Ok(node)
    }

    /// Empties an input's value (dispatching `input`) or a contenteditable's text.
    pub(super) async fn clear_node(&self, session: &str, node: u64) -> Result<()> {
        let object = self
            .call("DOM.resolveNode", json!({"backendNodeId": node}), session)
            .await?;
        let object = object["object"]["objectId"]
            .as_str()
            .context("element cannot be resolved")?;
        self.call(
            "Runtime.callFunctionOn",
            json!({
                "objectId": object,
                "functionDeclaration": "function() { if ('value' in this) { this.value = ''; this.dispatchEvent(new Event('input', {bubbles: true})); } else if (this.isContentEditable) { this.textContent = ''; } }",
            }),
            session,
        )
        .await?;
        Ok(())
    }

    pub(super) async fn insert(&self, session: &str, text: &str) -> Result<()> {
        self.call("Input.insertText", json!({"text": text}), session)
            .await
            .map(|_| ())
            .map_err(|_| anyhow!("could not insert text"))
    }

    pub(super) async fn press_key(&mut self, key: &str) -> Result<Value> {
        let name = if key == " " { "Space" } else { key };
        let Some(spec) = KEYS.iter().find(|spec| spec.name == name) else {
            bail!(
                "unsupported key; use Enter, Tab, Escape, Backspace, Delete, Space, arrows, Home, End, PageUp or PageDown"
            );
        };
        let (code, key_code, text) = (spec.name, spec.key_code, spec.text);
        let key = if code == "Space" { " " } else { code };
        let session = self.active_page().await?;
        self.note_input();
        let mut down = json!({
            "type": "keyDown", "key": key, "code": code,
            "windowsVirtualKeyCode": key_code, "nativeVirtualKeyCode": key_code,
        });
        if let Some(text) = text {
            down["text"] = json!(text);
        }
        self.call("Input.dispatchKeyEvent", down, &session).await?;
        self.call(
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyUp", "key": key, "code": code,
                "windowsVirtualKeyCode": key_code, "nativeVirtualKeyCode": key_code,
            }),
            &session,
        )
        .await?;
        Ok(json!({}))
    }
}
