//! Config context: key/value settings. Implements `SettingsStore`, and tells other contexts
//! whether the usage record is on.

mod models;
mod repo;
mod schema;

pub(crate) use repo::audit_enabled;
