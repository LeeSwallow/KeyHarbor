//! Placeholder substitution (02 §2.5.2): `{{secret:item.field}}` in text bound for a page is
//! replaced by the value only if every referenced item exists, is not discarded, is allowed
//! for the agent (`pdp`), lists the frame's origin when origin-bound (`origin`), has the
//! field, and the field may go into the element (`field-kind`: a password only into a
//! password input). One refusal stops everything: nothing is resolved and nothing is typed.

use crate::{
    pdp::{Action, decide},
    pep::origin,
};
use anyhow::{Result, anyhow};
use keyharbor_core::{ItemStatus, PlaintextAccess, Vault, fill_template};
use keyharbor_domain::{
    policy::Rule,
    ports::browser::Element,
    vault::{InputKind, ItemName, Origin, PlaceholderRef, parse_placeholders},
};
use zeroize::Zeroizing;

/// Text resolved for one input, with the items it used.
pub struct Prepared {
    pub text: Zeroizing<String>,
    pub items: Vec<ItemName>,
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

/// Checks every item, in order, then resolves the whole template. `deny(item, reason)`
/// records a refusal and returns the error the agent sees.
pub fn resolve_checked(
    vault: Option<&Vault>,
    rules: &[Rule],
    request: &Request<'_>,
    to: Destination<'_>,
    deny: &dyn Fn(&ItemName, &str) -> anyhow::Error,
) -> Result<Zeroizing<String>> {
    let Request {
        agent,
        template,
        refs,
        items,
    } = *request;
    let frame = to.frame;
    let first = &items[0];
    let Some(vault) = vault else {
        return Err(deny(first, "not_found"));
    };
    for item in items {
        let info = match vault.item_use(item) {
            Ok(Some(info)) => info,
            Ok(None) => return Err(deny(item, "not_found")),
            Err(_) => return Err(deny(item, "tampered")),
        };
        if info.status == ItemStatus::Discarded {
            return Err(deny(item, "discarded"));
        }
        let action = Action::UseItem {
            item: item.clone(),
            domain: info.domain,
        };
        if !decide(rules, agent, &action).allowed() {
            return Err(deny(item, "policy"));
        }
        if !origin::allows_frame(info.origin_binding, &info.origins, frame) {
            return Err(deny(item, "origin"));
        }
        let missing = refs
            .iter()
            .any(|r| &r.item == item && !info.fields.iter().any(|f| f == r.field.as_str()));
        if missing {
            return Err(deny(item, "not_found"));
        }
        if let Some(element) = to.element {
            let password_only = refs.iter().filter(|r| &r.item == item).any(|r| {
                info.kind
                    .field(r.field.as_str())
                    .is_some_and(|spec| spec.input == InputKind::Password)
            });
            if password_only && !element.is_password() {
                return Err(deny(item, "field-kind"));
            }
        }
    }
    // Text typed while the focus is not on something that takes text (a frame host, a
    // button, the body) could land anywhere.
    if to.element.is_some_and(|element| !element.takes_text()) {
        return Err(deny(first, "focus"));
    }
    // Every check passed: this is where the guard grants plaintext.
    let access = PlaintextAccess::issue();
    fill_template(template, |r| vault.resolve(&access, r)).map_err(|_| deny(first, "vault"))
}
