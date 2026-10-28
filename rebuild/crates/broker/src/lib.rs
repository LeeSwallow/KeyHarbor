//! The browser broker (adapter): the per-session supervisor that owns Chrome and its CDP pipe
//! (ADR-0004), the control socket, and the `SessionRuntime` the session use cases drive.
//!
//! The raw CDP browser (`browser`, `cdp`) is private to this crate: control requests reach it
//! only through `keyharbor_guard::GuardedBrowser` (`ops`).

mod browser;
mod cdp;
pub mod control;
pub(crate) mod ops;
pub mod paths;
mod runtime;
pub mod supervisor;

pub use runtime::{DEFAULT_BROWSER, ProcessRuntime};

/// Talking to a running supervisor.
pub mod client {
    pub use crate::control::call;
}
