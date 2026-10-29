//! Policy rows as stored ↔ `Rule`, grant rows ↔ `Grant`.

use super::models::{GrantModel, RuleModel};
use anyhow::{Context, Result};
use keyharbor_domain::{
    SessionId,
    grant::{Grant, GrantId},
    policy::{Effect, Resource, Rule},
    vault::{Description, Origin},
};

pub fn rule(model: RuleModel) -> Result<Rule> {
    Ok(Rule {
        agent: model.agent,
        resource: Resource::parse(&model.resource).context("invalid stored policy resource")?,
        target: model.target,
        effect: Effect::parse(&model.effect).context("invalid stored policy effect")?,
    })
}

pub fn rule_model(rule: &Rule) -> RuleModel {
    RuleModel {
        agent: rule.agent.clone(),
        resource: rule.resource.as_str().to_owned(),
        target: rule.target.clone(),
        effect: rule.effect.as_str().to_owned(),
    }
}

/// The only subject and scope kinds stored so far (the columns leave room for more).
pub const SESSION_SUBJECT: &str = "session";
pub const ORIGIN_SCOPE: &str = "origin";

fn seconds(value: i64) -> Result<u64> {
    u64::try_from(value).context("invalid stored grant time")
}
fn stored_seconds(value: u64) -> Result<i64> {
    i64::try_from(value).context("grant time out of range")
}

pub fn grant(model: GrantModel) -> Result<Grant> {
    if model.subject_kind != SESSION_SUBJECT || model.scope_kind != ORIGIN_SCOPE {
        anyhow::bail!("unsupported stored grant kind");
    }
    Ok(Grant {
        id: GrantId::parse(&model.id).context("invalid stored grant id")?,
        session: SessionId::parse(&model.subject).context("invalid stored grant subject")?,
        origin: Origin::parse(&model.scope)
            .map_err(|_| anyhow::anyhow!("invalid stored grant scope"))?,
        until: model.until.map(seconds).transpose()?,
        created_by: model.created_by,
        reason: model
            .reason
            .as_deref()
            .map(Description::parse)
            .transpose()
            .map_err(|_| anyhow::anyhow!("invalid stored grant reason"))?,
        created_at: seconds(model.created_at)?,
        revoked_at: model.revoked_at.map(seconds).transpose()?,
        cleared: model.cleared,
    })
}

pub fn grant_model(grant: &Grant) -> Result<GrantModel> {
    Ok(GrantModel {
        id: grant.id.to_string(),
        subject_kind: SESSION_SUBJECT.to_owned(),
        subject: grant.session.to_string(),
        scope_kind: ORIGIN_SCOPE.to_owned(),
        scope: grant.origin.as_str().to_owned(),
        until: grant.until.map(stored_seconds).transpose()?,
        created_by: grant.created_by.clone(),
        reason: grant.reason.as_ref().map(|r| r.as_str().to_owned()),
        created_at: stored_seconds(grant.created_at)?,
        revoked_at: grant.revoked_at.map(stored_seconds).transpose()?,
        cleared: grant.cleared,
    })
}
