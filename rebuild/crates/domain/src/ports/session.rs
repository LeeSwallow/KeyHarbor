//! Sessions, run generations and authentication handoff requests.
//!
//! Every state change is conditional on the session's current generation (compare-and-swap),
//! so a writer from an older run cannot move a newer one.

use crate::{Generation, HandoffStatus, RequestId, Session, SessionId, SessionName, State};
use anyhow::Result;

/// A session as stored, with the generation of its current run (if any).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub id: SessionId,
    pub name: SessionName,
    pub state: State,
    pub generation: Option<Generation>,
    /// Raw CDP was requested for the current run.
    pub cdp: bool,
}

pub trait SessionStore {
    fn create(&self, name: SessionName) -> Result<Session>;
    /// Every session, ordered by name.
    fn list(&self) -> Result<Vec<SessionRow>>;
    fn get(&self, id: SessionId) -> Result<Option<SessionRow>>;
    /// Deletes a stopped session and its records. Returns false if it is not stopped or absent.
    fn delete(&self, id: SessionId) -> Result<bool>;
    /// stopped → starting with a fresh generation. `None` if the session is not stopped.
    fn reserve(&self, id: SessionId, cdp: bool) -> Result<Option<Generation>>;
    /// Moves `from → to` only if the session is still in `from` under `generation`.
    /// Leaving `awaiting_user` for anything but `running` cancels the pending request.
    fn transition(
        &self,
        id: SessionId,
        generation: Generation,
        from: State,
        to: State,
    ) -> Result<bool>;
    fn generation(&self, id: SessionId) -> Result<Option<Generation>> {
        Ok(self.get(id)?.and_then(|row| row.generation))
    }
    fn cdp_enabled(&self, id: SessionId) -> Result<bool> {
        Ok(self.get(id)?.is_some_and(|row| row.cdp))
    }
    /// Recovery: any non-stopped state → stopped, dropping the generation so the old run is
    /// fenced off. The caller must hold the session's supervisor lock. False if already stopped.
    fn force_stopped(&self, id: SessionId) -> Result<bool>;
    /// running|awaiting_user → awaiting_user with a new request; an older pending one is superseded.
    fn request_handoff(
        &self,
        id: SessionId,
        generation: Generation,
        origin: Option<&str>,
    ) -> Result<RequestId>;
    /// The request the user is asked to finish, with the claimed origin.
    fn pending_handoff(&self, id: SessionId) -> Result<Option<(RequestId, Option<String>)>>;
    /// awaiting_user → running, only for the pending request of the current generation.
    fn resume_handoff(
        &self,
        id: SessionId,
        generation: Generation,
        request: RequestId,
    ) -> Result<()>;
    fn handoff_status(&self, id: SessionId, request: RequestId) -> Result<Option<HandoffStatus>>;
}
