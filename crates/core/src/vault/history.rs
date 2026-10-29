//! A field's previous value (developer secrets spec §2): an edit keeps the replaced record,
//! and `revert` swaps it back. Records are swapped as they are, sealed under the same key
//! and AD, so no value is opened. Only one step is kept; there is no rotation of values.

use super::Vault;
use crate::LOCAL_AGENT;
use anyhow::{Context, Result, bail};
use keyharbor_domain::vault::{FieldName, ItemName};

impl Vault {
    /// Swaps each named field (every field that has one, when `fields` is empty) with its
    /// previous value; returns the fields swapped. Reverting twice restores the change.
    pub fn revert(&self, name: &ItemName, fields: &[FieldName]) -> Result<Vec<String>> {
        let (mut row, mut stored) = self
            .store
            .item_get(name.as_str())?
            .context("item not found")?;
        let (kind, name, origins) = self.verified(&row)?;
        let wanted = |field: &str| fields.is_empty() || fields.iter().any(|f| f.as_str() == field);
        let mut swapped = Vec::new();
        for field in stored.iter_mut().filter(|f| wanted(&f.field)) {
            if let Some(previous) = field.previous.take() {
                field.previous = Some(std::mem::replace(&mut field.sealed, previous));
                swapped.push(field.field.clone());
            }
        }
        if swapped.is_empty() {
            bail!("{} has no previous value to go back to", name.as_str());
        }
        row.updated_at = self.clock.now_rfc3339();
        row.meta_mac = self.mac_for(&row, kind, &name, &origins);
        self.store
            .item_update(&self.guard(kind.domain()), &row, &stored)?;
        self.note_change_by(LOCAL_AGENT, &name, "reverted");
        Ok(swapped)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ItemChange,
        tests::{env, field, init, login, pref},
    };
    use keyharbor_domain::vault::{FieldName, ItemName};

    fn change_password(value: &str) -> ItemChange {
        ItemChange {
            set_fields: vec![field("password", value)],
            ..Default::default()
        }
    }

    #[test]
    fn an_edit_keeps_one_previous_value_and_revert_swaps_it_back() {
        let e = env();
        let vault = init(&e);
        let name = ItemName::parse("github").unwrap();
        vault.add(login("github", "first")).unwrap();
        assert!(
            vault.revert(&name, &[]).is_err(),
            "nothing to go back to yet"
        );
        vault.edit(&name, change_password("second")).unwrap();
        vault.edit(&name, change_password("third")).unwrap();
        assert_eq!(vault.revert(&name, &[]).unwrap(), ["password"]);
        assert_eq!(
            *vault.resolve(&pref("github", "password")).unwrap(),
            "second"
        );
        vault.revert(&name, &[]).unwrap();
        assert_eq!(
            *vault.resolve(&pref("github", "password")).unwrap(),
            "third"
        );
        let username = [FieldName::parse("username").unwrap()];
        assert!(
            vault.revert(&name, &username).is_err(),
            "username never changed"
        );
        assert!(!vault.list().unwrap()[0].tampered);
    }

    #[test]
    fn previous_values_survive_key_rotation_and_are_masked() {
        let e = env();
        let vault = init(&e);
        let name = ItemName::parse("github").unwrap();
        vault.add(login("github", "old-password-1234")).unwrap();
        vault
            .edit(&name, change_password("new-password-5678"))
            .unwrap();
        let masked = vault.mask_values().unwrap();
        let values: Vec<&str> = masked.values.iter().map(|(_, v)| v.as_str()).collect();
        assert!(
            values.contains(&"old-password-1234"),
            "the old value is still a secret"
        );
        let mut vault = vault;
        vault
            .rotate_domain(keyharbor_domain::vault::Domain::Credentials)
            .unwrap();
        vault.revert(&name, &[]).unwrap();
        assert_eq!(
            *vault.resolve(&pref("github", "password")).unwrap(),
            "old-password-1234"
        );
    }
}
