//! Settings and policies (09 §9.4.4, §9.4.7). A change that weakens a protection asks the
//! person first ([`crate::confirm`]).

use crate::{
    Ctx,
    confirm::{Outcome, confirmed_by_dialog},
};
use anyhow::{Result, anyhow};
use keyharbor_core::{policy, settings};
use keyharbor_domain::{
    policy::{Effect, Resource, Rule},
    settings::SETTINGS,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SettingDto {
    pub key: &'static str,
    pub value: String,
    pub default: &'static str,
}

pub fn settings(ctx: &Ctx) -> Result<Vec<SettingDto>> {
    let store = ctx.store()?;
    SETTINGS
        .iter()
        .map(|spec| {
            Ok(SettingDto {
                key: spec.key,
                value: settings::get(&*store, spec.key)?,
                default: spec.default,
            })
        })
        .collect()
}

pub fn set_setting(ctx: &Ctx, key: &str, value: &str, confirmed: bool) -> Result<Outcome> {
    let store = ctx.store()?;
    confirmed_by_dialog(confirmed, |confirm| {
        settings::set(&*store, confirm, key, value)
    })
}

#[derive(Debug, Serialize)]
pub struct RuleDto {
    pub agent: String,
    pub resource: &'static str,
    pub target: String,
    pub effect: &'static str,
}

pub fn policies(ctx: &Ctx) -> Result<Vec<RuleDto>> {
    Ok(policy::list(&*ctx.store()?)?
        .into_iter()
        .map(|r| RuleDto {
            agent: r.agent,
            resource: r.resource.as_str(),
            target: r.target,
            effect: r.effect.as_str(),
        })
        .collect())
}

fn resource(value: &str) -> Result<Resource> {
    Resource::parse(value).ok_or_else(|| anyhow!("unknown resource {value}"))
}

pub fn set_policy(ctx: &Ctx, rule: (&str, &str, &str, &str), confirmed: bool) -> Result<Outcome> {
    let (agent, res, target, effect) = rule;
    let effect = Effect::parse(effect).ok_or_else(|| anyhow!("effect must be allow or deny"))?;
    let rule = Rule::new(agent, resource(res)?, target, effect).map_err(|e| anyhow!("{e}"))?;
    let store = ctx.store()?;
    confirmed_by_dialog(confirmed, |confirm| policy::set(&*store, confirm, &rule))
}

pub fn remove_policy(
    ctx: &Ctx,
    agent: &str,
    res: &str,
    target: &str,
    confirmed: bool,
) -> Result<Outcome> {
    let resource = resource(res)?;
    let store = ctx.store()?;
    confirmed_by_dialog(confirmed, |confirm| {
        policy::remove(&*store, confirm, agent, resource, target).map(|_| ())
    })
}
