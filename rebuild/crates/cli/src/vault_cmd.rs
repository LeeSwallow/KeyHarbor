//! `keyharbor vault …`. Never prints secret values; secrets arrive only by hidden prompt or stdin JSON.

use crate::confirm::Terminal;
use age::secrecy::SecretString;
use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand, ValueEnum};
use keyharbor_core::{
    ItemChange, ItemDetails, ItemSource, ItemStatus, ItemSummary, NewItem, Vault, weaken,
};
use keyharbor_domain::vault::{Domain, FieldName, ItemKind, ItemName, Origin};
use keyharbor_sqlite::SqliteStore;
use keyharbor_vault::keystore::keystore_from_env;
use std::{
    io::{IsTerminal, Read},
    path::{Path, PathBuf},
};
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
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
    },
    /// Remove an item
    Rm { name: String },
    /// List items without values
    List {
        #[arg(long)]
        json: bool,
    },
    /// Import a .env file as an env item
    ImportEnv { name: String, path: PathBuf },
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
}

#[derive(Args)]
pub struct FieldInput {
    /// Non-secret field, e.g. --set username=me
    #[arg(long = "set", value_name = "FIELD=VALUE")]
    set: Vec<String>,
    /// Read fields as a JSON object from stdin
    #[arg(long)]
    stdin: bool,
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

/// Secret fields (per the kind registry, every `env` field included) never come from argv.
fn is_secret(kind: ItemKind, field: &str) -> bool {
    kind.is_secret_field(field)
}

fn item_name(name: &str) -> Result<ItemName> {
    ItemName::parse(name).map_err(|e| anyhow!(e.to_string()))
}

fn field_name(kind: ItemKind, name: &str) -> Result<FieldName> {
    if kind == ItemKind::Env {
        FieldName::parse_env(name)
    } else {
        FieldName::parse(name)
    }
    .map_err(|e| anyhow!(e.to_string()))
}

fn origins(values: &[String]) -> Result<Vec<Origin>> {
    values
        .iter()
        .map(|o| Origin::parse(o).map_err(|e| anyhow!(e.to_string())))
        .collect()
}

/// Gathers `--set` values and secret values (hidden prompt on a TTY, or `--stdin` JSON).
fn collect_fields(
    kind: ItemKind,
    input: &FieldInput,
    prompt_missing: bool,
) -> Result<Vec<(FieldName, Zeroizing<String>)>> {
    let mut fields = Vec::new();
    for pair in &input.set {
        let (name, value) = pair.split_once('=').context("--set expects FIELD=VALUE")?;
        if is_secret(kind, name) {
            bail!("field {name} is secret; use the prompt or --stdin");
        }
        fields.push((field_name(kind, name)?, Zeroizing::new(value.to_owned())));
    }
    if input.stdin {
        let mut raw = Zeroizing::new(String::new());
        std::io::stdin().read_to_string(&mut raw)?;
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&raw)
            .map_err(|_| anyhow!("stdin must be a JSON object of string fields"))?;
        for (name, value) in map {
            let serde_json::Value::String(value) = value else {
                bail!("field {name} must be a JSON string");
            };
            fields.push((field_name(kind, &name)?, Zeroizing::new(value)));
        }
        return Ok(fields);
    }
    if !prompt_missing {
        return Ok(fields);
    }
    let secret_fields: Vec<&str> = match kind.schema() {
        Some(schema) => schema.into_iter().filter(|f| is_secret(kind, f)).collect(),
        None => bail!("env items take values from --stdin or `vault import-env`"),
    };
    if !std::io::stdin().is_terminal() {
        bail!("secret input requires a terminal or --stdin");
    }
    for field in secret_fields {
        if fields.iter().any(|(n, _)| n.as_str() == field) {
            continue;
        }
        let value = Zeroizing::new(rpassword::prompt_password(format!(
            "{field} (empty to skip): "
        ))?);
        if !value.is_empty() {
            fields.push((field_name(kind, field)?, value));
        }
    }
    Ok(fields)
}

fn read_passphrase(from_stdin: bool, confirm: bool) -> Result<SecretString> {
    let value = if from_stdin {
        let mut line = Zeroizing::new(String::new());
        std::io::stdin().read_line(&mut line)?;
        Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_owned())
    } else {
        if !std::io::stdin().is_terminal() {
            bail!("passphrase input requires a terminal or --passphrase-stdin");
        }
        let first = Zeroizing::new(rpassword::prompt_password("Export passphrase: ")?);
        if confirm {
            let second = Zeroizing::new(rpassword::prompt_password("Repeat passphrase: ")?);
            if *first != *second {
                bail!("passphrases do not match");
            }
        }
        first
    };
    if value.is_empty() {
        bail!("passphrase must not be empty");
    }
    Ok(SecretString::from(value.to_string()))
}

fn summary_line(item: &ItemSummary) -> String {
    format!(
        "{}\t{}\t{}\t{}{}\t{}\t{}",
        item.name,
        item.kind.as_str(),
        item.domain.as_str(),
        item.status.as_str(),
        if item.tampered { " (tampered)" } else { "" },
        item.origins.join(","),
        item.fields.join(","),
    )
}

fn summary_json(item: &ItemSummary) -> serde_json::Value {
    serde_json::json!({
        "name": item.name,
        "kind": item.kind.as_str(),
        "domain": item.domain.as_str(),
        "fields": item.fields,
        "origins": item.origins,
        "origin_binding": item.origin_binding,
        "run_allowed": item.run_allowed,
        "status": item.status.as_str(),
        "source": item.source.as_str(),
        "updated_at": item.updated_at,
        "tampered": item.tampered,
    })
}

fn open(root: &Path) -> Result<Vault> {
    Vault::open(SqliteStore::open(root)?, keystore_from_env())
}

pub fn run(root: &Path, command: VaultCmd) -> Result<()> {
    match command {
        VaultCmd::Init => {
            let vault = Vault::init(SqliteStore::open(root)?, keystore_from_env())?;
            println!("vault\t{}", vault.vault_id().to_uuid_string());
        }
        VaultCmd::Add {
            name,
            kind,
            input,
            origins: o,
            no_origin_binding,
            run_allowed,
            draft,
        } => {
            let kind = ItemKind::from(kind);
            let name = item_name(&name)?;
            let fields = collect_fields(kind, &input, true)?;
            let vault = open(root)?;
            vault.add(NewItem {
                name: name.clone(),
                kind,
                fields,
                origins: origins(&o)?,
                origin_binding: no_origin_binding.then_some(false),
                status: if draft {
                    ItemStatus::Draft
                } else {
                    ItemStatus::Active
                },
                source: ItemSource::User,
                details: ItemDetails {
                    run_allowed: run_allowed.map(Toggle::on),
                },
            })?;
            println!("added\t{}", name.as_str());
        }
        VaultCmd::Edit {
            name,
            input,
            unset,
            origins: o,
            clear_origins,
            origin_binding,
            run_allowed,
            status,
        } => {
            let name = item_name(&name)?;
            let vault = open(root)?;
            let kind = vault
                .list()?
                .into_iter()
                .find(|i| i.name == name.as_str())
                .context("item not found")?
                .kind;
            let set_fields = collect_fields(kind, &input, false)?;
            let new_origins = if clear_origins {
                Some(Vec::new())
            } else if o.is_empty() {
                None
            } else {
                Some(origins(&o)?)
            };
            let change = ItemChange {
                set_fields,
                remove_fields: unset
                    .iter()
                    .map(|f| field_name(kind, f))
                    .collect::<Result<_>>()?,
                origins: new_origins,
                origin_binding: origin_binding.map(Toggle::on),
                run_allowed: run_allowed.map(Toggle::on),
                status: status.map(|s| match s {
                    StatusArg::Active => ItemStatus::Active,
                    StatusArg::Draft => ItemStatus::Draft,
                    StatusArg::Discarded => ItemStatus::Discarded,
                }),
            };
            // Widening where or how the item may be used needs a person (R-35).
            weaken::require(&Terminal, &vault.weakenings(&name, &change)?)?;
            vault.edit(&name, change)?;
            println!("updated\t{}", name.as_str());
        }
        VaultCmd::Rm { name } => {
            let name = item_name(&name)?;
            if !open(root)?.remove(&name)? {
                bail!("item not found");
            }
            println!("removed\t{}", name.as_str());
        }
        VaultCmd::List { json } => {
            let items = open(root)?.list()?;
            if json {
                let values: Vec<_> = items.iter().map(summary_json).collect();
                println!("{}", serde_json::Value::Array(values));
            } else {
                for item in &items {
                    println!("{}", summary_line(item));
                }
            }
        }
        VaultCmd::ImportEnv { name, path } => {
            let name = item_name(&name)?;
            let text = Zeroizing::new(
                std::fs::read_to_string(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
            );
            let count = open(root)?.import_env(name.clone(), &text)?;
            println!("imported\t{}\t{count} variables", name.as_str());
        }
        VaultCmd::Export {
            path,
            passphrase_stdin,
        } => {
            let vault = open(root)?;
            vault.export_age(read_passphrase(passphrase_stdin, true)?, &path)?;
            println!("exported\t{}", path.display());
        }
        VaultCmd::Import {
            path,
            passphrase_stdin,
        } => {
            let vault = open(root)?;
            let report = vault.import_age(read_passphrase(passphrase_stdin, false)?, &path)?;
            for name in &report.skipped_existing {
                eprintln!("skipped existing item {name}");
            }
            println!("imported\t{}", report.added);
        }
        VaultCmd::RotateRoot => {
            open(root)?.rotate_root()?;
            println!("rotated\troot");
        }
        VaultCmd::RotateDomain { domain } => {
            let domain = match domain {
                DomainArg::Credentials => Domain::Credentials,
                DomainArg::Payment => Domain::Payment,
                DomainArg::Personal => Domain::Personal,
            };
            open(root)?.rotate_domain(domain)?;
            println!("rotated\t{}", domain.as_str());
        }
    }
    Ok(())
}
