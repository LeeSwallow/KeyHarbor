//! Credential templates on the settings screen (09 §9.4.7, R-48): list, add, remove. The new
//! item form fills a credential's fields from one. A template holds no value, so changing one
//! weakens nothing; it is a settings change, so it passes the authenticator like one (J-281).

use crate::{Ctx, auth::AuthScope};
use anyhow::{Result, anyhow};
use keyharbor_core::template;
use keyharbor_domain::vault::{CredentialTemplate, ItemKind, TemplateField, TemplateName};
use serde::{Deserialize, Serialize};

/// One field of a template, as the screen shows and sends it.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateFieldDto {
    pub name: String,
    /// Secret fields are password inputs and masked; plain ones show in listings.
    pub secret: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateDto {
    pub name: String,
    pub fields: Vec<TemplateFieldDto>,
}

impl From<&CredentialTemplate> for TemplateDto {
    fn from(t: &CredentialTemplate) -> Self {
        Self {
            name: t.name().as_str().to_owned(),
            fields: t
                .fields()
                .iter()
                .map(|f| TemplateFieldDto {
                    name: f.name.as_str().to_owned(),
                    secret: f.secret,
                })
                .collect(),
        }
    }
}

fn name(value: &str) -> Result<TemplateName> {
    TemplateName::parse(value).map_err(|e| anyhow!("{e}"))
}

fn to_template(form: TemplateDto) -> Result<CredentialTemplate> {
    let fields = form
        .fields
        .into_iter()
        .map(|f| {
            Ok(TemplateField {
                name: ItemKind::Credential
                    .field_name(f.name.trim())
                    .map_err(|e| anyhow!("{e}"))?,
                secret: f.secret,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    CredentialTemplate::new(name(&form.name)?, fields).map_err(|e| anyhow!("{e}"))
}

pub fn templates(ctx: &Ctx) -> Result<Vec<TemplateDto>> {
    Ok(template::list(&*ctx.store()?)?
        .iter()
        .map(TemplateDto::from)
        .collect())
}

pub fn add_template(ctx: &Ctx, form: TemplateDto) -> Result<()> {
    ctx.challenge(AuthScope::PolicyChange)?;
    template::add(&*ctx.store()?, &to_template(form)?)
}

pub fn remove_template(ctx: &Ctx, template_name: &str) -> Result<()> {
    ctx.challenge(AuthScope::PolicyChange)?;
    template::remove(&*ctx.store()?, &name(template_name)?)
}
