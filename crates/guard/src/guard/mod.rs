//! `Guard`: the enforcement state of one session supervisor — the store it records to, and
//! the vault (reopened whenever its version moves) with the masker built from it.
//!
//! - [`codes`] — one-time codes the session typed, masked for a short while.
//! - [`denials`] — the per-second cap on recorded refusals.
//! - [`probes`] — refusing requests that carry a vault value, and locking the session.
//! - [`redact`] — masking every result and error before it leaves the supervisor.

use crate::{
    GuardedBrowser, audit,
    pep::{
        mask::{Masker, masker},
        substitute::{Destination, Prepared, Request, references, resolve_checked},
    },
};
use anyhow::{Result, anyhow};
use keyharbor_core::{ItemUse, Vault};
use keyharbor_domain::{
    SessionId,
    audit::{AuditEvent, AuditKind},
    denial::DenialReason,
    ports::{Clock, Store},
    vault::{Domain, ItemName, Origin, Placeholder},
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, MutexGuard, atomic::AtomicU32},
};

mod codes;
mod denials;
mod probes;
mod redact;

#[cfg(test)]
pub(crate) use denials::MAX_DENIALS_PER_SECOND;
pub(crate) use keyharbor_domain::session::substitution_lock::LockReason;
#[cfg(test)]
pub(crate) use probes::MAX_PROBES;

/// The store a supervisor shares between its control connections and the guard.
pub type SharedStore = Arc<Mutex<Box<dyn Store>>>;

/// Locks the shared store. A poisoned lock (a holder panicked) is an error, never a panic.
pub fn lock_store(store: &SharedStore) -> Result<MutexGuard<'_, Box<dyn Store>>> {
    store.lock().map_err(|_| anyhow!("store lock poisoned"))
}

/// Opens the vault (adapters chosen by the interface that started the supervisor).
pub type VaultOpener = Box<dyn Fn() -> Result<Vault> + Send + Sync>;

/// The vault as of one version, and the masker for its values once a response needed it.
pub(crate) struct Loaded {
    version: i64,
    pub(crate) vault: Vault,
    masker: Option<Masker>,
}

impl Loaded {
    /// The masker for this vault version, built on first use. A failed build is returned (the
    /// caller withholds) and tried again next time; it never panics while the vault is locked.
    pub(crate) fn masker(&mut self) -> Result<&Masker> {
        let built = match self.masker.take() {
            Some(built) => built,
            None => masker(&self.vault)?,
        };
        Ok(self.masker.insert(built))
    }
}

pub struct Guard {
    session: SessionId,
    store: SharedStore,
    open_vault: VaultOpener,
    /// Opened on first use, reopened when the vault version moves (so rotations are seen).
    vault: Mutex<Option<Loaded>>,
    /// Backend node ids of every element that received substituted text in this session.
    /// Snapshots show them as `[redacted]` whatever their value becomes (03 §3.7, C1).
    tainted: Mutex<BTreeSet<u64>>,
    /// Once set, nothing more is substituted in this session until a person lifts it (C2, I1,
    /// #190).
    locked: Mutex<Option<LockReason>>,
    /// Requests that carried a vault value (I1). At [`probes::MAX_PROBES`] the session stops
    /// substituting and refuses every request that carries agent text.
    probes: AtomicU32,
    /// Refusals recorded in the current second, and those left out (Minor: a flood of
    /// refused requests must not flood the record).
    denials: Mutex<denials::Denials>,
    /// One-time codes typed in this session, masked in responses for a short while (#206).
    codes: Mutex<codes::RecentCodes>,
    /// For the per-second refusal cap, and the moment a one-time code is made for.
    clock: Arc<dyn Clock>,
}

impl Guard {
    pub fn new(
        session: SessionId,
        store: SharedStore,
        open_vault: VaultOpener,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            session,
            store,
            open_vault,
            vault: Mutex::new(None),
            tainted: Mutex::new(BTreeSet::new()),
            locked: Mutex::new(None),
            probes: AtomicU32::new(0),
            denials: Mutex::new(denials::Denials::default()),
            codes: Mutex::new(codes::RecentCodes::default()),
            clock,
        }
    }

    /// The decorator through which `agent`'s request reaches `browser`.
    pub fn browser<'a, B>(&'a self, browser: &'a mut B, agent: &'a str) -> GuardedBrowser<'a, B> {
        GuardedBrowser::new(browser, self, agent)
    }

    /// Runs `work` with the vault at its current version, or `None` if it was never
    /// initialized. The root key is read only here, the first time a placeholder or a response
    /// needs it.
    pub(crate) fn with_vault<T>(&self, work: impl FnOnce(Option<&mut Loaded>) -> T) -> Result<T> {
        let row = lock_store(&self.store)?.vault_get()?;
        let mut slot = self
            .vault
            .lock()
            .map_err(|_| anyhow!("vault lock poisoned"))?;
        let Some(row) = row else {
            *slot = None;
            return Ok(work(None));
        };
        if slot
            .as_ref()
            .is_none_or(|loaded| loaded.version != row.version)
        {
            *slot = None;
            let vault = (self.open_vault)()?;
            *slot = Some(Loaded {
                version: row.version,
                vault,
                masker: None,
            });
        }
        Ok(work(slot.as_mut()))
    }

    /// Best effort: a failed write to the usage record does not undo or block the operation.
    pub(crate) fn record(&self, event: &AuditEvent) {
        if let Ok(store) = self.store.lock() {
            audit::record(&**store, event);
        }
    }

    /// Records a refused use and returns the error the agent sees: item name and reason only.
    pub(crate) fn denied(
        &self,
        agent: &str,
        item: &ItemName,
        origin: Option<&str>,
        reason: DenialReason,
    ) -> anyhow::Error {
        let session = self.session.to_string();
        self.record_denial(
            agent,
            &audit::denied(agent, Some(&session), item, origin, reason),
        );
        audit::denial_error(item, reason)
    }

    pub(crate) fn session(&self) -> SessionId {
        self.session
    }

    pub(crate) fn store(&self) -> &SharedStore {
        &self.store
    }

    /// Now, by the guard's clock.
    pub(crate) fn unix_seconds(&self) -> u64 {
        self.clock.unix_seconds()
    }

    /// What the vault says about `item` for `agent`'s request, refusing (and recording) a
    /// missing or tampered one.
    pub(crate) fn item_use(&self, agent: &str, item: &ItemName) -> Result<ItemUse> {
        let found = self.with_vault(|loaded| loaded.map(|l| l.vault.item_use(item)));
        match found {
            Ok(Some(Ok(Some(info)))) => Ok(info),
            Ok(Some(Err(_))) => Err(self.denied(agent, item, None, DenialReason::Tampered)),
            Ok(_) => Err(self.denied(agent, item, None, DenialReason::NotFound)),
            Err(_) => Err(self.denied(agent, item, None, DenialReason::Vault)),
        }
    }

    /// Records `first_use_origin` the first time a personal item goes into `origin` (R-36).
    /// Never refuses: identity and address details are meant for many sites.
    pub(crate) fn note_use(&self, agent: &str, item: &ItemName, domain: Domain, origin: &Origin) {
        if domain != Domain::Personal {
            return;
        }
        let first = self
            .store
            .lock()
            .ok()
            .and_then(|s| s.note_origin_use(item.as_str(), origin.as_str()).ok());
        if first == Some(true) {
            self.record(
                &AuditEvent::new(agent, AuditKind::FirstUseOrigin, "ok")
                    .session(self.session)
                    .item(item.as_str())
                    .origin(Some(origin.as_str())),
            );
        }
    }

    /// Remembers that `node` received substituted text.
    pub(crate) fn taint(&self, node: u64) {
        if let Ok(mut tainted) = self.tainted.lock() {
            tainted.insert(node);
        }
    }

    /// The elements snapshots must redact.
    pub(crate) fn tainted(&self) -> BTreeSet<u64> {
        self.tainted.lock().map(|t| t.clone()).unwrap_or_default()
    }

    /// Every text that will be typed into the page. Without placeholders it is used as is.
    /// Otherwise every item must exist, be allowed for `agent`, — for a one-time code — be
    /// allowed to give codes, — if origin-bound — list the origin of the frame the text goes
    /// into, and each field must suit the element. Any refusal is recorded and nothing
    /// resolves. A typed code is masked in responses from here on (`codes`).
    pub fn prepare_text(
        &self,
        agent: &str,
        template: &str,
        to: Destination<'_>,
    ) -> Result<Prepared> {
        let origin = to.frame;
        let (refs, items) = references(template)?;
        let Some(first) = items.first().cloned() else {
            return Ok(Prepared::plain(template));
        };
        let origin_text = origin.map(Origin::as_str);
        let deny =
            |item: &ItemName, reason: DenialReason| self.denied(agent, item, origin_text, reason);
        if self.is_locked() {
            return Err(deny(&first, DenialReason::Locked));
        }
        let asks_code = refs.iter().any(|r| matches!(r, Placeholder::Otp(_)));
        let (rules, agent_otp) = {
            let store = lock_store(&self.store)?;
            let agent_otp = asks_code && keyharbor_core::settings::agent_otp(&**store)?;
            (store.policy_rules()?, agent_otp)
        };
        let now = self.unix_seconds();
        let checked = self.with_vault(|loaded| {
            resolve_checked(
                loaded.map(|l| &l.vault),
                &rules,
                &Request {
                    agent,
                    template,
                    refs: &refs,
                    items: &items,
                    agent_otp,
                    unix_seconds: now,
                },
                to,
                &deny,
            )
        });
        let resolved = match checked {
            Ok(result) => result?,
            Err(_) => return Err(deny(&first, DenialReason::Vault)),
        };
        let codes: Vec<ItemName> = resolved.codes.iter().map(|(i, _)| i.clone()).collect();
        self.remember_codes(resolved.codes, now);
        Ok(Prepared {
            text: resolved.text,
            items,
            domains: resolved.domains,
            codes,
        })
    }

    /// Masks `codes` in responses from now on, for a short while.
    fn remember_codes(&self, codes: Vec<(ItemName, keyharbor_core::OneTimeCode)>, now: u64) {
        if codes.is_empty() {
            return;
        }
        let mut recent = self
            .codes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (item, code) in codes {
            recent.remember(&item, code, now);
        }
    }

    /// The masker for the one-time codes typed lately, if any.
    pub(crate) fn code_masker(&self) -> Result<Option<Masker>> {
        let now = self.unix_seconds();
        self.codes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .masker(now)
    }
}
