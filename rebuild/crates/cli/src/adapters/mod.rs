//! Adapters the CLI provides for core ports (ADR-0014): the interface owns the terminal.

mod tty_confirm;

pub use tty_confirm::TtyConfirm;
