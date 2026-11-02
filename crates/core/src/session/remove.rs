//! Removing sessions (`keyharbor rm`): the row, which takes its handoff requests and site
//! states with it, and the browser profile. A session someone may still be using is refused
//! unless the caller asks to close it first.

use super::{lifecycle, note, session};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    SessionId, State,
    audit::AuditKind,
    ports::{Clock, SessionRuntime, SessionStore, Store},
};

/// What `remove` may do to a session that is not stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Force {
    /// Refuse it.
    No,
    /// Close its browser (or return an abandoned one to stopped), then remove it.
    CloseFirst,
}

/// The session a person named: its id, or else its name.
pub fn resolve(store: &dyn SessionStore, key: &str) -> Result<SessionId> {
    if let Ok(id) = SessionId::parse(key) {
        return Ok(id);
    }
    store
        .list()?
        .into_iter()
        .find(|row| row.name.as_str() == key)
        .map(|row| row.id)
        .with_context(|| format!("no session is called {key}"))
}

/// Removes one session and its profile.
pub fn remove(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    clock: &dyn Clock,
    id: SessionId,
    force: Force,
    agent: &str,
) -> Result<()> {
    let row = session(store, id)?;
    if row.state != State::Stopped {
        if force == Force::No {
            bail!(
                "session {} is {}; stop it first or pass --force",
                row.name.as_str(),
                row.state.as_str()
            );
        }
        close(store, runtime, clock, id, row.state, agent)?;
    }
    let _lock = runtime
        .lock(id)
        .context("session supervisor is running; stop it first")?;
    if !store.delete(id)? {
        bail!("session changed while removing it; retry");
    }
    runtime.remove_profile(id)?;
    note(store, agent, AuditKind::SessionRemoved, id, "removed");
    Ok(())
}

fn close(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    clock: &dyn Clock,
    id: SessionId,
    state: State,
    agent: &str,
) -> Result<()> {
    let has_browser = matches!(
        state,
        State::Running | State::AwaitingUser | State::Stopping
    ) && runtime.is_alive(id);
    if has_browser {
        lifecycle::stop(store, runtime, clock, id, agent)?;
    } else {
        lifecycle::recover(store, runtime, id)?;
    }
    Ok(())
}
