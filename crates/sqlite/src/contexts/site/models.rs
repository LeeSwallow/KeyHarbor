//! Rows of the site context as Diesel reads and writes them.

use super::schema::site_states;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = site_states, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct SiteModel {
    pub session_id: String,
    pub origin: String,
    pub state: String,
    pub item_name: Option<String>,
    pub checked_at: String,
    pub recorded_by: String,
}
