//! A discarded item is never used (J-41); a draft may be (registration flows).

use super::{Check, UseContext};
use keyharbor_core::ItemStatus;
use keyharbor_domain::denial::DenialReason;

pub struct NotDiscarded;

impl Check for NotDiscarded {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        if ctx.info.status == ItemStatus::Discarded {
            return Err(DenialReason::Discarded);
        }
        Ok(())
    }
}
