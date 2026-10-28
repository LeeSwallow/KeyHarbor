//! Control operations: one JSON request → one command, dispatched to the session's browser
//! through `GuardedBrowser`, the only path from the control socket to the page. The guard
//! resolves placeholders in `fill`/`type` text and masks every result and error exactly once;
//! requests that never reach the browser (`status`, malformed requests) are masked here.

use crate::{browser::CdpBrowser, supervisor::Shared};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    policy::{LOCAL_AGENT, parse_agent},
    ports::{BrowserPort, browser::WaitFor},
};
use serde_json::{Value, json};

/// A parsed control request.
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
    Close,
    Status,
}

fn str_arg(request: &Value, name: &str) -> Result<String> {
    request
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("missing {name}"))
}

fn target(request: &Value) -> Result<Value> {
    request.get("target").cloned().context("missing target")
}

/// The agent (`cli` when absent) and the command a request asks for.
fn parse(request: &Value) -> Result<(String, Command)> {
    let op = request
        .get("op")
        .and_then(Value::as_str)
        .context("request has no op")?;
    let agent = match request.get("agent") {
        None => LOCAL_AGENT.to_owned(),
        Some(agent) => parse_agent(agent.as_str().context("agent must be a string")?)
            .map_err(|e| anyhow!("{e}"))?,
    };
    let command = match op {
        "navigate" => Command::Navigate(str_arg(request, "url")?),
        "snapshot" => Command::Snapshot,
        "screenshot" => Command::Screenshot,
        "click" => Command::Click(target(request)?),
        "fill" => {
            let target = target(request)?;
            Command::Fill(target, str_arg(request, "text")?)
        }
        "type" => Command::Type(str_arg(request, "text")?),
        "press" => Command::Press(str_arg(request, "key")?),
        "tabs" => Command::Tabs,
        "new_tab" => Command::NewTab(
            request
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
        "switch_tab" => Command::SwitchTab(str_arg(request, "id")?),
        "wait" => Command::Wait(match request.get("selector").and_then(Value::as_str) {
            Some(selector) => WaitFor::Selector(selector.to_owned()),
            None => WaitFor::Millis(
                request
                    .get("ms")
                    .and_then(Value::as_u64)
                    .context("wait needs ms or selector")?,
            ),
        }),
        "close" => Command::Close,
        "status" => Command::Status,
        _ => bail!("unknown operation"),
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
