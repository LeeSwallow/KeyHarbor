//! Handing the browser to the user: request, inspect and resume (02 §2.5.4).

use super::{note, note_event, require_supervisor, session};
use anyhow::{Context, Result};
use keyharbor_domain::{
    RequestId, SessionId,
    audit::{AuditEvent, AuditKind},
    ports::{SessionRuntime, Store},
    vault::Origin,
};

/// Marks a running session as waiting for the user and returns the new request ID.
pub fn wait_auth(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    id: SessionId,
    origin: Option<&str>,
    agent: &str,
) -> Result<RequestId> {
    let row = session(store, id)?;
    require_supervisor(runtime, id)?;
    let generation = row.generation.context("session is not running")?;
    let request = store.request_handoff(id, generation, origin)?;
    let origin = origin.and_then(|o| Origin::parse(o).ok());
    note_event(
        store,
        AuditEvent::new(agent, AuditKind::HandoffRequested, "awaiting_user")
            .session(id)
            .origin(origin.as_ref().map(Origin::as_str)),
    );
    Ok(request)
}

/// The pending request and its claimed origin.
pub fn pending_handoff(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    id: SessionId,
) -> Result<(RequestId, Option<String>)> {
    session(store, id)?;
    require_supervisor(runtime, id)?;
    store
        .pending_handoff(id)?
        .context("no pending authentication request")
}

pub fn resume(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    id: SessionId,
    request: &str,
    agent: &str,
) -> Result<()> {
    let request = RequestId::parse(request).context("invalid request id")?;
    let row = session(store, id)?;
    require_supervisor(runtime, id)?;
    let generation = row.generation.context("session is not running")?;
    store.resume_handoff(id, generation, request)?;
    note(store, agent, AuditKind::HandoffResumed, id, "running");
    Ok(())
}
