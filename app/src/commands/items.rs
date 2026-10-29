//! The item screens (09 §9.4, §9.4.2): metadata and placeholders, and "보기" for one value.

use crate::Ctx;
use anyhow::{Result, anyhow};
use keyharbor_domain::vault::{FieldName, ItemName, placeholder};
use keyharbor_guard::reveal as guard_reveal;
use serde::Serialize;
use std::collections::BTreeMap;

/// An item as the list and detail show it: no value but those marked not secret.
#[derive(Debug, Serialize)]
pub struct ItemDto {
    pub name: String,
    pub kind: &'static str,
    pub domain: &'static str,
    pub status: &'static str,
    pub source: &'static str,
    pub description: Option<String>,
    pub fields: Vec<String>,
    pub placeholders: Vec<String>,
    pub plain_values: BTreeMap<String, String>,
    pub origins: Vec<String>,
    pub tags: Vec<String>,
    pub expired: bool,
    /// "변조 의심": not used for input until re-signed (09 §9.10).
    pub tampered: bool,
}

pub fn items(ctx: &Ctx) -> Result<Vec<ItemDto>> {
    if ctx.store()?.vault_get()?.is_none() {
        return Ok(Vec::new());
    }
    Ok(ctx
        .vault()?
        .list()?
        .into_iter()
        .map(|item| ItemDto {
            placeholders: item
                .fields
                .iter()
                .map(|f| placeholder(&item.name, f))
                .collect(),
            kind: item.kind.as_str(),
            domain: item.domain.as_str(),
            status: item.status.as_str(),
            source: item.source.as_str(),
            name: item.name,
            description: item.description,
            fields: item.fields,
            plain_values: item.plain_values,
            origins: item.origins,
            tags: item.tags,
            expired: item.expired,
            tampered: item.tampered,
        })
        .collect())
}

/// One value, for the person at the app to see or copy (recorded by the guard).
pub fn reveal(ctx: &Ctx, item: &str, field: &str) -> Result<String> {
    let item = ItemName::parse(item).map_err(|e| anyhow!("{e}"))?;
    let field = FieldName::parse(field).map_err(|e| anyhow!("{e}"))?;
    let value = guard_reveal::reveal(&*ctx.store()?, &ctx.vault()?, &item, &field)?;
    Ok(value.as_str().to_owned())
}
