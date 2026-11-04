//! The per-session supervisor process: holds the OS lock, prepares the profile and socket
//! ([`prepare`]), starts the browser ([`launch`]), wraps it in the guard, serves the control
//! socket beside the page watcher ([`watch`]) and records the session state (through
//! `core::session::supervision`) when the browser exits.

mod launch;
mod prepare;
mod watch;

use crate::{argv::SuperviseArgs, control, lock::acquire_lock_within, ops::Ops, paths};
use anyhow::{Context, Result, bail};
use keyharbor_broker::{
    chrome::{self, ChromeLaunch},
    ego::EgoLaunch,
};
use keyharbor_core::{
    grant,
    session::supervision::{self, BrowserExit},
};
use keyharbor_domain::{
    Generation, SessionId,
    browser_choice::BrowserKind,
    ports::{Clock, Store, StoreFactory},
    settings::EGO_POLL_MS,
};
use keyharbor_guard::{Guard, SharedStore, VaultOpener, lock_store};
use launch::Launch;
use prepare::{bind, prepare};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::UnixListener, process::Child, time::Instant};
use watch::{sweep_ended_grants, watch_pages};

/// What a supervisor runs with, chosen by the binary's composition root.
pub struct Adapters {
    pub stores: Arc<dyn StoreFactory>,
    pub open_vault: VaultOpener,
    /// The guard's clock (refusal rate limit).
    pub clock: Arc<dyn Clock>,
    /// Start the browser headless (`KEYHARBOR_BROWSER_HEADLESS=1`, tests and CI).
    pub headless: bool,
}

/// How long the browser gets to exit after `close` before it is killed.
const CLOSE_GRACE: Duration = Duration::from_secs(10);

/// After the kill is sent the deadline moves this far out of the way: the kill is sent once,
/// and the browser's exit (`child.wait()`) ends the loop long before.
const AFTER_KILL: Duration = Duration::from_secs(3600);

/// How many times the browser's exit is written before giving up, and the wait between
/// tries: about ten seconds through a busy database.
const EXIT_RECORD_ATTEMPTS: u32 = 40;
const EXIT_RECORD_BACKOFF: Duration = Duration::from_millis(250);

/// How long the supervisor waits for `open` to finish handing the session lock over.
const LOCK_HANDOFF: Duration = Duration::from_secs(2);

/// What the supervisor shares with its control connections.
pub(crate) struct Shared {
    pub id: SessionId,
    pub generation: Generation,
    pub store: SharedStore,
    pub clock: Arc<dyn Clock>,
    /// The enforcement point every browser request goes through.
    pub guard: Guard,
    /// Requests run one at a time (02 §2.6.1).
    pub ops: tokio::sync::Mutex<Ops>,
    pub close: tokio::sync::Notify,
    pub close_requested: std::sync::atomic::AtomicBool,
}

impl Shared {
    pub fn request_close(&self) {
        self.close_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.close.notify_one();
    }
}

/// Runs the supervisor until the browser exits. The session must be `starting` under the
/// run's generation.
pub fn run(data: &Path, args: SuperviseArgs, adapters: Adapters) -> Result<()> {
    let SuperviseArgs {
        id,
        generation,
        browser,
        url,
        cdp,
        kind,
        headless,
    } = args;
    let _lock = acquire_lock_within(&paths::lock_path(data, &id), LOCK_HANDOFF)
        .context("another supervisor is running this session")?;
    let store = adapters.stores.open().context("open store")?;
    supervision::check_reservation(&*store, id, generation)?;
    let fail = |store: &dyn Store, error: anyhow::Error| {
        supervision::mark_failed(store, id, generation, &error);
        error
    };
    let is_ego = kind == BrowserKind::EgoLite;
    if is_ego && cdp {
        return Err(fail(
            &*store,
            anyhow::anyhow!("raw CDP is not available in ego lite sessions"),
        ));
    }
    let url = match url
        .as_deref()
        .map(keyharbor_domain::validate_start_url)
        .transpose()
    {
        Ok(url) => url,
        Err(error) => return Err(fail(&*store, anyhow::anyhow!("{error}"))),
    };
    // The page watcher attaches after Chrome loads its start page, so check that one here.
    if let Some(url) = &url
        && !grant::allows(&*store, &*adapters.clock, id, url)?
    {
        return Err(fail(
            &*store,
            anyhow::anyhow!("denied: navigation (out-of-grant)"),
        ));
    }
    let prepared = match prepare(data, &id, cdp, !is_ego) {
        Ok(prepared) => prepared,
        Err(error) => return Err(fail(&*store, error)),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let launch = if is_ego {
        let space = match store.get(id)? {
            Some(row) => format!("keyharbor-{}-{}", row.name.as_str(), &id.to_string()[..8]),
            None => return Err(fail(&*store, anyhow::anyhow!("session not found"))),
        };
        let poll = keyharbor_core::settings::get(&*store, EGO_POLL_MS)?
            .parse()
            .context("invalid ego_poll_ms")?;
        Launch::Ego(EgoLaunch {
            command: browser,
            socket: paths::ego_socket_path(data, &id),
            log: paths::ego_log_path(data, &id),
            space,
            url,
            poll: Duration::from_millis(poll),
        })
    } else {
        Launch::Chrome(ChromeLaunch {
            executable: browser,
            profile: prepared.profile,
            url,
            debug_port: cdp,
            headless: adapters.headless || headless,
        })
    };
    let run = Run {
        id,
        generation,
        socket: prepared.socket,
        shots: prepared.shots,
    };
    let guard_parts = (adapters.open_vault, adapters.clock);
    runtime.block_on(supervise(run, store, guard_parts, launch))
}

/// One run's identity and files.
struct Run {
    id: SessionId,
    generation: Generation,
    socket: PathBuf,
    shots: PathBuf,
}

async fn supervise(
    run: Run,
    store: Box<dyn Store>,
    (open_vault, clock): (VaultOpener, Arc<dyn Clock>),
    launch: Launch,
) -> Result<()> {
    let Run {
        id,
        generation,
        socket,
        shots,
    } = run;
    let fail = |store: &dyn Store, error: anyhow::Error| {
        supervision::mark_failed(store, id, generation, &error);
        error
    };
    let listener = match bind(&socket) {
        Ok(listener) => listener,
        Err(error) => return Err(fail(&*store, error)),
    };
    let gate = launch.gate();
    let chrome::Chrome {
        process: mut child,
        link,
    } = match launch.spawn().await {
        Ok(spawned) => spawned,
        Err(error) => {
            let _ = fs::remove_file(&socket);
            return Err(fail(&*store, error));
        }
    };
    match supervision::mark_running(&*store, id, generation) {
        Ok(true) => (),
        outcome => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let _ = fs::remove_file(&socket);
            return match outcome {
                Err(error) => Err(error.context("record browser start; browser stopped")),
                Ok(_) => bail!("session reservation changed; browser stopped"),
            };
        }
    }
    let store: SharedStore = Arc::new(Mutex::new(store));
    let browser = link.browser(shots, id.to_string());
    let shared = Arc::new(Shared {
        id,
        generation,
        clock: clock.clone(),
        guard: Guard::new(id, store.clone(), open_vault, clock.clone()),
        store,
        ops: tokio::sync::Mutex::new(Ops::new(browser)),
        close: tokio::sync::Notify::new(),
        close_requested: false.into(),
    });
    watch_pages(&shared, &link, clock.clone(), gate);
    sweep_ended_grants(&shared, &link, clock);
    let watcher = shared.clone();
    tokio::spawn(async move {
        // On macOS Chrome keeps running with no windows; closing the last tab closes it.
        if link.last_page_closed().await {
            watcher.request_close();
        }
    });
    let status = serve(&shared, &listener, &mut child).await;
    drop(listener);
    let _ = fs::remove_file(&socket);
    let status = status?;
    record_exit(&shared, status).await;
    Ok(())
}

/// Accepts control connections until the browser exits; kills it if `close` is not honoured.
async fn serve(
    shared: &Arc<Shared>,
    listener: &UnixListener,
    child: &mut Child,
) -> Result<ExitStatus> {
    let mut kill_at: Option<Instant> = None;
    loop {
        let deadline = async {
            match kill_at {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            status = child.wait() => return Ok(status?),
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    tokio::spawn(control::server::serve_connection(stream, shared.clone()));
                }
            }
            () = shared.close.notified(), if kill_at.is_none() => {
                kill_at = Some(Instant::now() + CLOSE_GRACE);
            }
            () = deadline => {
                let _ = child.start_kill();
                kill_at = Some(Instant::now() + AFTER_KILL);
            }
        }
    }
}

/// Records how the browser ended, retrying through brief DB errors.
async fn record_exit(shared: &Shared, status: ExitStatus) {
    let requested = shared
        .close_requested
        .load(std::sync::atomic::Ordering::SeqCst);
    let exit = if status.success() || requested {
        BrowserExit::Clean
    } else {
        BrowserExit::Unexpected
    };
    for _ in 0..EXIT_RECORD_ATTEMPTS {
        let outcome = lock_store(&shared.store).and_then(|store| {
            supervision::mark_exited(&**store, shared.id, shared.generation, exit)
        });
        if matches!(outcome, Ok(true)) {
            return;
        }
        tokio::time::sleep(EXIT_RECORD_BACKOFF).await;
    }
}
