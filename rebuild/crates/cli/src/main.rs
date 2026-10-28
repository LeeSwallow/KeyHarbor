use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use keyharbor_domain::SessionName;
use keyharbor_sqlite::SqliteStore;
use std::path::{Path, PathBuf};

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
    Create {
        name: String,
    },
    List,
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
        Command::Vault(command) => return vault_cmd::run(&root, command),
        Command::Create { name } => {
            let store = SqliteStore::open(&root)?;
            let session = store.create(SessionName::parse(&name)?)?;
            println!(
                "{}\t{}\t{}",
                session.id,
                session.name.as_str(),
                session.state.as_str()
            );
        }
        Command::List => {
            for session in SqliteStore::open(&root)?.list()? {
                println!(
                    "{}\t{}\t{}",
                    session.id,
                    session.name.as_str(),
                    session.state.as_str()
                );
            }
        }
    }
    Ok(())
}
