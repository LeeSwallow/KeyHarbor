//! `keyharbor run` injects only items that allow it (R-35; on by default for developer
//! items, R-36).

use super::{Check, UseContext};
use keyharbor_domain::denial::DenialReason;

pub struct RunAllowed;

impl Check for RunAllowed {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        if ctx.info.run_allowed {
            Ok(())
        } else {
            Err(DenialReason::RunNotAllowed)
        }
    }
}
