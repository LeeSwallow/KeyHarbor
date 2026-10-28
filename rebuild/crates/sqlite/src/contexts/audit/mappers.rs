//! Audit rows as stored ↔ `AuditRecord`.

use super::models::AuditModel;
use anyhow::{Context, Result};
use keyharbor_domain::audit::{AuditEvent, AuditKind, AuditRecord};

pub fn record(model: AuditModel) -> Result<AuditRecord> {
    Ok(AuditRecord {
        seq: model.seq,
        at: model.at,
        event: AuditEvent {
            agent: model.agent,
            session_id: model.session_id,
            kind: AuditKind::parse(&model.kind).context("invalid stored audit kind")?,
            item_name: model.item_name,
            origin: model.origin,
            result: model.result,
        },
    })
}
