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

/// What the menu bar shows (09 §9.4.6): a status line, what waits for the person, and the
/// count beside the icon when something waits. Counts only.
#[derive(Debug, PartialEq, Eq)]
pub struct MenuText {
    pub status: String,
    pub waiting: String,
    /// Beside the icon; `None` when nothing waits.
    pub badge: Option<String>,
}

pub fn menu_text(o: &OverviewDto) -> MenuText {
    let waiting = o.requests_pending + o.drafts;
    MenuText {
        status: if !o.vault_ready {
            "KeyHarbor · 금고 없음".to_owned()
        } else if o.sessions_open == 0 {
            "KeyHarbor · 실행 중인 세션 없음".to_owned()
        } else {
            format!("KeyHarbor · 세션 {} 실행 중", o.sessions_open)
        },
        waiting: format!("확인 대기 {waiting}"),
        badge: (waiting > 0).then(|| waiting.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overview(sessions: usize, requests: usize, drafts: usize) -> OverviewDto {
        OverviewDto {
            vault_ready: true,
            sessions_open: sessions,
            requests_pending: requests,
            drafts,
        }
    }

    #[test]
    fn the_menu_counts_sessions_and_what_waits() {
        let busy = menu_text(&overview(2, 1, 2));
        assert_eq!(busy.status, "KeyHarbor · 세션 2 실행 중");
        assert_eq!(
            (busy.waiting.as_str(), busy.badge.as_deref()),
            ("확인 대기 3", Some("3"))
        );
        let idle = menu_text(&overview(0, 0, 0));
        assert_eq!(
            (idle.status.as_str(), idle.badge),
            ("KeyHarbor · 실행 중인 세션 없음", None)
        );
        let none = menu_text(&OverviewDto {
            vault_ready: false,
            ..overview(0, 0, 0)
        });
        assert_eq!(none.status, "KeyHarbor · 금고 없음");
    }
}
