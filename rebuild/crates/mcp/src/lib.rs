//! `keyharbor mcp`: a stdio MCP server with session, browser and vault-metadata tools.
//!
//! Browser tools go through the session supervisor's control socket (ADR-0004); this server
//! never talks CDP itself. No tool returns a vault value: placeholders are resolved and
//! responses masked in the supervisor. Session tools apply the agent's policy (02 §2.8).

use anyhow::{Context, Result, bail};
use keyharbor_broker::{
    client,
    lifecycle::{self, DEFAULT_BROWSER, OpenOptions},
    paths, supervisor,
};
use keyharbor_core::policy::{Action, Decision, decide};
use keyharbor_domain::ports::*;
use keyharbor_domain::{
    RequestId, SessionId, SessionName, State,
    vault::{Domain, ItemKind},
};
use keyharbor_sqlite::SqliteStore;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};

pub struct Options {
    pub data: PathBuf,
    /// Only this session is visible and usable.
    pub session: Option<String>,
    pub allow_cdp: bool,
    /// The agent's name for policies and the usage record; `mcp` when absent.
    pub agent: Option<String>,
    pub browser: Option<PathBuf>,
}

/// Runs the server on stdin/stdout until the client disconnects.
pub fn run(options: Options) -> Result<()> {
    if let Some(agent) = &options.agent {
        if agent == keyharbor_domain::policy::ANY {
            bail!("agent name * is reserved for rules that apply to every agent");
        }
        keyharbor_domain::policy::parse_agent(agent).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    let scope = match &options.session {
        Some(id) => {
            let id = SessionId::parse(id).context("invalid session id")?;
            SqliteStore::open(&options.data)?
                .get(id)?
                .context("session not found")?;
            let lock = supervisor::acquire_lock(&paths::mcp_lock_path(&options.data, &id))
                .context("another scoped MCP server already owns this session")?;
            Some((id, lock))
        }
        None => None,
    };
    let server = Server {
        inner: Arc::new(Inner {
            exe: std::env::current_exe()?,
            data: options.data,
            scope: scope.as_ref().map(|(id, _)| *id),
            allow_cdp: options.allow_cdp,
            agent: options.agent,
            browser: options
                .browser
                .unwrap_or_else(|| PathBuf::from(DEFAULT_BROWSER)),
        }),
        tool_router: router(options.allow_cdp),
    };
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            server.serve(stdio()).await?.waiting().await?;
            anyhow::Ok(())
        })?;
    drop(scope);
    Ok(())
}

fn router(allow_cdp: bool) -> ToolRouter<Server> {
    let mut router = Server::tool_router();
    if !allow_cdp {
        router.remove_route("session_endpoint");
    }
    router
}

struct Inner {
    exe: PathBuf,
    data: PathBuf,
    scope: Option<SessionId>,
    allow_cdp: bool,
    agent: Option<String>,
    browser: PathBuf,
}

/// The agent name used when the server was started without `--agent`.
pub const DEFAULT_AGENT: &str = "mcp";

impl Inner {
    /// The name policies and the usage record know this server's agent by.
    fn agent(&self) -> &str {
        self.agent.as_deref().unwrap_or(DEFAULT_AGENT)
    }

    /// Fails unless the policy allows this server's agent `action`.
    fn allow(&self, action: &Action) -> Result<()> {
        let rules = SqliteStore::open(&self.data)?.policy_rules()?;
        match decide(&rules, self.agent(), action) {
            Decision::Allow => Ok(()),
            Decision::Deny { rule } => bail!("denied by policy ({rule})"),
        }
    }
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<Inner>,
    tool_router: ToolRouter<Server>,
}

fn ok(value: Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}
fn fail(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}
fn outcome(result: Result<Value>) -> CallToolResult {
    match result {
        Ok(value) => ok(value),
        Err(error) => fail(format!("{error:#}")),
    }
}

fn row_json(data: &std::path::Path, row: &SessionRow) -> Value {
    json!({
        "id": row.id.to_string(),
        "name": row.name.as_str(),
        "state": lifecycle::display_state(data, row),
        "cdp": row.cdp,
    })
}

impl Server {
    /// The session id an argument names, if this server may use it.
    fn session(&self, id: &str) -> Result<SessionId> {
        let id = SessionId::parse(id).context("invalid session id")?;
        if self.inner.scope.is_some_and(|scope| scope != id) {
            bail!("session is outside this server's scope");
        }
        Ok(id)
    }

    /// Runs blocking store or socket work off the async threads.
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Inner) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || work(&inner))
            .await
            .context("task failed")?
    }

    async fn browser(&self, session: &str, mut request: Value) -> CallToolResult {
        request["agent"] = json!(self.inner.agent());
        let id = match self.session(session) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(&Action::OpenSession(id))?;
                client::call(&inner.data, &id, &request)
            })
            .await,
        )
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
struct Name {
    /// Stored locally in plain text; must not contain secrets.
    name: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Id {
    id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct OpenSession {
    id: String,
    /// HTTPS URL, or HTTP on localhost, without credentials, query or fragment.
    url: Option<String>,
    /// Also expose a raw CDP endpoint (bypasses KeyHarbor); needs the server's --allow-cdp.
    #[serde(default)]
    cdp: bool,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct WaitAuth {
    id: String,
    /// Site origin the user should sign in to (no path), e.g. https://example.com. Not verified.
    origin: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct WaitForResume {
    id: String,
    /// Request ID returned by session_wait_auth.
    request_id: String,
    /// Maximum wait in seconds, 1 to 300; default 120.
    timeout_seconds: Option<u64>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Session {
    /// Session ID.
    session: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Navigate {
    session: String,
    /// HTTP or HTTPS URL.
    url: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Target {
    session: String,
    /// Element ref from browser_snapshot (the number in brackets).
    #[serde(rename = "ref")]
    reference: Option<u64>,
    /// CSS selector, used when ref is not given.
    selector: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Fill {
    session: String,
    /// Element ref from browser_snapshot.
    #[serde(rename = "ref")]
    reference: Option<u64>,
    /// CSS selector, used when ref is not given.
    selector: Option<String>,
    /// Text that replaces the element's value. May contain {{secret:item.field}} placeholders.
    text: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Text {
    session: String,
    /// Text typed into the focused element. May contain {{secret:item.field}} placeholders.
    text: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Key {
    session: String,
    /// Enter, Tab, Escape, Backspace, Delete, Space, ArrowUp/Down/Left/Right, Home, End, PageUp or PageDown.
    key: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct NewTab {
    session: String,
    /// HTTP or HTTPS URL; about:blank when omitted.
    url: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct SwitchTab {
    session: String,
    /// Tab id from browser_tabs.
    id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Wait {
    session: String,
    /// Milliseconds to sleep (at most 15000).
    ms: Option<u64>,
    /// Wait until an element matches this CSS selector (at most 15 seconds).
    selector: Option<String>,
}

fn target(reference: Option<u64>, selector: Option<String>) -> Result<Value> {
    match (reference, selector) {
        (Some(reference), _) => Ok(json!({"ref": reference})),
        (None, Some(selector)) => Ok(json!({"selector": selector})),
        (None, None) => bail!("give ref or selector"),
    }
}

#[tool_router]
impl Server {
    #[tool(
        description = "List KeyHarbor browser sessions with their state. recovery_required means the supervisor is gone; the user must run recover. States do not prove that a site is signed in."
    )]
    async fn session_list(&self) -> CallToolResult {
        outcome(
            self.blocking(|inner| {
                let rows = SqliteStore::open(&inner.data)?.list()?;
                Ok(Value::Array(
                    rows.iter()
                        .filter(|row| inner.scope.is_none_or(|scope| scope == row.id))
                        .map(|row| row_json(&inner.data, row))
                        .collect(),
                ))
            })
            .await,
        )
    }

    #[tool(description = "Create a stopped browser session with its own browser profile.")]
    async fn session_create(&self, Parameters(Name { name }): Parameters<Name>) -> CallToolResult {
        if self.inner.scope.is_some() {
            return fail("session creation is disabled for this scoped server");
        }
        outcome(
            self.blocking(move |inner| {
                inner.allow(&Action::CreateSession)?;
                let name = SessionName::parse(&name)?;
                let session = SqliteStore::open(&inner.data)?.create(name)?;
                Ok(json!({"id": session.id.to_string(), "name": session.name.as_str(), "state": session.state.as_str()}))
            })
            .await,
        )
    }

    #[tool(
        description = "Open a session's browser, optionally at a URL. Do not put secrets in URLs. Use the browser_* tools to operate it."
    )]
    async fn session_open(
        &self,
        Parameters(OpenSession { id, url, cdp }): Parameters<OpenSession>,
    ) -> CallToolResult {
        if cdp && !self.inner.allow_cdp {
            return fail(
                "raw CDP is disabled; the user must start the MCP server with --allow-cdp",
            );
        }
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(&Action::OpenSession(id))?;
                if cdp {
                    inner.allow(&Action::RawCdp)?;
                }
                let options = OpenOptions {
                    url: url.as_deref(),
                    cdp,
                    browser: &inner.browser,
                    agent: inner.agent(),
                };
                lifecycle::open(&inner.data, id, &options, &inner.exe)?;
                Ok(json!({"id": id.to_string(), "state": "running"}))
            })
            .await,
        )
    }

    #[tool(
        description = "Close the session's browser and stop the session. This does not sign out of websites."
    )]
    async fn session_stop(&self, Parameters(Id { id }): Parameters<Id>) -> CallToolResult {
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                let state = lifecycle::stop(&inner.data, id, inner.agent())?;
                Ok(json!({"id": id.to_string(), "state": state.as_str()}))
            })
            .await,
        )
    }

    #[tool(
        description = "Ask the user to sign in or solve a challenge in the browser window themselves. Returns a request_id; then call session_wait_for_resume. The user resumes with `keyharbor resume <id> <request_id>`."
    )]
    async fn session_wait_auth(
        &self,
        Parameters(WaitAuth { id, origin }): Parameters<WaitAuth>,
    ) -> CallToolResult {
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                let request = lifecycle::wait_auth(&inner.data, id, origin.as_deref(), inner.agent())?;
                Ok(json!({"id": id.to_string(), "state": "awaiting_user", "request_id": request.to_string()}))
            })
            .await,
        )
    }

    #[tool(
        description = "Wait until the user resumes the given authentication request, up to timeout_seconds (1–300, default 120). Success means the user said they are done; sign-in success is not verified."
    )]
    async fn session_wait_for_resume(
        &self,
        Parameters(WaitForResume {
            id,
            request_id,
            timeout_seconds,
        }): Parameters<WaitForResume>,
    ) -> CallToolResult {
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        let Ok(request) = RequestId::parse(&request_id) else {
            return fail("invalid request_id");
        };
        let timeout = timeout_seconds.unwrap_or(120);
        if !(1..=300).contains(&timeout) {
            return fail("timeout_seconds must be between 1 and 300");
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
        loop {
            let snapshot = self
                .blocking(move |inner| {
                    let store = SqliteStore::open(&inner.data)?;
                    let status = store.handoff_status(id, request)?;
                    let row = store.get(id)?;
                    Ok((status, row, supervisor::is_alive(&inner.data, &id)))
                })
                .await;
            let (status, row, alive) = match snapshot {
                Ok(snapshot) => snapshot,
                Err(error) => return fail(format!("{error:#}")),
            };
            use keyharbor_domain::HandoffStatus::*;
            match status {
                None => return fail("authentication request not found"),
                Some(Superseded) => return fail("request was replaced by a newer one"),
                Some(Cancelled) => {
                    return fail("request was cancelled because the session stopped");
                }
                Some(Resumed) => {
                    let state = row.map(|r| r.state);
                    return if state == Some(State::Running) {
                        ok(
                            json!({"id": id.to_string(), "state": "running", "note": "the user resumed; sign-in success is not verified"}),
                        )
                    } else {
                        fail(format!(
                            "request was resumed but the session is now {}",
                            state.map_or("gone", State::as_str)
                        ))
                    };
                }
                Some(Pending) if !alive => {
                    return fail("session supervisor is not running; the user must run recover");
                }
                Some(Pending) => (),
            }
            if tokio::time::Instant::now() >= deadline {
                return fail(
                    "Timed out waiting for resume; the session may still be awaiting the user",
                );
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + Duration::from_millis(250)).min(deadline),
            )
            .await;
        }
    }

    #[tool(
        description = "Raw CDP WebSocket endpoint of a session opened with cdp: true. Anyone holding it has full control of the browser and bypasses KeyHarbor's protections."
    )]
    async fn session_endpoint(&self, Parameters(Id { id }): Parameters<Id>) -> CallToolResult {
        if !self.inner.allow_cdp {
            return fail(
                "raw CDP is disabled; the user must start the MCP server with --allow-cdp",
            );
        }
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(&Action::OpenSession(id))?;
                inner.allow(&Action::RawCdp)?;
                Ok(json!({"endpoint": lifecycle::endpoint(&inner.data, id)?, "warning": "full browser control; KeyHarbor protections do not apply"}))
            })
            .await,
        )
    }

    #[tool(
        description = "Navigate the active tab to an HTTP(S) URL and wait for it to load. Returns url and title."
    )]
    async fn browser_navigate(
        &self,
        Parameters(Navigate { session, url }): Parameters<Navigate>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "navigate", "url": url}))
            .await
    }

    #[tool(
        description = "Accessibility snapshot of the active tab: one line per element, `[ref] role \"name\"`. Use ref with browser_click and browser_fill."
    )]
    async fn browser_snapshot(
        &self,
        Parameters(Session { session }): Parameters<Session>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "snapshot"})).await
    }

    #[tool(description = "Save a PNG screenshot of the active tab and return its file path.")]
    async fn browser_screenshot(
        &self,
        Parameters(Session { session }): Parameters<Session>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "screenshot"})).await
    }

    #[tool(description = "Click an element given by ref (from browser_snapshot) or CSS selector.")]
    async fn browser_click(
        &self,
        Parameters(Target {
            session,
            reference,
            selector,
        }): Parameters<Target>,
    ) -> CallToolResult {
        match target(reference, selector) {
            Ok(target) => {
                self.browser(&session, json!({"op": "click", "target": target}))
                    .await
            }
            Err(error) => fail(error.to_string()),
        }
    }

    #[tool(
        description = "Replace the value of an input given by ref or CSS selector with text. To enter a secret, write its placeholder from vault_list, e.g. {{secret:github.password}}: KeyHarbor types the value only if the policy allows it and, for origin-bound items, the input's frame is one of the item's origins. You never see the value; results show placeholders instead."
    )]
    async fn browser_fill(
        &self,
        Parameters(Fill {
            session,
            reference,
            selector,
            text,
        }): Parameters<Fill>,
    ) -> CallToolResult {
        match target(reference, selector) {
            Ok(target) => {
                self.browser(
                    &session,
                    json!({"op": "fill", "target": target, "text": text}),
                )
                .await
            }
            Err(error) => fail(error.to_string()),
        }
    }

    #[tool(
        description = "Type text into the focused element. Accepts {{secret:item.field}} placeholders under the same checks as browser_fill."
    )]
    async fn browser_type(
        &self,
        Parameters(Text { session, text }): Parameters<Text>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "type", "text": text}))
            .await
    }

    #[tool(description = "Press a key in the active tab, e.g. Enter or Tab.")]
    async fn browser_press(
        &self,
        Parameters(Key { session, key }): Parameters<Key>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "press", "key": key}))
            .await
    }

    #[tool(description = "List the browser's tabs with id, url, title and which one is active.")]
    async fn browser_tabs(
        &self,
        Parameters(Session { session }): Parameters<Session>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "tabs"})).await
    }

    #[tool(description = "Open a new tab (optionally at a URL) and make it active.")]
    async fn browser_new_tab(
        &self,
        Parameters(NewTab { session, url }): Parameters<NewTab>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "new_tab", "url": url}))
            .await
    }

    #[tool(description = "Make the tab with the given id (from browser_tabs) active.")]
    async fn browser_switch_tab(
        &self,
        Parameters(SwitchTab { session, id }): Parameters<SwitchTab>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "switch_tab", "id": id}))
            .await
    }

    #[tool(
        description = "Wait for a CSS selector to match (up to 15 s) or for a number of milliseconds."
    )]
    async fn browser_wait(
        &self,
        Parameters(Wait {
            session,
            ms,
            selector,
        }): Parameters<Wait>,
    ) -> CallToolResult {
        let mut request = json!({"op": "wait"});
        if let Some(ms) = ms {
            request["ms"] = json!(ms);
        }
        if let Some(selector) = selector {
            request["selector"] = json!(selector);
        }
        self.browser(&session, request).await
    }

    #[tool(
        description = "List vault items: name, kind, domain, field names, origins, status and the placeholder for each field (use it in browser_fill or browser_type). Values are never returned."
    )]
    async fn vault_list(&self) -> CallToolResult {
        outcome(
            self.blocking(|inner| {
                let store = SqliteStore::open(&inner.data)?;
                let mut items = Vec::new();
                for row in store.item_list()? {
                    let kind = ItemKind::parse(&row.kind);
                    let fields: Vec<String> = store
                        .item_get(&row.name)?
                        .map(|(_, fields)| fields.into_iter().map(|f| f.field).collect())
                        .unwrap_or_default();
                    let placeholders: Vec<String> = fields
                        .iter()
                        .map(|f| format!("{{{{secret:{}.{f}}}}}", row.name))
                        .collect();
                    items.push(json!({
                        "name": row.name,
                        "kind": kind.map(ItemKind::as_str),
                        "domain": kind.map(ItemKind::domain).map(Domain::as_str),
                        "fields": fields,
                        "placeholders": placeholders,
                        "origins": row.origins,
                        "origin_binding": row.origin_binding,
                        "status": row.status,
                        "source": row.source,
                    }));
                }
                Ok(Value::Array(items))
            })
            .await,
        )
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {}
