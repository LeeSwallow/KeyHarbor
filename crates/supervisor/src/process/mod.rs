//! The per-session supervisor process: holds the OS lock, prepares the profile and socket
//! ([`prepare`]), starts Chrome through the broker, wraps it in the guard, serves the control
//! socket and records the session state (through `core::session::supervision`) when the
//! browser exits.

mod prepare;

use crate::{argv::SuperviseArgs, control, lock::acquire_lock_within, ops::Ops, paths};
use anyhow::{Context, Result, bail};
use keyharbor_broker::chrome::{self, ChromeLaunch};
use keyharbor_core::session::supervision::{self, BrowserExit};
use keyharbor_domain::{
    Generation, SessionId,
    ports::{Clock, Store, StoreFactory},
};
use keyharbor_guard::{Guard, SharedStore, VaultOpener};
use prepare::{bind, prepare};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::UnixListener, process::Child, time::Instant};

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

/// What the supervisor shares with its control connections.
pub(crate) struct Shared {
    pub id: SessionId,
    pub generation: Generation,
    pub store: SharedStore,
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
/// How long the supervisor waits for `open` to finish handing the session lock over.
const LOCK_HANDOFF: std::time::Duration = std::time::Duration::from_secs(2);

pub fn run(data: &Path, args: SuperviseArgs, adapters: Adapters) -> Result<()> {
    let SuperviseArgs {
        id,
        generation,
        browser,
        url,
        cdp,
    } = args;
    let _lock = acquire_lock_within(&paths::lock_path(data, &id), LOCK_HANDOFF)
        .context("another supervisor is running this session")?;
    let store = adapters.stores.open()?;
    supervision::check_reservation(&*store, id, generation)?;
    let fail = |store: &dyn Store, error: anyhow::Error| {
        supervision::mark_failed(store, id, generation);
        error
    };
    let url = match url
        .as_deref()
        .map(keyharbor_domain::validate_start_url)
        .transpose()
    {
        Ok(url) => url,
        Err(error) => return Err(fail(&*store, anyhow::anyhow!("{error}"))),
    };
    let prepared = match prepare(data, &id, cdp) {
        Ok(prepared) => prepared,
        Err(error) => return Err(fail(&*store, error)),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let launch = ChromeLaunch {
        executable: browser,
        profile: prepared.profile,
        url,
        debug_port: cdp,
        headless: adapters.headless,
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
    launch: ChromeLaunch,
) -> Result<()> {
    let Run {
        id,
        generation,
        socket,
        shots,
    } = run;
    let fail = |store: &dyn Store, error: anyhow::Error| {
        supervision::mark_failed(store, id, generation);
        error
    };
    let listener = match bind(&socket) {
        Ok(listener) => listener,
        Err(error) => return Err(fail(&*store, error)),
    };
    let chrome::Chrome {
        process: mut child,
        link,
    } = match chrome::spawn(&launch) {
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
        guard: Guard::new(id, store.clone(), open_vault, clock),
        store,
        ops: tokio::sync::Mutex::new(Ops::new(browser)),
        close: tokio::sync::Notify::new(),
        close_requested: false.into(),
    });
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
                kill_at = Some(Instant::now() + Duration::from_secs(3600));
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
    for _ in 0..40 {
        let outcome = shared
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))
            .and_then(|store| {
                supervision::mark_exited(&**store, shared.id, shared.generation, exit)
            });
        if matches!(outcome, Ok(true)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
