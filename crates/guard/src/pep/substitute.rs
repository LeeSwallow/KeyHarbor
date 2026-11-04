//! Placeholder substitution (02 §2.5.2): `{{secret:item.field}}` and `{{otp:item}}` in text
//! bound for a page are replaced by the value or the code of the moment only if every
//! referenced item exists, is not discarded, is allowed for the agent (`pdp`), — for a code —
//! may give codes (`otp`), lists the frame's origin when origin-bound (`origin`), has the
//! field, and the field may go into the element (`field-kind`: a password only into a password
//! input). One refusal stops everything: nothing is resolved and nothing is typed.

use crate::pdp::checks::{self, CodeUse, Target, UseContext};
use anyhow::{Result, anyhow};
use keyharbor_core::{OneTimeCode, PlaintextAccess, Vault, fill_placeholders};
use keyharbor_domain::{
    denial::DenialReason,
    policy::Rule,
    ports::browser::Element,
    vault::{Domain, ItemName, Origin, Placeholder, parse_placeholders},
};
use zeroize::Zeroizing;

/// Text resolved for one input, with the items it used and their domains (same order).
pub struct Prepared {
    pub text: Zeroizing<String>,
    pub items: Vec<ItemName>,
    pub domains: Vec<Domain>,
    /// The items whose one-time code is in `text` (recorded as `otp_used`, never the code).
    pub codes: Vec<ItemName>,
}

impl Prepared {
    /// Text without placeholders, used as it is.
    pub fn plain(template: &str) -> Self {
        Self {
            text: Zeroizing::new(template.to_owned()),
            items: Vec::new(),
            domains: Vec::new(),
            codes: Vec::new(),
        }
    }
}

/// What [`resolve_checked`] produced.
pub struct Resolved {
    pub text: Zeroizing<String>,
    pub domains: Vec<Domain>,
    /// Each one-time code that went into `text`, with its item, for the masker.
    pub codes: Vec<(ItemName, OneTimeCode)>,
}

/// The template's placeholders and the items they name (first use first, once each).
pub fn references(template: &str) -> Result<(Vec<Placeholder>, Vec<ItemName>)> {
    let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
    let mut items: Vec<ItemName> = Vec::new();
    for r in &refs {
        if !items.contains(r.item()) {
            items.push(r.item().clone());
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
    pub refs: &'a [Placeholder],
    pub items: &'a [ItemName],
    /// The person lets agents type one-time codes (`agent_otp`).
    pub agent_otp: bool,
    /// Now, by the guard's clock: the moment a code is made for.
    pub unix_seconds: u64,
}

impl Request<'_> {
    /// The fields the template names in `item`'s `{{secret:…}}` placeholders.
    fn fields(&self, item: &ItemName) -> Vec<&str> {
        self.refs
            .iter()
            .filter_map(|r| match r {
                Placeholder::Secret(r) if &r.item == item => Some(r.field.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Whether the template asks for `item`'s one-time code.
    fn code(&self, item: &ItemName) -> CodeUse {
        let asked = self
            .refs
            .iter()
            .any(|r| matches!(r, Placeholder::Otp(i) if i == item));
        if asked {
            CodeUse::Asked {
                agent_otp: self.agent_otp,
            }
        } else {
            CodeUse::NotAsked
        }
    }
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
        let fields = request.fields(item);
        let ctx = UseContext {
            agent: request.agent,
            item,
            info: &info,
            rules,
            fields: &fields,
            code: request.code(item),
            to: Target::Browser(to),
        };
        checks::run(checks::BROWSER, &ctx).map_err(|reason| deny(item, reason))?;
        domains.push(info.domain);
    }
    // Every check passed: this is where the guard grants plaintext.
    let access = PlaintextAccess::issue();
    let mut codes: Vec<(ItemName, OneTimeCode)> = Vec::new();
    let text = fill_placeholders(request.template, |found| match found {
        Placeholder::Secret(r) => vault.resolve(&access, r),
        Placeholder::Otp(item) => {
            let code = vault.one_time_code(&access, item, request.unix_seconds)?;
            let typed = code.code.clone();
            codes.push((item.clone(), code));
            Ok(typed)
        }
    })
    .map_err(|_| deny(first, DenialReason::Vault))?;
    Ok(Resolved {
        text,
        domains,
        codes,
    })
}
