//! `keyharbor browser`: the side of site handoff that runs in the user's own browser (R-42).
//! `host` is the Native Messaging host the extension talks to ([`host`]); `install-host`
//! registers it with the browsers ([`install`]).

mod host;
mod install;

use crate::compose::App;
use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use std::ffi::OsString;

#[derive(Subcommand)]
pub enum BrowserCmd {
    /// The Native Messaging host (started by the browser, not by hand)
    Host {
        /// What the browser passes (the calling extension); not used
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        caller: Vec<OsString>,
    },
    /// Register the host with your browsers so the extension can hand sites over
    InstallHost {
        #[arg(long, value_enum, default_value = "all")]
        browser: HostBrowser,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HostBrowser {
    Chrome,
    Chromium,
    Firefox,
    All,
}

pub fn run(app: &App, command: BrowserCmd) -> Result<()> {
    match command {
        BrowserCmd::Host { caller: _ } => host::serve(app),
        BrowserCmd::InstallHost { browser } => install::install(app, browser),
    }
}
