//! The session supervisor (#11): one process per open session that owns Chrome and its CDP
//! pipe (ADR-0004), wraps the raw browser in the guard and serves the control socket.
//!
//! - [`run`] — the supervisor process itself, composed from the broker's `CdpBrowser`, the
//!   guard's `GuardedBrowser` and the session state rules in `core::session::supervision`.
//! - [`control`] — the control protocol ([`control::wire`]) and its client.
//! - [`ProcessRuntime`] — the `SessionRuntime` that starts supervisors as `__supervise`
//!   processes ([`SuperviseArgs`]) and talks to them.
//! - [`lock_mcp_scope`] — the lock a session-scoped MCP server holds.
//!
//! Control requests reach the browser only through `ops`, which hands each one to
//! `keyharbor_guard::GuardedBrowser`.

mod argv;
pub mod control;
mod lock;
pub(crate) mod ops;
mod paths;
mod process;
mod runtime;

pub use argv::{SUPERVISE_COMMAND, SuperviseArgs};
pub use keyharbor_broker::DEFAULT_BROWSER;
pub use lock::lock_mcp_scope;
pub use process::{Adapters, run};
pub use runtime::ProcessRuntime;
