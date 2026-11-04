//! Control operations: one parsed [`Command`] dispatched to the session's browser through
//! `GuardedBrowser`, the only path from the control socket to the page. The guard resolves
//! placeholders in `fill`/`type` text and masks every result and error exactly once;
//! requests that never reach the browser (`status`, malformed or refused requests) are
//! masked here.

use crate::{
    control::{
        command::{Command, parse},
        wire::{FillResultWire, LoginResultWire},
    },
    process::Shared,
    upload_files,
};
use anyhow::{Result, anyhow};
use keyharbor_broker::CdpBrowser;
use keyharbor_core::grant;
use keyharbor_domain::{
    State,
    audit::{AuditEvent, AuditKind},
    handoff_site::SiteCookie,
    ports::BrowserPort,
    upload::UploadPolicy,
    vault::Origin,
};
use keyharbor_guard::{audit, lock_store};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// The refusal for a document outside the session's grants (also at start, before the browser).
pub(crate) const DENIED_OUT_OF_GRANT: &str = "denied: navigation (out-of-grant)";
/// The refusal for page input while the browser is with the user.
const DENIED_WITH_USER: &str =
    "the browser is with the user until they resume; call session_wait_for_resume";

/// Whether the session's grants let it load `url` (R-43). The page watcher enforces the
/// same rule for every document; checking here gives the agent a plain refusal.
fn in_grants(shared: &Shared, url: &str) -> Result<bool> {
    let store = lock_store(&shared.store)?;
    grant::allows(&**store, &*shared.clock, shared.id, url)
}

/// The person's upload rule, read now so a change applies to the next request.
fn upload_policy(shared: &Shared) -> Result<UploadPolicy> {
    let store = lock_store(&shared.store)?;
    upload_files::policy(&**store, &upload_files::home())
}

/// Whether the session is waiting for the user (a handoff is open).
fn with_user(shared: &Shared) -> Result<bool> {
    let store = lock_store(&shared.store)?;
    Ok(store
        .get(shared.id)?
        .is_some_and(|row| row.state == State::AwaitingUser))
}

/// The refusal for `command` before it reaches the browser, if any: a document outside the
/// session's grants (R-43), or page input while the browser is with the user (J-80).
fn precheck(shared: &Shared, command: &Command) -> Result<Option<&'static str>> {
    if let Some(url) = command.document()
        && !in_grants(shared, url)?
    {
        return Ok(Some(DENIED_OUT_OF_GRANT));
    }
    if command.is_input() && with_user(shared)? {
        return Ok(Some(DENIED_WITH_USER));
    }
    Ok(None)
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
        if let Some(refusal) = precheck(shared, &command)? {
            return guard.redact_outcome(Err(anyhow!(refusal)));
        }
        // Not an agent's page operation: it talks to the browser directly (cookies, not text).
        if let Command::ReceiveSite {
            origin,
            cookies,
            local_storage,
            ttl_seconds,
        } = command
        {
            let received = self
                .receive_site(
                    shared,
                    origin,
                    &cookies,
                    local_storage.as_ref(),
                    ttl_seconds,
                )
                .await;
            return guard.redact_outcome(received);
        }
        self.on_page(shared, &agent, command).await
    }

    /// Runs a page command through the guard, which masks its outcome.
    async fn on_page(&mut self, shared: &Shared, agent: &str, command: Command) -> Result<Value> {
        let guard = &shared.guard;
        let mut browser = guard.browser(&mut self.browser, agent);
        match command {
            Command::Navigate(url) => browser.navigate(&url).await,
            Command::Snapshot => browser.snapshot().await,
            Command::Screenshot => browser.screenshot().await,
            Command::Click(target) => browser.click(&target).await,
            Command::Fill(target, text) => {
                wire(browser.fill(&target, &text).await, FillResultWire::from)
            }
            Command::Select(target, choice) => browser.select(&target, &choice).await,
            Command::Upload(target, files) => match upload_policy(shared) {
                Ok(policy) => {
                    let files = upload_files::resolve(&files);
                    browser.upload(&target, &policy, &files).await
                }
                Err(error) => guard.redact_outcome(Err(error)),
            },
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
            Command::ReceiveSite { .. } => Err(anyhow!("receive_site is handled above")),
        }
    }

    /// Grants the site for `ttl_seconds`, puts its cookies (and `localStorage`) into the
    /// profile and opens it (R-42). The grant comes first so the gate lets the origin load.
    async fn receive_site(
        &mut self,
        shared: &Shared,
        origin: Origin,
        cookies: &[SiteCookie],
        local_storage: Option<&BTreeMap<String, Zeroizing<String>>>,
        ttl_seconds: u64,
    ) -> Result<Value> {
        grant::add_for_handoff(
            &**lock_store(&shared.store)?,
            &*shared.clock,
            shared.id,
            origin.clone(),
            ttl_seconds,
        )?;
        let count = self
            .browser
            .receive_site(origin.as_str(), cookies, local_storage)
            .await?;
        if let Ok(store) = lock_store(&shared.store) {
            audit::record(
                &**store,
                &AuditEvent::new(grant::HANDOFF, AuditKind::SiteHandedOff, &count.to_string())
                    .session(shared.id)
                    .origin(Some(origin.as_str())),
            );
        }
        Ok(json!({"cookies": count}))
    }
}

/// A guard outcome (already masked) in its control-protocol shape.
fn wire<T, W: serde::Serialize>(outcome: Result<T>, shape: impl FnOnce(T) -> W) -> Result<Value> {
    Ok(serde_json::to_value(shape(outcome?))?)
}

fn status(shared: &Shared) -> Result<Value> {
    let row = lock_store(&shared.store)?.get(shared.id)?;
    Ok(json!({
        "state": row.as_ref().map(|r| r.state.as_str()),
        "generation": shared.generation.to_string(),
    }))
}
