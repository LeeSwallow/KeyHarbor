//! Fields marked not secret (developer secrets spec §2): `PORT=8081`, `SPRING_PROFILES_ACTIVE`.
//! Their values are shown in listings and left out of masking, so a short common value does
//! not blank out logs. Only fields the signed metadata (MAC v4) marks plain are opened here;
//! no secret value takes this path, so it needs no `PlaintextAccess`.

use super::{Vault, vault_err};
use anyhow::{Context, Result};
use keyharbor_domain::{
    ports::{FieldRow, ItemRow},
    vault::Domain,
};
use keyharbor_vault::record::{FieldAd, open_field};
use std::collections::BTreeMap;

impl Vault {
    /// The values of `row`'s plain fields. Call only for a row whose metadata verified.
    pub(super) fn plain_values(
        &self,
        row: &ItemRow,
        fields: &[FieldRow],
    ) -> Result<BTreeMap<String, String>> {
        let domain = Domain::from_byte(row.domain).context("unknown domain in database")?;
        let mut out = BTreeMap::new();
        for field in fields
            .iter()
            .filter(|f| row.plain_fields.contains(&f.field))
        {
            let ad = FieldAd {
                vault: &self.vault_id,
                domain,
                item: &row.id,
                field: &field.field,
            };
            let bytes =
                open_field(&self.domain_keys[&domain].1, &ad, &field.sealed).map_err(vault_err)?;
            let text = String::from_utf8(bytes.to_vec()).context("stored field is not UTF-8")?;
            out.insert(field.field.clone(), text);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ItemChange, ItemDetails, ItemSource, ItemStatus, NewItem,
        tests::{env, field, init, open_store},
        weaken::WeakeningOp,
    };
    use keyharbor_domain::vault::{FieldName, ItemKind, ItemName};

    fn app_env(plain: &[&str]) -> NewItem {
        NewItem {
            name: ItemName::parse("app").unwrap(),
            kind: ItemKind::Env,
            fields: vec![field("API_KEY", "key-SECRET-1234"), field("PORT", "8081")],
            origins: vec![],
            origin_binding: None,
            status: ItemStatus::Active,
            source: ItemSource::User,
            details: ItemDetails {
                plain_fields: plain.iter().map(|f| FieldName::parse(f).unwrap()).collect(),
                ..Default::default()
            },
        }
    }
    fn masked(vault: &crate::tests::TestVault) -> Vec<String> {
        vault
            .mask_values()
            .unwrap()
            .values
            .into_iter()
            .map(|(_, v)| v.to_string())
            .collect()
    }

    #[test]
    fn a_plain_field_is_listed_with_its_value_and_not_masked() {
        let e = env();
        let vault = init(&e);
        vault.add(app_env(&["PORT"])).unwrap();
        let item = vault.list().unwrap().remove(0);
        assert_eq!(
            item.plain_values.get("PORT").map(String::as_str),
            Some("8081")
        );
        assert!(!item.plain_values.contains_key("API_KEY"));
        let masked = masked(&vault);
        assert!(masked.iter().any(|v| v == "key-SECRET-1234"));
        assert!(!masked.iter().any(|v| v == "8081"));
        let mut bad = app_env(&["NOPE"]);
        bad.name = ItemName::parse("bad").unwrap();
        assert!(vault.add(bad).is_err(), "only fields the item has");
    }

    #[test]
    fn marking_a_field_plain_weakens_and_marking_it_secret_does_not() {
        let e = env();
        let vault = init(&e);
        vault.add(app_env(&[])).unwrap();
        let name = ItemName::parse("app").unwrap();
        let port = || vec![FieldName::parse("PORT").unwrap()];
        let to_plain = ItemChange {
            make_plain: port(),
            ..Default::default()
        };
        assert!(matches!(
            vault.weakenings(&name, &to_plain).unwrap().as_slice(),
            [WeakeningOp::FieldsPlain { fields, .. }] if fields == &["PORT"]
        ));
        vault.edit(&name, to_plain).unwrap();
        assert!(masked(&vault).iter().all(|v| v != "8081"));
        let to_secret = ItemChange {
            make_secret: port(),
            ..Default::default()
        };
        assert!(vault.weakenings(&name, &to_secret).unwrap().is_empty());
        vault.edit(&name, to_secret).unwrap();
        assert!(vault.list().unwrap()[0].plain_values.is_empty());
        assert!(masked(&vault).iter().any(|v| v == "8081"));
    }

    #[test]
    fn a_plain_mark_written_into_the_database_is_tampering_and_stays_masked() {
        let e = env();
        let vault = init(&e);
        vault.add(app_env(&[])).unwrap();
        open_store(&e.data).execute_for_tests("UPDATE vault_items SET plain_fields = 'API_KEY'");
        let item = vault.list().unwrap().remove(0);
        assert!(item.tampered);
        assert!(item.plain_values.is_empty());
        assert!(masked(&vault).iter().any(|v| v == "key-SECRET-1234"));
    }
}
