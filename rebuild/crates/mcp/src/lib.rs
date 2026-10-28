//! `keyharbor mcp`: a stdio MCP server with session, browser and vault-metadata tools.
//!
//! Browser tools go through the session supervisor's control socket (ADR-0004); this server
//! never talks CDP itself. No tool returns a vault value: placeholders are resolved and
//! responses masked in the supervisor. Session tools apply the agent's policy through the
//! guard's table (`keyharbor_guard::session`, 02 §2.8). The binary's composition root hands
//! the server its store, vault opener and session runtime ([`Deps`]).

mod scope;
mod server;
mod tools;

pub use server::DEFAULT_AGENT;

use anyhow::{Result, anyhow};
use keyharbor_broker::DEFAULT_BROWSER;
use keyharbor_domain::{policy::AgentName, ports::SessionRuntime};
use keyharbor_guard::{SharedStore, VaultOpener};
use rmcp::{ServiceExt, transport::stdio};
use server::{Inner, Server};
use std::path::PathBuf;

pub struct Options {
    /// The data directory: supervisors' control sockets and the scoped-server lock live here.
    pub data: PathBuf,
    /// Only this session is visible and usable.
    pub session: Option<String>,
    pub allow_cdp: bool,
    /// The agent's name for policies and the usage record; `mcp` when absent.
    pub agent: Option<String>,
    pub browser: Option<PathBuf>,
}

/// What the server runs on. Built once, by the binary's composition root.
pub struct Deps {
    /// The one store every tool uses (calls are serialised on it).
    pub store: SharedStore,
    /// Opens the vault when a tool needs verified item metadata.
    pub open_vault: VaultOpener,
    pub runtime: Box<dyn SessionRuntime>,
}

/// Runs the server on stdin/stdout until the client disconnects.
pub fn run(options: Options, deps: Deps) -> Result<()> {
    let agent = match &options.agent {
        Some(name) => AgentName::parse(name).map_err(|e| anyhow!("{e}"))?,
        None => AgentName::parse(DEFAULT_AGENT).expect("valid default agent name"),
    };
    let scope = match &options.session {
        Some(id) => Some(scope::claim(&deps.store, &options.data, id)?),
        None => None,
    };
    let server = Server::new(Inner {
        store: deps.store,
        open_vault: deps.open_vault,
        runtime: deps.runtime,
        data: options.data,
        scope: scope.as_ref().map(|(id, _)| *id),
        allow_cdp: options.allow_cdp,
        agent,
        browser: options
            .browser
            .unwrap_or_else(|| PathBuf::from(DEFAULT_BROWSER)),
    });
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
