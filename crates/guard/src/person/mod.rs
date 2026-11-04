//! The plaintext paths that end with a person, not an agent: what they look at in the admin
//! app, what fills their own browser, and what they typed during a handoff. Agents never
//! reach these: neither the MCP server nor the control socket calls them.
//!
//! - [`reveal`] — a person looking at a value in the admin app (recorded).
//! - [`autofill`] — filling a login into the person's own browser after Touch ID (recorded).
//! - [`capture`] — storing a sign-in the person typed during a handoff as a draft.

pub mod autofill;
pub mod capture;
pub mod reveal;

/// Who the usage record names for these paths: the person.
pub(crate) const PERSON: &str = "user";
