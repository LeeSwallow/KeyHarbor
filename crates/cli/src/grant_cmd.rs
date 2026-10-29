//! `keyharbor grant`: which sites a session may load (R-43). The rules are core's
//! (`keyharbor_core::grant`); adding one asks the person at the terminal.

use crate::{compose::App, tty_confirm::TtyConfirm};
use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use keyharbor_core::grant::{self, NewGrant};
use keyharbor_domain::{
    SessionId,
    grant::{Grant, GrantId, GrantTtl},
    policy::LOCAL_AGENT,
    vault::{Description, Origin},
};

#[derive(Subcommand)]
pub enum GrantCmd {
    /// List a session's grants, active and ended
    List { session: String },
    /// Let a session load a site (and its subdomains). Once a session has a grant, it may
    /// load only granted sites; asks for confirmation
    Add {
        session: String,
        origin: String,
        /// How long: <N>m, <N>h or <N>d (at most 30d); default until revoked
        #[arg(long)]
        ttl: Option<String>,
        /// Why, for the record
        #[arg(long)]
        reason: Option<String>,
    },
    /// End a grant now; the session's cookies for that site are removed
    Revoke { id: String },
}

/// `id  origin  active|ended  until|-  created_by`.
fn line(grant: &Grant, now: u64) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}",
        grant.id,
        grant.origin.as_str(),
        if grant.is_active(now) {
            "active"
        } else {
            "ended"
        },
        grant.until.map_or("-".to_owned(), |u| u.to_string()),
        grant.created_by
    )
}

fn invalid(error: keyharbor_domain::vault::InvalidValue) -> anyhow::Error {
    anyhow!("{error}")
}

fn session_id(value: &str) -> Result<SessionId> {
    SessionId::parse(value).context("invalid session id")
}

pub fn run(app: &App, command: GrantCmd) -> Result<()> {
    let store = app.store()?;
    let now = app.clock().unix_seconds();
    match command {
        GrantCmd::List { session } => {
            for grant in grant::list(&*store, session_id(&session)?)? {
                println!("{}", line(&grant, now));
            }
        }
        GrantCmd::Add {
            session,
            origin,
            ttl,
            reason,
        } => {
            let new = NewGrant {
                session: session_id(&session)?,
                origin: Origin::parse(&origin).map_err(invalid)?,
                ttl: ttl
                    .as_deref()
                    .map(GrantTtl::parse)
                    .transpose()
                    .map_err(invalid)?,
                reason: reason
                    .as_deref()
                    .map(Description::parse)
                    .transpose()
                    .map_err(invalid)?,
            };
            let added = grant::add(&*store, app.clock(), &TtyConfirm, LOCAL_AGENT, new)?;
            println!("{}", line(&added, now));
        }
        GrantCmd::Revoke { id } => {
            let id = GrantId::parse(&id).context("invalid grant id")?;
            let revoked = grant::revoke(&*store, app.clock(), LOCAL_AGENT, id)?;
            println!("{}\trevoked", revoked.id);
        }
    }
    Ok(())
}
