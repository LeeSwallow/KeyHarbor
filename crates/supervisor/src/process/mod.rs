//! The per-session supervisor process: holds the OS lock, prepares the profile and socket
//! ([`prepare`]), starts Chrome through the broker, wraps it in the guard, serves the control
//! socket and records the session state (through `core::session::supervision`) when the
//! browser exits.

mod prepare;

use crate::{argv::SuperviseArgs, control, lock::acquire_lock_within, ops::Ops, paths};
use anyhow::{Context, Result, bail};
use keyharbor_broker::chrome::{self, ChromeLaunch};
use keyharbor_core::{
    grant,
    session::supervision::{self, BrowserExit},
};
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
    // The page watcher attaches after Chrome loads its start page, so check that one here.
    if let Some(url) = &url
        && !grant::allows(&*store, &*adapters.clock, id, url)?
    {
        return Err(fail(
            &*store,
            anyhow::anyhow!("denied: navigation (out-of-grant)"),
        ));
    }
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
        clock: clock.clone(),
        guard: Guard::new(id, store.clone(), open_vault, clock.clone()),
        store,
        ops: tokio::sync::Mutex::new(Ops::new(browser)),
        close: tokio::sync::Notify::new(),
        close_requested: false.into(),
    });
    watch_pages(&shared, &link, clock.clone());
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

/// Watches every page: document loads are held to the session's grants (R-43), and every
/// sign-in the browser reports goes to the guard, which keeps it only during a handoff with
/// capture on (08 §8.2 ②). Values stay in this process.
fn watch_pages(shared: &Arc<Shared>, link: &chrome::CdpLink, clock: Arc<dyn Clock>) {
    let (sender, mut captured) = tokio::sync::mpsc::channel(16);
    let rule_shared = shared.clone();
    let documents: keyharbor_broker::DocumentRule = Arc::new(move |url: &str| {
        let store = rule_shared
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        grant::allows(&**store, &*clock, rule_shared.id, url)
    });
    let hooks = keyharbor_broker::PageHooks {
        captured: sender,
        documents,
    };
    let link = link.clone();
    tokio::spawn(async move { link.watch_pages(hooks).await });
    let shared = shared.clone();
    tokio::spawn(async move {
        while let Some(login) = captured.recv().await {
            let shared = shared.clone();
            let _ = tokio::task::spawn_blocking(move || shared.guard.capture(login)).await;
        }
    });
}

/// How often ended grants are looked for. Fixed: a grant's end is minutes or more away, and
/// navigation is refused from the moment it ends; this only bounds how long its cookies stay.
const GRANT_SWEEP: Duration = Duration::from_secs(5);

/// Removes the cookies of every grant of this session that has ended (R-43).
fn sweep_ended_grants(shared: &Arc<Shared>, link: &chrome::CdpLink, clock: Arc<dyn Clock>) {
    let (shared, link) = (shared.clone(), link.clone());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(GRANT_SWEEP).await;
            let ended = match shared.store.lock() {
                Ok(store) => {
                    grant::ended_uncleared(&**store, &*clock, shared.id).unwrap_or_default()
                }
                Err(_) => return,
            };
            for ended in ended {
                let Some(host) = ended.host() else { continue };
                if link.clear_cookies(&host).await.is_ok()
                    && let Ok(store) = shared.store.lock()
                {
                    let _ = grant::cleared(&**store, &ended);
                }
            }
        }
    });
}
