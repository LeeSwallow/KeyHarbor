//! Which URLs a session may open with and navigate to.

use crate::vault::{InvalidValue, is_https_or_local_http};

number_text!(
    /// The longest start URL, in bytes: it goes on the browser command line.
    pub START_URL_MAX_BYTES: usize = 2048,
    START_URL_TOO_LONG = "URL exceeds ", " bytes"
);
number_text!(
    /// The longest URL an agent may navigate to, in bytes.
    pub NAVIGATION_URL_MAX_BYTES: usize = 8192,
    NAVIGATION_URL_TOO_LONG = "URL exceeds ", " bytes"
);

/// The URL a session opens with. It is passed on the browser command line (visible in `ps`),
/// so it may not carry credentials, a query or a fragment, and must be HTTPS or local HTTP.
pub fn validate_start_url(value: &str) -> Result<String, InvalidValue> {
    if value.len() > START_URL_MAX_BYTES {
        return Err(InvalidValue(START_URL_TOO_LONG));
    }
    let url = url::Url::parse(value).map_err(|_| InvalidValue("invalid URL"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(InvalidValue(
            "URL must not include user information, query, or fragment",
        ));
    }
    if !is_https_or_local_http(&url) {
        return Err(InvalidValue("URL must use HTTPS or HTTP on localhost"));
    }
    Ok(url.into())
}

/// A URL an agent may navigate to: any http(s) URL, or `about:blank`.
pub fn validate_navigation_url(value: &str) -> Result<String, InvalidValue> {
    if value == "about:blank" {
        return Ok(value.to_owned());
    }
    if value.len() > NAVIGATION_URL_MAX_BYTES {
        return Err(InvalidValue(NAVIGATION_URL_TOO_LONG));
    }
    let url = url::Url::parse(value).map_err(|_| InvalidValue("invalid URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(InvalidValue("URL must use HTTP or HTTPS"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(InvalidValue("URL must not include user information"));
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_urls_reject_unsafe_destinations_and_credentials() {
        assert!(validate_start_url("https://example.com/login").is_ok());
        assert!(validate_start_url("http://127.0.0.1:8080/login").is_ok());
        for url in [
            "http://example.com",
            "file:///etc/passwd",
            "https://user:secret@example.com",
            "https://example.com/?token=secret",
            "https://example.com/#secret",
        ] {
            assert!(validate_start_url(url).is_err(), "{url}");
        }
        assert!(validate_navigation_url("http://example.com/?q=1").is_ok());
        assert!(validate_navigation_url("about:blank").is_ok());
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "chrome://settings",
            "https://u:p@example.com",
        ] {
            assert!(validate_navigation_url(url).is_err(), "{url}");
        }
    }
}
