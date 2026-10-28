//! Operations that weaken a protection need a person's confirmation (R-35, 03 §3.12).
//!
//! Which changes count as weakening is decided here, once, for every interface: the CLI asks
//! at the terminal and the admin app shows its own dialog, both through
//! [`Confirm`]. Making things stricter never asks.
//!
//! This is a weak boundary: a process that can drive a pseudo-terminal can answer for a
//! person. It stops an agent that merely runs commands, not one that emulates a terminal.

use crate::{ItemChange, Vault};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    date::Expiry,
    policy::{Effect, Rule},
    ports::{Answer, Confirm},
    settings::SettingSpec,
    vault::ItemName,
};
use std::fmt;

pub use keyharbor_domain::settings::CAPTURE_ENABLED;

/// Every operation that weakens a protection. What each one is shown as is defined here
/// only ([`fmt::Display`]); which changes produce one is decided by the functions below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeakeningOp {
    OriginBindingOff {
        item: String,
    },
    OriginsAdded {
        item: String,
        origins: Vec<String>,
    },
    RunAllowed {
        item: String,
    },
    ExpiryExtended {
        item: String,
        from: Expiry,
        to: Expiry,
    },
    ExpiryRemoved {
        item: String,
    },
    PolicyAllow(Rule),
    PolicyRemoveDeny(Rule),
    /// A setting change the registry marks as weakening (`SettingSpec::weakens`).
    Setting {
        key: &'static str,
        value: String,
    },
}

impl fmt::Display for WeakeningOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OriginBindingOff { item } => write!(f, "turning origin binding off for {item}"),
            Self::OriginsAdded { item, origins } => {
                write!(f, "adding {} to the origins of {item}", origins.join(", "))
            }
            Self::RunAllowed { item } => write!(f, "letting keyharbor run inject {item}"),
            Self::ExpiryExtended { item, from, to } => {
                write!(f, "extending the expiry of {item} from {from} to {to}")
            }
            Self::ExpiryRemoved { item } => write!(f, "removing the expiry of {item}"),
            Self::PolicyAllow(rule) => write!(f, "policy allow {}", target(rule)),
            Self::PolicyRemoveDeny(rule) => write!(f, "removing the rule {}", target(rule)),
            Self::Setting { key, value } => write!(f, "settings set {key} {value}"),
        }
    }
}

fn target(rule: &Rule) -> String {
    format!("{} {} {}", rule.agent, rule.resource.as_str(), rule.target)
}

/// Goes ahead only if every operation in `ops` is confirmed (one question for all).
pub fn require(confirm: &dyn Confirm, ops: &[WeakeningOp]) -> Result<()> {
    if ops.is_empty() {
        return Ok(());
    }
    let action = ops
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    match confirm.confirm(&action) {
        Answer::Yes => Ok(()),
        Answer::No => bail!("{action}: not confirmed"),
        Answer::Unavailable => {
            bail!("{action} needs confirmation from a person at a terminal (or the admin app)")
        }
    }
}

/// `policy allow …` (any target): a rule that allows can only widen what an agent may do.
pub fn policy_set(rule: &Rule) -> Option<WeakeningOp> {
    (rule.effect == Effect::Allow).then(|| WeakeningOp::PolicyAllow(rule.clone()))
}

/// `policy rm …` of a deny rule (removing an allow rule never widens access).
pub fn policy_remove(existing: &Rule) -> Option<WeakeningOp> {
    (existing.effect == Effect::Deny).then(|| WeakeningOp::PolicyRemoveDeny(existing.clone()))
}

/// `settings set`: whatever the registry marks as weakening (turning the usage record off,
/// changing capture at all).
pub fn setting(key: &str, value: &str) -> Option<WeakeningOp> {
    let spec = SettingSpec::find(key)?;
    spec.weakens(value).then(|| WeakeningOp::Setting {
        key: spec.key,
        value: value.to_owned(),
    })
}

impl Vault {
    /// What in `change` weakens the item's protection: turning origin binding off, adding an
    /// origin, letting `keyharbor run` inject it.
    pub fn weakenings(&self, name: &ItemName, change: &ItemChange) -> Result<Vec<WeakeningOp>> {
        let current = self.item_use(name)?.context("item not found")?;
        let item = name.as_str().to_owned();
        let mut out = Vec::new();
        if change.origin_binding == Some(false) && current.origin_binding {
            out.push(WeakeningOp::OriginBindingOff { item: item.clone() });
        }
        if let Some(origins) = &change.origins {
            let added: Vec<String> = origins
                .iter()
                .filter(|o| !current.origins.contains(o))
                .map(|o| o.as_str().to_owned())
                .collect();
            if !added.is_empty() {
                out.push(WeakeningOp::OriginsAdded {
                    item: item.clone(),
                    origins: added,
                });
            }
        }
        if change.run_allowed == Some(true) && !current.run_allowed {
            out.push(WeakeningOp::RunAllowed { item: item.clone() });
        }
        // Setting a first expiry or shortening one never asks (R-37).
        match (current.expires_at, change.expiry) {
            (Some(_), Some(None)) => out.push(WeakeningOp::ExpiryRemoved { item }),
            (Some(from), Some(Some(to))) if to > from => {
                out.push(WeakeningOp::ExpiryExtended { item, from, to });
            }
            _ => {}
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, init, login};
    use keyharbor_domain::{policy::Resource, settings::AUDIT_ENABLED, vault::Origin};
    use std::cell::RefCell;

    /// Answers `answer` and remembers what it was asked.
    struct Fake {
        answer: Answer,
        asked: RefCell<Vec<String>>,
    }
    impl Confirm for Fake {
        fn confirm(&self, action: &str) -> Answer {
            self.asked.borrow_mut().push(action.to_owned());
            self.answer
        }
    }
    fn fake(answer: Answer) -> Fake {
        Fake {
            answer,
            asked: RefCell::default(),
        }
    }

    #[test]
    fn weakening_needs_a_yes_and_nothing_else_asks() {
        let yes = fake(Answer::Yes);
        require(&yes, &[]).unwrap();
        assert!(yes.asked.borrow().is_empty());
        let ops = [
            setting(AUDIT_ENABLED, "false").unwrap(),
            setting(CAPTURE_ENABLED, "true").unwrap(),
        ];
        require(&yes, &ops).unwrap();
        assert_eq!(
            *yes.asked.borrow(),
            vec!["settings set audit_enabled false; settings set capture_enabled true".to_owned()]
        );
        let err = require(&fake(Answer::No), &ops[..1]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "settings set audit_enabled false: not confirmed"
        );
        let err = require(&fake(Answer::Unavailable), &ops[..1]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "settings set audit_enabled false needs confirmation from a person at a terminal (or the admin app)"
        );
    }

    #[test]
    fn policy_and_settings_changes_that_widen_access_are_weakenings() {
        let allow = Rule::new("*", Resource::Item, "github", Effect::Allow).unwrap();
        let deny = Rule::new("codex", Resource::Domain, "payment", Effect::Deny).unwrap();
        assert_eq!(
            policy_set(&allow).unwrap().to_string(),
            "policy allow * item github"
        );
        assert_eq!(policy_set(&deny), None);
        assert_eq!(
            policy_remove(&deny).unwrap().to_string(),
            "removing the rule codex domain payment"
        );
        assert_eq!(policy_remove(&allow), None);
        assert!(setting(AUDIT_ENABLED, "false").is_some());
        assert!(setting(AUDIT_ENABLED, "true").is_none());
        assert!(setting(CAPTURE_ENABLED, "false").is_some());
    }

    #[test]
    fn item_changes_that_widen_use_are_weakenings() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let name = ItemName::parse("github").unwrap();
        let github = Origin::parse("https://github.com").unwrap();
        let gist = Origin::parse("https://gist.github.com").unwrap();
        let none = v.weakenings(&name, &ItemChange::default()).unwrap();
        assert!(none.is_empty());
        let stricter = ItemChange {
            origins: Some(vec![]),
            origin_binding: Some(true),
            run_allowed: Some(false),
            ..Default::default()
        };
        assert!(v.weakenings(&name, &stricter).unwrap().is_empty());
        let same = ItemChange {
            origins: Some(vec![github.clone()]),
            ..Default::default()
        };
        assert!(v.weakenings(&name, &same).unwrap().is_empty());
        let wider = ItemChange {
            origins: Some(vec![github, gist]),
            origin_binding: Some(false),
            run_allowed: Some(true),
            ..Default::default()
        };
        let shown: Vec<String> = v
            .weakenings(&name, &wider)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            shown,
            vec![
                "turning origin binding off for github",
                "adding https://gist.github.com to the origins of github",
                "letting keyharbor run inject github",
            ]
        );
    }
}
