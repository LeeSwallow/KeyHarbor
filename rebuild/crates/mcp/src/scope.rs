//! Session scope: a server started with `--session ID` sees and uses only that session, and
//! only one such server runs per session (`run/mcp-<id>.lock`).

use crate::server::{Server, lock};
use anyhow::{Context, Result, bail};
use keyharbor_broker::{paths, supervisor};
use keyharbor_domain::SessionId;
use keyharbor_guard::SharedStore;
use std::{fs::File, path::Path};

/// Checks the session exists and takes its scoped-server lock.
pub(crate) fn claim(store: &SharedStore, data: &Path, id: &str) -> Result<(SessionId, File)> {
    let id = SessionId::parse(id).context("invalid session id")?;
    keyharbor_core::session::session(&**lock(store)?, id)?;
    let lock = supervisor::acquire_lock(&paths::mcp_lock_path(data, &id))
        .context("another scoped MCP server already owns this session")?;
    Ok((id, lock))
}

impl Server {
    /// The session id an argument names, if this server may use it.
    pub(crate) fn session(&self, id: &str) -> Result<SessionId> {
        let id = SessionId::parse(id).context("invalid session id")?;
        if self.inner.scope.is_some_and(|scope| scope != id) {
            bail!("session is outside this server's scope");
        }
        Ok(id)
    }
}
