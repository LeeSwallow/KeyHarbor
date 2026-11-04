//! Input: resolving targets, clicking, focusing, clearing, inserting text and key presses;
//! form controls ([`form`]) and sign-in inputs ([`login`]).

mod form;
mod login;

use super::CdpBrowser;
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::ports::browser::Target;
use serde_json::{Value, json};

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

    pub(crate) async fn click_target(&mut self, target: &Target) -> Result<Value> {
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
    pub(crate) async fn focus_target(&mut self, session: &str, target: &Target) -> Result<u64> {
        let node = self.resolve(session, target).await?;
        self.call("DOM.focus", json!({"backendNodeId": node}), session)
            .await?;
        Ok(node)
    }

    /// Empties an input's value (dispatching `input`) or a contenteditable's text.
    pub(crate) async fn clear_node(&self, session: &str, node: u64) -> Result<()> {
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

    pub(crate) async fn insert(&self, session: &str, text: &str) -> Result<()> {
        self.call("Input.insertText", json!({"text": text}), session)
            .await
            .map(|_| ())
            .map_err(|_| anyhow!("could not insert text"))
    }

    pub(crate) async fn press_key(&mut self, name: &str) -> Result<Value> {
        let spec = KeySpec::named(name)?;
        let session = self.active_page().await?;
        self.note_input();
        let mut down = spec.event("keyDown");
        if let Some(text) = spec.text {
            down["text"] = json!(text);
        }
        self.call("Input.dispatchKeyEvent", down, &session).await?;
        self.call("Input.dispatchKeyEvent", spec.event("keyUp"), &session)
            .await?;
        Ok(json!({}))
    }
}

/// A key `browser_press` can send.
struct KeySpec {
    /// The names an agent may use for it.
    names: &'static [&'static str],
    /// How the unsupported-key message lists it (consecutive keys may share one entry).
    listed_as: &'static str,
    /// CDP `key` and `code`.
    key: &'static str,
    code: &'static str,
    /// Windows and native virtual key code.
    key_code: u32,
    /// The text the key types, if any.
    text: Option<&'static str>,
}

const fn key(
    names: &'static [&'static str],
    listed_as: &'static str,
    key: &'static str,
    key_code: u32,
) -> KeySpec {
    KeySpec {
        names,
        listed_as,
        key,
        code: names[0],
        key_code,
        text: None,
    }
}

const KEYS: &[KeySpec] = &[
    KeySpec {
        text: Some("\r"),
        ..key(&["Enter"], "Enter", "Enter", 13)
    },
    key(&["Tab"], "Tab", "Tab", 9),
    key(&["Escape"], "Escape", "Escape", 27),
    key(&["Backspace"], "Backspace", "Backspace", 8),
    key(&["Delete"], "Delete", "Delete", 46),
    KeySpec {
        text: Some(" "),
        ..key(&["Space", " "], "Space", " ", 32)
    },
    key(&["ArrowUp"], "arrows", "ArrowUp", 38),
    key(&["ArrowDown"], "arrows", "ArrowDown", 40),
    key(&["ArrowLeft"], "arrows", "ArrowLeft", 37),
    key(&["ArrowRight"], "arrows", "ArrowRight", 39),
    key(&["Home"], "Home", "Home", 36),
    key(&["End"], "End", "End", 35),
    key(&["PageUp"], "PageUp", "PageUp", 33),
    key(&["PageDown"], "PageDown", "PageDown", 34),
];

impl KeySpec {
    fn named(name: &str) -> Result<&'static Self> {
        KEYS.iter()
            .find(|spec| spec.names.contains(&name))
            .ok_or_else(|| anyhow!("unsupported key; use {}", supported_keys()))
    }

    /// The `Input.dispatchKeyEvent` parameters of this key for event `kind`.
    fn event(&self, kind: &str) -> Value {
        json!({
            "type": kind, "key": self.key, "code": self.code,
            "windowsVirtualKeyCode": self.key_code, "nativeVirtualKeyCode": self.key_code,
        })
    }
}

/// `KEYS` as a list for people: "Enter, Tab, … or PageDown".
fn supported_keys() -> String {
    let mut listed: Vec<&str> = KEYS.iter().map(|spec| spec.listed_as).collect();
    listed.dedup();
    match listed.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_keys_are_refused_with_the_list_of_keys() {
        let error = KeySpec::named("F5").err().unwrap().to_string();
        assert_eq!(
            error,
            "unsupported key; use Enter, Tab, Escape, Backspace, Delete, Space, arrows, Home, End, PageUp or PageDown"
        );
    }

    #[test]
    fn space_is_sent_as_a_space_by_either_name() {
        for name in ["Space", " "] {
            let spec = KeySpec::named(name).unwrap();
            assert_eq!((spec.key, spec.code, spec.text), (" ", "Space", Some(" ")));
        }
        let enter = KeySpec::named("Enter").unwrap();
        assert_eq!(
            (enter.key, enter.code, enter.text),
            ("Enter", "Enter", Some("\r"))
        );
        assert_eq!(enter.event("keyUp")["windowsVirtualKeyCode"], 13);
    }
}
