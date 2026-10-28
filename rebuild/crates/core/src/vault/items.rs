//! Adding, changing and removing items. Every change is recorded (without values).

use super::{ItemChange, NewItem, Vault, check_fields, sorted_origins};
use crate::{LOCAL_AGENT, now_rfc3339};
use anyhow::{Context, Result};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    ports::{FieldRow, ItemRow},
    vault::ItemName,
};
use keyharbor_vault::keys::KeyId;

impl Vault {
    pub fn add(&self, item: NewItem) -> Result<()> {
        check_fields(item.kind, &item.fields)?;
        let id = *KeyId::new().as_bytes();
        let domain = item.kind.domain();
        let mut row = ItemRow {
            id,
            name: item.name.as_str().to_owned(),
            domain: domain.as_byte(),
            kind: item.kind.as_str().to_owned(),
            origin_binding: item
                .origin_binding
                .unwrap_or_else(|| item.kind.default_origin_binding()),
            run_allowed: item
                .details
                .run_allowed
                .unwrap_or_else(|| item.kind.default_run_allowed()),
            status: item.status.as_str().to_owned(),
            source: item.source.as_str().to_owned(),
            meta_mac: Vec::new(),
            updated_at: now_rfc3339(),
            origins: sorted_origins(&item.origins),
        };
        row.meta_mac = self.mac_for(&row, item.kind, &item.name, &item.origins);
        let fields: Vec<FieldRow> = item
            .fields
            .iter()
            .map(|(name, value)| self.seal(&id, domain, name, value))
            .collect();
        self.store.item_insert(&self.guard(domain), &row, &fields)?;
        self.note_change(&item.name, "added");
        Ok(())
    }

    /// Records a vault change. Best effort: the change itself is already committed.
    fn note_change(&self, name: &ItemName, result: &str) {
        let _ = self.store.record(
            &AuditEvent::new(LOCAL_AGENT, AuditKind::VaultItemChanged, result).item(name.as_str()),
        );
    }

    pub fn edit(&self, name: &ItemName, change: ItemChange) -> Result<()> {
        let (mut row, fields) = self
            .store
            .item_get(name.as_str())?
            .context("item not found")?;
        let (kind, name, origins) = self.verified(&row)?;
        check_fields(kind, &change.set_fields)?;
        let domain = kind.domain();
        let mut kept: Vec<FieldRow> = fields
            .into_iter()
            .filter(|f| {
                !change.remove_fields.iter().any(|r| r.as_str() == f.field)
                    && !change.set_fields.iter().any(|(s, _)| s.as_str() == f.field)
            })
            .collect();
        kept.extend(
            change
                .set_fields
                .iter()
                .map(|(n, v)| self.seal(&row.id, domain, n, v)),
        );
        let origins = change.origins.unwrap_or(origins);
        row.origins = sorted_origins(&origins);
        if let Some(binding) = change.origin_binding {
            row.origin_binding = binding;
        }
        if let Some(allowed) = change.run_allowed {
            row.run_allowed = allowed;
        }
        if let Some(status) = change.status {
            row.status = status.as_str().to_owned();
        }
        row.updated_at = now_rfc3339();
        row.meta_mac = self.mac_for(&row, kind, &name, &origins);
        self.store.item_update(&self.guard(domain), &row, &kept)?;
        self.note_change(&name, "updated");
        Ok(())
    }

    pub fn remove(&self, name: &ItemName) -> Result<bool> {
        let removed = self.store.item_delete(name.as_str())?;
        if removed {
            self.note_change(name, "removed");
        }
        Ok(removed)
    }
}
