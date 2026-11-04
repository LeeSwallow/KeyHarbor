//! Credential templates (R-48): adding, listing and removing them, and finding one for a new
//! `credential` item. One implementation for the CLI and the app. A template holds no value,
//! so none of this weakens a protection or needs a person (J-281).

use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    ports::TemplateStore,
    vault::{CredentialTemplate, TemplateName},
};

/// Every template, by name.
pub fn list(store: &dyn TemplateStore) -> Result<Vec<CredentialTemplate>> {
    store.template_list()
}

/// Adds `template`; refused if one of that name exists (remove it first).
pub fn add(store: &dyn TemplateStore, template: &CredentialTemplate) -> Result<()> {
    if !store.template_add(template)? {
        bail!(
            "a template named {} already exists",
            template.name().as_str()
        );
    }
    Ok(())
}

/// Removes the template `name`. Items made from it keep their fields.
pub fn remove(store: &dyn TemplateStore, name: &TemplateName) -> Result<()> {
    if !store.template_remove(name)? {
        bail!("template not found");
    }
    Ok(())
}

/// The template `name`, for a new item.
pub fn find(store: &dyn TemplateStore, name: &TemplateName) -> Result<CredentialTemplate> {
    list(store)?
        .into_iter()
        .find(|t| t.name() == name)
        .context("template not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ItemDetails, ItemSource, ItemStatus, NewItem,
        tests::{env, field, init, open_store},
    };
    use keyharbor_domain::vault::{FieldName, ItemKind, ItemName, TemplateField};

    fn bank() -> CredentialTemplate {
        let f = |name: &str, secret| TemplateField {
            name: FieldName::parse(name).unwrap(),
            secret,
        };
        CredentialTemplate::new(
            TemplateName::parse("bank").unwrap(),
            vec![f("second_password", true), f("member_no", false)],
        )
        .unwrap()
    }

    #[test]
    fn a_template_is_added_once_and_removed() {
        let e = env();
        let _vault = init(&e);
        let store = open_store(&e.data);
        add(&store, &bank()).unwrap();
        let again = add(&store, &bank()).unwrap_err();
        assert!(again.to_string().contains("already exists"), "{again}");
        assert_eq!(list(&store).unwrap(), [bank()]);
        assert_eq!(find(&store, bank().name()).unwrap(), bank());
        remove(&store, bank().name()).unwrap();
        assert!(remove(&store, bank().name()).is_err());
        assert!(find(&store, bank().name()).is_err());
    }

    #[test]
    fn an_item_made_from_a_template_keeps_its_fields_after_the_template_goes() {
        let e = env();
        let vault = init(&e);
        let store = open_store(&e.data);
        add(&store, &bank()).unwrap();
        let template = find(&store, bank().name()).unwrap();
        vault
            .add(NewItem {
                name: ItemName::parse("hana").unwrap(),
                kind: ItemKind::Credential,
                fields: vec![
                    field("second_password", "2nd-SECRET-4821"),
                    field("member_no", "M-20261104"),
                ],
                origins: vec![],
                origin_binding: None,
                status: ItemStatus::Active,
                source: ItemSource::User,
                details: ItemDetails {
                    plain_fields: template.plain_fields(),
                    ..Default::default()
                },
            })
            .unwrap();
        remove(&store, bank().name()).unwrap();
        let item = vault.list().unwrap().remove(0);
        assert_eq!(item.fields, ["member_no", "second_password"]);
        assert_eq!(item.plain_values["member_no"], "M-20261104");
        assert!(!item.plain_values.contains_key("second_password"));
        let masked = vault.mask_values().unwrap().values;
        assert!(masked.iter().any(|(_, v)| v.as_str() == "2nd-SECRET-4821"));
        assert!(!masked.iter().any(|(_, v)| v.as_str() == "M-20261104"));
    }
}
