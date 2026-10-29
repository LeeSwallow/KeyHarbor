//! Control operations: one JSON request → one command, dispatched to the session's browser
//! through `GuardedBrowser`, the only path from the control socket to the page. The guard
//! resolves placeholders in `fill`/`type` text and masks every result and error exactly once;
//! requests that never reach the browser (`status`, malformed requests) are masked here.

use crate::{
    control::wire::{
        ControlEnvelope, ControlRequest, FillResultWire, LoginResultWire, login_submits,
    },
    process::Shared,
};
use anyhow::{Context, Result, anyhow};
use keyharbor_broker::CdpBrowser;
use keyharbor_domain::{
    policy::AgentName,
    ports::{
        BrowserPort,
        browser::{Target, WaitFor},
    },
    vault::ItemName,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// A control request in our words.
enum Command {
    Navigate(String),
    Snapshot,
    Screenshot,
    Click(Target),
    Fill(Target, String),
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

impl Command {
    /// Commands that act on the page. While the session is handed to the user they are
    /// refused: the person at the browser is the only one typing, so what capture sees is
    /// theirs (J-80). Reading the page (snapshot, tabs, screenshot, wait) still works.
    /// The document a command loads, if it names one.
    fn document(&self) -> Option<&str> {
        match self {
            Self::Navigate(url) | Self::NewTab(Some(url)) => Some(url),
            _ => None,
        }
    }

    fn is_input(&self) -> bool {
        matches!(
            self,
            Self::Navigate(_)
                | Self::Click(_)
                | Self::Fill(..)
                | Self::Type(_)
                | Self::Press(_)
                | Self::NewTab(_)
                | Self::SwitchTab(_)
                | Self::Login { .. }
        )
    }
}

/// Whether the session's grants let it load `url` (R-43). The page watcher enforces the
/// same rule for every document; checking here gives the agent a plain refusal.
fn in_grants(shared: &Shared, url: &str) -> Result<bool> {
    let store = shared
        .store
        .lock()
        .map_err(|_| anyhow!("store lock poisoned"))?;
    keyharbor_core::grant::allows(&**store, &*shared.clock, shared.id, url)
}

/// Whether the session is waiting for the user (a handoff is open).
fn with_user(shared: &Shared) -> Result<bool> {
    let store = shared
        .store
        .lock()
        .map_err(|_| anyhow!("store lock poisoned"))?;
    Ok(store
        .get(shared.id)?
        .is_some_and(|row| row.state == keyharbor_domain::State::AwaitingUser))
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
        ControlRequest::Click { target } => Command::Click(target.into()),
        ControlRequest::Fill { target, text } => Command::Fill(target.into(), text),
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
        if let Some(url) = command.document()
            && !in_grants(shared, url)?
        {
            return guard.redact_outcome(Err(anyhow!("denied: navigation (out-of-grant)")));
        }
        if command.is_input() && with_user(shared)? {
            return guard.redact_outcome(Err(anyhow!(
                "the browser is with the user until they resume; call session_wait_for_resume"
            )));
        }
        let mut browser = guard.browser(&mut self.browser, &agent);
        match command {
            Command::Navigate(url) => browser.navigate(&url).await,
            Command::Snapshot => browser.snapshot().await,
            Command::Screenshot => browser.screenshot().await,
            Command::Click(target) => browser.click(&target).await,
            Command::Fill(target, text) => {
                wire(browser.fill(&target, &text).await, FillResultWire::from)
            }
            Command::Type(text) => wire(browser.type_text(&text).await, FillResultWire::from),
            Command::Press(key) => browser.press(&key).await,
            Command::Tabs => browser.tabs().await,
            Command::NewTab(url) => browser.new_tab(url.as_deref()).await,
            Command::SwitchTab(id) => browser.switch_tab(&id).await,
            Command::Wait(until) => browser.wait(&until).await,
            Command::Login { item, submit } => {
                wire(browser.login(&item, submit).await, LoginResultWire::from)
            }
            Command::Close => {
                let closed = browser.close();
                shared.request_close();
                closed.map(|()| json!({}))
            }
            Command::Status => guard.redact_outcome(status(shared)),
        }
    }
}

/// A guard outcome (already masked) in its control-protocol shape.
fn wire<T, W: serde::Serialize>(outcome: Result<T>, shape: impl FnOnce(T) -> W) -> Result<Value> {
    Ok(serde_json::to_value(shape(outcome?))?)
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
