//! Where an item may be used: a web origin.

use super::InvalidValue;

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Origin(String);
impl Origin {
    /// The origin of a page address (`https://github.com/login` → `https://github.com`), under the
    /// same rules as [`Origin::parse`]; for imports, whose rows hold page addresses.
    pub fn of_page(url: &str) -> Result<Self, InvalidValue> {
        let url = url::Url::parse(url).map_err(|_| InvalidValue("invalid origin"))?;
        Self::parse(&url.origin().ascii_serialization())
    }

    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let url = url::Url::parse(value).map_err(|_| InvalidValue("invalid origin"))?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(InvalidValue(
                "origin must not include a path, query, fragment, or user information",
            ));
        }
        if !is_https_or_local_http(&url) {
            return Err(InvalidValue("origin must use HTTPS or HTTP on localhost"));
        }
        Ok(Self(url.origin().ascii_serialization()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether `url` has a host and is HTTPS, or HTTP on this machine: the rule for an origin and
/// for a session's start URL alike.
pub(crate) fn is_https_or_local_http(url: &url::Url) -> bool {
    url.host().is_some()
        && (url.scheme() == "https" || (url.scheme() == "http" && is_loopback_host(url)))
}

/// Whether `url`'s host is `localhost` or a loopback address.
fn is_loopback_host(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::vault::*;
    #[test]
    fn a_page_address_gives_its_origin() {
        let of = |url: &str| Origin::of_page(url).map(|o| o.as_str().to_owned());
        assert_eq!(
            of("https://GitHub.com/login?x=1#f").unwrap(),
            "https://github.com"
        );
        assert_eq!(
            of("https://shop.example:8443/").unwrap(),
            "https://shop.example:8443"
        );
        assert_eq!(
            of("http://localhost:3000/a").unwrap(),
            "http://localhost:3000"
        );
        for bad in [
            "android://abc.example/",
            "http://plain.example/",
            "not a url",
            "",
        ] {
            assert!(of(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn origins_are_normalized() {
        assert_eq!(
            Origin::parse("HTTPS://GitHub.com:443").unwrap().as_str(),
            "https://github.com"
        );
        assert_eq!(
            Origin::parse("http://localhost:8080").unwrap().as_str(),
            "http://localhost:8080"
        );
        assert_eq!(
            Origin::parse("https://한국.kr").unwrap().as_str(),
            "https://xn--3e0b707e.kr"
        );
        for bad in [
            "github.com",
            "https://github.com/login",
            "https://u:p@github.com",
            "ftp://x.com",
            "http://example.com",
        ] {
            assert!(Origin::parse(bad).is_err(), "{bad:?}");
        }
    }
}
