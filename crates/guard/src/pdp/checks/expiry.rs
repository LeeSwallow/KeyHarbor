//! An item past its expiry date is kept but not used (R-37).

use super::{Check, UseContext};
use keyharbor_domain::denial::DenialReason;

pub struct NotExpired;

impl Check for NotExpired {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        if ctx.info.expired {
            Err(DenialReason::Expired)
        } else {
            Ok(())
        }
    }
}
