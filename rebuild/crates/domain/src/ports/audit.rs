//! The usage record and settings. Events never carry a value (02 §2.7.2).

use crate::audit::{AuditEvent, AuditRecord};
use anyhow::Result;

/// The setting that turns the usage record off (`false`) or on (anything else, default).
pub const AUDIT_ENABLED: &str = "audit_enabled";

pub trait AuditStore {
    /// Appends the event unless `audit_enabled` is `false` (then nothing at all is kept, J-23).
    /// Returns whether it was written.
    fn record(&self, event: &AuditEvent) -> Result<bool>;
    /// The most recent `limit` events (all when `None`), oldest first.
    fn list_audit(&self, limit: Option<usize>) -> Result<Vec<AuditRecord>>;
}

pub trait SettingsStore {
    fn setting_get(&self, key: &str) -> Result<Option<String>>;
    fn setting_set(&self, key: &str, value: &str) -> Result<()>;
}
