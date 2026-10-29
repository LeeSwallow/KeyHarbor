//! What callers may know about items without any value: summaries, verified origins and the
//! facts the guard checks before a use.

use super::{ItemSource, ItemStatus, ItemSummary, Vault};
use anyhow::{Context, Result, anyhow};
use keyharbor_domain::{
    date::{Date, Expiry},
    ports::{ItemRow, VaultStore},
    vault::{Domain, ItemKind, ItemName, Origin},
};
use std::collections::BTreeMap;

/// Verified, value-free facts about an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemUse {
    pub kind: ItemKind,
    pub domain: Domain,
    pub origin_binding: bool,
    pub run_allowed: bool,
    pub expires_at: Option<Expiry>,
    /// The expiry date has passed (as of today, UTC).
    pub expired: bool,
    pub origins: Vec<Origin>,
    pub status: ItemStatus,
    pub fields: Vec<String>,
}

/// Each field's description: the item's own, or its kind's default.
pub fn effective_descriptions(
    kind: ItemKind,
    fields: &[String],
    own: &[(String, String)],
) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|field| {
            let text = own
                .iter()
                .find(|(f, _)| f == field)
                .map(|(_, d)| d.as_str())
                .or_else(|| kind.field(field).map(|spec| spec.description))
                .unwrap_or_default();
            (field.clone(), text.to_owned())
        })
        .collect()
}

fn expiry(row: &ItemRow) -> Result<Option<Expiry>> {
    row.expires_at
        .as_deref()
        .map(Expiry::parse)
        .transpose()
        .map_err(|_| anyhow!("invalid expiry in database"))
}

fn is_expired(row: &ItemRow, today: Date) -> Result<bool> {
    Ok(expiry(row)?.is_some_and(|e| e.is_expired(today)))
}

/// One item as agents may see it. A tampered item shows only its name: its origins, run
/// permission, expiry and descriptions are unverified and could have been planted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogEntry {
    Verified(ItemSummary),
    Tampered { name: String },
}

/// What agents may know about the vault (MCP `vault_list`): every item, verified or marked
/// tampered, never a value. Nothing while no vault exists; `open` opens it otherwise.
pub fn catalog(
    store: &dyn VaultStore,
    open: impl FnOnce() -> Result<Vault>,
) -> Result<Vec<CatalogEntry>> {
    if store.vault_get()?.is_none() {
        return Ok(Vec::new());
    }
    open()?.catalog()
}

impl Vault {
    /// Every item with its metadata MAC checked; see [`CatalogEntry`].
    pub fn catalog(&self) -> Result<Vec<CatalogEntry>> {
        let mut out = Vec::new();
        for row in self.store.item_list()? {
            if self.verified(&row).is_err() {
                out.push(CatalogEntry::Tampered { name: row.name });
                continue;
            }
            out.push(CatalogEntry::Verified(self.summary(row, false)?));
        }
        Ok(out)
    }

    /// Every item as the local user may see it, tampered ones included and marked.
    pub fn list(&self) -> Result<Vec<ItemSummary>> {
        let mut out = Vec::new();
        for row in self.store.item_list()? {
            let tampered = self.verified(&row).is_err();
            out.push(self.summary(row, tampered)?);
        }
        Ok(out)
    }

    fn summary(&self, row: ItemRow, tampered: bool) -> Result<ItemSummary> {
        let (expires_at, expired) = (expiry(&row)?, is_expired(&row, self.clock.today())?);
        let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
        let stored = self
            .store
            .item_get(&row.name)?
            .map(|(_, f)| f)
            .unwrap_or_default();
        let plain_values = if tampered {
            BTreeMap::new()
        } else {
            self.plain_values(&row, &stored)?
        };
        let fields: Vec<String> = stored.into_iter().map(|f| f.field).collect();
        let field_descriptions = effective_descriptions(kind, &fields, &row.field_descriptions);
        Ok(ItemSummary {
            name: row.name,
            kind,
            domain: kind.domain(),
            fields,
            origins: row.origins,
            origin_binding: row.origin_binding,
            run_allowed: row.run_allowed,
            expires_at,
            expired,
            description: row.description,
            field_descriptions,
            tags: row.tags.clone(),
            plain_values,
            status: ItemStatus::parse(&row.status).context("unknown status in database")?,
            source: ItemSource::parse(&row.source).context("unknown source in database")?,
            updated_at: row.updated_at,
            tampered,
        })
    }

    /// Whether the item is origin-bound, and its origins once its metadata verifies.
    pub fn item_origins_verified(&self, name: &ItemName) -> Result<(bool, Vec<Origin>)> {
        let (row, _) = self
            .store
            .item_get(name.as_str())?
            .context("item not found")?;
        let (_, _, origins) = self.verified(&row)?;
        Ok((row.origin_binding, origins))
    }

    /// `Ok(None)` if there is no such item; an error if its metadata fails verification.
    pub fn item_use(&self, name: &ItemName) -> Result<Option<ItemUse>> {
        let Some((row, fields)) = self.store.item_get(name.as_str())? else {
            return Ok(None);
        };
        let (kind, _, origins) = self.verified(&row)?;
        Ok(Some(ItemUse {
            kind,
            domain: kind.domain(),
            origin_binding: row.origin_binding,
            run_allowed: row.run_allowed,
            expires_at: expiry(&row)?,
            expired: is_expired(&row, self.clock.today())?,
            origins,
            status: ItemStatus::parse(&row.status).context("unknown status in database")?,
            fields: fields.into_iter().map(|f| f.field).collect(),
        }))
    }

    /// The vault's change counter; it moves on every write and key rotation.
    pub fn version(&self) -> Result<i64> {
        self.store.vault_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, init, login, open_store};

    #[test]
    fn item_use_reports_verified_metadata() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let used = v
            .item_use(&ItemName::parse("github").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(used.kind, ItemKind::Login);
        assert_eq!(used.domain, Domain::Credentials);
        assert!(used.origin_binding);
        assert_eq!(
            used.origins,
            vec![Origin::parse("https://github.com").unwrap()]
        );
        assert_eq!(used.status, ItemStatus::Active);
        assert_eq!(used.fields, vec!["password", "username"]);
        assert!(
            v.item_use(&ItemName::parse("nope").unwrap())
                .unwrap()
                .is_none()
        );
        let before = v.version().unwrap();
        v.remove(&ItemName::parse("github").unwrap()).unwrap();
        assert_eq!(v.version().unwrap(), before + 1);
    }

    #[test]
    fn the_catalog_shows_a_tampered_item_by_name_only() {
        let e = env();
        assert!(
            catalog(&open_store(&e.data), || unreachable!())
                .unwrap()
                .is_empty()
        );
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        v.add(login("gitlab", "pw")).unwrap();
        open_store(&e.data).execute_for_tests(
            "UPDATE vault_items SET description = 'planted', expires_at = 'x' WHERE name = 'github'",
        );
        let entries = v.catalog().unwrap();
        assert_eq!(
            entries[0],
            CatalogEntry::Tampered {
                name: "github".into()
            }
        );
        let CatalogEntry::Verified(gitlab) = &entries[1] else {
            panic!("{entries:?}")
        };
        assert_eq!((gitlab.name.as_str(), gitlab.tampered), ("gitlab", false));
    }
}

/// Which items a listing shows: those whose name starts with `prefix` (`myapp/` for every
/// item under it) and that carry `tag`, when given (developer secrets spec §2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemFilter {
    pub prefix: Option<String>,
    pub tag: Option<keyharbor_domain::tag::Tag>,
}

impl ItemFilter {
    pub fn keeps(&self, item: &super::ItemSummary) -> bool {
        self.prefix
            .as_deref()
            .is_none_or(|p| item.name.starts_with(p))
            && self
                .tag
                .as_ref()
                .is_none_or(|t| item.tags.iter().any(|x| x == t.as_str()))
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::{
        ItemDetails,
        tests::{env, init, login},
    };
    use keyharbor_domain::{tag::Tag, vault::ItemName};

    #[test]
    fn listings_filter_by_name_prefix_and_tag() {
        let e = env();
        let vault = init(&e);
        let tagged = |name: &str, tags: &[&str]| {
            let mut item = login(name, "pw");
            item.name = ItemName::parse(name).unwrap();
            item.details = ItemDetails {
                tags: tags.iter().map(|t| Tag::parse(t).unwrap()).collect(),
                ..Default::default()
            };
            item
        };
        vault.add(tagged("myapp/prod/db", &["prod", "db"])).unwrap();
        vault.add(tagged("myapp/dev/db", &["dev", "db"])).unwrap();
        vault.add(tagged("other", &[])).unwrap();
        let names = |filter: ItemFilter| -> Vec<String> {
            vault
                .list()
                .unwrap()
                .into_iter()
                .filter(|i| filter.keeps(i))
                .map(|i| i.name)
                .collect()
        };
        assert_eq!(names(ItemFilter::default()).len(), 3);
        let under = ItemFilter {
            prefix: Some("myapp/".into()),
            tag: None,
        };
        assert_eq!(names(under), ["myapp/dev/db", "myapp/prod/db"]);
        let prod = ItemFilter {
            prefix: None,
            tag: Some(Tag::parse("prod").unwrap()),
        };
        assert_eq!(names(prod), ["myapp/prod/db"]);
        let item = vault
            .list()
            .unwrap()
            .into_iter()
            .find(|i| i.name == "myapp/prod/db")
            .unwrap();
        assert_eq!(item.tags, ["db", "prod"]);
        assert!(!item.tampered);
    }

    #[test]
    fn tags_change_by_adding_and_removing_and_stay_signed() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw")).unwrap();
        let name = ItemName::parse("github").unwrap();
        let t = |s: &str| Tag::parse(s).unwrap();
        vault
            .edit(
                &name,
                crate::ItemChange {
                    add_tags: vec![t("work"), t("git")],
                    ..Default::default()
                },
            )
            .unwrap();
        vault
            .edit(
                &name,
                crate::ItemChange {
                    remove_tags: vec![t("git")],
                    ..Default::default()
                },
            )
            .unwrap();
        let item = vault.list().unwrap().remove(0);
        assert_eq!((item.tags, item.tampered), (vec!["work".to_owned()], false));
    }
}
