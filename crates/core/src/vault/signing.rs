//! Sealing fields and signing metadata (07): the key generation a write is checked against,
//! unwrapping the domain keys, the metadata MAC and its verification. The MAC input
//! ([`ItemMeta`]) is the stored format; changing it is a format change (07 §7.5).

use super::{Vault, VaultBackend, vault_err};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::{FieldRow, ItemRow, KeyGuard},
    vault::{Domain, FieldName, ItemKind, ItemName, Origin},
};
use keyharbor_vault::{
    VaultError,
    keys::{KeyId, KeySet, SecretKey, VaultId},
    meta::{ItemMeta, meta_mac, verify_meta_mac},
    record::{FieldAd, seal_field, unwrap_domain_key},
};
use std::collections::BTreeMap;

impl Vault {
    /// The key generation this handle seals and signs with; stale handles are refused on write.
    pub(super) fn guard(&self, domain: Domain) -> Result<KeyGuard> {
        Ok(KeyGuard {
            kek_id: *self.kek_id.as_bytes(),
            domain: domain.as_byte(),
            dk_id: *self.domain_key(domain)?.0.as_bytes(),
        })
    }

    pub(crate) fn unwrap_all(
        store: &dyn VaultBackend,
        keys: &KeySet,
        vault_id: &VaultId,
    ) -> Result<BTreeMap<Domain, (KeyId, SecretKey)>> {
        let mut out = BTreeMap::new();
        for row in store.domain_keys()? {
            let domain = Domain::from_byte(row.domain).context("unknown domain in database")?;
            let dk_id = KeyId::from_bytes(row.dk_id);
            let dk = unwrap_domain_key(keys.kek(), vault_id, domain, &dk_id, &row.wrapped)
                .map_err(vault_err)
                .context("could not unwrap domain key")?;
            out.insert(domain, (dk_id, dk));
        }
        // A vault from before R-40 has no developer key yet; `open` adds it.
        if Domain::ALL
            .iter()
            .any(|d| *d != Domain::Developer && !out.contains_key(d))
        {
            bail!("domain keys are missing");
        }
        Ok(out)
    }

    /// The signed view of a stored row: its metadata in the domain the row is stored in.
    fn meta<'a>(
        &'a self,
        row: &'a ItemRow,
        domain: Domain,
        kind: ItemKind,
        name: &'a ItemName,
        origins: &'a [Origin],
    ) -> ItemMeta<'a> {
        ItemMeta {
            vault: &self.vault_id,
            item: &row.id,
            domain,
            kind,
            name,
            origin_binding: row.origin_binding,
            origins,
            status: &row.status,
            updated_at: &row.updated_at,
            run_allowed: row.run_allowed,
            expires_at: row.expires_at.as_deref(),
            description: row.description.as_deref(),
            field_descriptions: &row.field_descriptions,
            tags: &row.tags,
            plain_fields: &row.plain_fields,
            otp_allowed: row.otp_allowed,
        }
    }

    /// Signs `row` for the domain it is stored in (`row.domain`).
    pub(crate) fn mac_for(
        &self,
        row: &ItemRow,
        kind: ItemKind,
        name: &ItemName,
        origins: &[Origin],
    ) -> Vec<u8> {
        let domain = Domain::from_byte(row.domain).unwrap_or_else(|| kind.domain());
        meta_mac(
            self.keys.mac(),
            &self.meta(row, domain, kind, name, origins),
        )
        .to_vec()
    }

    pub(super) fn seal(
        &self,
        item: &[u8; 16],
        domain: Domain,
        field: &FieldName,
        value: &str,
    ) -> Result<FieldRow> {
        let (dk_id, dk) = self.domain_key(domain)?;
        let ad = FieldAd {
            vault: &self.vault_id,
            domain,
            item,
            field: field.as_str(),
        };
        Ok(FieldRow {
            field: field.as_str().to_owned(),
            sealed: seal_field(dk, dk_id, &ad, value.as_bytes()),
            previous: None,
        })
    }

    /// Decodes a stored row and checks its metadata MAC.
    pub(crate) fn verified(&self, row: &ItemRow) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        self.verified_with(&self.keys, row)
    }

    pub(crate) fn verified_with(
        &self,
        keys: &KeySet,
        row: &ItemRow,
    ) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
        if kind.domain().as_byte() != row.domain {
            return Err(vault_err(VaultError::Tampered));
        }
        self.verified_as_stored(keys, row)
    }

    /// Checks the MAC of `row` for the domain it is stored in, even if its kind now belongs
    /// to another one (an item written before R-40 moved secret and env items).
    pub(crate) fn verified_as_stored(
        &self,
        keys: &KeySet,
        row: &ItemRow,
    ) -> Result<(ItemKind, ItemName, Vec<Origin>)> {
        let kind = ItemKind::parse(&row.kind).context("unknown item kind in database")?;
        let domain = Domain::from_byte(row.domain).context("unknown domain in database")?;
        let name = ItemName::parse(&row.name)?;
        let origins = row
            .origins
            .iter()
            .map(|o| Origin::parse(o).map_err(anyhow::Error::from))
            .collect::<Result<Vec<_>>>()?;
        verify_meta_mac(
            keys.mac(),
            &self.meta(row, domain, kind, &name, &origins),
            &row.meta_mac,
        )
        .map_err(vault_err)?;
        Ok((kind, name, origins))
    }
}
