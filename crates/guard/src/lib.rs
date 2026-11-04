//! The security layer: every decision about and every use of a plaintext vault value.
//!
//! - [`pdp`] — the policy decision point: which rule wins for an agent and an action.
//! - [`pep`] — the policy enforcement points: [`pep::substitute`] resolves placeholders only
//!   after policy, item status and [`pep::origin`] checks; [`pep::mask`] turns every vault
//!   value in outgoing text back into its placeholder.
//! - [`audit`] — what the guard records (never a value).
//! - [`run`] — the checks, substitution and masker of `keyharbor run`.
//! - [`describe`] — descriptions must not hold vault values.
//! - [`items`] — adding and changing items in the safe order (values, then weakening).
//! - [`session`] — which policy actions each session operation needs.
//! - [`Guard`] — one supervisor's enforcement state: refusal cap, vault and masker, probe
//!   lock, masking on the way out.
//! - [`GuardedBrowser`] — a `BrowserPort` decorator: input goes through `substitute`, every
//!   result and error through `mask`. The supervisor hands control requests only to it.
//! - [`capture`] — storing a sign-in the user typed during a handoff as a draft.
//! - [`reveal`] — a person looking at a value in the admin app (recorded).
//! - [`autofill`] — filling a login into the person's own browser after Touch ID (recorded).
//!
//! This crate is the only one that issues [`keyharbor_core::PlaintextAccess`] (besides
//! core's own export), which `Vault::resolve` and `Vault::mask_values` demand.

pub mod audit;
mod browser;
pub mod describe;
mod guard;
pub mod items;
pub mod pdp;
pub mod pep;
mod person;
pub mod run;
pub mod session;
#[cfg(test)]
mod test_support;

pub use browser::{GuardedBrowser, LoginOutcome};
pub use guard::{Guard, SharedStore, VaultOpener, lock_store};
pub use person::{autofill, capture, reveal};
