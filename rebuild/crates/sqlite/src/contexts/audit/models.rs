//! Rows of the audit context as Diesel reads them.

use super::schema::audit;
use diesel::prelude::*;

#[derive(Queryable, Selectable)]
#[diesel(table_name = audit, check_for_backend(diesel::sqlite::Sqlite))]
pub struct AuditModel {
    pub seq: i64,
    pub at: String,
    pub agent: String,
    pub session_id: Option<String>,
    pub kind: String,
    pub item_name: Option<String>,
    pub origin: Option<String>,
    pub result: String,
}
