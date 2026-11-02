//! The MCP server state, result helpers and the tool router.

use anyhow::{Context, Result, anyhow};
use keyharbor_domain::{
    SessionId,
    browser_choice::BrowserChoice,
    policy::AgentName,
    ports::{Clock, SessionRuntime, Store},
};
use keyharbor_guard::{
    SharedStore, VaultOpener,
    session::{SessionOp, authorize},
};
use keyharbor_supervisor::control::{
    client,
    wire::{ControlEnvelope, ControlRequest},
};
use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{CallToolResult, ContentBlock},
    tool_handler,
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, MutexGuard},
};

pub(crate) struct Inner {
    pub store: SharedStore,
    pub open_vault: VaultOpener,
    pub runtime: Box<dyn SessionRuntime>,
    pub clock: Arc<dyn Clock>,
    pub data: PathBuf,
    pub scope: Option<SessionId>,
    pub allow_cdp: bool,
    pub agent: AgentName,
    /// The server's `--browser`; `None` means the person's `browser` setting.
    pub browser: Option<BrowserChoice>,
    pub interaction: Interaction,
}

/// Whether a person may be watching this server (`--non-interactive` says no, #119).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interaction {
    /// Tools that wait for a person wait, up to their timeout.
    Interactive,
    /// Nobody will answer soon: wait tools look once and say what is pending.
    NonInteractive,
}

/// The agent name used when the server was started without `--agent`.
pub const DEFAULT_AGENT: &str = "mcp";

impl Inner {
    /// The name policies and the usage record know this server's agent by.
    pub fn agent(&self) -> &str {
        self.agent.as_str()
    }

    /// The store, held until the guard is dropped (keep it to one use case).
    pub fn store(&self) -> Result<MutexGuard<'_, Box<dyn Store>>> {
        lock(&self.store)
    }

    /// How long a wait tool waits: what the agent asked for, or one look when nobody watches.
    pub fn wait(&self, requested: Option<u64>) -> Result<keyharbor_core::session::ResumeWait> {
        match self.interaction {
            Interaction::Interactive => keyharbor_core::session::ResumeWait::seconds(requested),
            Interaction::NonInteractive => Ok(keyharbor_core::session::ResumeWait::ONCE),
        }
    }

    pub fn is_watched(&self) -> bool {
        self.interaction == Interaction::Interactive
    }

    /// Fails unless the policy allows this server's agent `op`.
    pub fn allow(&self, op: SessionOp) -> Result<()> {
        authorize(&**self.store()?, self.agent(), op)
    }
}

pub(crate) fn lock(store: &SharedStore) -> Result<MutexGuard<'_, Box<dyn Store>>> {
    store.lock().map_err(|_| anyhow!("store lock poisoned"))
}

#[derive(Clone)]
pub struct Server {
    pub(crate) inner: Arc<Inner>,
    tool_router: ToolRouter<Server>,
}

pub(crate) fn ok(value: Value) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}
pub(crate) fn fail(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}
pub(crate) fn outcome(result: Result<Value>) -> CallToolResult {
    match result {
        Ok(value) => ok(value),
        Err(error) => fail(format!("{error:#}")),
    }
}

impl Server {
    pub(crate) fn new(inner: Inner) -> Self {
        let allow_cdp = inner.allow_cdp;
        Self {
            inner: Arc::new(inner),
            tool_router: router(allow_cdp),
        }
    }
    /// Runs blocking store or socket work off the async threads.
    pub(crate) async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Inner) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || work(&inner))
            .await
            .context("task failed")?
    }

    pub(crate) async fn browser(&self, session: &str, request: ControlRequest) -> CallToolResult {
        let request = ControlEnvelope::from_agent(self.inner.agent(), request);
        let id = match self.session(session) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(SessionOp::Browser(id))?;
                client::request(&inner.data, &id, &request)
            })
            .await,
        )
    }
}

/// The names of the tools a server offers (for `keyharbor mcp --list-tools`, which the plugin
/// checks its skills against), sorted.
pub(crate) fn tool_names(allow_cdp: bool) -> Vec<String> {
    let mut names: Vec<String> = router(allow_cdp)
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    names.sort();
    names
}

/// Every tool; `session_endpoint` only with `--allow-cdp`.
fn router(allow_cdp: bool) -> ToolRouter<Server> {
    let mut router = Server::session_tools()
        + Server::browser_tools()
        + Server::site_tools()
        + Server::vault_tools()
        + Server::request_tools();
    if !allow_cdp {
        router.remove_route("session_endpoint");
    }
    router
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {}
