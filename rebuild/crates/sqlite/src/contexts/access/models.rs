//! Rows of the access context as Diesel reads and writes them.

use super::schema::policy_rules;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = policy_rules, check_for_backend(diesel::sqlite::Sqlite))]
pub struct RuleModel {
    pub agent: String,
    pub resource: String,
    pub target: String,
    pub effect: String,
}
