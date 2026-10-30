//! Adding, changing and removing items. Every change is recorded (without values).

use super::{
    DEFAULT_ITEM_TTL, ExpiryChoice, ItemChange, ItemStatus, NO_TTL, NewItem, Vault, check_fields,
    sorted_origins,
};
use crate::{LOCAL_AGENT, expires_in};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    date::{Expiry, Ttl},
    ports::{FieldRow, ItemRow},
    tag::{Tag, tag_set},
    vault::{Description, Domain, FieldName, ItemName},
};
use keyharbor_vault::keys::KeyId;

/// The fields marked not secret, sorted and unique; each must be a field the item has.
fn plain_set<'a>(marked: impl Iterator<Item = &'a str>, present: &[&str]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for field in marked {
        if !present.contains(&field) {
            bail!("field {field} is not part of the item; it cannot be marked plain");
        }
        out.push(field.to_owned());
    }
    out.sort();
    out.dedup();
    Ok(out)
}

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
        self.insert(item, domain, LOCAL_AGENT)
    }

    /// Seals, signs and stores `item` in `domain`, recording `agent` as who added it.
    pub(super) fn insert(&self, item: NewItem, domain: Domain, agent: &str) -> Result<()> {
        if item.name.is_draft() && item.status != ItemStatus::Draft {
            bail!("a name with ~draft- is reserved for drafts");
        }
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
            tags: tag_set(item.details.tags.clone())
                .map_err(|e| anyhow!("{e}"))?
                .iter()
                .map(|t| t.as_str().to_owned())
                .collect(),
            plain_fields: plain_set(
                item.details.plain_fields.iter().map(|f| f.as_str()),
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
        self.note_change_by(agent, &item.name, "added");
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
        self.note_change_by(LOCAL_AGENT, name, result);
    }
    pub(super) fn note_change_by(&self, agent: &str, name: &ItemName, result: &str) {
        let _ = self.store.record(
            &AuditEvent::new(agent, AuditKind::VaultItemChanged, result).item(name.as_str()),
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
        // A replaced value stays as the field's `previous` record, for `revert`.
        let replaced = |name: &str| {
            fields
                .iter()
                .find(|f| f.field == name)
                .map(|f| f.sealed.clone())
        };
        let new_fields: Vec<FieldRow> = change
            .set_fields
            .iter()
            .map(|(n, v)| FieldRow {
                previous: replaced(n.as_str()),
                ..self.seal(&row.id, domain, n, v)
            })
            .collect();
        let mut kept: Vec<FieldRow> = fields
            .iter()
            .filter(|f| {
                !change.remove_fields.iter().any(|r| r.as_str() == f.field)
                    && !change.set_fields.iter().any(|(s, _)| s.as_str() == f.field)
            })
            .cloned()
            .collect();
        kept.extend(new_fields);
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
        // Already plain (minus those made secret, and fields that are gone), plus those made
        // plain now, which must exist.
        let still_plain = row.plain_fields.iter().map(String::as_str).filter(|f| {
            present.contains(f) && !change.make_secret.iter().any(|s| s.as_str() == *f)
        });
        let marked: Vec<&str> = still_plain
            .chain(change.make_plain.iter().map(FieldName::as_str))
            .collect();
        row.plain_fields = plain_set(marked.into_iter(), &present)?;
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
        if !change.add_tags.is_empty() || !change.remove_tags.is_empty() {
            let current = row
                .tags
                .iter()
                .map(|t| Tag::parse(t))
                .collect::<Result<Vec<_>, _>>();
            let mut tags = current.map_err(|e| anyhow!("{e}"))?;
            tags.retain(|t| !change.remove_tags.contains(t));
            tags.extend(change.add_tags.iter().cloned());
            row.tags = tag_set(tags)
                .map_err(|e| anyhow!("{e}"))?
                .iter()
                .map(|t| t.as_str().to_owned())
                .collect();
        }
        row.updated_at = self.clock.now_rfc3339();
        row.meta_mac = self.mac_for(&row, kind, &name, &origins);
        self.store.item_update(&self.guard(domain), &row, &kept)?;
        self.note_change(&name, "updated");
        Ok(())
    }

    /// Gives an item a new name. Crate-only: interfaces go through `rename::rename_item`, which
    /// first refuses while a policy rule names the item (J-104).
    pub(crate) fn rename(&self, from: &ItemName, to: &ItemName) -> Result<()> {
        if from.is_draft() {
            bail!("a draft is renamed by confirming it");
        }
        let to = ItemName::parse_new(to.as_str()).map_err(|e| anyhow!("{e}"))?;
        let names: Vec<String> = self.list()?.into_iter().map(|i| i.name).collect();
        if names.iter().any(|n| n == to.as_str()) {
            bail!("an item named {} already exists", to.as_str());
        }
        if names
            .iter()
            .filter_map(|n| ItemName::parse(n).ok())
            .any(|n| n.is_draft() && n.base() == *from)
        {
            bail!(
                "drafts of {} wait to be confirmed or discarded; settle them first",
                from.as_str()
            );
        }
        let (mut row, fields) = self
            .store
            .item_get(from.as_str())?
            .context("item not found")?;
        let (kind, _, origins) = self.verified(&row)?;
        row.name = to.as_str().to_owned();
        row.updated_at = self.clock.now_rfc3339();
        row.meta_mac = self.mac_for(&row, kind, &to, &origins);
        self.store
            .item_update(&self.guard(kind.domain()), &row, &fields)?;
        self.note_change(&to, &format!("renamed from {}", from.as_str()));
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
