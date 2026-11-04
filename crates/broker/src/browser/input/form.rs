//! Form controls that are not typed into: choosing an option of a `<select>`
//! (`browser_select`; the page function fires `input` and `change` so frameworks see a
//! person's choice) and putting files into a file input (`browser_upload`).

use crate::browser::CdpBrowser;
use anyhow::{Context, Result};
use keyharbor_domain::ports::browser::Choice;
use serde_json::{Value, json};

/// Runs on the `<select>` element: returns the chosen index, or -1 when nothing matches (and
/// then changes nothing).
const CHOOSE: &str = "function(kind, want) {
  if (!(this instanceof HTMLSelectElement)) return -2;
  const options = Array.from(this.options);
  const index = kind === 'index' ? Number(want)
    : options.findIndex(o => kind === 'value' ? o.value === want : o.label.trim() === String(want).trim());
  if (!(index >= 0 && index < options.length) || options[index].disabled) return -1;
  this.selectedIndex = index;
  this.dispatchEvent(new Event('input', {bubbles: true}));
  this.dispatchEvent(new Event('change', {bubbles: true}));
  return index;
}";

impl CdpBrowser {
    pub(crate) async fn choose_option(
        &self,
        session: &str,
        node: u64,
        choice: &Choice,
    ) -> Result<Option<u32>> {
        let object = self
            .call("DOM.resolveNode", json!({"backendNodeId": node}), session)
            .await?;
        let object = object["object"]["objectId"]
            .as_str()
            .context("element cannot be resolved")?;
        let (kind, want) = match choice {
            Choice::Value(text) => ("value", Value::from(text.as_str())),
            Choice::Label(text) => ("label", Value::from(text.as_str())),
            Choice::Index(index) => ("index", Value::from(*index)),
        };
        let result = self
            .call(
                "Runtime.callFunctionOn",
                json!({
                    "objectId": object,
                    "functionDeclaration": CHOOSE,
                    "arguments": [{"value": kind}, {"value": want}],
                    "returnByValue": true,
                }),
                session,
            )
            .await?;
        let index = result["result"]["value"].as_i64().unwrap_or(-1);
        if index == -2 {
            anyhow::bail!("browser_select needs a <select> element");
        }
        Ok(u32::try_from(index).ok())
    }
}

impl CdpBrowser {
    /// Puts local files into a file input, as a person choosing them in the file dialog.
    pub(crate) async fn put_files(
        &self,
        session: &str,
        node: u64,
        paths: &[std::path::PathBuf],
    ) -> Result<()> {
        let files: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        self.call(
            "DOM.setFileInputFiles",
            json!({"files": files, "backendNodeId": node}),
            session,
        )
        .await?;
        Ok(())
    }
}
