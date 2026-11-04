//! `TemplateStore` for the config context.

use super::{
    mappers::{from_row, to_row},
    models::TemplateRow,
    schema::credential_templates,
};
use crate::SqliteStore;
use anyhow::Result;
use diesel::prelude::*;
use keyharbor_domain::{
    ports::TemplateStore,
    vault::{CredentialTemplate, TemplateName},
};

impl TemplateStore for SqliteStore {
    fn template_list(&self) -> Result<Vec<CredentialTemplate>> {
        let rows: Vec<TemplateRow> = self.read(|conn| {
            Ok(credential_templates::table
                .order(credential_templates::name)
                .load(conn)?)
        })?;
        rows.into_iter().map(from_row).collect()
    }

    fn template_add(&self, template: &CredentialTemplate) -> Result<bool> {
        let row = to_row(template);
        self.write(|conn| {
            let added = diesel::insert_into(credential_templates::table)
                .values(&row)
                .on_conflict_do_nothing()
                .execute(conn)?;
            Ok(added == 1)
        })
    }

    fn template_remove(&self, name: &TemplateName) -> Result<bool> {
        self.write(|conn| {
            let removed =
                diesel::delete(credential_templates::table.find(name.as_str())).execute(conn)?;
            Ok(removed == 1)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::{
        ports::TemplateStore,
        vault::{CredentialTemplate, FieldName, TemplateField, TemplateName},
    };

    fn bank() -> CredentialTemplate {
        let field = |name: &str, secret| TemplateField {
            name: FieldName::parse(name).unwrap(),
            secret,
        };
        CredentialTemplate::new(
            TemplateName::parse("은행 2차").unwrap(),
            vec![field("second_password", true), field("member_no", false)],
        )
        .unwrap()
    }

    #[test]
    fn templates_are_added_once_listed_and_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        assert!(store.template_list().unwrap().is_empty());
        assert!(store.template_add(&bank()).unwrap());
        assert!(
            !store.template_add(&bank()).unwrap(),
            "a name is taken once"
        );
        assert_eq!(store.template_list().unwrap(), [bank()]);
        assert!(store.template_remove(bank().name()).unwrap());
        assert!(!store.template_remove(bank().name()).unwrap());
        assert!(store.template_list().unwrap().is_empty());
    }
}
