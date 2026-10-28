//! Control operations: one JSON request → one command, dispatched to the session's browser
//! through `GuardedBrowser`, the only path from the control socket to the page. The guard
//! resolves placeholders in `fill`/`type` text and masks every result and error exactly once;
//! requests that never reach the browser (`status`, malformed requests) are masked here.

use crate::{
    control::wire::{ControlEnvelope, ControlRequest, TargetWire, login_submits},
    process::Shared,
};
use anyhow::{Context, Result, anyhow};
use keyharbor_broker::CdpBrowser;
use keyharbor_domain::{
    policy::AgentName,
    ports::{BrowserPort, browser::WaitFor},
    vault::ItemName,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// A control request in our words.
enum Command {
    Navigate(String),
    Snapshot,
    Screenshot,
    Click(Value),
    Fill(Value, String),
    Type(String),
    Press(String),
    Tabs,
    NewTab(Option<String>),
    SwitchTab(String),
    Wait(WaitFor),
    /// Fill a login item into the page's sign-in form (R-39).
    Login {
        item: ItemName,
        submit: bool,
    },
    Close,
    Status,
}

/// The browser port's target shape for a wire target.
fn target(target: TargetWire) -> Value {
    match target {
        TargetWire::Ref(reference) => json!({"ref": reference}),
        TargetWire::Selector(selector) => json!({"selector": selector}),
    }
}

/// The agent (`cli` when absent) and the command a request asks for.
fn parse(request: &Value) -> Result<(String, Command)> {
    let envelope = ControlEnvelope::deserialize(request).context("invalid request")?;
    let agent = match envelope.agent {
        None => AgentName::local(),
        Some(agent) => AgentName::parse(&agent).map_err(|e| anyhow!("{e}"))?,
    }
    .as_str()
    .to_owned();
    let command = match envelope.request {
        ControlRequest::Navigate { url } => Command::Navigate(url),
        ControlRequest::Snapshot => Command::Snapshot,
        ControlRequest::Screenshot => Command::Screenshot,
        ControlRequest::Click { target: t } => Command::Click(target(t)),
        ControlRequest::Fill { target: t, text } => Command::Fill(target(t), text),
        ControlRequest::Type { text } => Command::Type(text),
        ControlRequest::Press { key } => Command::Press(key),
        ControlRequest::Tabs => Command::Tabs,
        ControlRequest::NewTab { url } => Command::NewTab(url),
        ControlRequest::SwitchTab { id } => Command::SwitchTab(id),
        ControlRequest::Wait { ms, selector } => Command::Wait(match (selector, ms) {
            (Some(selector), _) => WaitFor::Selector(selector),
            (None, Some(ms)) => WaitFor::Millis(ms),
            (None, None) => anyhow::bail!("wait needs ms or selector"),
        }),
        ControlRequest::Login { item, submit } => Command::Login {
            item: ItemName::parse(&item).map_err(|e| anyhow!("{e}"))?,
            submit: login_submits(submit),
        },
        ControlRequest::Close => Command::Close,
        ControlRequest::Status => Command::Status,
    };
    Ok((agent, command))
}

pub(crate) struct Ops {
    browser: CdpBrowser,
}

impl Ops {
    pub fn new(browser: CdpBrowser) -> Self {
        Self { browser }
    }

    pub async fn execute(&mut self, shared: &Shared, request: &Value) -> Result<Value> {
        let guard = &shared.guard;
        let (agent, command) = match parse(request) {
            Ok(parsed) => parsed,
            Err(error) => return guard.redact_outcome(Err(error)),
        };
        let mut browser = guard.browser(&mut self.browser, &agent);
        match command {
            Command::Navigate(url) => browser.navigate(&url).await,
            Command::Snapshot => browser.snapshot().await,
            Command::Screenshot => browser.screenshot().await,
            Command::Click(target) => browser.click(&target).await,
            Command::Fill(target, text) => browser.fill(&target, &text).await,
            Command::Type(text) => browser.type_text(&text).await,
            Command::Press(key) => browser.press(&key).await,
            Command::Tabs => browser.tabs().await,
            Command::NewTab(url) => browser.new_tab(url.as_deref()).await,
            Command::SwitchTab(id) => browser.switch_tab(&id).await,
            Command::Wait(until) => browser.wait(&until).await,
            Command::Login { item, submit } => browser.login(&item, submit).await,
            Command::Close => {
                let closed = browser.close();
                shared.request_close();
                closed
            }
            Command::Status => guard.redact_outcome(status(shared)),
        }
    }
}

fn status(shared: &Shared) -> Result<Value> {
    let store = shared
        .store
        .lock()
        .map_err(|_| anyhow!("store lock poisoned"))?;
    let row = store.get(shared.id)?;
    Ok(json!({
        "state": row.as_ref().map(|r| r.state.as_str()),
        "generation": shared.generation.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::policy::LOCAL_AGENT;

    #[test]
    fn requests_name_a_real_agent() {
        let (agent, _) = parse(&json!({"op": "snapshot"})).unwrap();
        assert_eq!(agent, LOCAL_AGENT);
        let (agent, _) = parse(&json!({"op": "snapshot", "agent": "claude"})).unwrap();
        assert_eq!(agent, "claude");
        for bad in [json!("*"), json!(""), json!("a b"), json!(3)] {
            assert!(
                parse(&json!({"op": "snapshot", "agent": bad})).is_err(),
                "{bad}"
            );
        }
    }
}
