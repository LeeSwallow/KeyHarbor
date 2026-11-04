//! Reading the usage record. Events never carry a value (02 §2.7.2).

use anyhow::Result;
use keyharbor_domain::{
    audit::{AuditKind, AuditRecord, Lookback},
    date,
    denial::StopCause,
    ports::{AuditStore, Clock},
};
use std::{cmp::Reverse, collections::HashMap, collections::HashSet};

/// The most recent `limit` events (all when `None`), oldest first.
pub fn list(store: &dyn AuditStore, limit: Option<usize>) -> Result<Vec<AuditRecord>> {
    Ok(store.list_audit(limit)?)
}

/// Which part of the record [`stops`] reads.
#[derive(Clone, Debug, Default)]
pub struct StopFilter {
    /// Only this session's events (its id, as the record keeps it).
    pub session: Option<String>,
    /// Only events this recent.
    pub since: Option<Lookback>,
}

/// Stops of one cause by one agent in one session at one item and origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopRow {
    pub agent: String,
    pub session: Option<String>,
    pub item: Option<String>,
    pub origin: Option<String>,
    pub count: u64,
    /// The latest of them, RFC 3339 UTC.
    pub last_at: String,
}

/// Every stop of one cause, most frequent row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopGroup {
    pub cause: StopCause,
    pub count: u64,
    pub last_at: String,
    pub rows: Vec<StopRow>,
}

type RowKey = (String, Option<String>, Option<String>, Option<String>);

/// Why agents stopped (J-240): refusals, locks and handoffs still waiting in the filtered
/// record, counted by cause and then by agent, session, item and origin. Causes and rows come most
/// frequent first, then most recent, then by name. Names only; the record has no values.
pub fn stops(
    store: &dyn AuditStore,
    clock: &dyn Clock,
    filter: &StopFilter,
) -> Result<Vec<StopGroup>> {
    let cutoff = filter
        .since
        .map(|since| date::rfc3339(clock.unix_seconds().saturating_sub(since.seconds())));
    let records: Vec<AuditRecord> = store
        .list_audit(None)?
        .into_iter()
        .filter(|r| cutoff.as_deref().is_none_or(|c| r.at.as_str() >= c))
        .filter(|r| {
            filter
                .session
                .as_deref()
                .is_none_or(|s| r.event.session_id.as_deref() == Some(s))
        })
        .collect();
    let mut counts: HashMap<StopCause, HashMap<RowKey, (u64, String)>> = HashMap::new();
    for (record, cause, weight) in still_stopped(&records) {
        let e = &record.event;
        let key = (
            e.agent.clone(),
            e.session_id.clone(),
            e.item_name.clone(),
            e.origin.clone(),
        );
        let entry = counts
            .entry(cause)
            .or_default()
            .entry(key)
            .or_insert((0, String::new()));
        entry.0 += weight;
        entry.1 = entry.1.clone().max(record.at.clone());
    }
    let mut groups: Vec<StopGroup> = counts.into_iter().map(group).collect();
    groups.sort_by(|a, b| {
        (Reverse(a.count), Reverse(&a.last_at), a.cause.as_str()).cmp(&(
            Reverse(b.count),
            Reverse(&b.last_at),
            b.cause.as_str(),
        ))
    });
    Ok(groups)
}

/// The stops in `records` (oldest first), leaving out handoff requests that were answered
/// or whose session stopped afterwards.
fn still_stopped(records: &[AuditRecord]) -> Vec<(&AuditRecord, StopCause, u64)> {
    let mut settled_later: HashSet<Option<&str>> = HashSet::new();
    let mut found = Vec::new();
    for record in records.iter().rev() {
        let session = record.event.session_id.as_deref();
        match record.event.kind {
            AuditKind::HandoffResumed | AuditKind::SessionStopped | AuditKind::SessionRemoved => {
                settled_later.insert(session);
            }
            AuditKind::HandoffRequested if settled_later.contains(&session) => continue,
            _ => {}
        }
        if let Some((cause, weight)) = record.event.stop() {
            found.push((record, cause, weight));
        }
    }
    found.reverse();
    found
}

fn group((cause, rows): (StopCause, HashMap<RowKey, (u64, String)>)) -> StopGroup {
    let mut rows: Vec<StopRow> = rows
        .into_iter()
        .map(
            |((agent, session, item, origin), (count, last_at))| StopRow {
                agent,
                session,
                item,
                origin,
                count,
                last_at,
            },
        )
        .collect();
    rows.sort_by(|a, b| {
        let key = |r: &StopRow| {
            (
                Reverse(r.count),
                Reverse(r.last_at.clone()),
                (r.agent.clone(), r.session.clone()),
                (r.item.clone(), r.origin.clone()),
            )
        };
        key(a).cmp(&key(b))
    });
    StopGroup {
        cause,
        count: rows.iter().map(|r| r.count).sum(),
        last_at: rows
            .iter()
            .map(|r| r.last_at.clone())
            .max()
            .unwrap_or_default(),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::{
        audit::AuditEvent,
        denial::DenialReason,
        ports::{ManualClock, StoreError},
    };

    /// The record as a list, each event at the given UTC time.
    struct Record(Vec<AuditRecord>);
    impl AuditStore for Record {
        fn record(&self, _: &AuditEvent) -> Result<bool, StoreError> {
            unreachable!("stops only reads")
        }
        fn list_audit(&self, _: Option<usize>) -> Result<Vec<AuditRecord>, StoreError> {
            Ok(self.0.clone())
        }
    }

    fn at(seconds: u64, event: AuditEvent) -> AuditRecord {
        AuditRecord {
            seq: i64::try_from(seconds).unwrap(),
            at: date::rfc3339(seconds),
            event,
        }
    }
    fn denied(agent: &str, reason: DenialReason, item: &str) -> AuditEvent {
        AuditEvent::new(agent, AuditKind::SecretDenied, reason.as_str())
            .session("s1")
            .item(item)
    }
    fn clock(seconds: u64) -> ManualClock {
        ManualClock::at_unix_seconds(seconds)
    }
    fn summary(groups: &[StopGroup]) -> Vec<(&str, u64)> {
        groups.iter().map(|g| (g.cause.as_str(), g.count)).collect()
    }

    #[test]
    fn stops_are_grouped_by_cause_then_counted_and_sorted() {
        let store = Record(vec![
            at(10, denied("codex", DenialReason::Origin, "gh")),
            at(20, denied("codex", DenialReason::Policy, "bank")),
            at(30, denied("claude", DenialReason::Origin, "gh")),
            at(40, denied("codex", DenialReason::Origin, "gh")),
            at(
                50,
                AuditEvent::new("session", AuditKind::NavigationDenied, "out-of-grant")
                    .session("s1")
                    .origin(Some("https://evil.example")),
            ),
            at(
                60,
                AuditEvent::new("codex", AuditKind::SecretUsed, "ok").session("s1"),
            ),
        ]);
        let groups = stops(&store, &clock(100), &StopFilter::default()).unwrap();
        // Equal counts: the more recent cause comes first.
        assert_eq!(
            summary(&groups),
            [("origin", 3), ("out-of-grant", 1), ("policy", 1)]
        );
        let origin = &groups[0];
        assert_eq!(origin.last_at, date::rfc3339(40));
        let rows: Vec<(&str, u64, &str)> = origin
            .rows
            .iter()
            .map(|r| (r.agent.as_str(), r.count, r.last_at.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("codex", 2, date::rfc3339(40).as_str()),
                ("claude", 1, date::rfc3339(30).as_str())
            ]
        );
        assert_eq!(origin.rows[0].item.as_deref(), Some("gh"));
        assert_eq!(
            groups[1].rows[0].origin.as_deref(),
            Some("https://evil.example")
        );
    }

    #[test]
    fn suppressed_refusals_count_each_and_locks_are_stops() {
        let store = Record(vec![
            at(
                10,
                AuditEvent::new("codex", AuditKind::DeniedSuppressed, "15").session("s1"),
            ),
            at(
                11,
                AuditEvent::new("codex", AuditKind::SubstitutionLocked, "focus_moved")
                    .session("s1"),
            ),
            at(12, denied("codex", DenialReason::Locked, "gh")),
        ]);
        let groups = stops(&store, &clock(100), &StopFilter::default()).unwrap();
        assert_eq!(
            summary(&groups),
            [("suppressed", 15), ("locked", 1), ("session-locked", 1)]
        );
    }

    #[test]
    fn only_handoffs_still_waiting_are_stops() {
        let handoff = |session: &str| {
            AuditEvent::new("claude", AuditKind::HandoffRequested, "awaiting_user").session(session)
        };
        let after = |kind, session: &str| AuditEvent::new("claude", kind, "ok").session(session);
        let store = Record(vec![
            at(10, handoff("answered")),
            at(11, after(AuditKind::HandoffResumed, "answered")),
            at(12, handoff("stopped")),
            at(13, after(AuditKind::SessionStopped, "stopped")),
            at(14, handoff("waiting")),
            at(15, after(AuditKind::HandoffResumed, "again")),
            at(16, handoff("again")),
        ]);
        let groups = stops(&store, &clock(100), &StopFilter::default()).unwrap();
        assert_eq!(summary(&groups), [("awaiting-handoff", 2)]);
        let sessions: Vec<_> = groups[0]
            .rows
            .iter()
            .map(|r| r.session.as_deref().unwrap())
            .collect();
        assert_eq!(sessions, ["again", "waiting"]);
    }

    #[test]
    fn the_filter_keeps_one_session_and_a_recent_period() {
        let store = Record(vec![
            at(10, denied("codex", DenialReason::Origin, "gh")),
            at(
                7_000,
                AuditEvent::new("codex", AuditKind::SecretDenied, "policy")
                    .session("s2")
                    .item("gh"),
            ),
            at(7_100, denied("codex", DenialReason::Expired, "gh")),
        ]);
        let recent = StopFilter {
            since: Some(Lookback::parse("1h").unwrap()),
            ..StopFilter::default()
        };
        let groups = stops(&store, &clock(7_200), &recent).unwrap();
        assert_eq!(summary(&groups), [("expired", 1), ("policy", 1)]);
        let one_session = StopFilter {
            session: Some("s1".into()),
            ..StopFilter::default()
        };
        let groups = stops(&store, &clock(7_200), &one_session).unwrap();
        assert_eq!(summary(&groups), [("expired", 1), ("origin", 1)]);
        assert!(
            stops(&Record(vec![]), &clock(0), &StopFilter::default())
                .unwrap()
                .is_empty()
        );
    }
}
