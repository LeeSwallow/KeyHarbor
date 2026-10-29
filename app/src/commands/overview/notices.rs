//! macOS notifications (09 §9.3, U2): what newly waits for the person, and denials. The menu
//! bar loop takes a [`snapshot`] every few seconds; [`notices`] says what is new since the last
//! one. Text names agents, items and sites, never a value or an agent's free-text reason.

use crate::Ctx;
use anyhow::Result;
use keyharbor_domain::settings::{NOTIFY_DENIALS, NOTIFY_DRAFTS, NOTIFY_HANDOFFS, NOTIFY_REQUESTS};
use std::collections::BTreeSet;

/// More new things than this at once become one notice with the count.
const MAX_SEPARATE: usize = 3;
/// The usage record kinds that count as a denial for a notice.
const DENIAL_KINDS: &[&str] = &["secret_denied", "navigation_denied"];
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
}

/// Which kinds the person wants notices for (settings `notify_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoticeKinds {
    pub requests: bool,
    pub handoffs: bool,
    pub drafts: bool,
    pub denials: bool,
}

/// What the last snapshot held. The first snapshot only fills it: what already waited when
/// the app started is on the home screen, not news.
#[derive(Debug, Default)]
pub struct Seen {
    primed: bool,
    waiting: BTreeSet<String>,
    last_denial: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

pub fn snapshot(ctx: &Ctx) -> Result<Snapshot> {
    let inbox = super::super::inbox(ctx)?;
    let denials = super::audit(ctx, DENIAL_WINDOW)?
        .into_iter()
        .filter(|e| DENIAL_KINDS.contains(&e.kind.as_str()))
        .map(|e| (e.at, e.agent, e.item.or(e.origin)))
        .collect();
    Ok(Snapshot {
        requests: inbox
            .requests
            .into_iter()
            .map(|r| (r.id, r.agent, r.name))
            .collect(),
        handoffs: super::super::handoffs(ctx)?
            .into_iter()
            .map(|h| (h.request_id, h.session_name, h.origin))
            .collect(),
        drafts: inbox.drafts.into_iter().map(|d| d.name).collect(),
        denials,
    })
}

/// The `notify_*` settings; each is on unless set to `false`.
pub fn notice_kinds(ctx: &Ctx) -> Result<NoticeKinds> {
    let store = ctx.store()?;
    let on =
        |key: &str| -> Result<bool> { Ok(store.setting_get(key)?.as_deref() != Some("false")) };
    Ok(NoticeKinds {
        requests: on(NOTIFY_REQUESTS)?,
        handoffs: on(NOTIFY_HANDOFFS)?,
        drafts: on(NOTIFY_DRAFTS)?,
        denials: on(NOTIFY_DENIALS)?,
    })
}

/// What in `now` is new since `seen`, for the kinds that are on; `seen` becomes `now`.
pub fn notices(seen: &mut Seen, now: &Snapshot, kinds: NoticeKinds) -> Vec<Notice> {
    let notice = |title: String, body: String| (title, body);
    // (key in `Seen::waiting`, whether its kind is on, the notice)
    let waiting = now
        .requests
        .iter()
        .map(|(id, agent, name)| {
            let text = notice(
                format!("입력 요청 · {agent}"),
                format!("{name} — 확인 대기에서 답하세요"),
            );
            (format!("request:{id}"), kinds.requests, text)
        })
        .chain(now.handoffs.iter().map(|(id, session, origin)| {
            let place = origin.as_deref().unwrap_or("브라우저");
            let text = notice(
                format!("브라우저 인계 · {session}"),
                format!("{place}에서 마치면 재개하세요"),
            );
            (format!("handoff:{id}"), kinds.handoffs, text)
        }))
        .chain(now.drafts.iter().map(|name| {
            let text = notice(
                format!("새 draft · {name}"),
                "확인 대기에서 확정하거나 폐기하세요".to_owned(),
            );
            (format!("draft:{name}"), kinds.drafts, text)
        }));
    let mut out = Vec::new();
    let mut keys = BTreeSet::new();
    for (key, on, text) in waiting {
        if seen.primed && on && !seen.waiting.contains(&key) {
            out.push(text);
        }
        keys.insert(key);
    }
    for (at, agent, target) in &now.denials {
        let newer = seen.last_denial.as_ref().is_none_or(|last| at > last);
        if seen.primed && kinds.denials && newer {
            out.push(notice(
                format!("거부됨 · {agent}"),
                target.clone().unwrap_or_default(),
            ));
        }
    }
    let newest = now.denials.iter().map(|(at, ..)| at).max();
    if let Some(at) = newest.filter(|at| seen.last_denial.as_ref() < Some(*at)) {
        seen.last_denial = Some(at.clone());
    }
    seen.waiting = keys;
    seen.primed = true;
    if out.len() > MAX_SEPARATE {
        return vec![Notice {
            title: "KeyHarbor".into(),
            body: format!("확인할 것 {}개가 새로 생겼습니다", out.len()),
        }];
    }
    out.into_iter()
        .map(|(title, body)| Notice { title, body })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: NoticeKinds = NoticeKinds {
        requests: true,
        handoffs: true,
        drafts: true,
        denials: true,
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
