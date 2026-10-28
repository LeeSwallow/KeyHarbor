//! The MCP server state, result helpers and the tool router.

use anyhow::{Context, Result, anyhow};
use keyharbor_broker::client;
use keyharbor_domain::{
    SessionId,
    policy::AgentName,
    ports::{SessionRuntime, Store},
};
use keyharbor_guard::{
    SharedStore, VaultOpener,
    session::{SessionOp, authorize},
};
use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{CallToolResult, ContentBlock},
    tool_handler,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, MutexGuard},
};

pub(crate) struct Inner {
    pub store: SharedStore,
    pub open_vault: VaultOpener,
    pub runtime: Box<dyn SessionRuntime>,
    pub data: PathBuf,
    pub scope: Option<SessionId>,
    pub allow_cdp: bool,
    pub agent: AgentName,
    pub browser: PathBuf,
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

    pub(crate) async fn browser(&self, session: &str, mut request: Value) -> CallToolResult {
        request["agent"] = json!(self.inner.agent());
        let id = match self.session(session) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(SessionOp::Browser(id))?;
                client::call(&inner.data, &id, &request)
            })
            .await,
        )
    }
}

/// Every tool; `session_endpoint` only with `--allow-cdp`.
fn router(allow_cdp: bool) -> ToolRouter<Server> {
    let mut router = Server::session_tools() + Server::browser_tools() + Server::vault_tools();
    if !allow_cdp {
        router.remove_route("session_endpoint");
    }
    router
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {}
