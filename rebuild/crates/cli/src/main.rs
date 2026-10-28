use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use keyharbor_domain::SessionName;
use keyharbor_sqlite::SqliteStore;
use std::path::PathBuf;

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
    Create { name: String },
    List,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let root = match args.data_dir {
        Some(path) => path,
        None => PathBuf::from(std::env::var_os("HOME").context("HOME is not set; use --data-dir")?)
            .join(".keyharbor-rebuild"),
    };
    let store = SqliteStore::open(&root)?;
    match args.command {
        Command::Create { name } => {
            let session = store.create(SessionName::parse(&name)?)?;
            println!(
                "{}\t{}\t{}",
                session.id,
                session.name.as_str(),
                session.state.as_str()
            );
        }
        Command::List => {
            for session in store.list()? {
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
