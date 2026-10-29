//! `keyharbor settings`: which settings exist and what a change weakens is core's
//! (`keyharbor_core::settings`); this module parses and prints.

use crate::tty_confirm::TtyConfirm;
use anyhow::Result;
use clap::Subcommand;
use keyharbor_core::settings;
use keyharbor_domain::ports::SettingsStore;

#[derive(Subcommand)]
pub enum SettingsCmd {
    /// Print a setting
    Get { key: String },
    /// Change a setting: audit_enabled true|false, capture_enabled true|false,
    /// default_item_ttl <N>d|none, draft_ttl <N>d|none
    Set { key: String, value: String },
}

pub fn run(store: &dyn SettingsStore, command: SettingsCmd) -> Result<()> {
    match command {
        SettingsCmd::Get { key } => println!("{}", settings::get(store, &key)?),
        SettingsCmd::Set { key, value } => {
            settings::set(store, &TtyConfirm, &key, &value)?;
            println!("{key}\t{value}");
        }
    }
    Ok(())
}
