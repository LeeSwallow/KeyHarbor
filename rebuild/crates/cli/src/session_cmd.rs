//! Session commands: `create`, `list`, `open`, `stop`, `wait-auth`, `handoff`, `resume`,
//! `recover`, `endpoint`, and the hidden `__supervise` used by `open`.

use crate::compose::App;
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use keyharbor_core::{
    LOCAL_AGENT,
    session::{self as lifecycle, DisplayState, OpenOptions, SessionView},
};
use keyharbor_domain::{
    SessionId, SessionName, State,
    ports::{Clock, SessionRuntime, Store},
    vault::Description,
};
use keyharbor_supervisor::{DEFAULT_BROWSER, SUPERVISE_COMMAND, SuperviseArgs};
use std::{ffi::OsString, path::PathBuf};

#[derive(Subcommand)]
pub enum SessionCmd {
    /// Create a stopped session
    Create {
        name: String,
        /// What the session is for, shown to agents
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
    },
    /// Change a session's description
    Edit {
        id: String,
        #[arg(
            long,
            value_name = "TEXT",
            conflicts_with = "no_description",
            required_unless_present = "no_description"
        )]
        description: Option<String>,
        #[arg(long)]
        no_description: bool,
    },
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
    /// Run a session's supervisor (started by `open`; arguments are `SuperviseArgs`)
    #[command(name = SUPERVISE_COMMAND, hide = true)]
    Supervise {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
}

fn parse_description(text: &str) -> Result<Description> {
    Description::parse(text).map_err(|e| anyhow::anyhow!("{e}"))
}

fn session_id(value: &str) -> Result<SessionId> {
    SessionId::parse(value).context("invalid session id")
}

/// `id  name  state[  cdp][  description]`.
fn view_line(view: &SessionView) -> String {
    let row = &view.row;
    let cdp = if row.cdp && view.state != DisplayState::Stored(State::Stopped) {
        "\tcdp"
    } else {
        ""
    };
    let about = row
        .description
        .as_ref()
        .map(|d| format!("\t{}", d.as_str().replace(['\t', '\n'], " ")))
        .unwrap_or_default();
    format!(
        "{}\t{}\t{}{cdp}{about}",
        row.id,
        row.name.as_str(),
        view.state
    )
}

/// What a session command runs on.
struct Ports<'a> {
    store: Box<dyn Store>,
    runtime: Box<dyn SessionRuntime>,
    clock: &'a dyn Clock,
}

pub fn run(app: &App, command: SessionCmd) -> Result<()> {
    if let SessionCmd::Supervise { args } = command {
        let args = SuperviseArgs::parse(&args)?;
        return keyharbor_supervisor::run(app.root(), args, app.supervisor_adapters());
    }
    let ports = Ports {
        store: app.store()?,
        runtime: app.runtime()?,
        clock: app.clock(),
    };
    ports.run(command)
}

impl Ports<'_> {
    fn run(&self, command: SessionCmd) -> Result<()> {
        match command {
            SessionCmd::Create { name, description } => self.create(&name, description)?,
            SessionCmd::List => {
                for view in lifecycle::list(&*self.store, &*self.runtime)? {
                    println!("{}", view_line(&view));
                }
            }
            SessionCmd::Edit {
                id,
                description,
                no_description,
            } => self.edit(&id, description, no_description)?,
            SessionCmd::Open {
                id,
                url,
                cdp,
                browser,
            } => self.open(&id, url.as_deref(), cdp, browser)?,
            SessionCmd::Stop { id } => {
                let id = session_id(&id)?;
                let state =
                    lifecycle::stop(&*self.store, &*self.runtime, self.clock, id, LOCAL_AGENT)?;
                println!("{id}\t{}", state.as_str());
            }
            SessionCmd::WaitAuth { id, origin } => {
                let id = session_id(&id)?;
                let request = lifecycle::wait_auth(
                    &*self.store,
                    &*self.runtime,
                    id,
                    origin.as_deref(),
                    LOCAL_AGENT,
                )?;
                println!("{id}\t{}\t{request}", State::AwaitingUser.as_str());
            }
            SessionCmd::Handoff { id } => {
                let id = session_id(&id)?;
                let (request, origin) =
                    lifecycle::pending_handoff(&*self.store, &*self.runtime, id)?;
                println!(
                    "{id}\t{}\t{request}\t{}",
                    State::AwaitingUser.as_str(),
                    origin.as_deref().unwrap_or("origin not provided")
                );
            }
            SessionCmd::Resume { id, request_id } => {
                let id = session_id(&id)?;
                lifecycle::resume(&*self.store, &*self.runtime, id, &request_id, LOCAL_AGENT)?;
                println!("{id}\t{}", State::Running.as_str());
            }
            SessionCmd::Recover {
                id,
                confirm_browser_closed,
            } => self.recover(&id, confirm_browser_closed)?,
            SessionCmd::Endpoint { id } => {
                let id = session_id(&id)?;
                println!("{}", lifecycle::endpoint(&*self.store, &*self.runtime, id)?);
            }
            SessionCmd::Supervise { .. } => unreachable!("handled before the ports are built"),
        }
        Ok(())
    }

    fn create(&self, name: &str, description: Option<String>) -> Result<()> {
        let description = description.as_deref().map(parse_description).transpose()?;
        let session = lifecycle::create(
            &*self.store,
            SessionName::parse(name)?,
            description.as_ref(),
        )?;
        println!(
            "{}\t{}\t{}",
            session.id,
            session.name.as_str(),
            session.state.as_str()
        );
        Ok(())
    }

    fn edit(&self, id: &str, description: Option<String>, no_description: bool) -> Result<()> {
        let id = session_id(id)?;
        let description = match (description, no_description) {
            (_, true) | (None, _) => None,
            (Some(text), false) => Some(parse_description(&text)?),
        };
        lifecycle::describe(&*self.store, id, description.as_ref())?;
        println!("updated\t{id}");
        Ok(())
    }

    fn open(&self, id: &str, url: Option<&str>, cdp: bool, browser: Option<PathBuf>) -> Result<()> {
        let id = session_id(id)?;
        let browser = browser.unwrap_or_else(|| PathBuf::from(DEFAULT_BROWSER));
        let options = OpenOptions {
            url,
            cdp,
            browser: &browser,
            agent: LOCAL_AGENT,
        };
        lifecycle::open(&*self.store, &*self.runtime, self.clock, id, &options)?;
        if cdp {
            eprintln!(
                "warning: raw CDP gives any local client full control of this browser, bypassing KeyHarbor"
            );
        }
        println!("{id}\t{}", State::Running.as_str());
        Ok(())
    }

    fn recover(&self, id: &str, confirm_browser_closed: bool) -> Result<()> {
        if !confirm_browser_closed {
            bail!(
                "close every browser window of this session, then confirm with --confirm-browser-closed"
            );
        }
        let id = session_id(id)?;
        lifecycle::recover(&*self.store, &*self.runtime, id)?;
        println!("{id}\t{}", State::Stopped.as_str());
        Ok(())
    }
}
