//! `browser_select` (#117): chooses an option of a `<select>`. A placeholder in the value or
//! label is resolved through the same checks as `fill` (focus on the element, the frame's
//! origin, policy, expiry, field kind, navigation), so choosing opens no new way for a value
//! to leave. The answer names only the chosen index when a placeholder was used.

use super::GuardedBrowser;
use anyhow::{Result, bail};
use keyharbor_domain::{
    ports::{
        BrowserPort, FrameInspector, PageInput,
        browser::{Choice, Target},
    },
    vault::has_placeholder,
};
use serde_json::{Value, json};
use zeroize::Zeroizing;

/// The refusal when no option matches; it never repeats what was asked for.
const NO_MATCH: &str = "no option matches";
/// The refusal when the target is not a `<select>`.
const NOT_A_SELECT: &str = "browser_select needs a <select> element";

impl<B: BrowserPort + PageInput + FrameInspector> GuardedBrowser<'_, B> {
    /// Chooses the option `choice` names in the `<select>` `target`.
    pub async fn select(&mut self, target: &Target, choice: &Choice) -> Result<Value> {
        self.target(target)?;
        let template = choice.text().unwrap_or_default();
        let secret = has_placeholder(template);
        if secret {
            self.literals(&[target.selector().unwrap_or_default()])?;
        } else {
            self.literals(&[target.selector().unwrap_or_default(), template])?;
        }
        let outcome = if secret {
            self.select_secret(target, choice, template).await
        } else {
            self.select_plain(target, choice).await
        };
        self.guard.redact_outcome(outcome)
    }

    async fn select_plain(&mut self, target: &Target, choice: &Choice) -> Result<Value> {
        let page = self.inner.page().await?;
        let node = self.inner.focus_element(&page, target).await?;
        if !self.inner.element(&page, node).await?.is_select() {
            bail!(NOT_A_SELECT);
        }
        match self.inner.select_option(&page, node, choice).await? {
            Some(index) => Ok(json!({"selected": {"index": index}})),
            None => bail!(NO_MATCH),
        }
    }

    async fn select_secret(
        &mut self,
        target: &Target,
        choice: &Choice,
        template: &str,
    ) -> Result<Value> {
        let page = self.inner.page().await?;
        let epoch = self.settled(&page, template).await?;
        let node = self.inner.focus_element(&page, target).await?;
        let (focus, element, origin) = self.checked_focus(&page, template, Some(node)).await?;
        if !element.is_select() {
            bail!(NOT_A_SELECT);
        }
        let prepared = self.guard.prepare_text(
            self.agent,
            template,
            crate::pep::substitute::Destination {
                frame: origin.as_ref(),
                element: Some(&element),
            },
        )?;
        self.still_there(&page, &focus.frame, epoch, &prepared, origin.as_ref())
            .await?;
        let resolved = Zeroizing::new(choice.with_text(prepared.text.to_string()));
        let chosen = self.inner.select_option(&page, node, &resolved).await;
        drop(resolved);
        let Some(index) = chosen? else {
            bail!(NO_MATCH);
        };
        self.guard.taint(node);
        let after = self.inner.frame_origin(&page, &focus.frame).await;
        self.note_used(
            &prepared.items,
            &prepared.domains,
            origin.as_ref(),
            after.as_ref(),
        );
        Ok(json!({"selected": {"index": index}}))
    }
}
