//! `--browser` for `mcp` and `open`.

use keyharbor_domain::browser_choice::BrowserChoice;

/// auto, chrome, ego-lite or an absolute path; the rule is the domain's (`BrowserChoice`).
pub fn parse_browser(value: &str) -> Result<BrowserChoice, String> {
    BrowserChoice::parse(value).map_err(|e| e.to_string())
}
