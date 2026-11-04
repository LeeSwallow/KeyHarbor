//! `screenshot`: a PNG of the active tab, saved 0600 in the private shots directory the
//! supervisor prepared. Elements that received a vault value draw their text as discs while
//! the screenshot is taken (03 §3.11 #2). The style is set through the DOM agent, not page
//! script, so the page cannot redefine the call that hides them.

use crate::browser::{CdpBrowser, dom::attribute};
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// An element hidden for a screenshot: its frontend node id and its `style` before.
struct Concealed {
    node: u64,
    style: Option<String>,
}

impl CdpBrowser {
    pub(crate) async fn save_screenshot(&mut self, hidden: &BTreeSet<u64>) -> Result<Value> {
        let session = self.active_page().await?;
        let concealed = self
            .conceal(&session, hidden)
            .await
            .context("filled inputs could not be hidden; no screenshot taken")?;
        let shot = self
            .call("Page.captureScreenshot", json!({"format": "png"}), &session)
            .await;
        self.reveal(&session, &concealed).await;
        let png = base64::engine::general_purpose::STANDARD
            .decode(
                shot?["data"]
                    .as_str()
                    .context("screenshot returned no data")?,
            )
            .context("screenshot data is not base64")?;
        self.write_shot(&png)
    }

    /// Hides every element of `hidden` still in the page; on any failure restores the ones
    /// already hidden and fails. Elements no longer in the page draw nothing and are skipped.
    async fn conceal(&self, session: &str, hidden: &BTreeSet<u64>) -> Result<Vec<Concealed>> {
        let mut done = Vec::new();
        if hidden.is_empty() {
            return Ok(done);
        }
        match self.conceal_each(session, hidden, &mut done).await {
            Ok(()) => Ok(done),
            Err(error) => {
                self.reveal(session, &done).await;
                Err(error)
            }
        }
    }

    async fn conceal_each(
        &self,
        session: &str,
        hidden: &BTreeSet<u64>,
        done: &mut Vec<Concealed>,
    ) -> Result<()> {
        self.call("DOM.getDocument", json!({"depth": 0}), session)
            .await?;
        let pushed = self
            .call(
                "DOM.pushNodesByBackendIdsToFrontend",
                json!({"backendNodeIds": hidden}),
                session,
            )
            .await?;
        let nodes = pushed["nodeIds"].as_array().context("no node ids")?;
        for node in nodes.iter().filter_map(Value::as_u64).filter(|&n| n != 0) {
            let described = self
                .call("DOM.describeNode", json!({"nodeId": node}), session)
                .await?;
            let element = &described["node"];
            let style = attribute(element, "style").map(str::to_owned);
            let name = element["nodeName"].as_str().unwrap_or_default();
            self.call(
                "DOM.setAttributeValue",
                json!({"nodeId": node, "name": "style", "value": concealing_style(style.as_deref(), name)}),
                session,
            )
            .await?;
            done.push(Concealed { node, style });
        }
        Ok(())
    }

    /// Puts back each element's style. A failure only leaves discs on the screen: the value
    /// itself is unchanged.
    async fn reveal(&self, session: &str, concealed: &[Concealed]) {
        for Concealed { node, style } in concealed {
            let _ = match style {
                Some(style) => {
                    self.call(
                        "DOM.setAttributeValue",
                        json!({"nodeId": node, "name": "style", "value": style}),
                        session,
                    )
                    .await
                }
                None => {
                    self.call(
                        "DOM.removeAttribute",
                        json!({"nodeId": node, "name": "style"}),
                        session,
                    )
                    .await
                }
            };
        }
    }

    fn write_shot(&self, png: &[u8]) -> Result<Value> {
        let mut n = 0;
        loop {
            n += 1;
            let path = self.shots.join(format!("{}-{n}.png", self.shot_prefix));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    std::io::Write::write_all(&mut file, png)?;
                    return Ok(json!({"path": path}));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }
}

/// The element's style with its text drawn as discs. A closed `<select>` ignores text
/// security, so its text is made transparent instead. `!important` in the inline style
/// outranks the page's stylesheets.
fn concealing_style(style: Option<&str>, node_name: &str) -> String {
    let mut conceal = String::from("-webkit-text-security: disc !important;");
    if node_name.eq_ignore_ascii_case("select") {
        conceal.push_str(" color: transparent !important; text-shadow: none !important;");
    }
    match style.map(str::trim).filter(|s| !s.is_empty()) {
        Some(style) => format!("{}; {conceal}", style.trim_end_matches(';')),
        None => conceal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concealing_keeps_the_page_style_and_adds_discs() {
        assert_eq!(
            concealing_style(None, "INPUT"),
            "-webkit-text-security: disc !important;"
        );
        assert_eq!(
            concealing_style(Some("width: 3em;"), "INPUT"),
            "width: 3em; -webkit-text-security: disc !important;"
        );
        assert!(concealing_style(Some("  "), "TEXTAREA").starts_with("-webkit"));
    }

    #[test]
    fn a_select_hides_its_text_by_colour() {
        assert!(concealing_style(None, "SELECT").contains("color: transparent !important"));
    }
}
