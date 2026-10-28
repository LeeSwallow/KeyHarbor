//! Policy rows as stored ↔ `Rule`.

use super::models::RuleModel;
use anyhow::{Context, Result};
use keyharbor_domain::policy::{Effect, Resource, Rule};

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
