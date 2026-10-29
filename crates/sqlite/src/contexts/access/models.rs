//! Rows of the access context as Diesel reads and writes them.

use super::schema::{grants, policy_rules};
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = policy_rules, check_for_backend(diesel::sqlite::Sqlite))]
pub struct RuleModel {
    pub agent: String,
    pub resource: String,
    pub target: String,
    pub effect: String,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = grants, check_for_backend(diesel::sqlite::Sqlite))]
pub struct GrantModel {
    pub id: String,
    pub subject_kind: String,
    pub subject: String,
    pub scope_kind: String,
    pub scope: String,
    pub until: Option<i64>,
    pub created_by: String,
    pub reason: Option<String>,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub cleared: bool,
}
