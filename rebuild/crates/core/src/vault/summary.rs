//! What callers may know about items without any value: summaries, verified origins and the
//! facts the guard checks before a use.

use super::{ItemSource, ItemStatus, ItemSummary, Vault};
use crate::today;
use anyhow::{Context, Result, anyhow};
use keyharbor_domain::{
    date::Expiry,
    ports::ItemRow,
    vault::{Domain, ItemKind, ItemName, Origin},
};

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

fn expiry(row: &ItemRow) -> Result<Option<Expiry>> {
    row.expires_at
        .as_deref()
        .map(Expiry::parse)
        .transpose()
        .map_err(|_| anyhow!("invalid expiry in database"))
}

fn is_expired(row: &ItemRow) -> Result<bool> {
    Ok(expiry(row)?.is_some_and(|e| e.is_expired(today())))
}

impl Vault {
    pub fn list(&self) -> Result<Vec<ItemSummary>> {
        let mut out = Vec::new();
        for row in self.store.item_list()? {
            let tampered = self.verified(&row).is_err();
            let (expires_at, expired) = (expiry(&row)?, is_expired(&row)?);
            let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
            let fields = self
                .store
                .item_get(&row.name)?
                .map(|(_, f)| f.into_iter().map(|f| f.field).collect())
                .unwrap_or_default();
            out.push(ItemSummary {
                name: row.name,
                kind,
                domain: kind.domain(),
                fields,
                origins: row.origins,
                origin_binding: row.origin_binding,
                run_allowed: row.run_allowed,
                expires_at,
                expired,
                status: ItemStatus::parse(&row.status).context("unknown status in database")?,
                source: ItemSource::parse(&row.source).context("unknown source in database")?,
                updated_at: row.updated_at,
                tampered,
            });
        }
        Ok(out)
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
            expired: is_expired(&row)?,
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
    use crate::tests::{env, init, login};

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
}
