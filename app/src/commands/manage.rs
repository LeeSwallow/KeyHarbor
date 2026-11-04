//! Adding, changing and removing items from the app (09 §9.4, §9.8), through the guard's
//! item path like the CLI: descriptions are checked for values, what a change weakens is
//! confirmed by the person ([`crate::confirm`]), then it is stored.

use super::ids::{item_name, new_item_name};
use crate::{
    Ctx,
    auth::AuthScope,
    confirm::{Outcome, confirmed_by_dialog},
};
use anyhow::{Result, anyhow, bail};
use keyharbor_core::{ItemChange, ItemDetails, ItemSource, ItemStatus, NewItem, generate_password};
use keyharbor_domain::{
    password::PasswordRule,
    vault::{FieldName, FieldSpec, ItemKind, KINDS, Origin},
};
use keyharbor_guard::items;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// A field a form asks for: the "새 항목" form here and the secure input form
/// ([`super::RequestForm`]) send the same shape.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct FormField {
    pub name: &'static str,
    /// Shown as a password input, never echoed.
    pub secret: bool,
}

impl From<&FieldSpec> for FormField {
    fn from(spec: &FieldSpec) -> Self {
        Self {
            name: spec.name,
            secret: spec.secret,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct KindForm {
    pub kind: &'static str,
    pub domain: &'static str,
    /// Empty for env items, whose variables the person names.
    pub fields: Vec<FormField>,
    pub free_fields: bool,
    pub custom_fields: bool,
    /// The field the form can make up instead of taking (`KindSpec::generated`); `None` hides
    /// the "생성" box.
    pub generates: Option<&'static str>,
}

/// What the "새 항목" form offers, from the kind registry.
pub fn kinds() -> Vec<KindForm> {
    KINDS
        .iter()
        .map(|spec| KindForm {
            kind: spec.name,
            domain: spec.domain.as_str(),
            fields: spec.fields.iter().map(FormField::from).collect(),
            free_fields: spec.free_fields,
            custom_fields: spec.custom_fields,
            generates: spec.generated,
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
    /// Make up the kind's generated field (a login's password) instead of taking one.
    #[serde(default)]
    pub generate_password: bool,
}

/// The form's values as they arrive, each wiped when dropped. Done first, before any check
/// can return early and drop a plain `String` (J-178).
type Typed = Vec<(String, Zeroizing<String>)>;

fn wiped(values: BTreeMap<String, String>) -> Typed {
    values
        .into_iter()
        .map(|(name, value)| (name, Zeroizing::new(value)))
        .collect()
}

fn fields(kind: ItemKind, values: Typed) -> Result<Vec<(FieldName, Zeroizing<String>)>> {
    let mut out = Vec::new();
    for (name, value) in values {
        if !value.is_empty() {
            out.push((kind.field_name(&name)?, value));
        }
    }
    Ok(out)
}

fn origins(values: &[String]) -> Result<Vec<Origin>> {
    values
        .iter()
        .filter(|o| !o.trim().is_empty())
        .map(|o| Ok(Origin::parse(o.trim())?))
        .collect()
}

pub fn add_item(ctx: &Ctx, form: NewItemForm) -> Result<String> {
    let typed = wiped(form.values);
    ctx.challenge(AuthScope::Edit)?;
    let kind = ItemKind::parse(&form.kind).ok_or_else(|| anyhow!("unknown kind"))?;
    let name = new_item_name(&form.name)?;
    let mut values = fields(kind, typed)?;
    if form.generate_password {
        let field = kind
            .generated_field()
            .ok_or_else(|| anyhow!("only a login gets a generated password"))?;
        values.retain(|(f, _)| f.as_str() != field);
        values.push((
            FieldName::parse(field)?,
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
    let typed = wiped(form.values);
    ctx.challenge(AuthScope::Edit)?;
    let name = item_name(name)?;
    let vault = ctx.vault()?;
    let kind = vault
        .item_use(&name)?
        .ok_or_else(|| anyhow!("item not found"))?
        .kind;
    // Moved, not cloned: `confirmed_by_dialog` runs the change once.
    let change = move || -> Result<ItemChange> {
        Ok(ItemChange {
            set_fields: fields(kind, typed)?,
            origins: form.origins.as_deref().map(origins).transpose()?,
            ..Default::default()
        })
    };
    confirmed_by_dialog(confirmed, |confirm| {
        items::edit(&vault, confirm, &name, change()?, &[])
    })
}

pub fn remove_item(ctx: &Ctx, name: &str) -> Result<()> {
    ctx.challenge(AuthScope::Edit)?;
    let name = item_name(name)?;
    if !ctx.vault()?.remove(&name)? {
        bail!("item not found");
    }
    Ok(())
}

/// Renames or moves an item (its placeholders change; the window warns first). Refused while a
/// policy rule names it (J-104).
pub fn rename_item(ctx: &Ctx, from: &str, to: &str) -> Result<String> {
    ctx.challenge(AuthScope::Edit)?;
    let from = item_name(from)?;
    let to = new_item_name(to)?;
    keyharbor_core::rename::rename_item(&*ctx.store()?, &ctx.vault()?, &from, &to)?;
    Ok(to.as_str().to_owned())
}
