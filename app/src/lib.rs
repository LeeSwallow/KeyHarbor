//! The KeyHarbor admin app (09, ADR-0006): the command layer the window calls, over the same
//! core and guard as the CLI. `main.rs` puts it behind Tauri.

pub mod commands;
pub mod confirm;
mod context;

pub use context::{Ctx, KeyStores, SystemClock};
