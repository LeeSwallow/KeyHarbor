//! The session states a supervisor records for its run (`generation`): it checks the
//! reservation, reports the browser started or failed, and records how the browser ended.
//! The supervisor process (`keyharbor-supervisor`) calls these; the rules are here.

use super::session;
use anyhow::{Result, bail};
use keyharbor_domain::{
    Generation, SessionId, State,
    audit::{AuditEvent, AuditKind},
    ports::{SessionStore, Store},
};

/// How the browser of a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserExit {
    /// It exited cleanly or was asked to close.
    Clean,
    /// It crashed or was killed.
    Unexpected,
}

/// Fails unless the session is still `starting` under `generation`.
pub fn check_reservation(
    store: &dyn SessionStore,
    id: SessionId,
    generation: Generation,
) -> Result<()> {
    let row = session(store, id)?;
    if row.state != State::Starting || row.generation != Some(generation) {
        bail!("session reservation is no longer valid");
    }
    Ok(())
}

/// Records that the run failed to start. Best effort: the caller is already failing.
pub fn mark_failed(store: &dyn SessionStore, id: SessionId, generation: Generation) {
    let _ = store.transition(id, generation, State::Starting, State::Failed);
}

/// Records that the browser is up; `false` if the reservation changed meanwhile.
pub fn mark_running(
    store: &dyn SessionStore,
    id: SessionId,
    generation: Generation,
) -> Result<bool> {
    store.transition(id, generation, State::Starting, State::Running)
}

/// Records `stopped` (clean exit, requested close, or `stop` in progress) or `failed`.
/// `Ok(true)` once nothing is left to record; `Ok(false)` if the state moved under us and the
/// caller should retry.
pub fn mark_exited(
    store: &dyn Store,
    id: SessionId,
    generation: Generation,
    exit: BrowserExit,
) -> Result<bool> {
    let Some(row) = store.get(id)? else {
        return Ok(true);
    };
    if row.generation != Some(generation) {
        return Ok(true);
    }
    if !matches!(
        row.state,
        State::Running | State::AwaitingUser | State::Stopping
    ) {
        return Ok(true);
    }
    let next = if exit == BrowserExit::Clean || row.state == State::Stopping {
        State::Stopped
    } else {
        State::Failed
    };
    let moved = store.transition(id, generation, row.state, next)?;
    // `stop` records its own event; this one is for browsers that ended otherwise.
    if moved && row.state != State::Stopping {
        let _ = store.record(
            &AuditEvent::new("browser", AuditKind::SessionStopped, next.as_str()).session(id),
        );
    }
    Ok(moved)
}
