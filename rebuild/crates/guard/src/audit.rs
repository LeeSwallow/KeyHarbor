//! What the guard writes to the usage record: who used or was refused which item, where.
//! Writing is best effort (J-40): a failed write never undoes or blocks what already happened.

use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    ports::AuditStore,
    vault::ItemName,
};

/// Records `event`, ignoring failures. `audit_enabled = false` is honoured by the store.
pub fn record(store: &dyn AuditStore, event: &AuditEvent) {
    let _ = store.record(event);
}

/// A refused use: the reason code only (`policy`, `origin`, `not_found`, …).
pub fn denied(
    agent: &str,
    session: Option<&str>,
    item: &ItemName,
    origin: Option<&str>,
    reason: &str,
) -> AuditEvent {
    let mut event = AuditEvent::new(agent, AuditKind::SecretDenied, reason)
        .item(item.as_str())
        .origin(origin);
    event.session_id = session.map(str::to_owned);
    event
}

/// The error an agent sees for a refusal: item name and reason, never a value.
pub fn denial_error(item: &ItemName, reason: &str) -> anyhow::Error {
    anyhow::anyhow!("denied: {} ({reason})", item.as_str())
}
