//! Where an item's values may go (`pep::origin`): the frame's origin in a browser, never a
//! process for origin-bound items.

use super::{Check, Target, UseContext};
use crate::pep::origin;
use keyharbor_domain::denial::DenialReason;

pub struct BrowserOrigin;

impl Check for BrowserOrigin {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        let Target::Browser(to) = ctx.to else {
            return Ok(());
        };
        let info = ctx.info;
        origin::allows_browser(info.domain, info.origin_binding, &info.origins, to.frame)
    }
}

pub struct ProcessOrigin;

impl Check for ProcessOrigin {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        if origin::allows_process(ctx.info.origin_binding) {
            Ok(())
        } else {
            Err(DenialReason::Origin)
        }
    }
}
