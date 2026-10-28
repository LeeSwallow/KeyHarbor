//! Rows of the config context as Diesel writes them (reads take the value column only).

use super::schema::settings;
use diesel::prelude::*;

#[derive(Insertable)]
#[diesel(table_name = settings)]
pub struct NewSetting<'a> {
    pub key: &'a str,
    pub value: &'a str,
}
