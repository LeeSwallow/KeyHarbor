//! The agent's policy (the PDP) must allow using the item.

use super::{Check, UseContext};
use crate::pdp::{Action, decide};
use keyharbor_domain::denial::DenialReason;

pub struct Allowed;

impl Check for Allowed {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        let action = Action::UseItem {
            item: ctx.item.clone(),
            domain: ctx.info.domain,
        };
        if decide(ctx.rules, ctx.agent, &action).allowed() {
            Ok(())
        } else {
            Err(DenialReason::Policy)
        }
    }
}
