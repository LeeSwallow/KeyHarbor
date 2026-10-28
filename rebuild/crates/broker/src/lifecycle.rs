//! Session commands shared by the CLI and the MCP server.

use crate::{control, paths, supervisor};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{RequestId, SessionId, State};
use keyharbor_sqlite::{SessionRow, SqliteStore};
use serde_json::json;
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const DEFAULT_BROWSER: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const START_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(15);

pub fn session(store: &SqliteStore, id: SessionId) -> Result<SessionRow> {
    store.get(id)?.context("session not found")
}

fn active(state: State) -> bool {
    matches!(
        state,
        State::Starting | State::Running | State::AwaitingUser | State::Stopping
    )
}

/// The state to show: `recovery_required` when the DB says active but no supervisor holds the lock.
pub fn display_state(data: &Path, row: &SessionRow) -> &'static str {
    if active(row.state) && !supervisor::is_alive(data, &row.id) {
        "recovery_required"
    } else {
        row.state.as_str()
    }
}

fn require_supervisor(data: &Path, id: &SessionId) -> Result<()> {
    if !supervisor::is_alive(data, id) {
        bail!(
            "session supervisor is not running; if the browser is closed, run recover --confirm-browser-closed"
        );
    }
    Ok(())
}

pub struct OpenOptions<'a> {
    pub url: Option<&'a str>,
    pub cdp: bool,
    pub browser: &'a Path,
}

/// Reserves the session and starts its supervisor (`exe __supervise …`) detached,
/// returning once the session is `running`.
pub fn open(data: &Path, id: SessionId, options: &OpenOptions<'_>, exe: &Path) -> Result<()> {
    let url = options
        .url
        .map(keyharbor_domain::validate_start_url)
        .transpose()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if !options.browser.is_file() {
        bail!(
            "browser executable not found: {}",
            options.browser.display()
        );
    }
    let store = SqliteStore::open(data)?;
    let row = session(&store, id)?;
    let lock = supervisor::acquire_lock(&paths::lock_path(data, &id))
        .context("session is already open")?;
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
    let mut command = Command::new(exe);
    command
        .arg("--data-dir")
        .arg(data)
        .arg("__supervise")
        .arg(id.to_string())
        .arg(generation.to_string())
        .arg(options.browser);
    if let Some(url) = &url {
        command.arg("--url").arg(url);
    }
    if options.cdp {
        command.arg("--cdp");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            store.transition(id, generation, State::Starting, State::Failed)?;
            return Err(error).context("start session supervisor");
        }
    };
    let deadline = Instant::now() + START_TIMEOUT;
    let result = loop {
        let row = session(&store, id)?;
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
        if let Some(status) = child.try_wait()? {
            let _ = store.transition(id, generation, State::Starting, State::Failed);
            break Err(anyhow::anyhow!(
                "session supervisor exited during startup ({status}); run recover --confirm-browser-closed before retrying"
            ));
        }
        if Instant::now() >= deadline {
            break Err(anyhow::anyhow!(
                "supervisor did not report ready; inspect the session with list before retrying"
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    // Reap the supervisor when it eventually exits, so a long-lived caller leaves no zombie.
    thread::spawn(move || {
        let _ = child.wait();
    });
    result
}

/// Asks the supervisor to close the browser and waits until the session has stopped.
pub fn stop(data: &Path, id: SessionId) -> Result<State> {
    let store = SqliteStore::open(data)?;
    let row = session(&store, id)?;
    if !matches!(
        row.state,
        State::Running | State::AwaitingUser | State::Stopping
    ) {
        bail!("session cannot be stopped from {}", row.state.as_str());
    }
    require_supervisor(data, &id)?;
    let generation = row.generation.context("session has no generation")?;
    if row.state != State::Stopping
        && !store.transition(id, generation, row.state, State::Stopping)?
    {
        bail!("session state changed; retry");
    }
    control::call(data, &id, &json!({"op": "close"}))?;
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        let row = session(&store, id)?;
        if row.generation != Some(generation) || row.state != State::Stopping {
            return Ok(row.state);
        }
        if Instant::now() >= deadline {
            bail!("browser did not exit in time; check the session with list");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Marks a running session as waiting for the user and returns the new request ID.
pub fn wait_auth(data: &Path, id: SessionId, origin: Option<&str>) -> Result<RequestId> {
    let store = SqliteStore::open(data)?;
    let row = session(&store, id)?;
    require_supervisor(data, &id)?;
    let generation = row.generation.context("session is not running")?;
    store.request_handoff(id, generation, origin)
}

/// The pending request and its claimed origin.
pub fn pending_handoff(data: &Path, id: SessionId) -> Result<(RequestId, Option<String>)> {
    let store = SqliteStore::open(data)?;
    session(&store, id)?;
    require_supervisor(data, &id)?;
    store
        .pending_handoff(id)?
        .context("no pending authentication request")
}

pub fn resume(data: &Path, id: SessionId, request: &str) -> Result<()> {
    let request = RequestId::parse(request).context("invalid request id")?;
    let store = SqliteStore::open(data)?;
    let row = session(&store, id)?;
    require_supervisor(data, &id)?;
    let generation = row.generation.context("session is not running")?;
    store.resume_handoff(id, generation, request)
}

/// Returns an abandoned session to `stopped`. Refused while a supervisor holds the lock.
pub fn recover(data: &Path, id: SessionId) -> Result<()> {
    let store = SqliteStore::open(data)?;
    session(&store, id)?;
    let _lock = supervisor::acquire_lock(&paths::lock_path(data, &id))
        .context("session supervisor is running; use stop instead")?;
    if !store.force_stopped(id)? {
        bail!("session is already stopped");
    }
    let _ = std::fs::remove_file(paths::socket_path(data, &id));
    Ok(())
}

/// The raw CDP WebSocket address of a running session opened with `cdp`. Withheld while the
/// session is awaiting the user, as the prototype did.
pub fn endpoint(data: &Path, id: SessionId) -> Result<String> {
    let store = SqliteStore::open(data)?;
    let row = session(&store, id)?;
    if !row.cdp {
        bail!("raw CDP is not enabled for this session; open it with --cdp");
    }
    if row.state != State::Running {
        bail!("session is {}, not running", row.state.as_str());
    }
    require_supervisor(data, &id)?;
    let file = paths::profile_dir(data, &id).join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(5);
    let text = loop {
        match std::fs::symlink_metadata(&file) {
            Ok(meta) if meta.is_file() && meta.len() <= 256 => {
                break std::fs::read_to_string(&file)?;
            }
            Ok(_) => bail!("invalid DevToolsActivePort file"),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Err(_) => bail!("browser debugging endpoint is not ready"),
        }
    };
    let mut lines = text.lines();
    let port: u16 = lines
        .next()
        .and_then(|p| p.parse().ok())
        .filter(|p| *p != 0)
        .context("invalid debug port")?;
    let path = lines.next().context("missing browser endpoint")?;
    let browser = path
        .strip_prefix("/devtools/browser/")
        .context("invalid browser endpoint")?;
    uuid_like(browser)?;
    if lines.next().is_some() {
        bail!("invalid browser endpoint");
    }
    let latest = session(&store, id)?;
    if latest.state != State::Running || latest.generation != row.generation {
        bail!("session changed while reading the endpoint");
    }
    Ok(format!("ws://127.0.0.1:{port}{path}"))
}

fn uuid_like(value: &str) -> Result<()> {
    let ok = value.len() == 36
        && value.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    if !ok {
        bail!("invalid browser endpoint");
    }
    Ok(())
}
