//! A control request in our words: [`parse`] turns the wire envelope ([`super::wire`]) into an
//! agent name and a [`Command`], checking every value it names on the way. Executing it is
//! `ops`' job.

use super::wire::{ControlEnvelope, ControlRequest, login_submits};
use anyhow::{Context, Result, anyhow};
use keyharbor_domain::{
    handoff_site::{self, SiteCookie},
    policy::AgentName,
    ports::browser::{Choice, Target, WaitFor},
    vault::{ItemName, Origin},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// A control request in our words.
pub(crate) enum Command {
    Navigate(String),
    Snapshot,
    Screenshot,
    Click(Target),
    Fill(Target, String),
    Select(Target, Choice),
    Upload(Target, Vec<String>),
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
    /// A site handed over by the native host (R-42).
    ReceiveSite {
        origin: Origin,
        cookies: Vec<SiteCookie>,
        local_storage: Option<BTreeMap<String, Zeroizing<String>>>,
        ttl_seconds: u64,
    },
    Close,
    Status,
    /// Let the session substitute again (#190): a person's request only.
    Unlock,
}

/// The refusal of a request only a person may send, when it names an agent. The MCP server
/// names its agent in every request, so no agent lifts a lock through it (#190, J-220).
pub(crate) const PERSON_ONLY: &str =
    "denied: only a person lifts a session's lock (keyharbor unlock, or the admin app)";

impl Command {
    /// The document a command loads, if it names one.
    pub(crate) fn document(&self) -> Option<&str> {
        match self {
            Self::Navigate(url) | Self::NewTab(Some(url)) => Some(url),
            _ => None,
        }
    }

    /// Commands that act on the page. While the session is handed to the user they are
    /// refused: the person at the browser is the only one typing, so what capture sees is
    /// theirs (J-80). Reading the page (snapshot, tabs, screenshot, wait) still works.
    pub(crate) fn is_input(&self) -> bool {
        matches!(
            self,
            Self::Navigate(_)
                | Self::Click(_)
                | Self::Fill(..)
                | Self::Select(..)
                | Self::Upload(..)
                | Self::Type(_)
                | Self::Press(_)
                | Self::NewTab(_)
                | Self::SwitchTab(_)
                | Self::Login { .. }
        )
    }

    /// Commands only a person may send: a request naming an agent is refused (#190).
    pub(crate) fn is_for_a_person(&self) -> bool {
        matches!(self, Self::Unlock)
    }
}

/// The agent (`cli` when absent) and the command a request asks for.
pub(crate) fn parse(request: &Value) -> Result<(String, Command)> {
    let envelope = ControlEnvelope::deserialize(request).context("invalid request")?;
    let agent = match &envelope.agent {
        None => AgentName::local(),
        Some(agent) => AgentName::parse(agent).map_err(|e| anyhow!("{e}"))?,
    }
    .as_str()
    .to_owned();
    let command = command(envelope.request)?;
    if envelope.agent.is_some() && command.is_for_a_person() {
        anyhow::bail!(PERSON_ONLY);
    }
    Ok((agent, command))
}

fn command(request: ControlRequest) -> Result<Command> {
    Ok(match request {
        ControlRequest::Navigate { url } => Command::Navigate(url),
        ControlRequest::Snapshot => Command::Snapshot,
        ControlRequest::Screenshot => Command::Screenshot,
        ControlRequest::Click { target } => Command::Click(target.into()),
        ControlRequest::Fill { target, text } => Command::Fill(target.into(), text),
        ControlRequest::Select {
            target,
            value,
            label,
            index,
        } => Command::Select(target.into(), choice(value, label, index)?),
        ControlRequest::Upload { target, files } => Command::Upload(target.into(), files),
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
        ControlRequest::ReceiveSite {
            origin,
            cookies,
            local_storage,
            ttl_seconds,
        } => {
            let origin = Origin::parse(&origin).map_err(|e| anyhow!("{e}"))?;
            let cookies: Vec<SiteCookie> = cookies.into_iter().map(Into::into).collect();
            handoff_site::check(&origin, &cookies, ttl_seconds).map_err(|e| anyhow!("{e}"))?;
            Command::ReceiveSite {
                origin,
                cookies,
                local_storage: local_storage.map(|items| {
                    items
                        .into_iter()
                        .map(|(k, v)| (k, Zeroizing::new(v)))
                        .collect()
                }),
                ttl_seconds,
            }
        }
        ControlRequest::Close => Command::Close,
        ControlRequest::Status => Command::Status,
        ControlRequest::Unlock => Command::Unlock,
    })
}

/// The one option a `select` names.
fn choice(value: Option<String>, label: Option<String>, index: Option<u32>) -> Result<Choice> {
    Ok(match (value, label, index) {
        (Some(value), None, None) => Choice::Value(value),
        (None, Some(label), None) => Choice::Label(label),
        (None, None, Some(index)) => Choice::Index(index),
        _ => anyhow::bail!("select needs exactly one of value, label or index"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::policy::LOCAL_AGENT;
    use serde_json::json;

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

    #[test]
    fn only_a_person_unlocks_a_session() {
        let (agent, command) = parse(&json!({"op": "unlock"})).unwrap();
        assert_eq!(agent, LOCAL_AGENT);
        assert!(matches!(command, Command::Unlock));
        // What the MCP server sends always names its agent, so no agent can lift a lock.
        for agent in ["claude", LOCAL_AGENT] {
            let Err(error) = parse(&json!({"op": "unlock", "agent": agent})) else {
                panic!("{agent} unlocked a session");
            };
            assert_eq!(error.to_string(), PERSON_ONLY);
        }
    }

    #[test]
    fn select_is_refused_while_awaiting_user() {
        let (_, command) = parse(&json!({
            "op": "select", "target": {"ref": 3}, "label": "Exempt"
        }))
        .unwrap();
        assert!(
            matches!(command, Command::Select(Target::Ref(3), Choice::Label(ref l)) if l == "Exempt")
        );
        assert!(
            command.is_input(),
            "a person at the browser is the only one choosing"
        );
    }

    #[test]
    fn select_needs_exactly_one_choice() {
        for request in [
            json!({"op": "select", "target": {"ref": 3}}),
            json!({"op": "select", "target": {"ref": 3}, "value": "a", "index": 1}),
        ] {
            let Err(error) = parse(&request) else {
                panic!("{request} should be refused");
            };
            assert_eq!(
                error.to_string(),
                "select needs exactly one of value, label or index"
            );
        }
    }
}
