//! A site's login handed from the user's own browser to an agent session (R-42): which
//! cookies may come along. The extension picks them; the host and the supervisor check them
//! again here, so a message cannot slip another site's cookies into the session.

use crate::vault::{InvalidValue, Origin};
use zeroize::Zeroizing;

/// More cookies than a real site sets; a message beyond this is refused.
pub const MAX_COOKIES: usize = 500;
/// Same bound as a grant TTL (`GrantTtl::MAX_SECONDS`), and at least a minute.
pub const TTL_SECONDS: std::ops::RangeInclusive<u64> = 60..=30 * 86_400;

/// A cookie for the session's profile, in our words (the CDP shape is the broker's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteCookie {
    pub name: String,
    pub value: Zeroizing<String>,
    /// The URL the cookie is set for; its host decides where a host-only cookie lives.
    pub url: String,
    /// `.github.com` for a domain cookie; `None` for a host-only one.
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    /// `Strict`, `Lax` or `None`; absent means the browser default.
    pub same_site: Option<String>,
    /// Unix seconds; absent for a session cookie.
    pub expires: Option<i64>,
}

fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
}

/// Whether `domain` is `host`, a subdomain of it, or a parent of it with at least two labels
/// (a site's cookies may sit on `.github.com` while its origin is `gist.github.com`; a bare
/// suffix such as `com` is never accepted).
fn related(domain: &str, host: &str) -> bool {
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    if domain.is_empty() {
        return false;
    }
    let child = domain == host || domain.ends_with(&format!(".{host}"));
    let parent = host.ends_with(&format!(".{domain}")) && domain.contains('.');
    child || parent
}

impl SiteCookie {
    /// Whether this cookie belongs to the site at `origin`: its URL's host and its domain (if
    /// any) are that host, a subdomain, or a parent domain of it.
    pub fn belongs_to(&self, origin: &Origin) -> bool {
        let Some(host) = host_of(origin.as_str()) else {
            return false;
        };
        let url_ok = host_of(&self.url).is_some_and(|h| related(&h, &host));
        let domain_ok = self.domain.as_deref().is_none_or(|d| related(d, &host));
        url_ok && domain_ok && !self.name.is_empty()
    }
}

/// Checks a handoff's cookies and TTL; the error names the rule, never a value.
pub fn check(
    origin: &Origin,
    cookies: &[SiteCookie],
    ttl_seconds: u64,
) -> Result<(), InvalidValue> {
    if !TTL_SECONDS.contains(&ttl_seconds) {
        return Err(InvalidValue("ttl_seconds must be 60 to 2592000"));
    }
    if cookies.len() > MAX_COOKIES {
        return Err(InvalidValue("too many cookies"));
    }
    if cookies.iter().any(|c| !c.belongs_to(origin)) {
        return Err(InvalidValue("a cookie does not belong to the site"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(url: &str, domain: Option<&str>) -> SiteCookie {
        SiteCookie {
            name: "sid".into(),
            value: Zeroizing::new("v".into()),
            url: url.into(),
            domain: domain.map(str::to_owned),
            path: Some("/".into()),
            secure: true,
            http_only: true,
            same_site: None,
            expires: None,
        }
    }

    #[test]
    fn cookies_of_the_site_its_subdomains_and_parent_domain_belong() {
        let gist = Origin::parse("https://gist.github.com").unwrap();
        assert!(cookie("https://gist.github.com/", None).belongs_to(&gist));
        assert!(cookie("https://github.com/", Some(".github.com")).belongs_to(&gist));
        assert!(cookie("https://a.gist.github.com/", None).belongs_to(&gist));
        assert!(!cookie("https://evil.example/", None).belongs_to(&gist));
        assert!(!cookie("https://gist.github.com/", Some(".evil.example")).belongs_to(&gist));
        assert!(!cookie("https://gist.github.com/", Some(".com")).belongs_to(&gist));
        assert!(!cookie("https://evilgithub.com/", None).belongs_to(&gist));
    }

    #[test]
    fn a_handoff_needs_a_sane_ttl_and_only_the_sites_cookies() {
        let site = Origin::parse("https://github.com").unwrap();
        let ok = [cookie("https://github.com/", None)];
        assert!(check(&site, &ok, 3_600).is_ok());
        assert!(check(&site, &ok, 59).is_err());
        assert!(check(&site, &ok, 31 * 86_400).is_err());
        assert!(check(&site, &[cookie("https://other.example/", None)], 3_600).is_err());
        let many = vec![cookie("https://github.com/", None); MAX_COOKIES + 1];
        assert!(check(&site, &many, 3_600).is_err());
    }
}
