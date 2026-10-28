//! Reading the usage record. Events never carry a value (02 §2.7.2).

use anyhow::Result;
use keyharbor_domain::{audit::AuditRecord, ports::AuditStore};

/// The most recent `limit` events (all when `None`), oldest first.
pub fn list(store: &dyn AuditStore, limit: Option<usize>) -> Result<Vec<AuditRecord>> {
    store.list_audit(limit)
}
