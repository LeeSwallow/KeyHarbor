//! The menu bar text (09 §9.4.6).

use super::OverviewDto;

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
