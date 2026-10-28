//! `browser_login` (R-39): finds the page's username and password inputs and fills the
//! login item's values into them through the same checked path as `fill` — focus, origin,
//! policy, expiry, field kind, navigation — so it opens no new way for a value to leave.
//! Nothing is typed unless every field the item needs was found.

use crate::GuardedBrowser;
use anyhow::{Result, bail};
use keyharbor_domain::{
    ports::{BrowserPort, FrameInspector, PageInput},
    vault::{ItemKind, ItemName, placeholder},
};
use serde_json::{Value, json};

impl<B: BrowserPort + PageInput + FrameInspector> GuardedBrowser<'_, B> {
    /// Fills `item`'s username (if it has one) and password, then presses Enter in the
    /// password input when `submit`. Returns which fields were filled and the URL after.
    pub async fn login(&mut self, item: &ItemName, submit: bool) -> Result<Value> {
        let outcome = self.login_checked(item, submit).await;
        self.guard.redact_outcome(outcome)
    }

    async fn login_checked(&mut self, item: &ItemName, submit: bool) -> Result<Value> {
        let info = self.guard.item_use(self.agent, item)?;
        if info.kind != ItemKind::Login {
            bail!("{} is not a login item", item.as_str());
        }
        let page = self.inner.page().await?;
        let found = self.inner.login_fields(&page).await?;
        let password = found
            .password
            .ok_or_else(|| anyhow::anyhow!("not found: password field"))?;
        let mut steps = Vec::new();
        if info.fields.iter().any(|f| f == "username") {
            let username = found
                .username
                .ok_or_else(|| anyhow::anyhow!("not found: username field"))?;
            steps.push(("username", username));
        }
        steps.push(("password", password));
        let mut filled = Vec::new();
        for (field, node) in steps {
            let template = placeholder(item.as_str(), field);
            self.fill_checked(&json!({"ref": node}), &template).await?;
            filled.push(field);
        }
        if submit {
            self.inner.press("Enter").await?;
            let _ = self.inner.settle(&page, None).await;
        }
        let url = self.inner.location(&page).await.unwrap_or_default();
        Ok(json!({"filled": filled, "submitted": submit, "url": url}))
    }
}
