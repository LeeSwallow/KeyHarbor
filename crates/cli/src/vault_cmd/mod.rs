//! `keyharbor vault …`. Never prints secret values; secrets arrive only by hidden prompt or stdin JSON.
//! The commands are here and each goes to one handler: adding and changing items in [`items`],
//! export and import files in [`files`], key rotation in [`keys`], credential templates in
//! [`templates`]; reading input is in [`input`], printing in [`output`]. Nine files, one past
//! the folder line: each is one command group and none groups with another.

mod drafts;
mod files;
mod input;
mod items;
mod keys;
mod output;
mod requests;
mod templates;

use crate::{args, compose::App};
use anyhow::Result;
use clap::{Subcommand, ValueEnum, builder::TypedValueParser};
use keyharbor_core::{ItemFilter, ItemStatus};
use keyharbor_domain::{
    tag::Tag,
    vault::{Domain, ItemKind, KINDS},
};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum VaultCmd {
    /// Create the vault and store its root key in the keychain
    Init,
    /// Add an item. Secret fields are read from a hidden prompt or --stdin JSON
    Add(items::AddArgs),
    /// Change fields, origins, binding or status of an item
    Edit(items::EditArgs),
    /// Remove an item
    Rm { name: String },
    /// Rename or move an item (its placeholders change); refused while a policy rule names it
    Mv { from: String, to: String },
    /// Swap fields back to their value before the last change (all that have one, or --field)
    Revert {
        name: String,
        #[arg(long = "field")]
        fields: Vec<String>,
    },
    /// List items without values
    List {
        #[arg(long)]
        json: bool,
        /// Only items whose name starts with this, e.g. myapp/
        #[arg(long)]
        prefix: Option<String>,
        /// Only items with this tag
        #[arg(long)]
        tag: Option<String>,
    },
    /// Import a .env file as an env item
    ImportEnv { name: String, path: PathBuf },
    /// Import a Chrome password export (CSV): one login per row, named chrome/<site>
    ImportChrome { path: PathBuf },
    /// Write an age-encrypted export (passphrase protected)
    Export {
        path: PathBuf,
        #[arg(long)]
        passphrase_stdin: bool,
    },
    /// Add items from an age-encrypted export; existing names are skipped
    Import {
        path: PathBuf,
        #[arg(long)]
        passphrase_stdin: bool,
    },
    /// Replace the root key (re-wraps domain keys, no field re-encryption)
    RotateRoot,
    /// Replace one domain key and re-encrypt that domain's fields
    RotateDomain {
        #[arg(value_parser = domain_parser())]
        domain: Domain,
    },
    #[command(flatten)]
    Request(requests::RequestCmd),
    #[command(flatten)]
    Draft(drafts::DraftCmd),
    /// Credential templates: the fields a new credential item starts with
    #[command(subcommand)]
    Template(templates::TemplateCmd),
}

/// `--kind`: the names in the kind registry (`KINDS`).
fn kind_parser() -> impl TypedValueParser<Value = ItemKind> {
    args::choice(KINDS.iter().map(|spec| spec.name), ItemKind::parse)
}

/// `rotate-domain`'s domain.
fn domain_parser() -> impl TypedValueParser<Value = Domain> {
    args::choice(Domain::ALL.map(Domain::as_str), Domain::parse)
}

/// `--status`: the statuses in the order `ItemStatus::ALL` shows them (J-185, J-260).
fn status_parser() -> impl TypedValueParser<Value = ItemStatus> {
    args::choice(ItemStatus::ALL.map(ItemStatus::as_str), ItemStatus::parse)
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Toggle {
    On,
    Off,
}
impl Toggle {
    fn on(self) -> bool {
        matches!(self, Self::On)
    }
}

pub fn run(app: &App, command: VaultCmd) -> Result<()> {
    match command {
        VaultCmd::Init => keys::init(app),
        VaultCmd::Add(args) => items::add(app, &args),
        VaultCmd::Edit(args) => items::edit(app, &args),
        VaultCmd::Revert { name, fields } => items::revert(app, &name, &fields),
        VaultCmd::Mv { from, to } => items::rename(app, &from, &to),
        VaultCmd::Rm { name } => items::remove(app, &name),
        VaultCmd::List { json, prefix, tag } => list(app, json, prefix, tag.as_deref()),
        VaultCmd::Request(command) => requests::run(app, command),
        VaultCmd::Draft(command) => drafts::run(app, command),
        VaultCmd::Template(command) => templates::run(app, command),
        VaultCmd::ImportEnv { name, path } => files::import_env(app, &name, &path),
        VaultCmd::ImportChrome { path } => files::import_chrome(app, &path),
        VaultCmd::Export {
            path,
            passphrase_stdin,
        } => files::export(app, &path, passphrase_stdin),
        VaultCmd::Import {
            path,
            passphrase_stdin,
        } => files::import(app, &path, passphrase_stdin),
        VaultCmd::RotateRoot => keys::rotate_root(app),
        VaultCmd::RotateDomain { domain } => keys::rotate_domain(app, domain),
    }
}

fn list(app: &App, json: bool, prefix: Option<String>, tag: Option<&str>) -> Result<()> {
    let tag = tag
        .map(Tag::parse)
        .transpose()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let filter = ItemFilter { prefix, tag };
    let mut items = app.vault()?.list()?;
    items.retain(|item| filter.keeps(item));
    if json {
        let values: Vec<_> = items.iter().map(output::summary_json).collect();
        println!("{}", serde_json::Value::Array(values));
    } else {
        for item in &items {
            println!("{}", output::summary_line(item));
        }
    }
    Ok(())
}
