//! Changing agent policy (02 §2.8): every interface lists, sets and removes rules through
//! these, so the weakening check and the usage record cannot be skipped.

use crate::{LOCAL_AGENT, weaken};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    policy::{Resource, Rule, normalize_target, parse_agent},
    ports::{AuditStore, Confirm, PolicyStore},
};

/// Every rule, ordered by agent, resource and target.
pub fn list(store: &dyn PolicyStore) -> Result<Vec<Rule>> {
    store.policy_rules()
}

/// Adds or changes a rule. Any allow rule widens access, so it needs a person (R-35).
pub fn set<S: PolicyStore + AuditStore + ?Sized>(
    store: &S,
    confirm: &dyn Confirm,
    rule: &Rule,
) -> Result<()> {
    weaken::require(confirm, weaken::policy_set(rule).as_slice())?;
    store.policy_set(rule)?;
    changed(
        store,
        &format!(
            "{} {} {} {}",
            rule.effect.as_str(),
            rule.agent,
            rule.resource.as_str(),
            rule.target
        ),
    );
    Ok(())
}

/// Removes the rule for `agent`, `resource` and `target` and returns it. Removing a deny
/// rule widens access, so it needs a person (R-35).
pub fn remove<S: PolicyStore + AuditStore + ?Sized>(
    store: &S,
    confirm: &dyn Confirm,
    agent: &str,
    resource: Resource,
    target: &str,
) -> Result<Rule> {
    let agent = parse_agent(agent).map_err(|e| anyhow!("{e}"))?;
    let target = normalize_target(resource, target).map_err(|e| anyhow!("{e}"))?;
    let existing = store
        .policy_rules()?
        .into_iter()
        .find(|r| r.agent == agent && r.resource == resource && r.target == target)
        .context("rule not found")?;
    weaken::require(confirm, weaken::policy_remove(&existing).as_slice())?;
    if !store.policy_remove(&agent, resource, &target)? {
        bail!("rule not found");
    }
    changed(
        store,
        &format!("removed {agent} {} {target}", resource.as_str()),
    );
    Ok(existing)
}

/// Records a policy change. Best effort: the change is already saved.
fn changed<S: AuditStore + ?Sized>(store: &S, what: &str) {
    let _ = store.record(&AuditEvent::new(
        LOCAL_AGENT,
        AuditKind::PolicyChanged,
        what,
    ));
}
