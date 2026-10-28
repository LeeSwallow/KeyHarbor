//! Opening, stopping and recovering a session's supervisor, and reading its raw CDP endpoint.

use super::{active, note, require_supervisor, session};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    SessionId, State,
    audit::AuditKind,
    ports::{Clock, SessionRuntime, Store, runtime::LaunchOptions},
};
use std::{path::Path, time::Duration};

const START_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(15);
/// How often `open` and `stop` look at the session while they wait.
const POLL: Duration = Duration::from_millis(50);

pub struct OpenOptions<'a> {
    pub url: Option<&'a str>,
    pub cdp: bool,
    pub browser: &'a Path,
    /// Who asked, for the usage record (`cli` for the local user).
    pub agent: &'a str,
}

/// Reserves the session and starts its supervisor, returning once the session is `running`.
pub fn open(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    clock: &dyn Clock,
    id: SessionId,
    options: &OpenOptions<'_>,
) -> Result<()> {
    let url = options
        .url
        .map(keyharbor_domain::validate_start_url)
        .transpose()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if !runtime.has_browser(options.browser) {
        bail!(
            "browser executable not found: {}",
            options.browser.display()
        );
    }
    let row = session(store, id)?;
    let lock = runtime.lock(id).context("session is already open")?;
    let Some(generation) = store.reserve(id, options.cdp)? else {
        if active(row.state) || row.state == State::Failed {
            bail!(
                "session is {} without a supervisor; close its browser, then run recover --confirm-browser-closed",
                row.state.as_str()
            );
        }
        bail!("session is {}", row.state.as_str());
    };
    drop(lock);
    let launch = LaunchOptions {
        browser: options.browser,
        url: url.as_deref(),
        cdp: options.cdp,
    };
    let mut child = match runtime.start(id, generation, &launch) {
        Ok(child) => child,
        Err(error) => {
            store.transition(id, generation, State::Starting, State::Failed)?;
            return Err(error).context("start session supervisor");
        }
    };
    let deadline = clock.now() + START_TIMEOUT;
    let result = loop {
        let row = session(store, id)?;
        if row.generation != Some(generation) {
            break Err(anyhow::anyhow!(
                "session reservation changed during startup"
            ));
        }
        match row.state {
            State::Running | State::AwaitingUser => break Ok(()),
            State::Starting => (),
            other => {
                break Err(anyhow::anyhow!(
                    "session did not start ({}); run recover --confirm-browser-closed before retrying",
                    other.as_str()
                ));
            }
        }
        if let Some(status) = child.exited()? {
            let _ = store.transition(id, generation, State::Starting, State::Failed);
            break Err(anyhow::anyhow!(
                "session supervisor exited during startup ({status}); run recover --confirm-browser-closed before retrying"
            ));
        }
        if clock.now() >= deadline {
            break Err(anyhow::anyhow!(
                "supervisor did not report ready; inspect the session with list before retrying"
            ));
        }
        clock.sleep(POLL);
    };
    // Reap the supervisor when it eventually exits, so a long-lived caller leaves no zombie.
    child.detach();
    if result.is_ok() {
        note(
            store,
            options.agent,
            AuditKind::SessionOpened,
            id,
            "running",
        );
    }
    result
}

/// Asks the supervisor to close the browser and waits until the session has stopped.
pub fn stop(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    clock: &dyn Clock,
    id: SessionId,
    agent: &str,
) -> Result<State> {
    let row = session(store, id)?;
    if !matches!(
        row.state,
        State::Running | State::AwaitingUser | State::Stopping
    ) {
        bail!("session cannot be stopped from {}", row.state.as_str());
    }
    require_supervisor(runtime, id)?;
    let generation = row.generation.context("session has no generation")?;
    if row.state != State::Stopping
        && !store.transition(id, generation, row.state, State::Stopping)?
    {
        bail!("session state changed; retry");
    }
    runtime.close(id)?;
    let deadline = clock.now() + STOP_TIMEOUT;
    loop {
        let row = session(store, id)?;
        if row.generation != Some(generation) || row.state != State::Stopping {
            note(
                store,
                agent,
                AuditKind::SessionStopped,
                id,
                row.state.as_str(),
            );
            return Ok(row.state);
        }
        if clock.now() >= deadline {
            bail!("browser did not exit in time; check the session with list");
        }
        clock.sleep(POLL);
    }
}

/// Returns an abandoned session to `stopped`. Refused while a supervisor holds the lock.
pub fn recover(store: &dyn Store, runtime: &dyn SessionRuntime, id: SessionId) -> Result<()> {
    session(store, id)?;
    let _lock = runtime
        .lock(id)
        .context("session supervisor is running; use stop instead")?;
    if !store.force_stopped(id)? {
        bail!("session is already stopped");
    }
    runtime.remove_socket(id);
    Ok(())
}

/// The raw CDP WebSocket address of a running session opened with `cdp`. Withheld while the
/// session is awaiting the user, as the prototype did.
pub fn endpoint(store: &dyn Store, runtime: &dyn SessionRuntime, id: SessionId) -> Result<String> {
    let row = session(store, id)?;
    if !row.cdp {
        bail!("raw CDP is not enabled for this session; open it with --cdp");
    }
    if row.state != State::Running {
        bail!("session is {}, not running", row.state.as_str());
    }
    require_supervisor(runtime, id)?;
    let endpoint = runtime.debug_endpoint(id)?;
    let latest = session(store, id)?;
    if latest.state != State::Running || latest.generation != row.generation {
        bail!("session changed while reading the endpoint");
    }
    Ok(endpoint)
}
