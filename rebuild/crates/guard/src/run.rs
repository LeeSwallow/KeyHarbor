//! `keyharbor run` (ADR-0010): which env values a child process may get, and the masker for
//! its output. Starting the process and pumping its output stay in the CLI.
//!
//! Every referenced item is checked first (exists, not discarded, allowed for the agent, not
//! origin-bound); a refusal is recorded and nothing is resolved.

use crate::{
    audit,
    pdp::{Action, decide},
    pep::{mask, mask::Masker, origin},
};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_core::{ItemStatus, PlaintextAccess, Vault, fill_template, parse_dotenv};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    policy::Rule,
    ports::Store,
    vault::{FieldName, ItemKind, ItemName, PlaceholderRef, parse_placeholders},
};
use std::path::PathBuf;
use zeroize::Zeroizing;

/// What to inject: whole env items and `.env`-style template files.
pub struct Request<'a> {
    pub agent: &'a str,
    pub items: &'a [String],
    pub files: &'a [PathBuf],
}

/// The checked, resolved variables for one child process and the masker for its output.
pub struct Injection {
    pub vars: Vec<(String, Zeroizing<String>)>,
    pub masker: Masker,
    agent: String,
    items: Vec<ItemName>,
}

impl Injection {
    /// Records `env_injected` for every item used, once the child has started.
    pub fn record_injected(&self, store: &dyn Store) {
        for item in &self.items {
            audit::record(
                store,
                &AuditEvent::new(&self.agent, AuditKind::EnvInjected, "ok").item(item.as_str()),
            );
        }
    }
}

struct Checker<'a> {
    vault: &'a Vault,
    store: &'a dyn Store,
    rules: Vec<Rule>,
    agent: &'a str,
    checked: Vec<ItemName>,
}

impl Checker<'_> {
    fn deny(&self, item: &ItemName, reason: &str) -> anyhow::Error {
        audit::record(
            self.store,
            &audit::denied(self.agent, None, item, None, reason),
        );
        audit::denial_error(item, reason)
    }

    /// Checks `item` once; returns its field names.
    fn check(&mut self, item: &ItemName, env_only: bool) -> Result<Vec<String>> {
        let info = match self.vault.item_use(item) {
            Ok(Some(info)) => info,
            Ok(None) => return Err(self.deny(item, "not_found")),
            Err(_) => return Err(self.deny(item, "tampered")),
        };
        if env_only && info.kind != ItemKind::Env {
            bail!(
                "{} is not an env item; use --env-file with placeholders",
                item.as_str()
            );
        }
        if info.status == ItemStatus::Discarded {
            return Err(self.deny(item, "discarded"));
        }
        let action = Action::UseItem {
            item: item.clone(),
            domain: info.domain,
        };
        if !decide(&self.rules, self.agent, &action).allowed() {
            return Err(self.deny(item, "policy"));
        }
        if !origin::allows_process(info.origin_binding) {
            return Err(self.deny(item, "origin"));
        }
        if !self.checked.contains(item) {
            self.checked.push(item.clone());
        }
        Ok(info.fields)
    }
}

fn add_var(
    vars: &mut Vec<(String, Zeroizing<String>)>,
    name: &str,
    value: Zeroizing<String>,
) -> Result<()> {
    if vars.iter().any(|(n, _)| n == name) {
        bail!("variable {name} given twice");
    }
    vars.push((name.to_owned(), value));
    Ok(())
}

/// Checks everything before resolving anything, then resolves and builds the masker.
pub fn prepare(store: &dyn Store, vault: &Vault, request: &Request<'_>) -> Result<Injection> {
    let mut checker = Checker {
        vault,
        store,
        rules: store.policy_rules()?,
        agent: request.agent,
        checked: Vec::new(),
    };
    let mut item_vars: Vec<(ItemName, Vec<String>)> = Vec::new();
    for name in request.items {
        let item = ItemName::parse(name).map_err(|e| anyhow!("{e}"))?;
        let fields = checker.check(&item, true)?;
        item_vars.push((item, fields));
    }
    let mut templates = Vec::new();
    for path in request.files {
        let text = Zeroizing::new(
            std::fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?,
        );
        for (name, template) in parse_dotenv(&text)? {
            for reference in parse_placeholders(&template).map_err(|e| anyhow!("{e}"))? {
                let fields = checker.check(&reference.item, false)?;
                if !fields.iter().any(|f| f == reference.field.as_str()) {
                    return Err(checker.deny(&reference.item, "not_found"));
                }
            }
            templates.push((name, template));
        }
    }

    // Every check passed: this is where the guard grants plaintext.
    let access = PlaintextAccess::issue();
    let mut vars: Vec<(String, Zeroizing<String>)> = Vec::new();
    for (item, fields) in &item_vars {
        for field in fields {
            let reference = PlaceholderRef {
                item: item.clone(),
                field: FieldName::parse(field).map_err(|e| anyhow!("{e}"))?,
            };
            add_var(&mut vars, field, vault.resolve(&access, &reference)?)?;
        }
    }
    for (name, template) in &templates {
        let value = fill_template(template, |r| vault.resolve(&access, r))?;
        add_var(&mut vars, name.as_str(), value)?;
    }
    Ok(Injection {
        vars,
        masker: mask::masker(vault)?,
        agent: request.agent.to_owned(),
        items: checker.checked,
    })
}
