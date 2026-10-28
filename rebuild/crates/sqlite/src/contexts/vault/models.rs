//! Rows of the vault context as Diesel reads and writes them.

use super::schema::{
    domain_keys, item_field_descriptions, item_origins, vault, vault_fields, vault_items,
};
use diesel::prelude::*;

#[derive(Queryable, Selectable)]
#[diesel(table_name = vault, check_for_backend(diesel::sqlite::Sqlite))]
pub struct VaultModel {
    pub id: Vec<u8>,
    pub kek_id: Vec<u8>,
    pub version: i64,
}

#[derive(Insertable)]
#[diesel(table_name = vault)]
pub struct NewVault<'a> {
    pub id: &'a [u8],
    pub kek_id: &'a [u8],
    pub version: i64,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = domain_keys, check_for_backend(diesel::sqlite::Sqlite))]
pub struct DomainKeyModel {
    pub domain: i32,
    pub dk_id: Vec<u8>,
    pub wrapped: Vec<u8>,
}

/// `treat_none_as_null`: an update writes every column, so clearing an optional one (an
/// expiry, a description) clears it instead of leaving the old value.
#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = vault_items, check_for_backend(diesel::sqlite::Sqlite), treat_none_as_null = true)]
pub struct ItemModel {
    pub id: Vec<u8>,
    pub name: String,
    pub domain: i32,
    pub kind: String,
    pub origin_binding: bool,
    pub status: String,
    pub source: String,
    pub meta_mac: Vec<u8>,
    pub updated_at: String,
    pub run_allowed: bool,
    pub expires_at: Option<String>,
    pub description: Option<String>,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = vault_fields, check_for_backend(diesel::sqlite::Sqlite))]
pub struct FieldModel {
    pub item_id: Vec<u8>,
    pub field: String,
    pub sealed: Vec<u8>,
}

#[derive(Insertable)]
#[diesel(table_name = item_field_descriptions)]
pub struct NewFieldDescription<'a> {
    pub item_id: &'a [u8],
    pub field: &'a str,
    pub description: &'a str,
}

#[derive(Insertable)]
#[diesel(table_name = item_origins)]
pub struct NewOrigin<'a> {
    pub item_id: &'a [u8],
    pub origin: &'a str,
}
