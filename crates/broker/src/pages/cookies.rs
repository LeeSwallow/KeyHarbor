//! Removing a site's cookies from the profile when its grant ends (R-43).

use crate::chrome::CdpLink;
use anyhow::Result;
use serde_json::{Value, json};

/// Whether a cookie for `cookie_domain` (`.github.com` or `github.com`) belongs to `host` or a
/// subdomain of it.
fn belongs(cookie_domain: &str, host: &str) -> bool {
    let domain = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    domain == host || domain.ends_with(&format!(".{host}"))
}

impl CdpLink {
    /// Deletes every cookie of `host` and its subdomains; returns how many.
    pub async fn clear_cookies(&self, host: &str) -> Result<usize> {
        let cdp = self.cdp();
        let all = cdp.call("Storage.getCookies", json!({}), None).await?;
        let expired: Vec<Value> = all["cookies"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|c| belongs(c["domain"].as_str().unwrap_or_default(), host))
            .map(|c| {
                // Setting a cookie that expired in 1970 is how CDP deletes at browser level.
                json!({"name": c["name"], "value": "", "domain": c["domain"], "path": c["path"],
                       "secure": c["secure"], "httpOnly": c["httpOnly"], "expires": 1})
            })
            .collect();
        if !expired.is_empty() {
            cdp.call("Storage.setCookies", json!({"cookies": expired}), None)
                .await?;
        }
        Ok(expired.len())
    }
}

#[cfg(test)]
mod tests {
    use super::belongs;

    #[test]
    fn cookies_belong_to_the_host_and_its_subdomains() {
        assert!(belongs(".github.com", "github.com"));
        assert!(belongs("gist.github.com", "github.com"));
        assert!(!belongs("evilgithub.com", "github.com"));
        assert!(!belongs("github.com", "gist.github.com"));
        assert!(belongs("127.0.0.1", "127.0.0.1"));
    }
}
