//! `keyharbor vault …`. Never prints secret values; secrets arrive only by hidden prompt or stdin JSON.
//! Arguments are here, reading input in [`input`], printing in [`output`], adding and
//! changing items in [`items`], export and import files in [`files`].

mod drafts;
mod files;
mod input;
mod items;
mod output;
mod requests;

use crate::{compose::App, description_args::DescriptionArgs, expiry_args::ExpiryArgs};
use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use input::{FieldInput, item_name, read_passphrase};
use keyharbor_core::{ItemFilter, ItemStatus};
use keyharbor_domain::{
    tag::Tag,
    vault::{Domain, ItemKind},
};
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Subcommand)]
pub enum VaultCmd {
    /// Create the vault and store its root key in the keychain
    Init,
    /// Add an item. Secret fields are read from a hidden prompt or --stdin JSON
    Add {
        name: String,
        #[arg(long, value_enum)]
        kind: KindArg,
        #[command(flatten)]
        input: FieldInput,
        #[arg(long = "origin")]
        origins: Vec<String>,
        #[arg(long)]
        no_origin_binding: bool,
        /// Whether `keyharbor run` may inject the item (default: on for secret and env)
        #[arg(long, value_enum)]
        run_allowed: Option<Toggle>,
        #[command(flatten)]
        expiry: ExpiryArgs,
        #[command(flatten)]
        about: DescriptionArgs,
        /// A label to find the item by, e.g. env:prod (repeat for more)
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// A field that is not secret (PORT): its value shows in listings, unmasked
        #[arg(long = "plain", value_name = "FIELD")]
        plain: Vec<String>,
        #[arg(long)]
        draft: bool,
    },
    /// Change fields, origins, binding or status of an item
    Edit {
        name: String,
        #[command(flatten)]
        input: FieldInput,
        #[arg(long = "unset")]
        unset: Vec<String>,
        #[arg(long = "origin")]
        origins: Vec<String>,
        #[arg(long)]
        clear_origins: bool,
        #[arg(long, value_enum)]
        origin_binding: Option<Toggle>,
        /// Whether `keyharbor run` may inject the item
        #[arg(long, value_enum)]
        run_allowed: Option<Toggle>,
        #[command(flatten)]
        expiry: ExpiryArgs,
        #[command(flatten)]
        about: DescriptionArgs,
        /// Add a tag (repeat for more)
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Remove a tag
        #[arg(long = "untag")]
        untag: Vec<String>,
        /// Mark a field not secret: its value shows in listings, unmasked (asks first)
        #[arg(long = "plain", value_name = "FIELD")]
        plain: Vec<String>,
        /// Mark a field secret again
        #[arg(long = "secret", value_name = "FIELD")]
        secret: Vec<String>,
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
    },
    /// Remove an item
    Rm { name: String },
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
        #[arg(value_enum)]
        domain: DomainArg,
    },
    #[command(flatten)]
    Request(requests::RequestCmd),
    #[command(flatten)]
    Draft(drafts::DraftCmd),
}

#[derive(Clone, Copy, ValueEnum)]
pub enum KindArg {
    Login,
    Secret,
    Env,
    Card,
    Identity,
    Address,
}
impl From<KindArg> for ItemKind {
    fn from(kind: KindArg) -> Self {
        match kind {
            KindArg::Login => ItemKind::Login,
            KindArg::Secret => ItemKind::Secret,
            KindArg::Env => ItemKind::Env,
            KindArg::Card => ItemKind::Card,
            KindArg::Identity => ItemKind::Identity,
            KindArg::Address => ItemKind::Address,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
pub enum DomainArg {
    Credentials,
    Payment,
    Personal,
    Developer,
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
#[derive(Clone, Copy, ValueEnum)]
pub enum StatusArg {
    Active,
    Draft,
    Discarded,
}

impl From<StatusArg> for ItemStatus {
    fn from(status: StatusArg) -> Self {
        match status {
            StatusArg::Active => ItemStatus::Active,
            StatusArg::Draft => ItemStatus::Draft,
            StatusArg::Discarded => ItemStatus::Discarded,
        }
    }
}

impl From<DomainArg> for Domain {
    fn from(domain: DomainArg) -> Self {
        match domain {
            DomainArg::Credentials => Domain::Credentials,
            DomainArg::Payment => Domain::Payment,
            DomainArg::Personal => Domain::Personal,
            DomainArg::Developer => Domain::Developer,
        }
    }
}

pub fn run(app: &App, command: VaultCmd) -> Result<()> {
    match command {
        VaultCmd::Init => {
            let vault = app.init_vault()?;
            println!("vault\t{}", vault.vault_id().to_uuid_string());
        }
        VaultCmd::Add {
            name,
            kind,
            input,
            origins,
            no_origin_binding,
            run_allowed,
            expiry,
            about,
            tags,
            plain,
            draft,
        } => items::add(
            app,
            &items::AddArgs {
                name,
                kind: kind.into(),
                input,
                origins,
                no_origin_binding,
                run_allowed,
                expiry,
                about,
                tags,
                plain,
                draft,
            },
        )?,
        VaultCmd::Edit {
            name,
            input,
            unset,
            origins,
            clear_origins,
            origin_binding,
            run_allowed,
            expiry,
            about,
            tags,
            untag,
            plain,
            secret,
            status,
        } => items::edit(
            app,
            &items::EditArgs {
                name,
                input,
                unset,
                origins,
                clear_origins,
                origin_binding,
                run_allowed,
                expiry,
                about,
                tags,
                untag,
                plain,
                secret,
                status,
            },
        )?,
        VaultCmd::Revert { name, fields } => {
            let name = item_name(&name)?;
            let vault = app.vault()?;
            let kind = vault
                .list()?
                .into_iter()
                .find(|i| i.name == name.as_str())
                .context("item not found")?
                .kind;
            let fields = fields
                .iter()
                .map(|f| input::field_name(kind, f))
                .collect::<Result<Vec<_>>>()?;
            for field in vault.revert(&name, &fields)? {
                println!("reverted\t{}\t{field}", name.as_str());
            }
        }
        VaultCmd::Rm { name } => {
            let name = item_name(&name)?;
            if !app.vault()?.remove(&name)? {
                bail!("item not found");
            }
            println!("removed\t{}", name.as_str());
        }
        VaultCmd::List { json, prefix, tag } => {
            let tag = tag
                .as_deref()
                .map(Tag::parse)
                .transpose()
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            list(app, json, &ItemFilter { prefix, tag })?
        }
        VaultCmd::Request(command) => requests::run(app, command)?,
        VaultCmd::Draft(command) => drafts::run(app, command)?,
        VaultCmd::ImportEnv { name, path } => {
            let name = item_name(&name)?;
            let text = Zeroizing::new(
                std::fs::read_to_string(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
            );
            let count = app.vault()?.import_env(name.clone(), &text)?;
            println!("imported\t{}\t{count} variables", name.as_str());
        }
        VaultCmd::ImportChrome { path } => {
            let text = Zeroizing::new(
                std::fs::read_to_string(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
            );
            let report = app.vault()?.import_chrome_csv(&text)?;
            println!(
                "imported\t{} logins\tskipped {} existing, {} without a web address",
                report.added, report.skipped_existing, report.skipped_not_web
            );
        }
        VaultCmd::Export {
            path,
            passphrase_stdin,
        } => {
            let vault = app.vault()?;
            let bytes = vault.export_age(read_passphrase(passphrase_stdin, true)?)?;
            files::write_new_private(&path, &bytes)?;
            println!("exported\t{}", path.display());
        }
        VaultCmd::Import {
            path,
            passphrase_stdin,
        } => {
            let vault = app.vault()?;
            let encrypted = files::read(&path)?;
            let report = vault.import_age(read_passphrase(passphrase_stdin, false)?, &encrypted)?;
            for name in &report.skipped_existing {
                eprintln!("skipped existing item {name}");
            }
            println!("imported\t{}", report.added);
        }
        VaultCmd::RotateRoot => {
            app.vault()?.rotate_root()?;
            println!("rotated\troot");
        }
        VaultCmd::RotateDomain { domain } => {
            let domain = Domain::from(domain);
            app.vault()?.rotate_domain(domain)?;
            println!("rotated\t{}", domain.as_str());
        }
    }
    Ok(())
}

fn list(app: &App, json: bool, filter: &ItemFilter) -> Result<()> {
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
