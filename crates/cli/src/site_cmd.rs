//! `keyharbor site`: a session's recorded sign-in state per site. The rules are core's
//! (`keyharbor_core::site`); this module parses and prints.

use crate::compose::App;
use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use keyharbor_core::site::{self, SiteChange};
use keyharbor_domain::{
    SessionId,
    policy::LOCAL_AGENT,
    site::{SiteRecord, SiteState},
    vault::{ItemName, Origin},
};

#[derive(Subcommand)]
pub enum SiteCmd {
    /// List a session's recorded sign-in states
    List { session: String },
    /// Record a session's sign-in state at a site: logged_in, expired or unknown
    Set {
        session: String,
        origin: String,
        state: String,
        /// Vault item that signed in
        #[arg(long)]
        item: Option<String>,
    },
}

/// `origin  state  item|-  checked_at  recorded_by`.
fn line(record: &SiteRecord) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}",
        record.origin.as_str(),
        record.state.as_str(),
        record.item.as_ref().map_or("-", |item| item.as_str()),
        record.checked_at,
        record.recorded_by
    )
}

fn invalid(error: keyharbor_domain::vault::InvalidValue) -> anyhow::Error {
    anyhow!("{error}")
}

pub fn run(app: &App, command: SiteCmd) -> Result<()> {
    let store = app.store()?;
    match command {
        SiteCmd::List { session } => {
            let id = SessionId::parse(&session).context("invalid session id")?;
            for record in site::list(&*store, id)? {
                println!("{}", line(&record));
            }
        }
        SiteCmd::Set {
            session,
            origin,
            state,
            item,
        } => {
            let change = SiteChange {
                session: SessionId::parse(&session).context("invalid session id")?,
                origin: Origin::parse(&origin).map_err(invalid)?,
                state: SiteState::parse(&state).map_err(invalid)?,
                item: item
                    .as_deref()
                    .map(ItemName::parse)
                    .transpose()
                    .map_err(invalid)?,
            };
            let (record, confirmed) =
                site::set_and_confirm(&*store, || app.vault(), app.clock(), LOCAL_AGENT, change)?;
            println!("{}", line(&record));
            if let Some(item) = confirmed {
                println!("confirmed\t{}", item.as_str());
            }
        }
    }
    Ok(())
}
