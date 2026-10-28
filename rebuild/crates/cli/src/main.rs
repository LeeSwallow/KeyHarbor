use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod audit_cmd;
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

fn main() -> Result<()> {
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
