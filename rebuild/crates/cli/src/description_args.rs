//! `--description`, `--no-description` and `--field-description FIELD=TEXT` (R-38). Parsing
//! only; the texts are checked against the vault's values before anything is stored.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use keyharbor_domain::vault::{Description, FieldName, ItemKind};

#[derive(Args)]
pub struct DescriptionArgs {
    /// What the item is for, shown to agents (at most 500 characters, never a secret)
    #[arg(long, value_name = "TEXT", conflicts_with = "no_description")]
    description: Option<String>,
    /// Remove the item's description
    #[arg(long)]
    no_description: bool,
    /// What one field holds, e.g. --field-description API_KEY="OpenAI, project X";
    /// an empty TEXT goes back to the kind's default
    #[arg(long = "field-description", value_name = "FIELD=TEXT")]
    fields: Vec<String>,
}

/// A description given on the command line.
pub fn parse(text: &str) -> Result<Description> {
    Description::parse(text).map_err(|e| anyhow!("{e}"))
}

impl DescriptionArgs {
    /// `Some(None)` to remove, `None` when not given.
    pub fn item(&self) -> Result<Option<Option<Description>>> {
        if self.no_description {
            return Ok(Some(None));
        }
        self.description
            .as_deref()
            .map(parse)
            .transpose()
            .map(|d| d.map(Some))
    }

    /// Each field with its new description, `None` to go back to the default.
    pub fn fields(&self, kind: ItemKind) -> Result<Vec<(FieldName, Option<Description>)>> {
        self.fields
            .iter()
            .map(|pair| {
                let (field, text) = pair
                    .split_once('=')
                    .context("--field-description expects FIELD=TEXT")?;
                let field = kind.field_name(field).map_err(|e| anyhow!("{e}"))?;
                let text = (!text.trim().is_empty()).then(|| parse(text)).transpose()?;
                Ok((field, text))
            })
            .collect()
    }

    /// Field descriptions for a new item, where removing one means nothing.
    pub fn new_fields(&self, kind: ItemKind) -> Result<Vec<(FieldName, Description)>> {
        self.fields(kind)?
            .into_iter()
            .map(|(field, text)| match text {
                Some(text) => Ok((field, text)),
                None => bail!("--field-description {} needs a text", field.as_str()),
            })
            .collect()
    }

    /// Every text given, to check against the vault's values.
    pub fn texts(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.description.iter().map(String::as_str).collect();
        out.extend(
            self.fields
                .iter()
                .filter_map(|pair| pair.split_once('=').map(|(_, text)| text)),
        );
        out
    }
}
