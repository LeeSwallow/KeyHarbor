//! Settling drafts (08 §8.3): confirm, discard, prune. The rules are core's (`Vault::confirm`
//! and friends); this module parses and prints names only.

use super::input::item_name;
use crate::compose::App;
use anyhow::Result;
use clap::Subcommand;
use keyharbor_domain::policy::LOCAL_AGENT;

#[derive(Subcommand)]
pub enum DraftCmd {
    /// Make a draft active; a numbered draft (NAME~draft-N) replaces NAME
    Confirm { name: String },
    /// Mark a draft discarded so it can no longer be used
    Discard { name: String },
    /// Remove discarded items and drafts older than the draft_ttl setting
    PruneDrafts,
}

pub fn run(app: &App, command: DraftCmd) -> Result<()> {
    let vault = app.vault()?;
    match command {
        DraftCmd::Confirm { name } => {
            let confirmed = vault.confirm(&item_name(&name)?, LOCAL_AGENT)?;
            println!("{}\tactive", confirmed.as_str());
        }
        DraftCmd::Discard { name } => {
            let name = item_name(&name)?;
            vault.discard(&name)?;
            println!("{}\tdiscarded", name.as_str());
        }
        DraftCmd::PruneDrafts => {
            for name in vault.prune_drafts()? {
                println!("{}\tremoved", name.as_str());
            }
        }
    }
    Ok(())
}
