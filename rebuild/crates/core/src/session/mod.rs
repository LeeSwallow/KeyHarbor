//! Session use cases shared by the CLI and the MCP server: creating and listing sessions
//! ([`manage`]), opening and stopping a session's supervisor ([`lifecycle`]) and handing the
//! browser to the user ([`handoff`]). They run on the store ports and the `SessionRuntime`
//! port; processes and sockets are the broker's.

mod handoff;
mod lifecycle;
mod manage;

pub use handoff::{ResumeWait, pending_handoff, resume, resume_check, wait_auth, wait_for_resume};
pub use lifecycle::{OpenOptions, endpoint, open, recover, stop};
pub use manage::{SessionView, create, describe, list};

use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    SessionId, State,
    audit::{AuditEvent, AuditKind},
    ports::{AuditStore, SessionRow, SessionRuntime, SessionStore},
};

pub fn session(store: &dyn SessionStore, id: SessionId) -> Result<SessionRow> {
    store.get(id)?.context("session not found")
}

fn active(state: State) -> bool {
    matches!(
        state,
        State::Starting | State::Running | State::AwaitingUser | State::Stopping
    )
}

/// The state to show: `recovery_required` when the DB says active but no supervisor holds the lock.
pub fn display_state(runtime: &dyn SessionRuntime, row: &SessionRow) -> &'static str {
    if active(row.state) && !runtime.is_alive(row.id) {
        "recovery_required"
    } else {
        row.state.as_str()
    }
}

fn require_supervisor(runtime: &dyn SessionRuntime, id: SessionId) -> Result<()> {
    if !runtime.is_alive(id) {
        bail!(
            "session supervisor is not running; if the browser is closed, run recover --confirm-browser-closed"
        );
    }
    Ok(())
}

/// Records a session event. Best effort: the change it describes has already happened.
fn note(store: &dyn AuditStore, agent: &str, kind: AuditKind, id: SessionId, result: &str) {
    note_event(store, AuditEvent::new(agent, kind, result).session(id));
}
fn note_event(store: &dyn AuditStore, event: AuditEvent) {
    let _ = store.record(&event);
}
