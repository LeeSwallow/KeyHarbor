//! The control socket: the protocol ([`wire`]), the server the supervisor runs and the
//! client the MCP server and `ProcessRuntime` use.

pub mod client;
pub(crate) mod server;
pub mod wire;
