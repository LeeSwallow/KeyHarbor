//! Requests whose agent-written text holds a vault value (I1), and the session lock that
//! stops all substitution once the session cannot be trusted any more (C2). Only a person
//! lifts the lock ([`Guard::unlock`], #190).

use super::Guard;
use anyhow::{Result, bail};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    session::substitution_lock::{LockReason, UNLOCKED},
    vault::placeholder,
};
use std::sync::{MutexGuard, atomic::Ordering};

/// Requests carrying a vault value a session tolerates before it locks (I1). Fixed, not a
/// setting: raising it gives a guessing agent more answers, and an agent cannot be trusted
/// to set it.
pub const MAX_PROBES: u32 = 3;

/// The refusal once the session has locked for probing.
const NO_MORE_TEXT: &str =
    "denied: this session sent vault values in requests; it accepts no more text";
/// The refusal when the values to check against cannot be loaded (or masked).
const CHECK_WITHHELD: &str = "request withheld: vault values could not be loaded for checking";

/// The refusal of a request that carries a vault value: it names the placeholder form only.
fn probe_refusal() -> String {
    format!(
        "denied: the request contains a vault value; write its {} placeholder instead",
        placeholder("item", "field")
    )
}

impl Guard {
    /// The lock slot. A holder that panicked left a plain `Option`, so it is read through.
    fn lock_slot(&self) -> MutexGuard<'_, Option<LockReason>> {
        self.locked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Stops all substitution until a person lifts it ([`Self::unlock`]) and records why.
    pub(crate) fn lock(&self, agent: &str, reason: LockReason) {
        let mut slot = self.lock_slot();
        if slot.is_none() {
            *slot = Some(reason);
            drop(slot);
            self.record(
                &AuditEvent::new(agent, AuditKind::SubstitutionLocked, reason.as_str())
                    .session(self.session),
            );
        }
    }

    pub fn is_locked(&self) -> bool {
        self.lock_reason().is_some()
    }

    /// Why the session stopped substituting, or `None` while it still does.
    pub fn lock_reason(&self) -> Option<LockReason> {
        *self.lock_slot()
    }

    /// Lets the session substitute again: the lock and the count of requests that carried a
    /// vault value start over. For a person's request only — the supervisor refuses it from an
    /// agent (#190, J-220). Records `substitution_locked`/`unlocked` under `person` when a lock
    /// was lifted, and returns why it had locked.
    pub fn unlock(&self, person: &str) -> Option<LockReason> {
        let lifted = self.lock_slot().take();
        self.probes.store(0, Ordering::SeqCst);
        if lifted.is_some() {
            self.record(
                &AuditEvent::new(person, AuditKind::SubstitutionLocked, UNLOCKED)
                    .session(self.session),
            );
        }
        lifted
    }

    /// Refuses a request whose agent-written text (URL, typed text, selector, tab id) holds a
    /// vault value in any masked form: answering it would let the masker confirm a guess
    /// (I1). Each such request is recorded as `mask_probe` without naming the item; after
    /// [`MAX_PROBES`] the session locks and refuses every request that carries text. If the
    /// values cannot be loaded or the masker cannot be built, the request is withheld.
    pub fn check_literals(&self, agent: &str, literals: &[&str]) -> Result<()> {
        if literals.iter().all(|l| l.is_empty()) {
            return Ok(());
        }
        if self.probes.load(Ordering::SeqCst) >= MAX_PROBES {
            bail!(NO_MORE_TEXT);
        }
        let probed = self.with_vault(|loaded| -> Result<bool> {
            let Some(loaded) = loaded else {
                return Ok(false);
            };
            let masker = loaded.masker()?;
            Ok(literals.iter().any(|l| masker.redact(l) != *l))
        });
        match probed {
            Ok(Ok(false)) => Ok(()),
            Ok(Ok(true)) => {
                self.record(
                    &AuditEvent::new(agent, AuditKind::MaskProbe, "denied").session(self.session),
                );
                if self.probes.fetch_add(1, Ordering::SeqCst) + 1 >= MAX_PROBES {
                    self.lock(agent, LockReason::MaskProbe);
                }
                bail!(probe_refusal())
            }
            _ => bail!(CHECK_WITHHELD),
        }
    }
}
