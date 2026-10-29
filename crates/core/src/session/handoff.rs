//! Handing the browser to the user: request, inspect and resume (02 §2.5.4).

use super::{note, note_event, require_supervisor, session};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    HandoffStatus, RequestId, SessionId, State,
    audit::{AuditEvent, AuditKind},
    ports::{SessionRuntime, SessionStore, Store},
    vault::Origin,
};
use std::time::Duration;

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

/// How long an agent waits for the user to resume a request.
///
/// Fixed, not a setting: the bounds keep one tool call shorter than an MCP client's own
/// timeout, and an agent that needs longer asks again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResumeWait(Duration);

impl ResumeWait {
    pub const DEFAULT_SECONDS: u64 = 120;
    pub const MAX_SECONDS: u64 = 300;
    /// How often the request is looked at while waiting.
    pub const POLL: Duration = Duration::from_millis(250);

    /// The wait an agent asked for, in seconds (`None` for the default).
    pub fn seconds(requested: Option<u64>) -> Result<Self> {
        let seconds = requested.unwrap_or(Self::DEFAULT_SECONDS);
        if !(1..=Self::MAX_SECONDS).contains(&seconds) {
            bail!(
                "timeout_seconds must be between 1 and {}",
                Self::MAX_SECONDS
            );
        }
        Ok(Self(Duration::from_secs(seconds)))
    }
}

/// One look at a handoff request: `true` once the user resumed and the session runs again,
/// `false` while it is pending under a live supervisor, an error for every other outcome.
pub fn resume_check(
    store: &dyn SessionStore,
    runtime: &dyn SessionRuntime,
    id: SessionId,
    request: RequestId,
) -> Result<bool> {
    let status = store.handoff_status(id, request)?;
    match status.context("authentication request not found")? {
        HandoffStatus::Superseded => bail!("request was replaced by a newer one"),
        HandoffStatus::Cancelled => bail!("request was cancelled because the session stopped"),
        HandoffStatus::Resumed => {
            let state = store.get(id)?.map(|row| row.state);
            if state != Some(State::Running) {
                bail!(
                    "request was resumed but the session is now {}",
                    state.map_or("gone", State::as_str)
                );
            }
            Ok(true)
        }
        HandoffStatus::Pending if !runtime.is_alive(id) => {
            bail!("session supervisor is not running; the user must run recover")
        }
        HandoffStatus::Pending => Ok(false),
    }
}

/// Looks with `check` (normally [`resume_check`]) every [`ResumeWait::POLL`], sleeping with
/// `sleep`, until it says resumed, fails, or `wait` has been slept.
pub fn wait_for_resume(
    wait: ResumeWait,
    mut check: impl FnMut() -> Result<bool>,
    sleep: impl FnMut(Duration),
) -> Result<()> {
    poll(wait, || Ok(check()?.then_some(())), sleep)?.ok_or_else(|| {
        anyhow!("Timed out waiting for resume; the session may still be awaiting the user")
    })
}

/// Looks with `check` every [`ResumeWait::POLL`], sleeping with `sleep`, until it gives an
/// answer, fails, or `wait` has been slept (then `None`).
pub fn poll<T>(
    wait: ResumeWait,
    mut check: impl FnMut() -> Result<Option<T>>,
    mut sleep: impl FnMut(Duration),
) -> Result<Option<T>> {
    let mut waited = Duration::ZERO;
    loop {
        if let Some(answer) = check()? {
            return Ok(Some(answer));
        }
        if waited >= wait.0 {
            return Ok(None);
        }
        let step = ResumeWait::POLL.min(wait.0 - waited);
        sleep(step);
        waited += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn resume_waits_are_bounded() {
        assert_eq!(ResumeWait::seconds(None).unwrap().0.as_secs(), 120);
        assert_eq!(ResumeWait::seconds(Some(300)).unwrap().0.as_secs(), 300);
        for bad in [0, 301] {
            assert_eq!(
                ResumeWait::seconds(Some(bad)).unwrap_err().to_string(),
                "timeout_seconds must be between 1 and 300"
            );
        }
    }

    #[test]
    fn waiting_polls_until_resumed_or_the_time_is_slept() {
        let looks = Cell::new(0);
        let slept = Cell::new(Duration::ZERO);
        let wait = ResumeWait::seconds(Some(1)).unwrap();
        let sleep = |d| slept.set(slept.get() + d);
        wait_for_resume(
            wait,
            || {
                looks.set(looks.get() + 1);
                Ok(looks.get() == 3)
            },
            sleep,
        )
        .unwrap();
        assert_eq!(slept.get(), ResumeWait::POLL * 2);
        let err = wait_for_resume(wait, || Ok(false), sleep).unwrap_err();
        assert!(err.to_string().starts_with("Timed out"), "{err}");
        assert_eq!(slept.get(), ResumeWait::POLL * 2 + Duration::from_secs(1));
        let failed = wait_for_resume(wait, || bail!("request was replaced by a newer one"), sleep);
        assert!(failed.is_err());
    }
}
