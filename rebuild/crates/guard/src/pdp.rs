//! The policy decision point (02 §2.8). Rules come from the policy store; deciding is a pure
//! function of the rules, the agent and the action.
//!
//! Applicable rules are the agent's own and the `*` rules. The most specific one wins: an
//! agent rule beats a `*` rule; then an item target beats a domain target, which beats `*`.
//! At equal specificity deny wins. No applicable rule means allow.

use keyharbor_domain::{
    SessionId,
    policy::{ANY, Effect, Resource, Rule},
    vault::{Domain, ItemName},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    OpenSession(SessionId),
    CreateSession,
    UseItem { item: ItemName, domain: Domain },
    RawCdp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    Allow,
    /// `rule` describes the deciding rule, e.g. `codex domain payment deny`.
    Deny {
        rule: String,
    },
}
impl Decision {
    pub fn allowed(&self) -> bool {
        *self == Self::Allow
    }
}

/// How specifically `rule`'s target names `action`; `None` if it does not apply.
fn target_level(rule: &Rule, action: &Action) -> Option<u8> {
    let any = rule.target == ANY;
    match (action, rule.resource) {
        (Action::OpenSession(id), Resource::Session) => {
            if any {
                Some(0)
            } else {
                (rule.target == id.to_string()).then_some(2)
            }
        }
        (Action::CreateSession, Resource::CreateSession) | (Action::RawCdp, Resource::RawCdp) => {
            Some(0)
        }
        (Action::UseItem { item, .. }, Resource::Item) => {
            if any {
                Some(0)
            } else {
                (rule.target == item.as_str()).then_some(2)
            }
        }
        (Action::UseItem { domain, .. }, Resource::Domain) => {
            if any {
                Some(0)
            } else {
                (rule.target == domain.as_str()).then_some(1)
            }
        }
        _ => None,
    }
}

pub fn decide(rules: &[Rule], agent: &str, action: &Action) -> Decision {
    let mut best: Option<((u8, u8), &Rule)> = None;
    for rule in rules {
        let agent_level = if rule.agent == agent {
            1
        } else if rule.agent == ANY {
            0
        } else {
            continue;
        };
        let Some(target) = target_level(rule, action) else {
            continue;
        };
        let score = (agent_level, target);
        let better = match best {
            None => true,
            Some((top, current)) => {
                score > top
                    || (score == top
                        && rule.effect == Effect::Deny
                        && current.effect == Effect::Allow)
            }
        };
        if better {
            best = Some((score, rule));
        }
    }
    match best {
        Some((_, rule)) if rule.effect == Effect::Deny => Decision::Deny {
            rule: rule.to_string(),
        },
        _ => Decision::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(agent: &str, resource: Resource, target: &str, effect: Effect) -> Rule {
        Rule::new(agent, resource, target, effect).unwrap()
    }
    fn use_item(item: &str, domain: Domain) -> Action {
        Action::UseItem {
            item: ItemName::parse(item).unwrap(),
            domain,
        }
    }
    fn denied(rules: &[Rule], agent: &str, action: &Action) -> bool {
        !decide(rules, agent, action).allowed()
    }
    use Effect::{Allow, Deny};
    use Resource::{CreateSession, Item, RawCdp, Session};
    const DOM: Resource = Resource::Domain;

    #[test]
    fn no_rules_allow_everything() {
        let id = SessionId::new();
        for action in [
            Action::OpenSession(id),
            Action::CreateSession,
            Action::RawCdp,
            use_item("github", Domain::Credentials),
        ] {
            assert_eq!(decide(&[], "claude", &action), Decision::Allow);
        }
    }

    #[test]
    fn star_rules_apply_to_every_agent_and_agent_rules_win() {
        let bank = use_item("bank", Domain::Credentials);
        let rules = [
            rule("*", Item, "bank", Deny),
            rule("claude", Item, "bank", Allow),
        ];
        assert!(!denied(&rules, "claude", &bank));
        assert!(denied(&rules, "codex", &bank));
        assert_eq!(
            decide(&rules, "codex", &bank),
            Decision::Deny {
                rule: "* item bank deny".into()
            }
        );
        // An agent rule wins even when it is less specific than the `*` rule.
        let rules = [
            rule("*", Item, "bank", Deny),
            rule("claude", Item, "*", Allow),
        ];
        assert!(!denied(&rules, "claude", &bank));
        let rules = [
            rule("*", Item, "bank", Allow),
            rule("claude", Item, "*", Deny),
        ];
        assert!(denied(&rules, "claude", &bank));
        // Other agents' rules never apply.
        assert!(!denied(&[rule("codex", Item, "*", Deny)], "claude", &bank));
    }

    #[test]
    fn item_beats_domain_beats_star() {
        let visa = use_item("visa", Domain::Payment);
        let rules = [
            rule("codex", DOM, "payment", Deny),
            rule("codex", Item, "visa", Allow),
        ];
        assert!(!denied(&rules, "codex", &visa));
        assert!(denied(&rules, "codex", &use_item("amex", Domain::Payment)));
        assert!(!denied(
            &rules,
            "codex",
            &use_item("github", Domain::Credentials)
        ));
        let rules = [
            rule("codex", Item, "*", Deny),
            rule("codex", DOM, "payment", Allow),
        ];
        assert!(!denied(&rules, "codex", &visa));
        assert!(denied(
            &rules,
            "codex",
            &use_item("github", Domain::Credentials)
        ));
        let rules = [
            rule("codex", DOM, "*", Allow),
            rule("codex", Item, "visa", Deny),
        ];
        assert!(denied(&rules, "codex", &visa));
        let rules = [rule("*", DOM, "*", Deny), rule("*", DOM, "personal", Allow)];
        assert!(denied(&rules, "claude", &visa));
        assert!(!denied(
            &rules,
            "claude",
            &use_item("home", Domain::Personal)
        ));
    }

    #[test]
    fn deny_wins_at_equal_specificity() {
        let visa = use_item("visa", Domain::Payment);
        let rules = [
            rule("codex", Item, "*", Allow),
            rule("codex", DOM, "*", Deny),
        ];
        assert!(denied(&rules, "codex", &visa));
        let rules = [
            rule("codex", DOM, "*", Deny),
            rule("codex", Item, "*", Allow),
        ];
        assert!(denied(&rules, "codex", &visa));
        let rules = [rule("*", Item, "*", Allow), rule("*", DOM, "*", Deny)];
        assert!(denied(&rules, "claude", &visa));
    }

    #[test]
    fn sessions_create_and_raw_cdp() {
        let (one, two) = (SessionId::new(), SessionId::new());
        let rules = [
            rule("codex", Session, "*", Deny),
            rule("codex", Session, &one.to_string(), Allow),
            rule("codex", CreateSession, "*", Deny),
            rule("*", RawCdp, "*", Deny),
        ];
        assert!(!denied(&rules, "codex", &Action::OpenSession(one)));
        assert!(denied(&rules, "codex", &Action::OpenSession(two)));
        assert!(!denied(&rules, "claude", &Action::OpenSession(two)));
        assert!(denied(&rules, "codex", &Action::CreateSession));
        assert!(!denied(&rules, "claude", &Action::CreateSession));
        assert!(denied(&rules, "claude", &Action::RawCdp));
        // Rules for one resource never decide another.
        assert!(!denied(
            &rules,
            "codex",
            &use_item("github", Domain::Credentials)
        ));
        assert!(!denied(
            &[rule("codex", Item, "*", Deny)],
            "codex",
            &Action::OpenSession(one)
        ));
    }
}
