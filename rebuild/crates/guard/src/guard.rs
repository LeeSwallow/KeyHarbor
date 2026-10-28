//! `Guard`: the enforcement state of one session supervisor — the store it records to, and
//! the vault (reopened whenever its version moves) with the masker built from it.

use crate::{
    GuardedBrowser, audit,
    pep::{
        mask::{Masker, masker},
        substitute::{Destination, Prepared, Request, references, resolve_checked},
    },
};
use anyhow::{Result, anyhow, bail};
use keyharbor_core::Vault;
use keyharbor_domain::{
    SessionId,
    audit::{AuditEvent, AuditKind},
    ports::Store,
    vault::{ItemName, Origin},
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};
use zeroize::Zeroizing;

/// The store a supervisor shares between its control connections and the guard.
pub type SharedStore = Arc<Mutex<Box<dyn Store>>>;

/// Opens the vault (adapters chosen by the interface that started the supervisor).
pub type VaultOpener = Box<dyn Fn() -> Result<Vault> + Send + Sync>;

/// The vault as of one version, and the masker for its values once a response needed it.
struct Loaded {
    version: i64,
    vault: Vault,
    masker: Option<Masker>,
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
    /// Once set, nothing more is substituted in this session (C2, I1).
    locked: AtomicBool,
    /// Requests that carried a vault value (I1). At [`MAX_PROBES`] the session stops
    /// substituting and refuses every request that carries agent text.
    probes: AtomicU32,
    /// Refusals recorded in the current second, and those left out (Minor: a flood of
    /// refused requests must not flood the record).
    denials: Mutex<Denials>,
    /// Seconds since the epoch; replaced in tests.
    clock: fn() -> u64,
}

/// Refusals a session records per second; the rest become one counter event.
pub const MAX_DENIALS_PER_SECOND: u32 = 10;

#[derive(Default)]
struct Denials {
    second: u64,
    recorded: u32,
    suppressed: u32,
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Requests carrying a vault value a session tolerates before it locks (I1).
pub const MAX_PROBES: u32 = 3;

impl Guard {
    pub fn new(session: SessionId, store: SharedStore, open_vault: VaultOpener) -> Self {
        Self {
            session,
            store,
            open_vault,
            vault: Mutex::new(None),
            tainted: Mutex::new(BTreeSet::new()),
            locked: AtomicBool::new(false),
            probes: AtomicU32::new(0),
            denials: Mutex::new(Denials::default()),
            clock: unix_seconds,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_clock(mut self, clock: fn() -> u64) -> Self {
        self.clock = clock;
        self
    }

    /// Records a refusal unless this second's cap is reached; a new second first records how
    /// many were left out of the last one.
    fn record_denial(&self, agent: &str, event: &AuditEvent) {
        let now = (self.clock)();
        let (flush, record) = {
            let Ok(mut d) = self.denials.lock() else {
                return;
            };
            let flush = if d.second != now {
                let left_out = std::mem::take(&mut d.suppressed);
                d.second = now;
                d.recorded = 0;
                left_out
            } else {
                0
            };
            let record = d.recorded < MAX_DENIALS_PER_SECOND;
            if record {
                d.recorded += 1;
            } else {
                d.suppressed += 1;
            }
            (flush, record)
        };
        if flush > 0 {
            self.record_suppressed(agent, flush);
        }
        if record {
            self.record(event);
        }
    }

    fn record_suppressed(&self, agent: &str, count: u32) {
        self.record(
            &AuditEvent::new(agent, AuditKind::DeniedSuppressed, &count.to_string())
                .session(self.session),
        );
    }

    /// The decorator through which `agent`'s request reaches `browser`.
    pub fn browser<'a, B>(&'a self, browser: &'a mut B, agent: &'a str) -> GuardedBrowser<'a, B> {
        GuardedBrowser::new(browser, self, agent)
    }

    /// Runs `work` with the vault at its current version, or `None` if it was never
    /// initialized. The root key is read only here, the first time a placeholder or a response
    /// needs it.
    fn with_vault<T>(&self, work: impl FnOnce(Option<&mut Loaded>) -> T) -> Result<T> {
        let row = self
            .store
            .lock()
            .map_err(|_| anyhow!("store lock poisoned"))?
            .vault_get()?;
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
        reason: &str,
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

    /// Remembers that `node` received substituted text.
    pub(crate) fn taint(&self, node: u64) {
        if let Ok(mut tainted) = self.tainted.lock() {
            tainted.insert(node);
        }
    }

    /// Stops all substitution for the rest of the session and records why.
    pub(crate) fn lock(&self, agent: &str, reason: &str) {
        if !self.locked.swap(true, Ordering::SeqCst) {
            self.record(
                &AuditEvent::new(agent, AuditKind::SubstitutionLocked, reason)
                    .session(self.session),
            );
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }

    /// Refuses a request whose agent-written text (URL, typed text, selector, tab id) holds a
    /// vault value in any masked form: answering it would let the masker confirm a guess
    /// (I1). Each such request is recorded as `mask_probe` without naming the item; after
    /// [`MAX_PROBES`] the session locks and refuses every request that carries text.
    pub fn check_literals(&self, agent: &str, literals: &[&str]) -> Result<()> {
        if literals.iter().all(|l| l.is_empty()) {
            return Ok(());
        }
        if self.probes.load(Ordering::SeqCst) >= MAX_PROBES {
            bail!("denied: this session sent vault values in requests; it accepts no more text");
        }
        let probed = self.with_vault(|loaded| -> Result<bool> {
            let Some(loaded) = loaded else {
                return Ok(false);
            };
            if loaded.masker.is_none() {
                loaded.masker = Some(masker(&loaded.vault)?);
            }
            let masker = loaded.masker.as_ref().expect("built");
            Ok(literals.iter().any(|l| masker.redact(l) != *l))
        });
        match probed {
            Ok(Ok(false)) => Ok(()),
            Ok(Ok(true)) => {
                self.record(
                    &AuditEvent::new(agent, AuditKind::MaskProbe, "denied").session(self.session),
                );
                if self.probes.fetch_add(1, Ordering::SeqCst) + 1 >= MAX_PROBES {
                    self.lock(agent, "mask_probe");
                }
                bail!(
                    "denied: the request contains a vault value; write its {{{{secret:item.field}}}} placeholder instead"
                )
            }
            _ => bail!("request withheld: vault values could not be loaded for checking"),
        }
    }

    /// The elements snapshots must redact.
    pub(crate) fn tainted(&self) -> BTreeSet<u64> {
        self.tainted.lock().map(|t| t.clone()).unwrap_or_default()
    }

    /// Every text that will be typed into the page. Without placeholders it is used as is.
    /// Otherwise every item must exist, be allowed for `agent`, — if origin-bound — list the
    /// origin of the frame the text goes into, and each field must suit the element. Any
    /// refusal is recorded and nothing resolves.
    pub fn prepare_text(
        &self,
        agent: &str,
        template: &str,
        to: Destination<'_>,
    ) -> Result<Prepared> {
        let origin = to.frame;
        let (refs, items) = references(template)?;
        let Some(first) = items.first().cloned() else {
            return Ok(Prepared {
                text: Zeroizing::new(template.to_owned()),
                items,
            });
        };
        let origin_text = origin.map(Origin::as_str);
        let deny = |item: &ItemName, reason: &str| self.denied(agent, item, origin_text, reason);
        if self.is_locked() {
            return Err(deny(&first, "locked"));
        }
        let rules = self
            .store
            .lock()
            .map_err(|_| anyhow!("store lock poisoned"))?
            .policy_rules()?;
        let checked = self.with_vault(|loaded| {
            resolve_checked(
                loaded.map(|l| &l.vault),
                &rules,
                &Request {
                    agent,
                    template,
                    refs: &refs,
                    items: &items,
                },
                to,
                &deny,
            )
        });
        let text = match checked {
            Ok(result) => result?,
            Err(_) => return Err(deny(&first, "vault")),
        };
        Ok(Prepared { text, items })
    }

    /// Every result before it leaves the supervisor: each vault value becomes its placeholder.
    /// If the values cannot be loaded the response is withheld, never sent unmasked.
    pub fn redact(&self, value: Value) -> Result<Value> {
        let masked = self.with_vault(|loaded| -> Result<Value> {
            let Some(loaded) = loaded else {
                return Ok(value);
            };
            if loaded.masker.is_none() {
                loaded.masker = Some(masker(&loaded.vault)?);
            }
            Ok(loaded.masker.as_ref().expect("built").redact_json(value))
        });
        match masked {
            Ok(Ok(value)) => Ok(value),
            _ => bail!("response withheld: vault values could not be loaded for masking"),
        }
    }

    /// A whole outcome, masked exactly once: the value, or the error's message.
    pub fn redact_outcome(&self, outcome: Result<Value>) -> Result<Value> {
        match outcome {
            Ok(value) => self.redact(value),
            Err(error) => match self.redact(Value::String(format!("{error:#}")))? {
                Value::String(message) => Err(anyhow!(message)),
                _ => bail!("operation failed"),
            },
        }
    }
}

impl Drop for Guard {
    /// The supervisor is ending: the refusals left out of the last second are counted.
    fn drop(&mut self) {
        let left_out = self
            .denials
            .get_mut()
            .map(|d| std::mem::take(&mut d.suppressed))
            .unwrap_or(0);
        if left_out > 0 {
            self.record_suppressed(keyharbor_domain::policy::LOCAL_AGENT, left_out);
        }
    }
}
