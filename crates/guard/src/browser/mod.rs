//! `GuardedBrowser`: the decorator every control request goes through (ADR-0014).
//!
//! It implements `BrowserPort` over a raw browser `B`. Text for `fill` and `type_text` with
//! `{{secret:item.field}}` placeholders is resolved only after the guard's checks against the
//! frame that holds the focused element (03 §3.8, J-35); everything that comes back — values
//! and error messages alike — is masked exactly once. Error messages never include input text.
//!
//! - [`fill`] — the checked path every value takes into a page (`fill`, `type_text`).
//! - [`login`], [`select`], [`upload`] — operations built on that path.

use crate::Guard;
use anyhow::Result;
use keyharbor_domain::ports::{
    BrowserPort, FrameInspector, PageInput,
    browser::{FillOutcome, Target, WaitFor},
};
use serde_json::Value;

mod fill;
mod login;
mod select;
#[cfg(test)]
mod tests;
mod upload;

pub use login::LoginOutcome;

pub struct GuardedBrowser<'a, B> {
    pub(crate) inner: &'a mut B,
    pub(crate) guard: &'a Guard,
    pub(crate) agent: &'a str,
}

impl<'a, B> GuardedBrowser<'a, B> {
    pub(crate) fn new(inner: &'a mut B, guard: &'a Guard, agent: &'a str) -> Self {
        Self {
            inner,
            guard,
            agent,
        }
    }
}

/// The CSS selector a target carries, if any.
fn selector(target: &Target) -> &str {
    target.selector().unwrap_or_default()
}

impl<B: BrowserPort + PageInput + FrameInspector> GuardedBrowser<'_, B> {
    /// Refuses (and masks the refusal of) requests whose text holds a vault value (I1).
    pub(crate) fn literals(&self, literals: &[&str]) -> Result<()> {
        self.guard.check_literals(self.agent, literals)
    }

    /// Refuses selectors outside the structural grammar of `pep::selector` (I3).
    pub(crate) fn target(&self, target: &Target) -> Result<()> {
        match target.selector() {
            Some(css) => crate::pep::selector::check(css),
            None => Ok(()),
        }
    }
}

impl<B: BrowserPort + PageInput + FrameInspector> BrowserPort for GuardedBrowser<'_, B> {
    async fn navigate(&mut self, url: &str) -> Result<Value> {
        self.literals(&[url])?;
        let outcome = self.inner.navigate(url).await;
        self.guard.redact_outcome(outcome)
    }
    async fn snapshot(&mut self) -> Result<Value> {
        let hidden = self.guard.tainted();
        let outcome = self.inner.redacted_snapshot(&hidden).await;
        self.guard.redact_outcome(outcome)
    }
    async fn screenshot(&mut self) -> Result<Value> {
        let hidden = self.guard.tainted();
        let outcome = self.inner.redacted_screenshot(&hidden).await;
        self.guard.redact_outcome(outcome)
    }
    async fn click(&mut self, target: &Target) -> Result<Value> {
        self.target(target)?;
        self.literals(&[selector(target)])?;
        let outcome = self.inner.click(target).await;
        self.guard.redact_outcome(outcome)
    }
    async fn fill(&mut self, target: &Target, text: &str) -> Result<FillOutcome> {
        self.target(target)?;
        self.literals(&[selector(target), text])?;
        let outcome = self.fill_checked(target, text).await;
        self.guard.redact_error(outcome)
    }
    async fn type_text(&mut self, text: &str) -> Result<FillOutcome> {
        self.literals(&[text])?;
        let outcome = self.type_checked(text).await;
        self.guard.redact_error(outcome)
    }
    async fn press(&mut self, key: &str) -> Result<Value> {
        self.literals(&[key])?;
        let outcome = self.inner.press(key).await;
        self.guard.redact_outcome(outcome)
    }
    async fn tabs(&mut self) -> Result<Value> {
        let outcome = self.inner.tabs().await;
        self.guard.redact_outcome(outcome)
    }
    async fn new_tab(&mut self, url: Option<&str>) -> Result<Value> {
        self.literals(&[url.unwrap_or_default()])?;
        let outcome = self.inner.new_tab(url).await;
        self.guard.redact_outcome(outcome)
    }
    async fn switch_tab(&mut self, id: &str) -> Result<Value> {
        self.literals(&[id])?;
        let outcome = self.inner.switch_tab(id).await;
        self.guard.redact_outcome(outcome)
    }
    async fn wait(&mut self, until: &WaitFor) -> Result<Value> {
        if let WaitFor::Selector(selector) = until {
            crate::pep::selector::check(selector)?;
            self.literals(&[selector])?;
        }
        let outcome = self.inner.wait(until).await;
        self.guard.redact_outcome(outcome)
    }
    fn close(&mut self) -> Result<()> {
        let outcome = self.inner.close();
        self.guard.redact_error(outcome)
    }
}
