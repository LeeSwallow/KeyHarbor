//! Grants (R-43): who may act, where, until when. Phase 4 has one kind: a session may load
//! documents only from the origins it was granted (a site handed over from the user's
//! Chrome, or added by a person). Other subjects and scopes (an agent, item prefixes) come
//! with the developer secrets channel.

use crate::{
    session::SessionId,
    vault::{Description, InvalidValue, Origin},
};

uuid_id!(
    /// Identifies one grant; `keyharbor grant revoke` quotes it.
    GrantId
);

// The bounds are literals behind macros so a message elsewhere can `concat!` them
// (`handoff_site`; J-171).
macro_rules! grant_ttl_min_seconds {
    () => {
        60
    };
}
macro_rules! grant_ttl_max_seconds {
    () => {
        2_592_000 // 30 days
    };
}
pub(crate) use {grant_ttl_max_seconds, grant_ttl_min_seconds};

/// How long a grant lasts: `<N>m`, `<N>h` or `<N>d`, from one minute to 30 days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantTtl(u64);
impl GrantTtl {
    pub const MIN_SECONDS: u64 = grant_ttl_min_seconds!();
    pub const MAX_SECONDS: u64 = grant_ttl_max_seconds!();
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let invalid = InvalidValue("grant TTL must be <N>m, <N>h or <N>d, at most 30d");
        let (number, unit) = value.split_at(value.len().saturating_sub(1));
        let unit = match unit {
            "m" => 60,
            "h" => 3_600,
            "d" => 86_400,
            _ => return Err(invalid),
        };
        let n: u64 = number.parse().map_err(|_| invalid)?;
        match n.checked_mul(unit) {
            Some(seconds) if (Self::MIN_SECONDS..=Self::MAX_SECONDS).contains(&seconds) => {
                Ok(Self(seconds))
            }
            _ => Err(invalid),
        }
    }
    pub fn seconds(self) -> u64 {
        self.0
    }
}

/// One grant: session `session` may load documents from `origin` (and its subdomains).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub id: GrantId,
    pub session: SessionId,
    pub origin: Origin,
    /// Unix seconds; `None` lasts until revoked.
    pub until: Option<u64>,
    /// Who made it: `cli`, or the handoff (`handoff`).
    pub created_by: String,
    pub reason: Option<Description>,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
    /// Its cookies were removed from the session's profile after it ended.
    pub cleared: bool,
}

impl Grant {
    /// The granted host (domain or IP), for matching cookies.
    pub fn host(&self) -> Option<String> {
        url::Url::parse(self.origin.as_str())
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
    }

    pub fn is_active(&self, now: u64) -> bool {
        self.revoked_at.is_none() && self.until.is_none_or(|until| now < until)
    }

    /// Whether `url` is in this grant's site: same scheme and port, and the granted host or
    /// a subdomain of it (`github.com` covers `gist.github.com`).
    pub fn covers(&self, url: &url::Url) -> bool {
        let Ok(granted) = url::Url::parse(self.origin.as_str()) else {
            return false;
        };
        let same_host = match (granted.host(), url.host()) {
            (Some(url::Host::Domain(g)), Some(url::Host::Domain(h))) => {
                let (g, h) = (g.to_ascii_lowercase(), h.to_ascii_lowercase());
                h == g || h.ends_with(&format!(".{g}"))
            }
            (Some(g), Some(h)) => g == h,
            _ => false,
        };
        same_host
            && granted.scheme() == url.scheme()
            && granted.port_or_known_default() == url.port_or_known_default()
    }
}

/// Whether a session with `grants` may load the document at `url` (R-43): a session that
/// never had a grant is not limited; one that had any may load only what an active grant
/// covers, so an ended grant closes the session rather than opening it.
pub fn allows_document(grants: &[Grant], now: u64, url: &str) -> bool {
    if grants.is_empty() {
        return true;
    }
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return true;
    }
    grants
        .iter()
        .any(|grant| grant.is_active(now) && grant.covers(&url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(origin: &str, until: Option<u64>, revoked_at: Option<u64>) -> Grant {
        Grant {
            id: GrantId::from_bytes([1; 16]),
            session: SessionId::from_bytes([2; 16]),
            origin: Origin::parse(origin).unwrap(),
            until,
            created_by: "cli".into(),
            reason: None,
            created_at: 0,
            revoked_at,
            cleared: false,
        }
    }

    #[test]
    fn ttls_are_minutes_hours_or_days_up_to_30_days() {
        assert_eq!(GrantTtl::parse("30m").unwrap().seconds(), 1_800);
        assert_eq!(GrantTtl::parse("2h").unwrap().seconds(), 7_200);
        assert_eq!(GrantTtl::parse("30d").unwrap().seconds(), 30 * 86_400);
        for bad in ["", "m", "0m", "31d", "5", "1w", "-1h", "1.5h"] {
            assert!(GrantTtl::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_grant_covers_its_host_and_subdomains_with_the_same_scheme_and_port() {
        let g = grant("https://github.com", None, None);
        let covers = |u: &str| g.covers(&url::Url::parse(u).unwrap());
        assert!(covers("https://github.com/login"));
        assert!(covers("https://gist.github.com/x"));
        assert!(!covers("https://evilgithub.com/"));
        assert!(!covers("http://github.com/"));
        assert!(!covers("https://github.com:8443/"));
        let local = grant("http://127.0.0.1:8080", None, None);
        assert!(local.covers(&url::Url::parse("http://127.0.0.1:8080/a").unwrap()));
        assert!(!local.covers(&url::Url::parse("http://127.0.0.1:9090/a").unwrap()));
    }

    #[test]
    fn only_active_grants_allow_and_an_ended_one_closes_the_session() {
        assert!(allows_document(&[], 10, "https://anywhere.example/"));
        let active = grant("https://github.com", Some(100), None);
        assert!(allows_document(
            std::slice::from_ref(&active),
            10,
            "https://github.com/"
        ));
        assert!(!allows_document(
            std::slice::from_ref(&active),
            10,
            "https://example.com/"
        ));
        assert!(!allows_document(
            std::slice::from_ref(&active),
            100,
            "https://github.com/"
        ));
        let revoked = grant("https://github.com", None, Some(5));
        assert!(!allows_document(&[revoked], 10, "https://github.com/"));
        assert!(allows_document(
            std::slice::from_ref(&active),
            10,
            "about:blank"
        ));
        assert!(!allows_document(&[active], 10, "not a url"));
    }
}
