//! Input: resolving targets, clicking, focusing, clearing, inserting text and key presses.

use super::CdpBrowser;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

impl CdpBrowser {
    /// The backend DOM node id a target names: `{"ref": N}` from a snapshot, or `{"selector": css}`.
    async fn resolve(&mut self, session: &str, target: &Value) -> Result<u64> {
        if let Some(reference) = target.get("ref").and_then(Value::as_u64) {
            return Ok(reference);
        }
        let selector = target
            .get("selector")
            .and_then(Value::as_str)
            .context("target needs a ref or a selector")?;
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

    pub(super) async fn click_target(&mut self, target: &Value) -> Result<Value> {
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
    pub(super) async fn focus_target(&mut self, session: &str, target: &Value) -> Result<u64> {
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
        let (code, key_code, text) = match key {
            "Enter" => ("Enter", 13, Some("\r")),
            "Tab" => ("Tab", 9, None),
            "Escape" => ("Escape", 27, None),
            "Backspace" => ("Backspace", 8, None),
            "Delete" => ("Delete", 46, None),
            "Space" | " " => ("Space", 32, Some(" ")),
            "ArrowUp" => ("ArrowUp", 38, None),
            "ArrowDown" => ("ArrowDown", 40, None),
            "ArrowLeft" => ("ArrowLeft", 37, None),
            "ArrowRight" => ("ArrowRight", 39, None),
            "Home" => ("Home", 36, None),
            "End" => ("End", 35, None),
            "PageUp" => ("PageUp", 33, None),
            "PageDown" => ("PageDown", 34, None),
            _ => bail!(
                "unsupported key; use Enter, Tab, Escape, Backspace, Delete, Space, arrows, Home, End, PageUp or PageDown"
            ),
        };
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
