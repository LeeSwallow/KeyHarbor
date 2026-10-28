//! `SettingsStore` for the config context.

use super::{models::NewSetting, schema::settings};
use crate::SqliteStore;
use anyhow::Result;
use diesel::{prelude::*, upsert::excluded};
use keyharbor_domain::{
    ports::{AUDIT_ENABLED, SettingsStore},
    settings::audit_enabled as audit_on,
};

fn get(conn: &mut SqliteConnection, key: &str) -> QueryResult<Option<String>> {
    settings::table
        .find(key)
        .select(settings::value)
        .first(conn)
        .optional()
}

/// Whether the usage record is on, by the registry's rule (`settings::audit_enabled`, J-73).
pub(crate) fn audit_enabled(conn: &mut SqliteConnection) -> QueryResult<bool> {
    Ok(audit_on(get(conn, AUDIT_ENABLED)?.as_deref()))
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
