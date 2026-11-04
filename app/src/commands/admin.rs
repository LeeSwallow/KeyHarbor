//! Settings and policies (09 §9.4.4, §9.4.7). A change that weakens a protection asks the
//! person first ([`crate::confirm`]).

use crate::{
    Ctx,
    auth::AuthScope,
    confirm::{Outcome, confirmed_by_dialog},
};
use anyhow::{Result, anyhow};
use keyharbor_core::{policy, settings};
use keyharbor_domain::{
    policy::{Effect, Resource, Rule},
    settings::{SETTINGS, SettingWeakening},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SettingDto {
    pub key: &'static str,
    pub value: String,
    pub default: &'static str,
    /// Which change asks the person first, from the domain registry, so the screen's words
    /// ("30초보다 길게 하면 확인합니다") follow the rule instead of copying it (J-203).
    pub weakening: WeakeningDto,
}

/// [`SettingWeakening`] as the window reads it.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "when", rename_all = "snake_case")]
pub enum WeakeningDto {
    /// Setting `value` weakens (`audit_enabled` → `false`).
    SetTo {
        value: &'static str,
    },
    /// A number above `than` (the default) weakens.
    Above {
        than: &'static str,
    },
    AnyChange,
    Never,
}

impl WeakeningDto {
    fn from_spec(weakening: SettingWeakening, default: &'static str) -> Self {
        match weakening {
            SettingWeakening::WhenSetTo(value) => Self::SetTo { value },
            SettingWeakening::AboveDefault => Self::Above { than: default },
            SettingWeakening::AnyChange => Self::AnyChange,
            SettingWeakening::Never => Self::Never,
        }
    }
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
                weakening: WeakeningDto::from_spec(spec.weakening, spec.default),
            })
        })
        .collect()
}

pub fn set_setting(ctx: &Ctx, key: &str, value: &str, confirmed: bool) -> Result<Outcome> {
    ctx.challenge(AuthScope::PolicyChange)?;
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

/// A rule as the "규칙 추가" form sends it.
#[derive(Debug, Deserialize)]
pub struct RuleForm {
    pub agent: String,
    pub resource: String,
    pub target: String,
    pub effect: String,
}

pub fn set_policy(ctx: &Ctx, form: RuleForm, confirmed: bool) -> Result<Outcome> {
    ctx.challenge(AuthScope::PolicyChange)?;
    let effect =
        Effect::parse(&form.effect).ok_or_else(|| anyhow!("effect must be allow or deny"))?;
    let rule = Rule::new(&form.agent, resource(&form.resource)?, &form.target, effect)?;
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
    ctx.challenge(AuthScope::PolicyChange)?;
    let resource = resource(res)?;
    let store = ctx.store()?;
    confirmed_by_dialog(confirmed, |confirm| {
        policy::remove(&*store, confirm, agent, resource, target).map(|_| ())
    })
}
