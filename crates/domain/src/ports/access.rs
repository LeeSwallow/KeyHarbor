//! Agent policy rules (02 §2.7, §2.8). One rule per (agent, resource, target).

use crate::{
    SessionId,
    grant::{Grant, GrantId},
    policy::{Resource, Rule},
};
use anyhow::Result;

pub trait PolicyStore {
    /// Every rule, ordered by agent, resource and target.
    fn policy_rules(&self) -> Result<Vec<Rule>>;
    /// Adds the rule or changes the effect of the rule with the same agent, resource and target.
    fn policy_set(&self, rule: &Rule) -> Result<()>;
    /// False if no such rule existed.
    fn policy_remove(&self, agent: &str, resource: Resource, target: &str) -> Result<bool>;
    /// Remembers that `item` was typed into a frame of `origin`; true the first time (R-36).
    fn note_origin_use(&self, item: &str, origin: &str) -> Result<bool>;
}

/// Grants (R-43). Kept after they end, so a session that had one stays limited.
pub trait GrantStore {
    fn grant_add(&self, grant: &Grant) -> Result<()>;
    /// Every grant of `session`, active or ended, oldest first.
    fn grants_for(&self, session: SessionId) -> Result<Vec<Grant>>;
    fn grant_get(&self, id: GrantId) -> Result<Option<Grant>>;
    /// Marks an unrevoked grant revoked at `at`; false if there is none.
    fn grant_revoke(&self, id: GrantId, at: u64) -> Result<bool>;
    /// Records that an ended grant's cookies were removed.
    fn grant_mark_cleared(&self, id: GrantId) -> Result<()>;
}
