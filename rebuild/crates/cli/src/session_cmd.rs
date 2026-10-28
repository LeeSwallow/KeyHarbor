//! Session commands: `create`, `list`, `open`, `stop`, `wait-auth`, `handoff`, `resume`,
//! `recover`, `endpoint`, and the hidden `__supervise` used by `open`.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use keyharbor_broker::{
    lifecycle::{self, DEFAULT_BROWSER, OpenOptions},
    supervisor,
};
use keyharbor_domain::{Generation, SessionId, SessionName};
use keyharbor_sqlite::SqliteStore;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum SessionCmd {
    /// Create a stopped session
    Create { name: String },
    /// List sessions with their state
    List,
    /// Open the session's browser under a supervisor
    Open {
        id: String,
        /// HTTPS (or local HTTP) URL without credentials, query or fragment
        #[arg(long)]
        url: Option<String>,
        /// Also expose a raw CDP endpoint; this bypasses KeyHarbor's protections
        #[arg(long)]
        cdp: bool,
        #[arg(long)]
        browser: Option<PathBuf>,
    },
    /// Close the browser and stop the session
    Stop { id: String },
    /// Ask the user to finish authentication in the browser
    WaitAuth {
        id: String,
        /// Site origin the user should sign in to, e.g. https://example.com
        #[arg(long)]
        origin: Option<String>,
    },
    /// Show the pending authentication request
    Handoff { id: String },
    /// Resume after authentication, quoting the request ID
    Resume { id: String, request_id: String },
    /// Return an abandoned session to stopped
    Recover {
        id: String,
        /// Confirm that every browser process using this session's profile has exited
        #[arg(long)]
        confirm_browser_closed: bool,
    },
    /// Print the raw CDP endpoint of a session opened with --cdp
    Endpoint { id: String },
    #[command(name = "__supervise", hide = true)]
    Supervise {
        id: String,
        generation: String,
        browser: PathBuf,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        cdp: bool,
    },
}

fn session_id(value: &str) -> Result<SessionId> {
    SessionId::parse(value).context("invalid session id")
}

pub fn run(root: &Path, command: SessionCmd) -> Result<()> {
    match command {
        SessionCmd::Create { name } => {
            let store = SqliteStore::open(root)?;
            let session = store.create(SessionName::parse(&name)?)?;
            println!(
                "{}\t{}\t{}",
                session.id,
                session.name.as_str(),
                session.state.as_str()
            );
        }
        SessionCmd::List => {
            for row in SqliteStore::open(root)?.list()? {
                let state = lifecycle::display_state(root, &row);
                let cdp = if row.cdp && state != "stopped" {
                    "\tcdp"
                } else {
                    ""
                };
                println!("{}\t{}\t{state}{cdp}", row.id, row.name.as_str());
            }
        }
        SessionCmd::Open {
            id,
            url,
            cdp,
            browser,
        } => {
            let id = session_id(&id)?;
            let browser = browser.unwrap_or_else(|| PathBuf::from(DEFAULT_BROWSER));
            let options = OpenOptions {
                url: url.as_deref(),
                cdp,
                browser: &browser,
            };
            lifecycle::open(root, id, &options, &std::env::current_exe()?)?;
            if cdp {
                eprintln!(
                    "warning: raw CDP gives any local client full control of this browser, bypassing KeyHarbor"
                );
            }
            println!("{id}\trunning");
        }
        SessionCmd::Stop { id } => {
            let id = session_id(&id)?;
            let state = lifecycle::stop(root, id)?;
            println!("{id}\t{}", state.as_str());
        }
        SessionCmd::WaitAuth { id, origin } => {
            let id = session_id(&id)?;
            let request = lifecycle::wait_auth(root, id, origin.as_deref())?;
            println!("{id}\tawaiting_user\t{request}");
        }
        SessionCmd::Handoff { id } => {
            let id = session_id(&id)?;
            let (request, origin) = lifecycle::pending_handoff(root, id)?;
            println!(
                "{id}\tawaiting_user\t{request}\t{}",
                origin.as_deref().unwrap_or("origin not provided")
            );
        }
        SessionCmd::Resume { id, request_id } => {
            let id = session_id(&id)?;
            lifecycle::resume(root, id, &request_id)?;
            println!("{id}\trunning");
        }
        SessionCmd::Recover {
            id,
            confirm_browser_closed,
        } => {
            if !confirm_browser_closed {
                bail!(
                    "close every browser window of this session, then confirm with --confirm-browser-closed"
                );
            }
            let id = session_id(&id)?;
            lifecycle::recover(root, id)?;
            println!("{id}\tstopped");
        }
        SessionCmd::Endpoint { id } => {
            let id = session_id(&id)?;
            println!("{}", lifecycle::endpoint(root, id)?);
        }
        SessionCmd::Supervise {
            id,
            generation,
            browser,
            url,
            cdp,
        } => {
            let id = session_id(&id)?;
            let generation = Generation::parse(&generation).context("invalid generation")?;
            supervisor::run(root, id, generation, &browser, url.as_deref(), cdp)?;
        }
    }
    Ok(())
}
