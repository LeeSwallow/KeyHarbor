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
    /// Non-secret field, e.g. --set username=me (a field you name only once marked --plain)
    #[arg(long = "set", value_name = "FIELD=VALUE", id = "set")]
    set: Vec<String>,
    /// Read fields as a JSON object from stdin
    #[arg(long)]
    stdin: bool,
    /// Type this field's value at a hidden prompt; a name the kind does not have adds a field
    /// of your own, e.g. --field second_password (repeat for more)
    #[arg(long = "field", value_name = "FIELD", conflicts_with = "stdin")]
    prompt: Vec<String>,
}

/// What [`collect_fields`] prompts for besides `--field`, and which fields are plain.
#[derive(Default)]
pub struct Fill<'a> {
    /// Prompt for the kind's secret fields that were not given (a new item).
    pub kind_secrets: bool,
    /// Fields a template gives a new item, prompted for in order.
    pub template: &'a [FieldName],
    /// Fields marked plain: a field the person names may then come from `--set`.
    pub plain: &'a [FieldName],
}

/// Secret fields (per the kind registry, every `env` field included) never come from argv.
/// A field the person names and marks plain may (R-48), unless some kind declares that name
/// secret (`password`): such a name stays off the command line whatever it is marked.
fn allowed_on_argv(kind: ItemKind, field: &str, plain: &[FieldName]) -> bool {
    !kind.is_secret_field(field)
        || (kind.is_custom_field(field)
            && plain.iter().any(|p| p.as_str() == field)
            && !ItemKind::is_declared_secret_anywhere(field))
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
    fill: Fill<'_>,
) -> Result<Vec<(FieldName, Zeroizing<String>)>> {
    let mut fields = Vec::new();
    for pair in &input.set {
        let (name, value) = pair.split_once('=').context("--set expects FIELD=VALUE")?;
        if !allowed_on_argv(kind, name, fill.plain) {
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
    let asked: Vec<String> = prompted_fields(kind, input, &fill)?
        .into_iter()
        .filter(|f| !fields.iter().any(|(n, _)| n.as_str() == f))
        .collect();
    if asked.is_empty() {
        return Ok(fields);
    }
    if !std::io::stdin().is_terminal() {
        bail!("secret input requires a terminal or --stdin");
    }
    for field in asked {
        let value = Zeroizing::new(rpassword::prompt_password(prompt(kind, &field))?);
        if !value.is_empty() {
            fields.push((field_name(kind, &field)?, value));
        }
    }
    Ok(fields)
}

/// The hidden prompt for `field`: a seed says which forms it takes.
fn prompt(kind: ItemKind, field: &str) -> String {
    if kind.field(field).is_some_and(|spec| spec.otp_seed) {
        format!("{field} (base32 or otpauth:// URI, empty to skip): ")
    } else {
        format!("{field} (empty to skip): ")
    }
}

/// The fields to ask for at a hidden prompt, in order and once each: the kind's secret
/// fields (a new item), the template's, then `--field`.
fn prompted_fields(kind: ItemKind, input: &FieldInput, fill: &Fill<'_>) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    if fill.kind_secrets {
        let Some(schema) = kind.schema() else {
            bail!("env items take values from --stdin or `vault import-env`");
        };
        let secrets = schema.into_iter().filter(|f| kind.is_secret_field(f));
        out.extend(secrets.map(str::to_owned));
    }
    out.extend(fill.template.iter().map(|f| f.as_str().to_owned()));
    for field in &input.prompt {
        out.push(field_name(kind, field)?.as_str().to_owned());
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|f| seen.insert(f.clone()));
    Ok(out)
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
