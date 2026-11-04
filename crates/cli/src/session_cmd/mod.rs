//! Session commands: `create`, `edit`, `list` (`ls`), `rm`, `open`, `stop`, `wait-auth`,
//! `receive`, `handoff`, `resume`, `recover`, `endpoint`, `unlock`, and the hidden
//! `__supervise` used by `open` ([`supervise`]). A session's sites are in [`site`] (sign-in
//! state) and [`grant`] (which sites it may load).

pub mod grant;
pub mod site;

use crate::{
    args::{parse_browser, parse_description},
    compose::App,
    tty_confirm::TtyConfirm,
};
use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use keyharbor_core::{
    LOCAL_AGENT,
    grant::wait_handed_off,
    session::{
        self as lifecycle, DisplayState, OpenOptions, ResumeWait, SessionView,
        remove::{self, Force},
    },
};
use keyharbor_domain::{
    SessionId, SessionName, State,
    browser_choice::{BrowserChoice, BrowserKind},
    ports::{Clock, SessionRuntime, Store},
    vault::Origin,
};
use keyharbor_supervisor::SuperviseArgs;
use std::ffi::OsString;

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
    #[command(visible_alias = "ls")]
    List,
    /// Remove stopped sessions and their browser profiles
    Rm {
        /// Session ids or names
        #[arg(required = true)]
        sessions: Vec<String>,
        /// Close a session that is still open (or return an abandoned one to stopped) first
        #[arg(long, short)]
        force: bool,
    },
    /// Open the session's browser under a supervisor
    Open {
        id: String,
        /// HTTPS (or local HTTP) URL without credentials, query or fragment
        #[arg(long)]
        url: Option<String>,
        /// Also expose a raw CDP endpoint; this bypasses KeyHarbor's protections
        #[arg(long)]
        cdp: bool,
        /// auto, chrome, ego-lite or an absolute path to a Chrome-like executable (default:
        /// the `browser` setting)
        #[arg(long, value_parser = parse_browser)]
        browser: Option<BrowserChoice>,
        /// Run Chrome without a window
        #[arg(long)]
        headless: bool,
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
    /// Wait until the person hands a site over to this session from their browser (the
    /// extension), then print its origin
    Receive {
        id: String,
        /// Only this site, e.g. https://github.com
        #[arg(long)]
        origin: Option<String>,
        /// How long to wait, 1–300 seconds (default 120)
        #[arg(long)]
        timeout_seconds: Option<u64>,
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
    /// Let a session that stopped substituting (a vault value in requests, or the focus moved
    /// while typing) substitute again; asks for confirmation
    Unlock { id: String },
}

/// `__supervise`: arguments are `SuperviseArgs`. Not a [`SessionCmd`]: it runs on the
/// supervisor's adapters, not on [`Ports`], so `main` sends it here first.
#[derive(Args)]
pub struct SuperviseCmd {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

fn session_id(value: &str) -> Result<SessionId> {
    SessionId::parse(value).context("invalid session id")
}

/// `id  name  state[  cdp][  ego-lite][  description]`.
fn view_line(view: &SessionView) -> String {
    let row = &view.row;
    let live = view.state != DisplayState::Stored(State::Stopped);
    let cdp = if row.cdp && live { "\tcdp" } else { "" };
    // Chrome is the usual case; only a browser others can drive is named (03 §3.13).
    let ego = if live && row.browser == Some(BrowserKind::EgoLite) {
        "\tego-lite"
    } else {
        ""
    };
    let about = row
        .description
        .as_ref()
        .map(|d| format!("\t{}", d.as_str().replace(['\t', '\n'], " ")))
        .unwrap_or_default();
    format!(
        "{}\t{}\t{}{cdp}{ego}{about}",
        row.id,
        row.name.as_str(),
        view.state
    )
}

/// Runs a session's supervisor (started by `open`).
pub fn supervise(app: &App, command: SuperviseCmd) -> Result<()> {
    let args = SuperviseArgs::parse(&command.args)?;
    keyharbor_supervisor::run(app.root(), args, app.supervisor_adapters())
}

/// What a session command runs on.
struct Ports<'a> {
    store: Box<dyn Store>,
    runtime: Box<dyn SessionRuntime>,
    clock: &'a dyn Clock,
}

pub fn run(app: &App, command: SessionCmd) -> Result<()> {
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
            SessionCmd::Create { name, description } => self.create(&name, description),
            SessionCmd::List => self.list(),
            SessionCmd::Rm { sessions, force } => self.remove(&sessions, force),
            SessionCmd::Edit {
                id,
                description,
                no_description,
            } => self.edit(&id, description, no_description),
            SessionCmd::Open {
                id,
                url,
                cdp,
                browser,
                headless,
            } => self.open(&id, url.as_deref(), cdp, browser.as_ref(), headless),
            SessionCmd::Stop { id } => self.stop(&id),
            SessionCmd::WaitAuth { id, origin } => self.wait_auth(&id, origin.as_deref()),
            SessionCmd::Receive {
                id,
                origin,
                timeout_seconds,
            } => self.receive(&id, origin.as_deref(), timeout_seconds),
            SessionCmd::Handoff { id } => self.handoff(&id),
            SessionCmd::Resume { id, request_id } => self.resume(&id, &request_id),
            SessionCmd::Recover {
                id,
                confirm_browser_closed,
            } => self.recover(&id, confirm_browser_closed),
            SessionCmd::Endpoint { id } => self.endpoint(&id),
            SessionCmd::Unlock { id } => self.unlock(&id),
        }
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

    fn list(&self) -> Result<()> {
        for view in lifecycle::list(&*self.store, &*self.runtime)? {
            println!("{}", view_line(&view));
        }
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

    fn open(
        &self,
        id: &str,
        url: Option<&str>,
        cdp: bool,
        browser: Option<&BrowserChoice>,
        headless: bool,
    ) -> Result<()> {
        let id = session_id(id)?;
        let browser = lifecycle::resolve_browser(&*self.store, &*self.runtime, browser)?;
        let options = OpenOptions {
            url,
            cdp,
            browser: &browser,
            headless,
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

    fn stop(&self, id: &str) -> Result<()> {
        let id = session_id(id)?;
        let state = lifecycle::stop(&*self.store, &*self.runtime, self.clock, id, LOCAL_AGENT)?;
        println!("{id}\t{}", state.as_str());
        Ok(())
    }

    fn wait_auth(&self, id: &str, origin: Option<&str>) -> Result<()> {
        let id = session_id(id)?;
        let request = lifecycle::wait_auth(&*self.store, &*self.runtime, id, origin, LOCAL_AGENT)?;
        println!("{id}\t{}\t{request}", State::AwaitingUser.as_str());
        Ok(())
    }

    /// Waits until the person hands a site over from the extension. The waiting is meant to
    /// move to core (a later change); this only splits it out of `run`.
    fn receive(&self, id: &str, origin: Option<&str>, timeout_seconds: Option<u64>) -> Result<()> {
        let id = session_id(id)?;
        let origin = origin
            .map(|o| Origin::parse(o).map_err(|e| anyhow::anyhow!("{e}")))
            .transpose()?;
        let wait = ResumeWait::seconds(timeout_seconds)?;
        let store = || Ok(&self.store);
        let handed = wait_handed_off(
            store,
            self.clock,
            id,
            origin.as_ref(),
            wait,
            std::thread::sleep,
        )?;
        let Some(grant) = handed else {
            bail!("no site was handed over before the wait ran out");
        };
        println!("{id}\treceived\t{}", grant.origin.as_str());
        Ok(())
    }

    fn handoff(&self, id: &str) -> Result<()> {
        let id = session_id(id)?;
        let (request, origin) = lifecycle::pending_handoff(&*self.store, &*self.runtime, id)?;
        println!(
            "{id}\t{}\t{request}\t{}",
            State::AwaitingUser.as_str(),
            origin.as_deref().unwrap_or("origin not provided")
        );
        Ok(())
    }

    fn resume(&self, id: &str, request_id: &str) -> Result<()> {
        let id = session_id(id)?;
        lifecycle::resume(&*self.store, &*self.runtime, id, request_id, LOCAL_AGENT)?;
        println!("{id}\t{}", State::Running.as_str());
        Ok(())
    }

    fn remove(&self, keys: &[String], force: bool) -> Result<()> {
        let force = if force { Force::CloseFirst } else { Force::No };
        for key in keys {
            let id = remove::resolve(&*self.store, key)?;
            remove::remove(
                &*self.store,
                &*self.runtime,
                self.clock,
                id,
                force,
                LOCAL_AGENT,
            )?;
            println!("{id}\tremoved");
        }
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

    /// A weakening: the person at the terminal confirms (#190, J-220). No MCP tool does this.
    fn unlock(&self, id: &str) -> Result<()> {
        let id = session_id(id)?;
        let reason = lifecycle::unlock(&*self.runtime, &TtyConfirm, id)?;
        println!("{id}\tunlocked\t{}", reason.as_str());
        Ok(())
    }

    fn endpoint(&self, id: &str) -> Result<()> {
        let id = session_id(id)?;
        println!("{}", lifecycle::endpoint(&*self.store, &*self.runtime, id)?);
        Ok(())
    }
}
