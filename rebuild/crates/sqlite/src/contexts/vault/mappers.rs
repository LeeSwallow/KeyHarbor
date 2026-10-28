//! Vault rows as stored ↔ the port's row types.

use super::models::{DomainKeyModel, FieldModel, ItemModel, VaultModel};
use anyhow::{Context, Result};
use keyharbor_domain::ports::vault::{DomainKeyRow, FieldRow, ItemRow, VaultRow};

pub fn id16(bytes: Vec<u8>) -> Result<[u8; 16]> {
    bytes.try_into().ok().context("invalid stored id")
}

pub fn vault_row(model: VaultModel) -> Result<VaultRow> {
    Ok(VaultRow {
        vault_id: id16(model.id)?,
        kek_id: id16(model.kek_id)?,
        version: model.version,
    })
}

pub fn domain_key_row(model: DomainKeyModel) -> Result<DomainKeyRow> {
    Ok(DomainKeyRow {
        domain: u8::try_from(model.domain).context("invalid stored domain")?,
        dk_id: id16(model.dk_id)?,
        wrapped: model.wrapped,
    })
}

pub fn domain_key_model(row: &DomainKeyRow) -> DomainKeyModel {
    DomainKeyModel {
        domain: i32::from(row.domain),
        dk_id: row.dk_id.to_vec(),
        wrapped: row.wrapped.clone(),
    }
}

/// `origins` are read separately and sorted.
pub fn item_row(model: ItemModel, origins: Vec<String>) -> Result<ItemRow> {
    Ok(ItemRow {
        id: id16(model.id)?,
        name: model.name,
        domain: u8::try_from(model.domain).context("invalid stored domain")?,
        kind: model.kind,
        origin_binding: model.origin_binding,
        run_allowed: model.run_allowed,
        expires_at: model.expires_at,
        status: model.status,
        source: model.source,
        meta_mac: model.meta_mac,
        updated_at: model.updated_at,
        origins,
    })
}

pub fn item_model(row: &ItemRow) -> ItemModel {
    ItemModel {
        id: row.id.to_vec(),
        name: row.name.clone(),
        domain: i32::from(row.domain),
        kind: row.kind.clone(),
        origin_binding: row.origin_binding,
        status: row.status.clone(),
        source: row.source.clone(),
        meta_mac: row.meta_mac.clone(),
        updated_at: row.updated_at.clone(),
        run_allowed: row.run_allowed,
        expires_at: row.expires_at.clone(),
    }
}

pub fn field_row(model: FieldModel) -> FieldRow {
    FieldRow {
        field: model.field,
        sealed: model.sealed,
    }
}

pub fn field_model(item: &[u8; 16], row: &FieldRow) -> FieldModel {
    FieldModel {
        item_id: item.to_vec(),
        field: row.field.clone(),
        sealed: row.sealed.clone(),
    }
}
