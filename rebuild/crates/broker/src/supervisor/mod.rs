//! The per-session supervisor process: holds the OS lock ([`lock`]), runs Chrome with the
//! CDP pipe ([`launch`]), serves the control socket through the guard and records the session
//! state when the browser exits.

mod launch;
mod lock;

pub use lock::{acquire_lock, is_alive, is_locked};

use crate::{browser::CdpBrowser, cdp::Cdp, control, ops::Ops, paths};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    Generation, SessionId, State,
    audit::{AuditEvent, AuditKind},
    ports::{Store, StoreFactory},
};
use keyharbor_guard::{Guard, SharedStore, VaultOpener};
use launch::{Launch, bind, prepare, spawn_browser};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::UnixListener, process::Child, time::Instant};

/// The adapters a supervisor runs with, chosen by the interface that starts it.
pub struct Adapters {
    pub stores: Arc<dyn StoreFactory>,
    pub open_vault: VaultOpener,
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

/// Runs the supervisor until the browser exits. The session must be `starting` under `generation`.
pub fn run(
    data: &Path,
    id: SessionId,
    generation: Generation,
    browser: &Path,
    url: Option<&str>,
    cdp: bool,
    adapters: Adapters,
) -> Result<()> {
    let _lock = acquire_lock(&paths::lock_path(data, &id))
        .context("another supervisor is running this session")?;
    let store = adapters.stores.open()?;
    let row = store.get(id)?.context("session not found")?;
    if row.state != State::Starting || row.generation != Some(generation) {
        bail!("session reservation is no longer valid");
    }
    let fail = |store: &dyn Store, error: anyhow::Error| {
        let _ = store.transition(id, generation, State::Starting, State::Failed);
        error
    };
    let url = match url.map(keyharbor_domain::validate_start_url).transpose() {
        Ok(url) => url,
        Err(error) => return Err(fail(&*store, anyhow::anyhow!("{error}"))),
    };
    let (profile, socket) = match prepare(data, &id, cdp) {
        Ok(paths) => paths,
        Err(error) => return Err(fail(&*store, error)),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let launch = Launch {
        browser: browser.to_path_buf(),
        profile,
        url,
        cdp,
    };
    runtime.block_on(supervise(
        data,
        id,
        generation,
        store,
        adapters.open_vault,
        launch,
        socket,
    ))
}

async fn supervise(
    data: &Path,
    id: SessionId,
    generation: Generation,
    store: Box<dyn Store>,
    open_vault: VaultOpener,
    launch: Launch,
    socket: PathBuf,
) -> Result<()> {
    let fail = |store: &dyn Store, error: anyhow::Error| {
        let _ = store.transition(id, generation, State::Starting, State::Failed);
        error
    };
    let listener = match bind(&socket) {
        Ok(listener) => listener,
        Err(error) => return Err(fail(&*store, error)),
    };
    let (mut child, sender, receiver) = match spawn_browser(&launch) {
        Ok(spawned) => spawned,
        Err(error) => {
            let _ = fs::remove_file(&socket);
            return Err(fail(&*store, error));
        }
    };
    match store.transition(id, generation, State::Starting, State::Running) {
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
    let cdp = Arc::new(Cdp::new(sender, receiver));
    let store: SharedStore = Arc::new(Mutex::new(store));
    let browser = CdpBrowser::new(cdp.clone(), data.to_path_buf(), id);
    let shared = Arc::new(Shared {
        id,
        generation,
        guard: Guard::new(id, store.clone(), open_vault),
        store,
        ops: tokio::sync::Mutex::new(Ops::new(browser)),
        close: tokio::sync::Notify::new(),
        close_requested: false.into(),
    });
    tokio::spawn(close_when_last_tab_closes(cdp, shared.clone()));
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
                    tokio::spawn(control::serve_connection(stream, shared.clone()));
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

/// On macOS Chrome keeps running with no windows; treat closing the last tab as closing the browser.
async fn close_when_last_tab_closes(cdp: Arc<Cdp>, shared: Arc<Shared>) {
    let mut events = cdp.events();
    if cdp
        .call(
            "Target.setDiscoverTargets",
            serde_json::json!({"discover": true}),
            None,
        )
        .await
        .is_err()
    {
        return;
    }
    loop {
        match events.recv().await {
            Ok(event) if event["method"] == "Target.targetDestroyed" => {
                let Ok(targets) = cdp
                    .call("Target.getTargets", serde_json::json!({}), None)
                    .await
                else {
                    return;
                };
                let pages = targets["targetInfos"]
                    .as_array()
                    .is_some_and(|t| t.iter().any(|t| t["type"] == "page"));
                if !pages {
                    cdp.send_and_close("Browser.close", serde_json::json!({}));
                    shared.request_close();
                    return;
                }
            }
            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
            Err(_) => return,
        }
    }
}

/// Records `stopped` (clean exit or requested close) or `failed`, retrying through brief DB errors.
async fn record_exit(shared: &Shared, status: ExitStatus) {
    let requested = shared
        .close_requested
        .load(std::sync::atomic::Ordering::SeqCst);
    for _ in 0..40 {
        let outcome = (|| -> Result<bool> {
            let store = shared
                .store
                .lock()
                .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
            let Some(row) = store.get(shared.id)? else {
                return Ok(true);
            };
            if row.generation != Some(shared.generation) {
                return Ok(true);
            }
            if !matches!(
                row.state,
                State::Running | State::AwaitingUser | State::Stopping
            ) {
                return Ok(true);
            }
            let next = if status.success() || requested || row.state == State::Stopping {
                State::Stopped
            } else {
                State::Failed
            };
            let moved = store.transition(shared.id, shared.generation, row.state, next)?;
            // `stop` records its own event; this one is for browsers that ended otherwise.
            if moved && row.state != State::Stopping {
                let _ = store.record(
                    &AuditEvent::new("browser", AuditKind::SessionStopped, next.as_str())
                        .session(shared.id),
                );
            }
            Ok(moved)
        })();
        if matches!(outcome, Ok(true)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
