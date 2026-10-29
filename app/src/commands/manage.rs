//! Adding, changing and removing items from the app (09 §9.4, §9.8), through the guard's
//! item path like the CLI: descriptions are checked for values, what a change weakens is
//! confirmed by the person ([`crate::confirm`]), then it is stored.

use crate::{
    Ctx,
    confirm::{Outcome, confirmed_by_dialog},
};
use anyhow::{Result, anyhow, bail};
use keyharbor_core::{ItemChange, ItemDetails, ItemSource, ItemStatus, NewItem, generate_password};
use keyharbor_domain::{
    password::PasswordRule,
    vault::{FieldName, ItemKind, ItemName, KINDS, Origin},
};
use keyharbor_guard::items;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

#[derive(Debug, Serialize)]
pub struct KindForm {
    pub kind: &'static str,
    pub domain: &'static str,
    /// `(name, secret)`; empty for env items, whose variables the person names.
    pub fields: Vec<(&'static str, bool)>,
    pub free_fields: bool,
}

/// What the "새 항목" form offers, from the kind registry.
pub fn kinds() -> Vec<KindForm> {
    KINDS
        .iter()
        .map(|spec| KindForm {
            kind: spec.name,
            domain: spec.domain.as_str(),
            fields: spec.fields.iter().map(|f| (f.name, f.secret)).collect(),
            free_fields: spec.free_fields,
        })
        .collect()
}

/// A new item as the form sends it.
#[derive(Debug, Deserialize)]
pub struct NewItemForm {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub origins: Vec<String>,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// Make up the login's password instead of taking one.
    #[serde(default)]
    pub generate_password: bool,
}

fn invalid(e: keyharbor_domain::vault::InvalidValue) -> anyhow::Error {
    anyhow!("{e}")
}

fn fields(
    kind: ItemKind,
    values: BTreeMap<String, String>,
) -> Result<Vec<(FieldName, Zeroizing<String>)>> {
    let mut out = Vec::new();
    for (name, value) in values {
        let value = Zeroizing::new(value);
        if !value.is_empty() {
            out.push((kind.field_name(&name).map_err(invalid)?, value));
        }
    }
    Ok(out)
}

fn origins(values: &[String]) -> Result<Vec<Origin>> {
    values
        .iter()
        .filter(|o| !o.trim().is_empty())
        .map(|o| Origin::parse(o.trim()).map_err(invalid))
        .collect()
}

pub fn add_item(ctx: &Ctx, form: NewItemForm) -> Result<String> {
    let kind = ItemKind::parse(&form.kind).ok_or_else(|| anyhow!("unknown kind"))?;
    let name = ItemName::parse_new(&form.name).map_err(invalid)?;
    let mut values = fields(kind, form.values)?;
    if form.generate_password {
        if kind != ItemKind::Login {
            bail!("only a login gets a generated password");
        }
        values.retain(|(f, _)| f.as_str() != "password");
        values.push((
            FieldName::parse("password")?,
            generate_password(&PasswordRule::default()),
        ));
    }
    if values.is_empty() {
        bail!("an item needs at least one value");
    }
    let item = NewItem {
        name: name.clone(),
        kind,
        fields: values,
        origins: origins(&form.origins)?,
        origin_binding: None,
        status: ItemStatus::Active,
        source: ItemSource::User,
        details: ItemDetails::default(),
    };
    items::add(&ctx.vault()?, item, &[])?;
    Ok(name.as_str().to_owned())
}

/// A change as the form sends it: values to replace (empty ones are left as they are) and,
/// when given, the whole new list of sites.
#[derive(Debug, Deserialize)]
pub struct ChangeForm {
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    #[serde(default)]
    pub origins: Option<Vec<String>>,
}

pub fn edit_item(ctx: &Ctx, name: &str, form: ChangeForm, confirmed: bool) -> Result<Outcome> {
    let name = ItemName::parse(name).map_err(invalid)?;
    let vault = ctx.vault()?;
    let kind = vault
        .list()?
        .into_iter()
        .find(|i| i.name == name.as_str())
        .ok_or_else(|| anyhow!("item not found"))?
        .kind;
    let change = || -> Result<ItemChange> {
        Ok(ItemChange {
            set_fields: fields(kind, form.values.clone())?,
            origins: form.origins.as_deref().map(origins).transpose()?,
            ..Default::default()
        })
    };
    confirmed_by_dialog(confirmed, |confirm| {
        items::edit(&vault, confirm, &name, change()?, &[])
    })
}

pub fn remove_item(ctx: &Ctx, name: &str) -> Result<()> {
    let name = ItemName::parse(name).map_err(invalid)?;
    if !ctx.vault()?.remove(&name)? {
        bail!("item not found");
    }
    Ok(())
}
