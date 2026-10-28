//! The named fields must exist, and each must suit the element it goes into (C1: a password
//! only into a password input).

use super::{Check, Target, UseContext};
use keyharbor_domain::{denial::DenialReason, vault::InputKind};

pub struct Exist;

impl Check for Exist {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        let missing = ctx
            .fields
            .iter()
            .any(|field| !ctx.info.fields.iter().any(|f| f == field));
        if missing {
            Err(DenialReason::NotFound)
        } else {
            Ok(())
        }
    }
}

pub struct SuitTheElement;

impl Check for SuitTheElement {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason> {
        let Target::Browser(to) = ctx.to else {
            return Ok(());
        };
        let Some(element) = to.element else {
            return Ok(());
        };
        let password_only = ctx.fields.iter().any(|field| {
            ctx.info
                .kind
                .field(field)
                .is_some_and(|spec| spec.input == InputKind::Password)
        });
        if password_only && !element.is_password() {
            Err(DenialReason::FieldKind)
        } else {
            Ok(())
        }
    }
}
