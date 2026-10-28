//! The security layer: every decision about and every use of a plaintext vault value.
//!
//! - [`pdp`] — the policy decision point: which rule wins for an agent and an action.
//! - [`pep`] — the policy enforcement points: [`pep::substitute`] resolves placeholders only
//!   after policy, item status and [`pep::origin`] checks; [`pep::mask`] turns every vault
//!   value in outgoing text back into its placeholder.
//! - [`audit`] — what the guard records (never a value).
//! - [`run`] — the checks, substitution and masker of `keyharbor run`.
//! - [`describe`] — descriptions must not hold vault values.
//! - [`GuardedBrowser`] — a `BrowserPort` decorator: input goes through `substitute`, every
//!   result and error through `mask`. The supervisor hands control requests only to it.
//!
//! This crate is the only one that issues [`keyharbor_core::PlaintextAccess`] (besides
//! core's own export), which `Vault::resolve` and `Vault::mask_values` demand.

pub mod audit;
mod browser;
pub mod describe;
mod guard;
pub mod pdp;
pub mod pep;
pub mod run;
#[cfg(test)]
mod test_support;

pub use browser::GuardedBrowser;
pub use guard::{Guard, SharedStore, VaultOpener};
