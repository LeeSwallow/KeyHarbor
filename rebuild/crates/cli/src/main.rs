use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod adapters;
mod audit_cmd;
mod expiry_args;
mod policy_cmd;
mod run_cmd;
mod session_cmd;
mod vault_cmd;

#[derive(Parser)]
#[command(version, about = "Local browser session management")]
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
    /// Serve MCP tools over stdio for an AI agent
    Mcp {
        /// Expose and allow only this session
        #[arg(long)]
        session: Option<String>,
        /// Allow sessions with a raw CDP endpoint (bypasses KeyHarbor's protections)
        #[arg(long)]
        allow_cdp: bool,
        /// Name of the agent using this server (used by policies in a later phase)
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        browser: Option<PathBuf>,
    },
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
    Settings(audit_cmd::SettingsCmd),
    /// Run a command with env items injected; its output is masked
    Run(run_cmd::RunArgs),
}

/// The macOS per-user application data folder; `--data-dir` and `KEYHARBOR_HOME` override it.
fn default_data_dir(home: &Path) -> PathBuf {
    home.join("Library/Application Support/KeyHarbor")
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
    let args = Cli::parse();
    let root = match args.data_dir {
        Some(path) => path,
        None => default_data_dir(&PathBuf::from(
            std::env::var_os("HOME").context("HOME is not set; use --data-dir")?,
        )),
    };
    match args.command {
        Command::Vault(command) => vault_cmd::run(&root, command),
        Command::Policy(command) => policy_cmd::run(&root, command),
        Command::Audit(args) => audit_cmd::audit(&root, args),
        Command::Settings(command) => audit_cmd::settings(&root, command),
        Command::Run(args) => {
            let code = run_cmd::run(&root, args)?;
            std::process::exit(code)
        }
        Command::Session(command) => session_cmd::run(&root, command),
        Command::Mcp {
            session,
            allow_cdp,
            agent,
            browser,
        } => keyharbor_mcp::run(keyharbor_mcp::Options {
            data: root,
            session,
            allow_cdp,
            agent,
            browser,
        }),
    }
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
