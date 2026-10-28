//! Agent policy rules (02 §2.8). Deciding which rule wins lives in `keyharbor-core::policy`.

use crate::{
    SessionId,
    vault::{Domain, InvalidValue, ItemName},
};
use std::fmt;

/// What a rule is about.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Resource {
    /// Using one session (target: session id or `*`).
    Session,
    /// Using one vault item (target: item name or `*`).
    Item,
    /// Using any item of a vault domain (target: domain name or `*`).
    Domain,
    /// Creating sessions (target: `*`).
    CreateSession,
    /// Opening or reading a raw CDP endpoint (target: `*`).
    RawCdp,
}
impl Resource {
    pub const ALL: [Resource; 5] = [
        Self::Session,
        Self::Item,
        Self::Domain,
        Self::CreateSession,
        Self::RawCdp,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Item => "item",
            Self::Domain => "domain",
            Self::CreateSession => "create_session",
            Self::RawCdp => "raw_cdp",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Effect {
    Allow,
    Deny,
}
impl Effect {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [Self::Allow, Self::Deny]
            .into_iter()
            .find(|e| e.as_str() == value)
    }
}

/// Matches every agent or every target.
pub const ANY: &str = "*";

/// Agent names: 1–64 ASCII letters, digits, '-', '_' or '.', or `*` for every agent.
/// The name is chosen by whoever starts the MCP server; it is a label, not an identity.
pub fn parse_agent(value: &str) -> Result<String, InvalidValue> {
    let valid = value == ANY
        || (!value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')));
    if !valid {
        return Err(InvalidValue(
            "agent name must be 1–64 ASCII letters, digits, '-', '_' or '.', or *",
        ));
    }
    Ok(value.to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    pub agent: String,
    pub resource: Resource,
    pub target: String,
    pub effect: Effect,
}
impl Rule {
    /// Validates the agent and that the target fits the resource.
    pub fn new(
        agent: &str,
        resource: Resource,
        target: &str,
        effect: Effect,
    ) -> Result<Self, InvalidValue> {
        let agent = parse_agent(agent)?;
        let target = normalize_target(resource, target)?;
        Ok(Self {
            agent,
            resource,
            target,
            effect,
        })
    }
}
impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.agent,
            self.resource.as_str(),
            self.target,
            self.effect.as_str()
        )
    }
}

/// The stored form of a target: `*`, a session id, an item name or a domain name.
pub fn normalize_target(resource: Resource, target: &str) -> Result<String, InvalidValue> {
    if target == ANY {
        return Ok(target.to_owned());
    }
    match resource {
        Resource::Session => SessionId::parse(target)
            .map(|id| id.to_string())
            .map_err(|_| InvalidValue("session target must be a session id or *")),
        Resource::Item => ItemName::parse(target).map(|n| n.as_str().to_owned()),
        Resource::Domain => {
            Domain::parse(target)
                .map(|d| d.as_str().to_owned())
                .ok_or(InvalidValue(
                    "domain target must be credentials, payment, personal or *",
                ))
        }
        Resource::CreateSession | Resource::RawCdp => Err(InvalidValue(
            "create_session and raw_cdp rules take only the target *",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_validate_agent_and_target() {
        let id = SessionId::new().to_string();
        assert!(Rule::new("claude", Resource::Session, &id, Effect::Deny).is_ok());
        assert!(Rule::new("*", Resource::Item, "github", Effect::Allow).is_ok());
        assert_eq!(
            Rule::new("codex", Resource::Domain, "payment", Effect::Deny)
                .unwrap()
                .to_string(),
            "codex domain payment deny"
        );
        assert!(Rule::new("codex", Resource::RawCdp, "*", Effect::Deny).is_ok());
        for (agent, resource, target) in [
            ("", Resource::Item, "github"),
            ("a b", Resource::Item, "github"),
            ("claude", Resource::Session, "not-an-id"),
            ("claude", Resource::Item, "my.github"),
            ("claude", Resource::Domain, "money"),
            ("claude", Resource::CreateSession, "x"),
            ("claude", Resource::RawCdp, &id),
        ] {
            assert!(
                Rule::new(agent, resource, target, Effect::Deny).is_err(),
                "{agent:?} {resource:?} {target:?}"
            );
        }
        for resource in Resource::ALL {
            assert_eq!(Resource::parse(resource.as_str()), Some(resource));
        }
        assert_eq!(Effect::parse("deny"), Some(Effect::Deny));
        assert_eq!(Effect::parse("maybe"), None);
    }
}
