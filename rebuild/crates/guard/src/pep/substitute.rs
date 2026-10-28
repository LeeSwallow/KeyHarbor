//! Placeholder substitution (02 §2.5.2): `{{secret:item.field}}` in text bound for a page is
//! replaced by the value only if every referenced item exists, is not discarded, is allowed
//! for the agent (`pdp`), lists the frame's origin when origin-bound (`origin`), has the
//! field, and the field may go into the element (`field-kind`: a password only into a
//! password input). One refusal stops everything: nothing is resolved and nothing is typed.

use crate::pdp::checks::{self, Target, UseContext};
use anyhow::{Result, anyhow};
use keyharbor_core::{PlaintextAccess, Vault, fill_template};
use keyharbor_domain::{
    denial::DenialReason,
    policy::Rule,
    ports::browser::Element,
    vault::{Domain, ItemName, Origin, PlaceholderRef, parse_placeholders},
};
use zeroize::Zeroizing;

/// Text resolved for one input, with the items it used and their domains (same order).
pub struct Prepared {
    pub text: Zeroizing<String>,
    pub items: Vec<ItemName>,
    pub domains: Vec<Domain>,
}

/// What [`resolve_checked`] produced.
pub struct Resolved {
    pub text: Zeroizing<String>,
    pub domains: Vec<Domain>,
}

/// The template's placeholders and the items they name (first use first, once each).
pub fn references(template: &str) -> Result<(Vec<PlaceholderRef>, Vec<ItemName>)> {
    let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
    let mut items: Vec<ItemName> = Vec::new();
    for r in &refs {
        if !items.contains(&r.item) {
            items.push(r.item.clone());
        }
    }
    Ok((refs, items))
}

/// Where resolved text goes: the frame's origin and the element, as the browser reports them.
#[derive(Clone, Copy, Default)]
pub struct Destination<'a> {
    pub frame: Option<&'a Origin>,
    /// `None` when the caller has no element (then field kinds are not checked).
    pub element: Option<&'a Element>,
}

/// What one input asks for: the template, its references and the items they name.
pub struct Request<'a> {
    pub agent: &'a str,
    pub template: &'a str,
    pub refs: &'a [PlaceholderRef],
    pub items: &'a [ItemName],
}

/// Checks every item, in order, through the browser chain (`pdp::checks::BROWSER`), then
/// resolves the whole template. `deny(item, reason)` records a refusal and returns the
/// error the agent sees.
pub fn resolve_checked(
    vault: Option<&Vault>,
    rules: &[Rule],
    request: &Request<'_>,
    to: Destination<'_>,
    deny: &dyn Fn(&ItemName, DenialReason) -> anyhow::Error,
) -> Result<Resolved> {
    let first = &request.items[0];
    let Some(vault) = vault else {
        return Err(deny(first, DenialReason::NotFound));
    };
    let mut domains = Vec::with_capacity(request.items.len());
    for item in request.items {
        let info = match vault.item_use(item) {
            Ok(Some(info)) => info,
            Ok(None) => return Err(deny(item, DenialReason::NotFound)),
            Err(_) => return Err(deny(item, DenialReason::Tampered)),
        };
        let fields: Vec<&str> = request
            .refs
            .iter()
            .filter(|r| &r.item == item)
            .map(|r| r.field.as_str())
            .collect();
        let ctx = UseContext {
            agent: request.agent,
            item,
            info: &info,
            rules,
            fields: &fields,
            to: Target::Browser(to),
        };
        checks::run(checks::BROWSER, &ctx).map_err(|reason| deny(item, reason))?;
        domains.push(info.domain);
    }
    // Every check passed: this is where the guard grants plaintext.
    let access = PlaintextAccess::issue();
    let text = fill_template(request.template, |r| vault.resolve(&access, r))
        .map_err(|_| deny(first, DenialReason::Vault))?;
    Ok(Resolved { text, domains })
}
