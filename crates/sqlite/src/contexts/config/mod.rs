//! Config context: key/value settings and credential templates. Implements `SettingsStore`
//! and `TemplateStore`, and tells other contexts whether the usage record is on.

mod mappers;
mod models;
mod repo;
mod schema;
mod templates;

pub(crate) use repo::audit_enabled;
