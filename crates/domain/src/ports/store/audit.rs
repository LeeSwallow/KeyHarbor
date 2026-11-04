//! The usage record. Events never carry a value (02 §2.7.2).

use super::StoreError;
use crate::audit::{AuditEvent, AuditRecord};

type Result<T> = std::result::Result<T, StoreError>;

pub trait AuditStore {
    /// Appends the event unless `audit_enabled` is `false` (`settings::audit_enabled`; then
    /// nothing at all is kept, J-23).
    /// Returns whether it was written.
    fn record(&self, event: &AuditEvent) -> Result<bool>;
    /// The most recent `limit` events (all when `None`), oldest first.
    fn list_audit(&self, limit: Option<usize>) -> Result<Vec<AuditRecord>>;
}
