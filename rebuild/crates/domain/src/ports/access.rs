//! Agent policy rules (02 §2.7, §2.8). One rule per (agent, resource, target).

use crate::policy::{Resource, Rule};
use anyhow::Result;

pub trait PolicyStore {
    /// Every rule, ordered by agent, resource and target.
    fn policy_rules(&self) -> Result<Vec<Rule>>;
    /// Adds the rule or changes the effect of the rule with the same agent, resource and target.
    fn policy_set(&self, rule: &Rule) -> Result<()>;
    /// False if no such rule existed.
    fn policy_remove(&self, agent: &str, resource: Resource, target: &str) -> Result<bool>;
}
