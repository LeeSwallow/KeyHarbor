//! The checks an item use must pass, as a chain of small objects (Chain of Responsibility).
//! Each check looks at one thing and either passes or names a [`DenialReason`]; the first
//! refusal stops the chain. A new check is one file here plus one line in a chain. Eight
//! checks and this file pass the folder line by one: each check is the one-file extension
//! point writing-code.md names, and none groups with another.
//!
//! Existence and metadata verification come first and are not checks: they produce the
//! [`ItemUse`] every check reads.

mod expiry;
mod fields;
mod focus;
mod origin;
pub mod otp;
mod policy;
mod run;
mod status;

use crate::pep::substitute::Destination;
use keyharbor_core::ItemUse;
use keyharbor_domain::{denial::DenialReason, policy::Rule, vault::ItemName};

/// Where the values go.
#[derive(Clone, Copy)]
pub enum Target<'a> {
    /// Typed into a page.
    Browser(Destination<'a>),
    /// Injected into a process by `keyharbor run`.
    Process,
}

/// Whether a use asks for the item's one-time code (`{{otp:item}}`), and if so whether the
/// person lets agents type codes at all (`agent_otp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeUse {
    NotAsked,
    Asked { agent_otp: bool },
}

/// One use of one item, as the checks see it.
pub struct UseContext<'a> {
    pub agent: &'a str,
    pub item: &'a ItemName,
    pub info: &'a ItemUse,
    pub rules: &'a [Rule],
    /// The fields the use names (empty for a whole-item use, `run --env`).
    pub fields: &'a [&'a str],
    /// The one-time code, which names no field: the item's seed field is never typed itself.
    pub code: CodeUse,
    pub to: Target<'a>,
}

pub trait Check: Sync {
    fn check(&self, ctx: &UseContext<'_>) -> Result<(), DenialReason>;
}

/// Typing into a page (`fill`, `type`, `browser_login`).
pub static BROWSER: &[&dyn Check] = &[
    &status::NotDiscarded,
    &expiry::NotExpired,
    &policy::Allowed,
    &otp::OtpAllowed,
    &origin::BrowserOrigin,
    &fields::Exist,
    &fields::SuitTheElement,
    &focus::TakesText,
];

/// Injecting into a process (`keyharbor run`).
pub static PROCESS: &[&dyn Check] = &[
    &status::NotDiscarded,
    &expiry::NotExpired,
    &policy::Allowed,
    &origin::ProcessOrigin,
    &run::RunAllowed,
    &fields::Exist,
];

/// Runs `chain` in order; the first refusal wins.
pub fn run(chain: &[&dyn Check], ctx: &UseContext<'_>) -> Result<(), DenialReason> {
    chain.iter().try_for_each(|check| check.check(ctx))
}
