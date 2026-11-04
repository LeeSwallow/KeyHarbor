//! macOS notifications (09 §9.3, U2): what newly waits for the person, denials, and sessions
//! that stopped substituting. The menu bar loop takes a [`snapshot`] every few seconds;
//! [`notices`] says what is new since the last one. Text names agents, items, sites and
//! sessions, never a value or an agent's free-text reason.

use crate::{
    Ctx,
    commands::{audit, handoffs, inbox},
};
use anyhow::Result;
use keyharbor_domain::{
    audit::AuditKind,
    session::substitution_lock::LockReason,
    settings::{
        NOTIFY_DENIALS, NOTIFY_DRAFTS, NOTIFY_HANDOFFS, NOTIFY_LOCKS, NOTIFY_REQUESTS,
        notify_enabled,
    },
};
use std::collections::{BTreeMap, BTreeSet};

/// More new things than this at once become one notice with the count.
const MAX_SEPARATE: usize = 3;
/// How far back the usage record is read for denials, each time.
const DENIAL_WINDOW: usize = 50;

/// What waits now, as the notices need it.
#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    /// (request id, agent, item name)
    pub requests: Vec<(String, String, String)>,
    /// (request id, session name, origin)
    pub handoffs: Vec<(String, String, Option<String>)>,
    pub drafts: Vec<String>,
    /// (at, agent, item or origin), newest first as the record reads.
    pub denials: Vec<(String, String, Option<String>)>,
    /// (record sequence number, session name, why), for each `substitution_locked` lock.
    pub locks: Vec<(i64, String, LockReason)>,
}

/// Which kinds the person wants notices for (settings `notify_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoticeKinds {
    pub requests: bool,
    pub handoffs: bool,
    pub drafts: bool,
    pub denials: bool,
    pub locks: bool,
}

/// What the last snapshot held. The first snapshot only fills it: what already waited when
/// the app started is on the home screen, not news.
#[derive(Debug, Default)]
pub struct Seen {
    primed: bool,
    waiting: BTreeSet<String>,
    last_denial: Option<String>,
    last_lock: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

impl Notice {
    fn new(title: String, body: String) -> Self {
        Self { title, body }
    }
}

pub fn snapshot(ctx: &Ctx) -> Result<Snapshot> {
    let inbox = inbox(ctx)?;
    // The same flag the usage record screen marks "거부" with (J-200).
    let denials = audit(ctx, DENIAL_WINDOW)?
        .into_iter()
        .filter(|e| e.denied)
        .map(|e| (e.at, e.agent, e.item.or(e.origin)))
        .collect();
    Ok(Snapshot {
        requests: inbox
            .requests
            .into_iter()
            .map(|r| (r.id, r.agent, r.name))
            .collect(),
        handoffs: handoffs(ctx)?
            .into_iter()
            .map(|h| (h.request_id, h.session_name, h.origin))
            .collect(),
        drafts: inbox.drafts.into_iter().map(|d| d.name).collect(),
        denials,
        locks: locks(ctx)?,
    })
}

/// The recent `substitution_locked` records that locked a session (not those that lifted a
/// lock), with the session's name.
fn locks(ctx: &Ctx) -> Result<Vec<(i64, String, LockReason)>> {
    let store = ctx.store()?;
    let names: BTreeMap<String, String> = store
        .list()?
        .into_iter()
        .map(|row| (row.id.to_string(), row.name.as_str().to_owned()))
        .collect();
    Ok(store
        .list_audit(Some(DENIAL_WINDOW))?
        .into_iter()
        .filter(|r| r.event.kind == AuditKind::SubstitutionLocked)
        .filter_map(|r| {
            let reason = LockReason::parse(&r.event.result)?;
            let session = r.event.session_id?;
            let name = names.get(&session).cloned().unwrap_or(session);
            Some((r.seq, name, reason))
        })
        .collect())
}

/// The `notify_*` settings, read by the domain's rule (`notify_enabled`).
pub fn notice_kinds(ctx: &Ctx) -> Result<NoticeKinds> {
    let store = ctx.store()?;
    let on = |key: &str| -> Result<bool> { Ok(notify_enabled(store.setting_get(key)?.as_deref())) };
    Ok(NoticeKinds {
        requests: on(NOTIFY_REQUESTS)?,
        handoffs: on(NOTIFY_HANDOFFS)?,
        drafts: on(NOTIFY_DRAFTS)?,
        denials: on(NOTIFY_DENIALS)?,
        locks: on(NOTIFY_LOCKS)?,
    })
}

/// What in `now` is new since `seen`, for the kinds that are on; `seen` becomes `now`.
pub fn notices(seen: &mut Seen, now: &Snapshot, kinds: NoticeKinds) -> Vec<Notice> {
    let mut out = waiting_notices(seen, now, kinds);
    out.extend(denial_notices(seen, now, kinds));
    out.extend(lock_notices(seen, now, kinds));
    seen.primed = true;
    collapse(out)
}

/// A notice for each request, handoff and draft not waiting last time; remembers what waits.
fn waiting_notices(seen: &mut Seen, now: &Snapshot, kinds: NoticeKinds) -> Vec<Notice> {
    // (key in `Seen::waiting`, whether its kind is on, the notice)
    let requests = now.requests.iter().map(|(id, agent, name)| {
        let text = Notice::new(
            format!("입력 요청 · {agent}"),
            format!("{name} — 확인 대기에서 답하세요"),
        );
        (format!("request:{id}"), kinds.requests, text)
    });
    let handoffs = now.handoffs.iter().map(|(id, session, origin)| {
        let place = origin.as_deref().unwrap_or("브라우저");
        let text = Notice::new(
            format!("브라우저 인계 · {session}"),
            format!("{place}에서 마치면 재개하세요"),
        );
        (format!("handoff:{id}"), kinds.handoffs, text)
    });
    let drafts = now.drafts.iter().map(|name| {
        let text = Notice::new(
            format!("새 draft · {name}"),
            "확인 대기에서 확정하거나 폐기하세요".to_owned(),
        );
        (format!("draft:{name}"), kinds.drafts, text)
    });
    let mut out = Vec::new();
    let mut keys = BTreeSet::new();
    for (key, on, text) in requests.chain(handoffs).chain(drafts) {
        if seen.primed && on && !seen.waiting.contains(&key) {
            out.push(text);
        }
        keys.insert(key);
    }
    seen.waiting = keys;
    out
}

/// A notice for each denial newer than the newest seen; remembers the newest.
fn denial_notices(seen: &mut Seen, now: &Snapshot, kinds: NoticeKinds) -> Vec<Notice> {
    let mut out = Vec::new();
    for (at, agent, target) in &now.denials {
        let newer = seen.last_denial.as_ref().is_none_or(|last| at > last);
        if seen.primed && kinds.denials && newer {
            out.push(Notice::new(
                format!("거부됨 · {agent}"),
                target.clone().unwrap_or_default(),
            ));
        }
    }
    let newest = now.denials.iter().map(|(at, ..)| at).max();
    if let Some(at) = newest.filter(|at| seen.last_denial.as_ref() < Some(*at)) {
        seen.last_denial = Some(at.clone());
    }
    out
}

/// Why a session stopped, in the person's words. No value, no item name (I1 records none).
fn lock_words(reason: LockReason) -> &'static str {
    match reason {
        LockReason::MaskProbe => "금고 값이 든 요청이 세 번 와서 값 넣기를 멈췄습니다",
        LockReason::FocusMoved => "값을 넣는 중에 페이지가 포커스를 옮겨 값 넣기를 멈췄습니다",
    }
}

/// A notice for each lock newer than the newest seen; remembers the newest.
fn lock_notices(seen: &mut Seen, now: &Snapshot, kinds: NoticeKinds) -> Vec<Notice> {
    let mut out = Vec::new();
    for (seq, session, reason) in &now.locks {
        let newer = seen.last_lock.is_none_or(|last| *seq > last);
        if seen.primed && kinds.locks && newer {
            out.push(Notice::new(
                format!("치환 잠김 · {session}"),
                format!(
                    "{} — 세션 화면에서 다시 허용할 수 있습니다",
                    lock_words(*reason)
                ),
            ));
        }
    }
    if let Some(newest) = now.locks.iter().map(|(seq, ..)| *seq).max() {
        seen.last_lock = seen.last_lock.max(Some(newest));
    }
    out
}

/// More than [`MAX_SEPARATE`] at once become one notice with the count.
fn collapse(out: Vec<Notice>) -> Vec<Notice> {
    if out.len() > MAX_SEPARATE {
        return vec![Notice::new(
            "KeyHarbor".into(),
            format!("확인할 것 {}개가 새로 생겼습니다", out.len()),
        )];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: NoticeKinds = NoticeKinds {
        requests: true,
        handoffs: true,
        drafts: true,
        denials: true,
        locks: true,
    };

    fn request(id: &str) -> (String, String, String) {
        (id.into(), "codex".into(), "github".into())
    }
    fn denial(at: &str) -> (String, String, Option<String>) {
        (at.into(), "codex".into(), Some("bank".into()))
    }

    #[test]
    fn what_waited_at_start_is_not_news() {
        let mut seen = Seen::default();
        let start = Snapshot {
            requests: vec![request("r1")],
            drafts: vec!["~draft-a".into()],
            denials: vec![denial("2026-10-29T01:00:00Z")],
            ..Default::default()
        };
        assert!(notices(&mut seen, &start, ALL).is_empty());
        assert!(
            notices(&mut seen, &start, ALL).is_empty(),
            "nor the second time"
        );
    }

    #[test]
    fn a_new_request_handoff_and_draft_each_get_one_notice() {
        let mut seen = Seen::default();
        notices(&mut seen, &Snapshot::default(), ALL);
        let now = Snapshot {
            requests: vec![request("r1")],
            handoffs: vec![(
                "r2".into(),
                "work".into(),
                Some("https://bank.example".into()),
            )],
            drafts: vec!["~draft-a".into()],
            ..Default::default()
        };
        let notice = |title: &str, body: &str| Notice {
            title: title.into(),
            body: body.into(),
        };
        assert_eq!(
            notices(&mut seen, &now, ALL),
            [
                notice("입력 요청 · codex", "github — 확인 대기에서 답하세요"),
                notice(
                    "브라우저 인계 · work",
                    "https://bank.example에서 마치면 재개하세요"
                ),
                notice("새 draft · ~draft-a", "확인 대기에서 확정하거나 폐기하세요"),
            ]
        );
        assert!(notices(&mut seen, &now, ALL).is_empty(), "each once");
    }

    #[test]
    fn many_at_once_become_one_count() {
        let mut seen = Seen::default();
        notices(&mut seen, &Snapshot::default(), ALL);
        let now = Snapshot {
            requests: (1..=5).map(|i| request(&format!("r{i}"))).collect(),
            ..Default::default()
        };
        assert_eq!(
            notices(&mut seen, &now, ALL),
            [Notice {
                title: "KeyHarbor".into(),
                body: "확인할 것 5개가 새로 생겼습니다".into()
            }]
        );
    }

    #[test]
    fn a_kind_turned_off_gives_no_notice_and_only_newer_denials_count() {
        let mut seen = Seen::default();
        let old = Snapshot {
            denials: vec![denial("2026-10-29T01:00:00Z")],
            ..Default::default()
        };
        notices(&mut seen, &old, ALL);
        let now = Snapshot {
            drafts: vec!["~draft-b".into()],
            denials: vec![
                denial("2026-10-29T01:05:00Z"),
                denial("2026-10-29T01:00:00Z"),
            ],
            ..Default::default()
        };
        let quiet = NoticeKinds {
            drafts: false,
            ..ALL
        };
        assert_eq!(
            notices(&mut seen, &now, quiet),
            [Notice {
                title: "거부됨 · codex".into(),
                body: "bank".into()
            }]
        );
    }

    #[test]
    fn a_locked_session_says_why_in_words_and_only_once() {
        let mut seen = Seen::default();
        let old = Snapshot {
            locks: vec![(1, "work".into(), LockReason::MaskProbe)],
            ..Default::default()
        };
        assert!(
            notices(&mut seen, &old, ALL).is_empty(),
            "not news at start"
        );
        let now = Snapshot {
            locks: vec![
                (1, "work".into(), LockReason::MaskProbe),
                (7, "jobs".into(), LockReason::FocusMoved),
            ],
            ..Default::default()
        };
        assert_eq!(
            notices(&mut seen, &now, ALL),
            [Notice {
                title: "치환 잠김 · jobs".into(),
                body: "값을 넣는 중에 페이지가 포커스를 옮겨 값 넣기를 멈췄습니다 — 세션 화면에서 다시 허용할 수 있습니다".into()
            }]
        );
        assert!(notices(&mut seen, &now, ALL).is_empty(), "each once");
        let quiet = NoticeKinds {
            locks: false,
            ..ALL
        };
        let later = Snapshot {
            locks: vec![(9, "work".into(), LockReason::MaskProbe)],
            ..Default::default()
        };
        assert!(notices(&mut seen, &later, quiet).is_empty());
        for reason in LockReason::ALL {
            assert!(!lock_words(reason).is_empty());
        }
    }

    #[test]
    fn a_request_that_leaves_and_returns_is_news_again() {
        let mut seen = Seen::default();
        notices(&mut seen, &Snapshot::default(), ALL);
        let one = Snapshot {
            requests: vec![request("r1")],
            ..Default::default()
        };
        assert_eq!(notices(&mut seen, &one, ALL).len(), 1);
        notices(&mut seen, &Snapshot::default(), ALL);
        assert_eq!(notices(&mut seen, &one, ALL).len(), 1);
    }
}
