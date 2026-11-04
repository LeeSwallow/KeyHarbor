//! What runs beside the control socket for the whole session: the page watcher (grants and
//! captured sign-ins) and the sweep that clears the cookies of ended grants.

use super::Shared;
use keyharbor_broker::{DocumentRule, Gate, PageHooks, chrome::CdpLink};
use keyharbor_core::grant;
use keyharbor_domain::{
    SessionId,
    audit::{AuditEvent, AuditKind},
    policy::SESSION_AGENT,
    ports::Clock,
    vault::Origin,
};
use keyharbor_guard::{audit, lock_store};
use std::{sync::Arc, time::Duration};

/// The `result` of a `credential_captured` record whose capture failed. Stored, so the text
/// never changes.
const CAPTURE_FAILED: &str = "failed";

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
            let origin = Origin::parse(&login.origin).ok();
            let worker = shared.clone();
            let outcome = tokio::task::spawn_blocking(move || worker.guard.capture(login)).await;
            if !matches!(outcome, Ok(Ok(_)))
                && let Ok(store) = lock_store(&shared.store)
            {
                audit::record(&**store, &capture_failed(shared.id, origin.as_ref()));
            }
        }
    });
}

/// The record of a sign-in the guard could not capture (the vault would not open, the store
/// failed, the task panicked): the origin and a fixed result, never the error, whose text
/// is not ours to vouch for, and never a value (J-181).
fn capture_failed(session: SessionId, origin: Option<&Origin>) -> AuditEvent {
    AuditEvent::new(SESSION_AGENT, AuditKind::CredentialCaptured, CAPTURE_FAILED)
        .session(session)
        .origin(origin.map(Origin::as_str))
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

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-1111-4111-8111-111111111111";

    #[test]
    fn a_failed_capture_is_recorded_with_its_origin_and_a_fixed_result() {
        let id = SessionId::parse(ID).unwrap();
        let origin = Origin::parse("https://example.com").unwrap();
        let event = capture_failed(id, Some(&origin));
        assert_eq!(event.agent, SESSION_AGENT);
        assert_eq!(event.kind, AuditKind::CredentialCaptured);
        assert_eq!(event.result, "failed");
        assert_eq!(event.session_id.as_deref(), Some(ID));
        assert_eq!(event.origin.as_deref(), Some("https://example.com"));
        assert_eq!(event.item_name, None, "no item was stored");
    }

    #[test]
    fn a_capture_from_an_unreadable_origin_is_recorded_without_one() {
        let event = capture_failed(SessionId::parse(ID).unwrap(), None);
        assert_eq!(event.origin, None);
        assert_eq!(event.result, "failed");
    }
}
