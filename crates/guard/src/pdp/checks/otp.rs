//! A one-time code (`{{otp:item}}`, #206) goes into a page only while the person lets agents
//! type codes (`agent_otp`, off by default) and the item does too (`otp_allowed`), and only for
//! an item bound to origins: the code then goes into the item's own sites alone, which
//! `origin::BrowserOrigin` checks next (R-48(3)). The item must hold a seed.

use super::{Check, CodeUse, UseContext};
use keyharbor_core::ItemSummary;
use keyharbor_domain::{denial::DenialReason, vault::ItemKind};

/// What decides whether an item gives codes, read from an `ItemUse` or an `ItemSummary`.
struct CodeFacts<'a> {
    agent_otp: bool,
    otp_allowed: bool,
    origin_binding: bool,
    has_origins: bool,
    kind: ItemKind,
    fields: &'a [String],
}

/// Why the item gives no code now, or `None` when it does (frame origin aside).
fn refusal(facts: CodeFacts<'_>) -> Option<DenialReason> {
    if !facts.agent_otp {
        return Some(DenialReason::OtpOff);
    }
    if !facts.otp_allowed {
        return Some(DenialReason::OtpNotAllowed);
    }
    if !facts.origin_binding || !facts.has_origins {
        return Some(DenialReason::NoOrigin);
    }
    let has_seed = facts
        .kind
        .otp_field()
        .is_some_and(|seed| facts.fields.iter().any(|f| f == seed));
    (!has_seed).then_some(DenialReason::NotFound)
}

/// For listings (MCP `vault_list`): why `item` would give no code now, by the same rule the
/// check applies; `None` when a page of one of its origins would receive one.
pub fn code_refusal(item: &ItemSummary, agent_otp: bool) -> Option<DenialReason> {
    refusal(CodeFacts {
        agent_otp,
        otp_allowed: item.otp_allowed,
        origin_binding: item.origin_binding,
        has_origins: !item.origins.is_empty(),
        kind: item.kind,
        fields: &item.fields,
    })
}

pub struct OtpAllowed;

impl Check for OtpAllowed {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        let CodeUse::Asked { agent_otp } = ctx.code else {
            return Ok(());
        };
        let info = ctx.info;
        let refused = refusal(CodeFacts {
            agent_otp,
            otp_allowed: info.otp_allowed,
            origin_binding: info.origin_binding,
            has_origins: !info.origins.is_empty(),
            kind: info.kind,
            fields: &info.fields,
        });
        refused.map_or(Ok(()), Err)
    }
}
