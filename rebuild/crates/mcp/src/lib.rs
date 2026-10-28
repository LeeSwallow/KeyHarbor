//! `keyharbor mcp`: a stdio MCP server with session, browser and vault-metadata tools.
//!
//! Browser tools go through the session supervisor's control socket (ADR-0004); this server
//! never talks CDP itself. No tool returns a vault value: placeholders are resolved and
//! responses masked in the supervisor. Session tools apply the agent's policy (02 §2.8).

mod scope;
mod server;
mod tools;

pub use server::DEFAULT_AGENT;

use anyhow::{Result, bail};
use keyharbor_broker::{DEFAULT_BROWSER, ProcessRuntime};
use rmcp::{ServiceExt, transport::stdio};
use server::{Inner, Server};
use std::path::PathBuf;

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
        Some(id) => Some(scope::claim(&options.data, id)?),
        None => None,
    };
    let server = Server::new(Inner {
        runtime: ProcessRuntime::new(&options.data, &std::env::current_exe()?),
        data: options.data,
        scope: scope.as_ref().map(|(id, _)| *id),
        allow_cdp: options.allow_cdp,
        agent: options.agent,
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
