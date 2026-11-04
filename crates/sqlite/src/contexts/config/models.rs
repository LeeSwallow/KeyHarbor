//! Rows of the config context as Diesel writes them (reads take the value column only).

use super::schema::{credential_templates, settings};
use diesel::prelude::*;

#[derive(Insertable)]
#[diesel(table_name = settings)]
pub struct NewSetting<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

/// A credential template as stored: its fields as `name=secret name=plain …`.
#[derive(Insertable, Queryable)]
#[diesel(table_name = credential_templates)]
pub struct TemplateRow {
    pub name: String,
    pub fields: String,
}
