//! Credential templates ↔ their stored rows. Field names are ASCII letters, digits and `_`
//! (`FieldName`), so a space and `=` never occur inside one.

use super::models::TemplateRow;
use anyhow::{Context, Result, anyhow};
use keyharbor_domain::vault::{CredentialTemplate, FieldName, TemplateField, TemplateName};

const SECRET: &str = "secret";
const PLAIN: &str = "plain";

pub fn to_row(template: &CredentialTemplate) -> TemplateRow {
    let fields: Vec<String> = template
        .fields()
        .iter()
        .map(|f| {
            format!(
                "{}={}",
                f.name.as_str(),
                if f.secret { SECRET } else { PLAIN }
            )
        })
        .collect();
    TemplateRow {
        name: template.name().as_str().to_owned(),
        fields: fields.join(" "),
    }
}

pub fn from_row(row: TemplateRow) -> Result<CredentialTemplate> {
    let fields = row
        .fields
        .split(' ')
        .filter(|f| !f.is_empty())
        .map(|f| {
            let (name, kind) = f.split_once('=').context("stored template field")?;
            let secret = match kind {
                SECRET => true,
                PLAIN => false,
                _ => return Err(anyhow!("stored template field")),
            };
            Ok(TemplateField {
                name: FieldName::parse(name)?,
                secret,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CredentialTemplate::new(
        TemplateName::parse(&row.name)?,
        fields,
    )?)
}
