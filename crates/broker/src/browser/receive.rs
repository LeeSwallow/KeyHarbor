//! Taking in a site handed over from the user's browser (R-42): its cookies go into the
//! profile, its `localStorage` (if sent) into the origin, and the origin opens. Values are
//! sent to Chrome only; nothing here returns them.

use super::CdpBrowser;
use anyhow::Result;
use keyharbor_domain::handoff_site::SiteCookie;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// The CDP `Network.CookieParam` for a cookie.
fn cookie_param(cookie: &SiteCookie) -> Value {
    let mut param = json!({
        "name": cookie.name,
        "value": cookie.value.as_str(),
        "url": cookie.url,
        "secure": cookie.secure,
        "httpOnly": cookie.http_only,
    });
    let optional = [
        ("domain", cookie.domain.as_ref().map(|d| json!(d))),
        ("path", cookie.path.as_ref().map(|p| json!(p))),
        ("sameSite", cookie.same_site.as_ref().map(|s| json!(s))),
        ("expires", cookie.expires.map(|e| json!(e))),
    ];
    for (key, value) in optional {
        if let Some(value) = value {
            param[key] = value;
        }
    }
    param
}

impl CdpBrowser {
    /// Sets `cookies`, fills `local_storage` into `origin`, and leaves the active tab there.
    /// Returns how many cookies were set.
    pub async fn receive_site(
        &mut self,
        origin: &str,
        cookies: &[SiteCookie],
        local_storage: Option<&BTreeMap<String, Zeroizing<String>>>,
    ) -> Result<usize> {
        if !cookies.is_empty() {
            let params: Vec<Value> = cookies.iter().map(cookie_param).collect();
            self.cdp
                .call("Storage.setCookies", json!({"cookies": params}), None)
                .await?;
        }
        self.navigate_to(origin).await?;
        if let Some(items) = local_storage.filter(|items| !items.is_empty()) {
            let items: BTreeMap<&str, &str> = items
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let script = Zeroizing::new(format!(
                "for (const [k, v] of Object.entries({})) localStorage.setItem(k, v)",
                serde_json::to_string(&items)?
            ));
            let page = self.active_page().await?;
            self.call(
                "Runtime.evaluate",
                json!({"expression": script.as_str()}),
                &page,
            )
            .await?;
            self.navigate_to(origin).await?;
        }
        Ok(cookies.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_become_cdp_params_with_only_the_fields_given() {
        let host_only = SiteCookie {
            name: "sid".into(),
            value: Zeroizing::new("v".into()),
            url: "https://github.com/".into(),
            domain: None,
            path: Some("/".into()),
            secure: true,
            http_only: true,
            same_site: Some("Lax".into()),
            expires: Some(1_893_456_000),
        };
        assert_eq!(
            cookie_param(&host_only),
            json!({"name": "sid", "value": "v", "url": "https://github.com/", "secure": true,
                   "httpOnly": true, "path": "/", "sameSite": "Lax", "expires": 1_893_456_000})
        );
        let session = SiteCookie {
            domain: Some(".github.com".into()),
            path: None,
            same_site: None,
            expires: None,
            ..host_only
        };
        let param = cookie_param(&session);
        assert_eq!(param["domain"], ".github.com");
        assert!(param.get("expires").is_none() && param.get("sameSite").is_none());
    }
}
