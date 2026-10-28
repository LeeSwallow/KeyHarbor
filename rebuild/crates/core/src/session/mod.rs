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

/// The state a session is shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayState {
    Stored(State),
    /// The store says active but no supervisor holds the lock; the user must recover it.
    RecoveryRequired,
}

impl DisplayState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stored(state) => state.as_str(),
            Self::RecoveryRequired => "recovery_required",
        }
    }
}

impl std::fmt::Display for DisplayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The state to show for `row`, given whether its supervisor runs.
pub fn display_state(runtime: &dyn SessionRuntime, row: &SessionRow) -> DisplayState {
    if active(row.state) && !runtime.is_alive(row.id) {
        DisplayState::RecoveryRequired
    } else {
        DisplayState::Stored(row.state)
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
