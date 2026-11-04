use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use keyharbor_domain::browser_choice::BrowserChoice;
use keyharbor_supervisor::SUPERVISE_COMMAND;
use std::path::{Path, PathBuf};

mod args;
mod audit_cmd;
mod browser_cmd;
mod compose;
mod policy_cmd;
mod run_cmd;
mod session_cmd;
mod settings_cmd;
mod status_cmd;
mod tty_confirm;
mod vault_cmd;

use compose::App;
use session_cmd::{grant, site};

#[derive(Parser)]
#[command(
    version,
    about = "Local vault and browser sessions for AI agents: they use secrets without seeing them"
)]
struct Cli {
    #[arg(long, global = true, env = "KEYHARBOR_HOME")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    #[command(flatten)]
    Session(session_cmd::SessionCmd),
    /// Run a session's supervisor (started by `open`; arguments are `SuperviseArgs`)
    #[command(name = SUPERVISE_COMMAND, hide = true)]
    Supervise(session_cmd::SuperviseCmd),
    /// Counts at a glance: vault ready, sessions open, requests waiting, drafts (no names)
    Status,
    /// Serve MCP tools over stdio for an AI agent
    Mcp(McpArgs),
    /// Manage the local encrypted vault
    #[command(subcommand)]
    Vault(vault_cmd::VaultCmd),
    /// Limit what each agent may use (default: everything allowed)
    #[command(subcommand)]
    Policy(policy_cmd::PolicyCmd),
    /// Show the usage record (never contains values)
    Audit(audit_cmd::AuditArgs),
    /// Read or change settings
    #[command(subcommand)]
    Settings(settings_cmd::SettingsCmd),
    /// A session's recorded sign-in state per site
    #[command(subcommand)]
    Site(site::SiteCmd),
    /// Which sites a session may load (grants)
    #[command(subcommand)]
    Grant(grant::GrantCmd),
    /// Hand sites over from your own browser (Native Messaging host)
    #[command(subcommand)]
    Browser(browser_cmd::BrowserCmd),
    /// Run a command with env items injected; its output is masked
    Run(run_cmd::RunArgs),
}

#[derive(Args)]
struct McpArgs {
    /// Expose and allow only this session
    #[arg(long)]
    session: Option<String>,
    /// Allow sessions with a raw CDP endpoint (bypasses KeyHarbor's protections)
    #[arg(long)]
    allow_cdp: bool,
    /// Name of the agent using this server, for policies and the usage record
    #[arg(long)]
    agent: Option<String>,
    /// auto, chrome, ego-lite or an absolute path to a Chrome-like executable (default:
    /// the `browser` setting)
    #[arg(long, value_parser = args::parse_browser)]
    browser: Option<BrowserChoice>,
    /// Nobody is watching (a scheduled run): tools that wait for a person look once and
    /// return `needs_user` instead of waiting
    #[arg(long)]
    non_interactive: bool,
    /// Print the tool names and exit (used to check the plugin's skills)
    #[arg(long, hide = true)]
    list_tools: bool,
}

/// The macOS per-user application data folder; `--data-dir` and `KEYHARBOR_HOME` override it.
fn default_data_dir(home: &Path) -> PathBuf {
    home.join("Library/Application Support/KeyHarbor")
}

fn data_dir(given: Option<PathBuf>) -> Result<PathBuf> {
    match given {
        Some(path) => Ok(path),
        None => Ok(default_data_dir(&PathBuf::from(
            std::env::var_os("HOME").context("HOME is not set; use --data-dir")?,
        ))),
    }
}

/// No core dumps of this process (it holds vault values while it runs `run`, resolves
/// placeholders as a session supervisor, or reads secret input; 07 §7.10).
fn disable_core_dumps() -> std::io::Result<()> {
    let none = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: setrlimit only reads the struct it is given.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &none) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn main() -> Result<()> {
    disable_core_dumps().context("could not turn core dumps off")?;
    let cli = Cli::parse();
    let app = App::new(data_dir(cli.data_dir)?);
    dispatch(&app, cli.command)
}

/// Sends a command to its module; each builds only the ports it uses.
fn dispatch(app: &App, command: Command) -> Result<()> {
    match command {
        Command::Session(command) => session_cmd::run(app, command),
        Command::Supervise(command) => session_cmd::supervise(app, command),
        Command::Status => status_cmd::run(app),
        Command::Mcp(args) => serve_mcp(app, args),
        Command::Vault(command) => vault_cmd::run(app, command),
        Command::Policy(command) => policy_cmd::run(&*app.store()?, command),
        Command::Audit(args) => audit_cmd::run(app, args),
        Command::Settings(command) => settings_cmd::run(&*app.store()?, command),
        Command::Site(command) => site::run(app, command),
        Command::Grant(command) => grant::run(app, command),
        Command::Browser(command) => browser_cmd::run(app, command),
        Command::Run(args) => {
            let code = run_cmd::run(app, args)?;
            std::process::exit(code)
        }
    }
}

fn serve_mcp(app: &App, args: McpArgs) -> Result<()> {
    if args.list_tools {
        for name in keyharbor_mcp::tool_names(args.allow_cdp) {
            println!("{name}");
        }
        return Ok(());
    }
    let interaction = if args.non_interactive {
        keyharbor_mcp::Interaction::NonInteractive
    } else {
        keyharbor_mcp::Interaction::Interactive
    };
    let options = keyharbor_mcp::Options {
        data: app.root().to_path_buf(),
        session: args.session,
        allow_cdp: args.allow_cdp,
        agent: args.agent,
        browser: args.browser,
        interaction,
    };
    keyharbor_mcp::run(options, app.mcp_deps()?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn core_dumps_are_turned_off() {
        super::disable_core_dumps().unwrap();
        let mut limit = libc::rlimit {
            rlim_cur: 1,
            rlim_max: 1,
        };
        // SAFETY: getrlimit writes only the struct it is given.
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) }, 0);
        assert_eq!((limit.rlim_cur, limit.rlim_max), (0, 0));
    }
}
