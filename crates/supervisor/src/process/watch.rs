//! What runs beside the control socket for the whole session: the page watcher (grants and
//! captured sign-ins) and the sweep that clears the cookies of ended grants.

use super::Shared;
use keyharbor_broker::{DocumentRule, Gate, PageHooks, chrome::CdpLink};
use keyharbor_core::grant;
use keyharbor_domain::ports::Clock;
use keyharbor_guard::lock_store;
use std::{sync::Arc, time::Duration};

/// Captured sign-ins waiting for the guard. A person submits one form at a time; when the
/// queue is full the page watcher waits (`send().await`), so none is dropped.
const CAPTURE_QUEUE: usize = 16;

/// How often ended grants are looked for. Fixed: a grant's end is minutes or more away, and
/// navigation is refused from the moment it ends; this only bounds how long its cookies stay.
const GRANT_SWEEP: Duration = Duration::from_secs(5);

/// Watches every page: document loads are held to the session's grants (R-43), and every
/// sign-in the browser reports goes to the guard, which keeps it only during a handoff with
/// capture on (08 §8.2 ②). Values stay in this process.
pub(super) fn watch_pages(shared: &Arc<Shared>, link: &CdpLink, clock: Arc<dyn Clock>, gate: Gate) {
    let (sender, mut captured) = tokio::sync::mpsc::channel(CAPTURE_QUEUE);
    let rule_shared = shared.clone();
    let documents: DocumentRule = Arc::new(move |url: &str| {
        let store = lock_store(&rule_shared.store)?;
        grant::allows(&**store, &*clock, rule_shared.id, url)
    });
    let hooks = PageHooks {
        captured: sender,
        documents,
        gate,
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

/// Removes the cookies of every grant of this session that has ended (R-43).
pub(super) fn sweep_ended_grants(shared: &Arc<Shared>, link: &CdpLink, clock: Arc<dyn Clock>) {
    let (shared, link) = (shared.clone(), link.clone());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(GRANT_SWEEP).await;
            let ended = match lock_store(&shared.store) {
                // A failed read clears nothing this round; the next sweep reads again.
                Ok(store) => {
                    grant::ended_uncleared(&**store, &*clock, shared.id).unwrap_or_default()
                }
                Err(_) => return,
            };
            for ended in ended {
                let Some(host) = ended.host() else { continue };
                if link.clear_cookies(&host).await.is_ok()
                    && let Ok(store) = lock_store(&shared.store)
                {
                    // Left unmarked, the grant is found and cleared again next sweep.
                    let _ = grant::cleared(&**store, &ended);
                }
            }
        }
    });
}
