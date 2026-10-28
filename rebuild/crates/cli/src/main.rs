use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

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
    /// Manage the local encrypted vault
    #[command(subcommand)]
    Vault(vault_cmd::VaultCmd),
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
        Command::Session(command) => session_cmd::run(&root, command),
    }
}
