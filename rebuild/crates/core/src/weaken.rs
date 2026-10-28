//! Operations that weaken a protection need a person's confirmation (R-35, 03 §3.12).
//!
//! Which changes count as weakening is decided here, once, for every interface: the CLI asks
//! at the terminal and the admin app shows its own dialog, both through
//! [`Confirmation`]. Making things stricter never asks.
//!
//! This is a weak boundary: a process that can drive a pseudo-terminal can answer for a
//! person. It stops an agent that merely runs commands, not one that emulates a terminal.

use crate::{ItemChange, Vault};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    policy::{Effect, Rule},
    ports::{AUDIT_ENABLED, Answer, Confirmation},
    vault::ItemName,
};

/// The setting that turns credential capture (phase 4) on or off.
pub const CAPTURE_ENABLED: &str = "capture_enabled";

/// Goes ahead only if every action in `weakenings` is confirmed (one question for all).
pub fn require(confirm: &dyn Confirmation, weakenings: &[String]) -> Result<()> {
    if weakenings.is_empty() {
        return Ok(());
    }
    let action = weakenings.join("; ");
    match confirm.confirm(&action) {
        Answer::Yes => Ok(()),
        Answer::No => bail!("{action}: not confirmed"),
        Answer::Unavailable => {
            bail!("{action} needs confirmation from a person at a terminal (or the admin app)")
        }
    }
}

/// `policy allow …` (any target): a rule that allows can only widen what an agent may do.
pub fn policy_set(rule: &Rule) -> Option<String> {
    (rule.effect == Effect::Allow).then(|| format!("policy allow {}", describe(rule)))
}

/// `policy rm …` of a deny rule (removing an allow rule never widens access).
pub fn policy_remove(existing: &Rule) -> Option<String> {
    (existing.effect == Effect::Deny).then(|| format!("removing the rule {}", describe(existing)))
}

fn describe(rule: &Rule) -> String {
    format!("{} {} {}", rule.agent, rule.resource.as_str(), rule.target)
}

/// `settings set`: turning the usage record off, or changing capture at all.
pub fn setting(key: &str, value: &str) -> Option<String> {
    match key {
        AUDIT_ENABLED if value != "true" => Some(format!("settings set {key} {value}")),
        CAPTURE_ENABLED => Some(format!("settings set {key} {value}")),
        _ => None,
    }
}

impl Vault {
    /// What in `change` weakens the item's protection: turning origin binding off, adding an
    /// origin, letting `keyharbor run` inject it.
    pub fn weakenings(&self, name: &ItemName, change: &ItemChange) -> Result<Vec<String>> {
        let current = self.item_use(name)?.context("item not found")?;
        let item = name.as_str();
        let mut out = Vec::new();
        if change.origin_binding == Some(false) && current.origin_binding {
            out.push(format!("turning origin binding off for {item}"));
        }
        if let Some(origins) = &change.origins {
            let added: Vec<&str> = origins
                .iter()
                .filter(|o| !current.origins.contains(o))
                .map(|o| o.as_str())
                .collect();
            if !added.is_empty() {
                out.push(format!(
                    "adding {} to the origins of {item}",
                    added.join(", ")
                ));
            }
        }
        if change.run_allowed == Some(true) && !current.run_allowed {
            out.push(format!("letting keyharbor run inject {item}"));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, init, login};
    use keyharbor_domain::{policy::Resource, vault::Origin};
    use std::cell::RefCell;

    /// Answers `answer` and remembers what it was asked.
    struct Fake {
        answer: Answer,
        asked: RefCell<Vec<String>>,
    }
    impl Confirmation for Fake {
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
        require(&yes, &["a".into(), "b".into()]).unwrap();
        assert_eq!(*yes.asked.borrow(), vec!["a; b".to_owned()]);
        let err = require(&fake(Answer::No), &["x".into()]).unwrap_err();
        assert_eq!(err.to_string(), "x: not confirmed");
        let err = require(&fake(Answer::Unavailable), &["x".into()]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "x needs confirmation from a person at a terminal (or the admin app)"
        );
    }

    #[test]
    fn policy_and_settings_changes_that_widen_access_are_weakenings() {
        let allow = Rule::new("*", Resource::Item, "github", Effect::Allow).unwrap();
        let deny = Rule::new("codex", Resource::Domain, "payment", Effect::Deny).unwrap();
        assert_eq!(
            policy_set(&allow).as_deref(),
            Some("policy allow * item github")
        );
        assert_eq!(policy_set(&deny), None);
        assert_eq!(
            policy_remove(&deny).as_deref(),
            Some("removing the rule codex domain payment")
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
        assert_eq!(
            v.weakenings(&name, &wider).unwrap(),
            vec![
                "turning origin binding off for github",
                "adding https://gist.github.com to the origins of github",
                "letting keyharbor run inject github",
            ]
        );
    }
}
