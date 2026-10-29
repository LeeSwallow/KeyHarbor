//! The home screen (09 §9.4.1): counts, and the recent usage record.

use crate::Ctx;
use anyhow::Result;
use keyharbor_core::status;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct OverviewDto {
    pub vault_ready: bool,
    pub sessions_open: usize,
    pub requests_pending: usize,
    pub drafts: usize,
}

pub fn overview(ctx: &Ctx) -> Result<OverviewDto> {
    let s = status::status(&*ctx.store()?, &*ctx.runtime(), ctx.clock())?;
    Ok(OverviewDto {
        vault_ready: s.vault_ready,
        sessions_open: s.sessions_open,
        requests_pending: s.requests_pending,
        drafts: s.drafts,
    })
}

/// One usage event: who, what, where, the outcome. Never a value.
#[derive(Debug, Serialize)]
pub struct AuditDto {
    pub at: String,
    pub agent: String,
    pub kind: String,
    pub item: Option<String>,
    pub origin: Option<String>,
    pub result: String,
}

/// The most recent `limit` events, newest first.
pub fn audit(ctx: &Ctx, limit: usize) -> Result<Vec<AuditDto>> {
    let mut events = ctx.store()?.list_audit(Some(limit))?;
    events.reverse();
    Ok(events
        .into_iter()
        .map(|r| AuditDto {
            at: r.at,
            agent: r.event.agent,
            kind: r.event.kind.as_str().to_owned(),
            item: r.event.item_name,
            origin: r.event.origin,
            result: r.event.result,
        })
        .collect())
}
