//! Text typed while the focus is not on something that takes text (a frame host, a button,
//! the body) could land anywhere (C2).

use super::{Check, Target, UseContext};
use keyharbor_domain::denial::DenialReason;

pub struct TakesText;

impl Check for TakesText {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        match ctx.to {
            Target::Browser(to) if to.element.is_some_and(|e| !e.takes_text()) => {
                Err(DenialReason::Focus)
            }
            _ => Ok(()),
        }
    }
}
