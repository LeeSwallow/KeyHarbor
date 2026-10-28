//! Adding, changing and removing items. Every change is recorded (without values).

use super::{
    DEFAULT_ITEM_TTL, ExpiryChoice, ItemChange, NO_TTL, NewItem, Vault, check_fields,
    sorted_origins,
};
use crate::{LOCAL_AGENT, expires_in};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    date::{Expiry, Ttl},
    ports::{FieldRow, ItemRow},
    vault::{Description, Domain, ItemName},
};
use keyharbor_vault::keys::KeyId;

/// `current` with `changes` applied (`None` removes), keeping only fields in `present`
/// and refusing a description for a field the item does not have. Sorted by field.
fn field_descriptions<'a>(
    changes: impl Iterator<Item = (&'a str, Option<&'a Description>)>,
    current: Vec<(String, String)>,
    present: &[&str],
) -> Result<Vec<(String, String)>> {
    let mut out: std::collections::BTreeMap<String, String> = current.into_iter().collect();
    for (field, description) in changes {
        if !present.contains(&field) {
            bail!("field {field} is not part of the item; it cannot have a description");
        }
        match description {
            Some(d) => out.insert(field.to_owned(), d.as_str().to_owned()),
            None => out.remove(field),
        };
    }
    out.retain(|field, _| present.contains(&field.as_str()));
    Ok(out.into_iter().collect())
}

impl Vault {
    pub fn add(&self, item: NewItem) -> Result<()> {
        let domain = item.kind.domain();
        self.add_in_domain(item, domain)
    }

    /// `add`, sealing and signing in `domain`; anything but the kind's domain only builds
    /// vaults as earlier releases left them, for tests.
    pub(crate) fn add_in_domain(&self, item: NewItem, domain: Domain) -> Result<()> {
        check_fields(item.kind, &item.fields)?;
        let id = *KeyId::new().as_bytes();
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
            expires_at: self.new_expiry(item.details.expiry)?.map(|e| e.to_string()),
            description: item
                .details
                .description
                .as_ref()
                .map(|d| d.as_str().to_owned()),
            field_descriptions: field_descriptions(
                item.details
                    .field_descriptions
                    .iter()
                    .map(|(f, d)| (f.as_str(), Some(d))),
                Vec::new(),
                &item
                    .fields
                    .iter()
                    .map(|(f, _)| f.as_str())
                    .collect::<Vec<_>>(),
            )?,
            status: item.status.as_str().to_owned(),
            source: item.source.as_str().to_owned(),
            meta_mac: Vec::new(),
            updated_at: self.clock.now_rfc3339(),
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

    /// The expiry `choice` gives an item created today.
    fn new_expiry(&self, choice: ExpiryChoice) -> Result<Option<Expiry>> {
        Ok(match choice {
            ExpiryChoice::Never => None,
            ExpiryChoice::On(expiry) => Some(expiry),
            ExpiryChoice::Default => match self.store.setting_get(DEFAULT_ITEM_TTL)?.as_deref() {
                None | Some(NO_TTL) => None,
                Some(ttl) => Some(expires_in(
                    &*self.clock,
                    Ttl::parse(ttl).map_err(|e| anyhow!("{DEFAULT_ITEM_TTL}: {e}"))?,
                )),
            },
        })
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
        if let Some(expiry) = change.expiry {
            row.expires_at = expiry.map(|e| e.to_string());
        }
        if let Some(description) = &change.description {
            row.description = description.as_ref().map(|d| d.as_str().to_owned());
        }
        let present: Vec<&str> = kept.iter().map(|f| f.field.as_str()).collect();
        row.field_descriptions = field_descriptions(
            change
                .field_descriptions
                .iter()
                .map(|(f, d)| (f.as_str(), d.as_ref())),
            std::mem::take(&mut row.field_descriptions),
            &present,
        )?;
        if let Some(status) = change.status {
            row.status = status.as_str().to_owned();
        }
        row.updated_at = self.clock.now_rfc3339();
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
