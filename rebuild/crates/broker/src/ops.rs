//! Browser operations behind the control socket.
//!
//! This is the single place where an op's input text and its response pass through:
//! [`Ops::prepare_text`] for every text typed into the page and [`Ops::redact`] for every
//! result. Text with `{{secret:item.field}}` placeholders is checked against the agent's
//! policy and the item's origin binding (the frame that holds the focused element, 03 §3.8)
//! and only then resolved; a refusal types nothing. Error messages never include input text.

use crate::{cdp::Cdp, paths, supervisor::Shared};
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use keyharbor_core::{
    ItemStatus, LOCAL_AGENT, Vault, fill_template,
    policy::{Action, decide},
};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    policy::parse_agent,
    vault::{ItemName, Origin, parse_placeholders},
};
use keyharbor_sqlite::SqliteStore;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::Instant;
use zeroize::Zeroizing;

const LOAD_TIMEOUT: Duration = Duration::from_secs(15);
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(100);
const MAX_SNAPSHOT_LINES: usize = 2000;
/// Iframes nested deeper than this are refused for substitution.
const MAX_FRAME_DEPTH: usize = 16;

pub(crate) struct Ops {
    cdp: Arc<Cdp>,
    /// Target the page ops act on.
    active: Option<String>,
    /// targetId → flattened CDP sessionId.
    attached: HashMap<String, String>,
    /// Opened on first use, reopened when the vault version moves (so rotations are seen).
    /// A mutex only to make `Ops` shareable; requests already run one at a time.
    vault: std::sync::Mutex<Option<(i64, Vault)>>,
}

/// The element that has keyboard focus and the frame whose document holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Focus {
    frame: String,
    node: u64,
}

/// Placeholder text resolved for one input, with the items it used.
struct Prepared {
    text: Zeroizing<String>,
    items: Vec<ItemName>,
}

struct Tab {
    id: String,
    url: String,
    title: String,
}

impl Ops {
    pub fn new(cdp: Arc<Cdp>) -> Self {
        Self {
            cdp,
            active: None,
            attached: HashMap::new(),
            vault: std::sync::Mutex::new(None),
        }
    }

    /// Runs `work` with the vault at its current version, or `None` if it was never
    /// initialized. The root key is read only here, the first time a placeholder or a response
    /// needs it.
    fn with_vault<T>(&self, shared: &Shared, work: impl FnOnce(Option<&Vault>) -> T) -> Result<T> {
        let row = shared
            .store
            .lock()
            .map_err(|_| anyhow!("store lock poisoned"))?
            .vault_get()?;
        let mut slot = self
            .vault
            .lock()
            .map_err(|_| anyhow!("vault lock poisoned"))?;
        let Some(row) = row else {
            *slot = None;
            return Ok(work(None));
        };
        if slot.as_ref().is_none_or(|(v, _)| *v != row.version) {
            *slot = None;
            let vault = Vault::open(
                SqliteStore::open(&shared.data)?,
                keyharbor_vault::keystore::keystore_from_env(),
            )?;
            *slot = Some((row.version, vault));
        }
        Ok(work(slot.as_ref().map(|(_, vault)| vault)))
    }

    /// Every text that will be typed into the page. Without placeholders it is used as is.
    /// Otherwise every item must exist, be allowed for `agent`, and — if origin-bound — list
    /// `origin` (the frame the text goes into). Any refusal is recorded and nothing resolves.
    fn prepare_text(
        &self,
        shared: &Shared,
        agent: &str,
        template: &str,
        origin: Option<&Origin>,
    ) -> Result<Prepared> {
        let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
        let mut items: Vec<ItemName> = Vec::new();
        for r in &refs {
            if !items.contains(&r.item) {
                items.push(r.item.clone());
            }
        }
        let Some(first) = items.first().cloned() else {
            return Ok(Prepared {
                text: Zeroizing::new(template.to_owned()),
                items,
            });
        };
        let origin_text = origin.map(Origin::as_str);
        let deny = |item: &ItemName, reason: &str| denied(shared, agent, item, origin_text, reason);
        let rules = shared
            .store
            .lock()
            .map_err(|_| anyhow!("store lock poisoned"))?
            .policy_rules()?;
        let checked = self.with_vault(shared, |vault| -> Result<Zeroizing<String>> {
            let Some(vault) = vault else {
                return Err(deny(&first, "not_found"));
            };
            for item in &items {
                let info = match vault.item_use(item) {
                    Ok(Some(info)) => info,
                    Ok(None) => return Err(deny(item, "not_found")),
                    Err(_) => return Err(deny(item, "tampered")),
                };
                if info.status == ItemStatus::Discarded {
                    return Err(deny(item, "discarded"));
                }
                let action = Action::UseItem {
                    item: item.clone(),
                    domain: info.domain,
                };
                if !decide(&rules, agent, &action).allowed() {
                    return Err(deny(item, "policy"));
                }
                if info.origin_binding && !origin.is_some_and(|o| info.origins.contains(o)) {
                    return Err(deny(item, "origin"));
                }
                let missing = refs
                    .iter()
                    .any(|r| &r.item == item && !info.fields.iter().any(|f| f == r.field.as_str()));
                if missing {
                    return Err(deny(item, "not_found"));
                }
            }
            fill_template(template, |r| vault.resolve(r)).map_err(|_| deny(&first, "vault"))
        });
        let text = match checked {
            Ok(result) => result?,
            Err(_) => return Err(deny(&first, "vault")),
        };
        Ok(Prepared { text, items })
    }

    /// Every result before it leaves the supervisor. Phase 3: masking.
    fn redact(&self, value: Value) -> Value {
        value
    }

    pub async fn execute(&mut self, shared: &Shared, request: &Value) -> Result<Value> {
        let op = request
            .get("op")
            .and_then(Value::as_str)
            .context("request has no op")?;
        let agent = match request.get("agent") {
            None => LOCAL_AGENT.to_owned(),
            Some(agent) => parse_agent(agent.as_str().context("agent must be a string")?)
                .map_err(|e| anyhow!("{e}"))?,
        };
        let result = match op {
            "navigate" => self.navigate(str_arg(request, "url")?).await?,
            "snapshot" => self.snapshot().await?,
            "screenshot" => self.screenshot(shared).await?,
            "click" => self.click(target(request)?).await?,
            "fill" => {
                self.fill(shared, &agent, target(request)?, str_arg(request, "text")?)
                    .await?
            }
            "type" => {
                self.type_text(shared, &agent, str_arg(request, "text")?)
                    .await?
            }
            "press" => self.press(str_arg(request, "key")?).await?,
            "tabs" => self.tabs().await?,
            "new_tab" => {
                self.new_tab(request.get("url").and_then(Value::as_str))
                    .await?
            }
            "switch_tab" => self.switch_tab(str_arg(request, "id")?).await?,
            "wait" => self.wait(request).await?,
            "close" => {
                self.cdp.send_and_close("Browser.close", json!({}));
                shared.request_close();
                json!({})
            }
            "status" => {
                let store = shared
                    .store
                    .lock()
                    .map_err(|_| anyhow!("store lock poisoned"))?;
                let row = store.get(shared.id)?;
                json!({
                    "state": row.as_ref().map(|r| r.state.as_str()),
                    "generation": shared.generation.to_string(),
                })
            }
            _ => bail!("unknown operation"),
        };
        Ok(self.redact(result))
    }

    async fn call(&self, method: &str, params: Value, session: &str) -> Result<Value> {
        self.cdp.call(method, params, Some(session)).await
    }

    async fn page_tabs(&self) -> Result<Vec<Tab>> {
        let targets = self.cdp.call("Target.getTargets", json!({}), None).await?;
        Ok(targets["targetInfos"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|t| t["type"] == "page")
            .map(|t| Tab {
                id: t["targetId"].as_str().unwrap_or_default().to_owned(),
                url: t["url"].as_str().unwrap_or_default().to_owned(),
                title: t["title"].as_str().unwrap_or_default().to_owned(),
            })
            .collect())
    }

    /// The CDP session of the active tab, attaching on first use. Falls back to the first tab
    /// if the active one was closed.
    async fn page(&mut self) -> Result<String> {
        let tabs = self.page_tabs().await?;
        self.attached
            .retain(|target, _| tabs.iter().any(|t| &t.id == target));
        let active = match &self.active {
            Some(id) if tabs.iter().any(|t| &t.id == id) => id.clone(),
            _ => tabs
                .first()
                .context("the browser has no open tab")?
                .id
                .clone(),
        };
        self.active = Some(active.clone());
        self.attach(&active).await
    }

    async fn attach(&mut self, target: &str) -> Result<String> {
        if let Some(session) = self.attached.get(target) {
            return Ok(session.clone());
        }
        let attached = self
            .cdp
            .call(
                "Target.attachToTarget",
                json!({"targetId": target, "flatten": true}),
                None,
            )
            .await?;
        let session = attached["sessionId"]
            .as_str()
            .context("attach returned no session")?
            .to_owned();
        self.call("Page.enable", json!({}), &session).await?;
        self.attached.insert(target.to_owned(), session.clone());
        Ok(session)
    }

    async fn page_info(&self, session: &str) -> Result<(String, String)> {
        let value = self
            .call(
                "Runtime.evaluate",
                json!({"expression": "[location.href, document.title]", "returnByValue": true}),
                session,
            )
            .await?;
        let pair = &value["result"]["value"];
        Ok((
            pair[0].as_str().unwrap_or_default().to_owned(),
            pair[1].as_str().unwrap_or_default().to_owned(),
        ))
    }

    async fn navigate(&mut self, url: &str) -> Result<Value> {
        let url = keyharbor_domain::validate_navigation_url(url).map_err(|e| anyhow!("{e}"))?;
        let session = self.page().await?;
        let mut events = self.cdp.events();
        let navigated = self
            .call("Page.navigate", json!({"url": url}), &session)
            .await?;
        if let Some(error) = navigated["errorText"].as_str().filter(|e| !e.is_empty()) {
            bail!("navigation failed: {error}");
        }
        let deadline = Instant::now() + LOAD_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, events.recv())
                .await
                .map_err(|_| anyhow!("page did not finish loading within 15 seconds"))?;
            match event {
                Ok(event)
                    if event["method"] == "Page.loadEventFired"
                        && event["sessionId"] == session.as_str() =>
                {
                    break;
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
                Err(_) => bail!("browser connection closed"),
            }
        }
        let (url, title) = self.page_info(&session).await?;
        Ok(json!({"url": url, "title": title}))
    }

    async fn snapshot(&mut self) -> Result<Value> {
        let session = self.page().await?;
        let tree = self
            .call("Accessibility.getFullAXTree", json!({}), &session)
            .await?;
        let mut text = render_ax_tree(
            tree["nodes"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default(),
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
                    for line in render_ax_tree(nodes).lines() {
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

    async fn screenshot(&mut self, shared: &Shared) -> Result<Value> {
        let session = self.page().await?;
        let shot = self
            .call("Page.captureScreenshot", json!({"format": "png"}), &session)
            .await?;
        let png = base64::engine::general_purpose::STANDARD
            .decode(
                shot["data"]
                    .as_str()
                    .context("screenshot returned no data")?,
            )
            .context("screenshot data is not base64")?;
        let dir = paths::shots_dir(&shared.data);
        paths::private_dir(dir.parent().context("invalid shots path")?)?;
        paths::private_dir(&dir)?;
        for n in 1.. {
            let path = dir.join(format!("{}-{n}.png", shared.id));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    std::io::Write::write_all(&mut file, &png)?;
                    return Ok(json!({"path": path}));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!()
    }

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

    async fn click(&mut self, target: &Value) -> Result<Value> {
        let session = self.page().await?;
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

    async fn fill(
        &mut self,
        shared: &Shared,
        agent: &str,
        target: &Value,
        template: &str,
    ) -> Result<Value> {
        let session = self.page().await?;
        let node = self.resolve(&session, target).await?;
        self.call("DOM.focus", json!({"backendNodeId": node}), &session)
            .await?;
        let has_placeholders = template.contains("{{secret:");
        let checked = if has_placeholders {
            Some(
                self.checked_focus(shared, agent, &session, template, Some(node))
                    .await?,
            )
        } else {
            None
        };
        let prepared = match &checked {
            Some((focus, origin)) => {
                let prepared = self.prepare_text(shared, agent, template, origin.as_ref())?;
                Some((focus.clone(), origin.clone(), prepared))
            }
            None => None,
        };
        let object = self
            .call("DOM.resolveNode", json!({"backendNodeId": node}), &session)
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
            &session,
        )
        .await?;
        match prepared {
            None => {
                self.insert_text(&session, template).await?;
                Ok(json!({}))
            }
            Some((focus, origin, prepared)) => {
                // Clearing ran page code; the text must still go where it was checked.
                if self.focus(&session).await.ok().as_ref() != Some(&focus) {
                    let first = &prepared.items[0];
                    return Err(denied(
                        shared,
                        agent,
                        first,
                        origin.as_ref().map(Origin::as_str),
                        "focus",
                    ));
                }
                self.insert_prepared(shared, agent, &session, &focus, origin, prepared)
                    .await
            }
        }
    }

    async fn type_text(&mut self, shared: &Shared, agent: &str, template: &str) -> Result<Value> {
        let session = self.page().await?;
        if !template.contains("{{secret:") {
            self.insert_text(&session, template).await?;
            return Ok(json!({}));
        }
        let (focus, origin) = self
            .checked_focus(shared, agent, &session, template, None)
            .await?;
        let prepared = self.prepare_text(shared, agent, template, origin.as_ref())?;
        self.insert_prepared(shared, agent, &session, &focus, origin, prepared)
            .await
    }

    /// The focused element and its frame's origin, refusing (and recording) when the focus
    /// cannot be located or, for `fill`, is not on the target element.
    async fn checked_focus(
        &mut self,
        shared: &Shared,
        agent: &str,
        session: &str,
        template: &str,
        expected: Option<u64>,
    ) -> Result<(Focus, Option<Origin>)> {
        let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
        let first = &refs.first().context("no placeholder")?.item;
        let focus = match self.focus(session).await {
            Ok(focus) => focus,
            Err(_) => return Err(denied(shared, agent, first, None, "frame")),
        };
        let origin = self.frame_origin(session, &focus.frame).await;
        if expected.is_some_and(|node| node != focus.node) {
            return Err(denied(
                shared,
                agent,
                first,
                origin.as_ref().map(Origin::as_str),
                "focus",
            ));
        }
        Ok((focus, origin))
    }

    /// Types resolved text, then re-checks the frame's origin (detection only: the text is
    /// already in) and records the use.
    async fn insert_prepared(
        &mut self,
        shared: &Shared,
        agent: &str,
        session: &str,
        focus: &Focus,
        origin: Option<Origin>,
        prepared: Prepared,
    ) -> Result<Value> {
        let inserted = self.insert_text(session, &prepared.text).await;
        drop(prepared.text);
        inserted?;
        let after = self.frame_origin(session, &focus.frame).await;
        let changed = after != origin;
        for item in &prepared.items {
            let used = AuditEvent::new(agent, AuditKind::SecretUsed, "ok")
                .session(shared.id)
                .item(item.as_str())
                .origin(origin.as_ref().map(Origin::as_str));
            record(shared, &used);
            if changed {
                let moved = AuditEvent::new(
                    agent,
                    AuditKind::OriginChangedAfterFill,
                    after.as_ref().map_or("unknown", Origin::as_str),
                )
                .session(shared.id)
                .item(item.as_str())
                .origin(origin.as_ref().map(Origin::as_str));
                record(shared, &moved);
            }
        }
        Ok(if changed {
            json!({"warning": "origin_changed_after_fill"})
        } else {
            json!({})
        })
    }

    /// Follows `document.activeElement` down through same-process iframes, evaluating in an
    /// isolated world in each frame so page scripts cannot redefine what it returns.
    async fn focus(&self, session: &str) -> Result<Focus> {
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
    async fn frame_origin(&self, session: &str, frame: &str) -> Option<Origin> {
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

    async fn insert_text(&self, session: &str, text: &str) -> Result<()> {
        self.call("Input.insertText", json!({"text": text}), session)
            .await
            .map(|_| ())
            .map_err(|_| anyhow!("could not insert text"))
    }

    async fn press(&mut self, key: &str) -> Result<Value> {
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
        let session = self.page().await?;
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

    async fn tabs(&mut self) -> Result<Value> {
        let tabs = self.page_tabs().await?;
        let active = match &self.active {
            Some(id) if tabs.iter().any(|t| &t.id == id) => Some(id.clone()),
            _ => tabs.first().map(|t| t.id.clone()),
        };
        Ok(Value::Array(
            tabs.into_iter()
                .map(|t| {
                    let is_active = Some(&t.id) == active.as_ref();
                    json!({"id": t.id, "url": t.url, "title": t.title, "active": is_active})
                })
                .collect(),
        ))
    }

    async fn new_tab(&mut self, url: Option<&str>) -> Result<Value> {
        let url = match url {
            Some(url) => {
                keyharbor_domain::validate_navigation_url(url).map_err(|e| anyhow!("{e}"))?
            }
            None => "about:blank".to_owned(),
        };
        let created = self
            .cdp
            .call("Target.createTarget", json!({"url": url}), None)
            .await?;
        let id = created["targetId"]
            .as_str()
            .context("no target id")?
            .to_owned();
        self.activate(&id).await?;
        Ok(json!({"id": id}))
    }

    async fn switch_tab(&mut self, id: &str) -> Result<Value> {
        if !self.page_tabs().await?.iter().any(|t| t.id == id) {
            bail!("no such tab");
        }
        self.activate(id).await?;
        Ok(json!({}))
    }

    async fn activate(&mut self, id: &str) -> Result<()> {
        self.cdp
            .call("Target.activateTarget", json!({"targetId": id}), None)
            .await?;
        self.attach(id).await?;
        self.active = Some(id.to_owned());
        Ok(())
    }

    async fn wait(&mut self, request: &Value) -> Result<Value> {
        if let Some(selector) = request.get("selector").and_then(Value::as_str) {
            let expression = format!(
                "document.querySelector({}) !== null",
                serde_json::to_string(selector)?
            );
            let deadline = Instant::now() + WAIT_TIMEOUT;
            loop {
                // The page may be between documents; errors mean "not yet".
                if let Ok(session) = self.page().await {
                    let found = self
                        .call(
                            "Runtime.evaluate",
                            json!({"expression": expression, "returnByValue": true}),
                            &session,
                        )
                        .await;
                    if found.is_ok_and(|v| v["result"]["value"] == true) {
                        return Ok(json!({}));
                    }
                }
                if Instant::now() >= deadline {
                    bail!("timed out waiting for the selector");
                }
                tokio::time::sleep(POLL).await;
            }
        }
        let ms = request
            .get("ms")
            .and_then(Value::as_u64)
            .context("wait needs ms or selector")?;
        tokio::time::sleep(Duration::from_millis(ms).min(WAIT_TIMEOUT)).await;
        Ok(json!({}))
    }
}

/// Best effort: a failed write to the usage record does not undo or block the operation.
fn record(shared: &Shared, event: &AuditEvent) {
    if let Ok(store) = shared.store.lock() {
        let _ = store.record(event);
    }
}

/// Records a refused use and returns the error the agent sees: item name and reason only.
fn denied(
    shared: &Shared,
    agent: &str,
    item: &ItemName,
    origin: Option<&str>,
    reason: &str,
) -> anyhow::Error {
    record(
        shared,
        &AuditEvent::new(agent, AuditKind::SecretDenied, reason)
            .session(shared.id)
            .item(item.as_str())
            .origin(origin),
    );
    anyhow!("denied: {} ({reason})", item.as_str())
}

fn str_arg<'a>(request: &'a Value, name: &str) -> Result<&'a str> {
    request
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("missing {name}"))
}

fn target(request: &Value) -> Result<&Value> {
    request.get("target").context("missing target")
}

/// Renders the accessibility tree as indented lines `[ref] role "name" value="…"`, where ref is
/// the backend DOM node id. Ignored nodes, inline text boxes and unnamed generic containers are
/// left out (their children move up a level).
pub(crate) fn render_ax_tree(nodes: &[Value]) -> String {
    let by_id: HashMap<&str, &Value> = nodes
        .iter()
        .filter_map(|n| n["nodeId"].as_str().map(|id| (id, n)))
        .collect();
    let mut stack: Vec<(&Value, usize)> = nodes
        .iter()
        .filter(|n| {
            n["parentId"]
                .as_str()
                .is_none_or(|parent| !by_id.contains_key(parent))
        })
        .rev()
        .map(|n| (n, 0))
        .collect();
    let mut lines = Vec::new();
    while let Some((node, depth)) = stack.pop() {
        if lines.len() >= MAX_SNAPSHOT_LINES {
            lines.push(format!("… truncated at {MAX_SNAPSHOT_LINES} lines"));
            break;
        }
        let role = node["role"]["value"].as_str().unwrap_or("");
        let name = node["name"]["value"].as_str().unwrap_or("");
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
            if let Some(value) = node["value"]["value"].as_str().filter(|v| !v.is_empty()) {
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
                    stack.push((child, child_depth));
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
            render_ax_tree(&nodes),
            "[10] RootWebArea \"Login\"\n  [12] textbox \"User\" value=\"alice\"\n  [13] button \"로그인\"\n  [-] StaticText \"say \\\"hi\\\"\""
        );
    }

    #[test]
    fn ax_tree_is_capped() {
        let mut nodes = vec![
            json!({"nodeId": "0", "role": {"value": "RootWebArea"}, "name": {"value": "r"}, "childIds": (1..3000).map(|i| i.to_string()).collect::<Vec<_>>()}),
        ];
        for i in 1..3000 {
            nodes.push(json!({"nodeId": i.to_string(), "parentId": "0", "role": {"value": "link"}, "name": {"value": "x"}}));
        }
        let text = render_ax_tree(&nodes);
        assert_eq!(text.lines().count(), MAX_SNAPSHOT_LINES + 1);
        assert!(text.ends_with("truncated at 2000 lines"));
    }
}
