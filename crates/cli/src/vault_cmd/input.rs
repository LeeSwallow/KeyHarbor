//! Reading item fields and passphrases: `--set` values, a hidden prompt on a terminal, or a
//! JSON object on stdin. Secret fields never come from argv.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use keyharbor_core::Passphrase;
use keyharbor_domain::vault::{FieldName, ItemKind, ItemName, Origin};
use std::io::{IsTerminal, Read};
use zeroize::Zeroizing;

#[derive(Args)]
pub struct FieldInput {
    /// Non-secret field, e.g. --set username=me
    #[arg(long = "set", value_name = "FIELD=VALUE", id = "set")]
    set: Vec<String>,
    /// Read fields as a JSON object from stdin
    #[arg(long)]
    stdin: bool,
}

/// Secret fields (per the kind registry, every `env` field included) never come from argv.
fn is_secret(kind: ItemKind, field: &str) -> bool {
    kind.is_secret_field(field)
}

pub fn item_name(name: &str) -> Result<ItemName> {
    ItemName::parse(name).map_err(|e| anyhow!(e.to_string()))
}

pub fn field_name(kind: ItemKind, name: &str) -> Result<FieldName> {
    kind.field_name(name).map_err(|e| anyhow!(e.to_string()))
}

pub fn origins(values: &[String]) -> Result<Vec<Origin>> {
    values
        .iter()
        .map(|o| Origin::parse(o).map_err(|e| anyhow!(e.to_string())))
        .collect()
}

/// Gathers `--set` values and secret values (hidden prompt on a TTY, or `--stdin` JSON).
pub fn collect_fields(
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

pub fn read_passphrase(from_stdin: bool, confirm: bool) -> Result<Passphrase> {
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
    Ok(Passphrase::new(value.to_string()))
}
