//! Creating, describing and listing sessions: one implementation for every interface.

use super::{DisplayState, display_state};
use anyhow::{Result, bail};
use keyharbor_domain::{
    Session, SessionId, SessionName,
    ports::{SessionRow, SessionRuntime, SessionStore},
    vault::Description,
};

/// A session as listed: the stored row and the state to show for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub row: SessionRow,
    pub state: DisplayState,
}

/// Creates a stopped session with its description in one write.
pub fn create(
    store: &dyn SessionStore,
    name: SessionName,
    description: Option<&Description>,
) -> Result<Session> {
    store.create_described(name, description)
}

/// Sets or clears a session's description.
pub fn describe(
    store: &dyn SessionStore,
    id: SessionId,
    description: Option<&Description>,
) -> Result<()> {
    if !store.set_description(id, description)? {
        bail!("session not found");
    }
    Ok(())
}

/// Every session, ordered by name, with the state to show.
pub fn list(store: &dyn SessionStore, runtime: &dyn SessionRuntime) -> Result<Vec<SessionView>> {
    Ok(store
        .list()?
        .into_iter()
        .map(|row| SessionView {
            state: display_state(runtime, &row),
            row,
        })
        .collect())
}
