//! What a session hook or `keyharbor status` shows at a glance: counts only, read from the
//! store (no key store, so no keychain prompt), never a name or a value.

use crate::{
    ItemStatus, request,
    session::{self, DisplayState},
};
use anyhow::Result;
use keyharbor_domain::{
    State,
    ports::{Clock, SessionRuntime, Store},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub vault_ready: bool,
    /// Sessions whose browser is open (running or handed to the user).
    pub sessions_open: usize,
    /// Agent requests for information waiting for the user.
    pub requests_pending: usize,
    /// Items waiting to be confirmed or discarded.
    pub drafts: usize,
}

pub fn status(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    clock: &dyn Clock,
) -> Result<Status> {
    let open = |state: DisplayState| {
        matches!(
            state,
            DisplayState::Stored(State::Running | State::AwaitingUser)
        )
    };
    Ok(Status {
        vault_ready: store.vault_get()?.is_some(),
        sessions_open: session::list(store, runtime)?
            .into_iter()
            .filter(|view| open(view.state))
            .count(),
        requests_pending: request::pending(store, clock)?.len(),
        drafts: store
            .item_list()?
            .iter()
            .filter(|row| row.status == ItemStatus::Draft.as_str())
            .count(),
    })
}
