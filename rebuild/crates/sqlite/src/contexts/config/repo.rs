//! `SettingsStore` for the config context.

use super::{models::NewSetting, schema::settings};
use crate::SqliteStore;
use anyhow::Result;
use diesel::{prelude::*, upsert::excluded};
use keyharbor_domain::ports::{AUDIT_ENABLED, SettingsStore};

fn get(conn: &mut SqliteConnection, key: &str) -> QueryResult<Option<String>> {
    settings::table
        .find(key)
        .select(settings::value)
        .first(conn)
        .optional()
}

/// The usage record is on while `audit_enabled` is missing or exactly `true` (J-23).
pub(crate) fn audit_enabled(conn: &mut SqliteConnection) -> QueryResult<bool> {
    Ok(get(conn, AUDIT_ENABLED)?.as_deref().unwrap_or("true") == "true")
}

impl SettingsStore for SqliteStore {
    fn setting_get(&self, key: &str) -> Result<Option<String>> {
        self.read(|conn| Ok(get(conn, key)?))
    }

    fn setting_set(&self, key: &str, value: &str) -> Result<()> {
        self.read(|conn| {
            diesel::insert_into(settings::table)
                .values(NewSetting { key, value })
                .on_conflict(settings::key)
                .do_update()
                .set(settings::value.eq(excluded(settings::value)))
                .execute(conn)?;
            Ok(())
        })
    }
}
