//! `Guard`: the enforcement state of one session supervisor — the store it records to, and
//! the vault (reopened whenever its version moves) with the masker built from it.

use crate::{
    GuardedBrowser, audit,
    pep::{
        mask::{Masker, masker},
        substitute::{Prepared, references, resolve_checked},
    },
};
use anyhow::{Result, anyhow, bail};
use keyharbor_core::Vault;
use keyharbor_domain::{
    SessionId,
    audit::AuditEvent,
    ports::Store,
    vault::{ItemName, Origin},
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
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
}

impl Guard {
    pub fn new(session: SessionId, store: SharedStore, open_vault: VaultOpener) -> Self {
        Self {
            session,
            store,
            open_vault,
            vault: Mutex::new(None),
        }
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
        self.record(&audit::denied(agent, Some(&session), item, origin, reason));
        audit::denial_error(item, reason)
    }

    pub(crate) fn session(&self) -> SessionId {
        self.session
    }

    /// Every text that will be typed into the page. Without placeholders it is used as is.
    /// Otherwise every item must exist, be allowed for `agent`, and — if origin-bound — list
    /// `origin` (the frame the text goes into). Any refusal is recorded and nothing resolves.
    pub fn prepare_text(
        &self,
        agent: &str,
        template: &str,
        origin: Option<&Origin>,
    ) -> Result<Prepared> {
        let (refs, items) = references(template)?;
        let Some(first) = items.first().cloned() else {
            return Ok(Prepared {
                text: Zeroizing::new(template.to_owned()),
                items,
            });
        };
        let origin_text = origin.map(Origin::as_str);
        let deny = |item: &ItemName, reason: &str| self.denied(agent, item, origin_text, reason);
        let rules = self
            .store
            .lock()
            .map_err(|_| anyhow!("store lock poisoned"))?
            .policy_rules()?;
        let checked = self.with_vault(|loaded| {
            resolve_checked(
                loaded.map(|l| &l.vault),
                &rules,
                agent,
                template,
                &refs,
                &items,
                origin,
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
