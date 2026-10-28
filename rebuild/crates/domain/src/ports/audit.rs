//! The usage record and settings. Events never carry a value (02 §2.7.2).

use crate::audit::{AuditEvent, AuditRecord};
use anyhow::Result;

pub use crate::settings::AUDIT_ENABLED;

pub trait AuditStore {
    /// Appends the event unless `audit_enabled` is `false` (`settings::audit_enabled`; then
    /// nothing at all is kept, J-23).
    /// Returns whether it was written.
    fn record(&self, event: &AuditEvent) -> Result<bool>;
    /// The most recent `limit` events (all when `None`), oldest first.
    fn list_audit(&self, limit: Option<usize>) -> Result<Vec<AuditRecord>>;
}

/// Stores setting strings; what they mean is `crate::settings`.
pub trait SettingsStore {
    fn setting_get(&self, key: &str) -> Result<Option<String>>;
    fn setting_set(&self, key: &str, value: &str) -> Result<()>;
}
