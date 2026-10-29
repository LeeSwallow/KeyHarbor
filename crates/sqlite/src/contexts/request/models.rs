//! Rows of the request context as Diesel reads and writes them.

use super::schema::vault_requests;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = vault_requests, check_for_backend(diesel::sqlite::Sqlite))]
pub struct RequestModel {
    pub id: String,
    pub agent: String,
    pub kind: String,
    pub name: String,
    pub reason: String,
    pub origin: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub item_name: Option<String>,
}
