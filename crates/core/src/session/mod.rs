//! Session use cases shared by the CLI and the MCP server: creating and listing sessions
//! ([`manage`]), opening and stopping a session's supervisor ([`lifecycle`]) and handing the
//! browser to the user ([`handoff`]), and the states a supervisor records for its run
//! ([`supervision`]). They run on the store ports and the `SessionRuntime` port; processes and
//! sockets are `keyharbor-supervisor`'s.

mod handoff;
mod lifecycle;
mod manage;
pub mod remove;
pub mod supervision;
mod unlock;

pub use handoff::{
    ResumeWait, find_handoff, pending_handoff, poll, resume, resume_check, wait_auth,
    wait_for_resume,
};
pub use lifecycle::{
    OpenOptions, ego_lite_answers, endpoint, open, recover, resolve_browser, stop,
};
pub use manage::{SessionView, create, describe, list};
pub use unlock::unlock;

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
    /// A run is under way and its supervisor holds it: the browser it names is the one in use.
    pub fn is_live(self) -> bool {
        matches!(self, Self::Stored(state) if active(state))
    }
    /// Only `recover` (after the person closed the browser) brings it back to `stopped`.
    pub fn is_recoverable(self) -> bool {
        matches!(self, Self::RecoveryRequired | Self::Stored(State::Failed))
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

/// What the user runs once a session's browser is closed and no supervisor holds it.
const RECOVER: &str = "run recover --confirm-browser-closed";

fn require_supervisor(runtime: &dyn SessionRuntime, id: SessionId) -> Result<()> {
    if !runtime.is_alive(id) {
        bail!("session supervisor is not running; if the browser is closed, {RECOVER}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_run_and_a_recoverable_one_are_told_apart() {
        let live: Vec<&str> = [
            State::Stopped,
            State::Starting,
            State::Running,
            State::AwaitingUser,
            State::Stopping,
            State::Failed,
        ]
        .into_iter()
        .map(DisplayState::Stored)
        .chain([DisplayState::RecoveryRequired])
        .filter(|s| s.is_live())
        .map(DisplayState::as_str)
        .collect();
        assert_eq!(live, ["starting", "running", "awaiting_user", "stopping"]);
        assert!(DisplayState::RecoveryRequired.is_recoverable());
        assert!(DisplayState::Stored(State::Failed).is_recoverable());
        assert!(!DisplayState::Stored(State::Stopped).is_recoverable());
        assert!(!DisplayState::Stored(State::Running).is_recoverable());
    }
}
