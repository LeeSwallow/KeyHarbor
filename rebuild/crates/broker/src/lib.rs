//! The browser broker: the per-session supervisor that owns Chrome and its CDP pipe
//! (ADR-0004), the control socket, and the session lifecycle commands built on them.

pub mod control;
pub mod lifecycle;
pub(crate) mod ops;
pub mod paths;
pub mod supervisor;

/// Talking to a running supervisor.
pub mod client {
    pub use crate::control::call;
}
