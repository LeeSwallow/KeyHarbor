//! `keyharbor policy …`: rules that limit what each agent may use (02 §2.8).
//! With no rules everything is allowed; `*` as the agent applies a rule to every agent.

use crate::tty_confirm::TtyConfirm;
use anyhow::{Context, Result, anyhow};
use clap::{Args, Subcommand};
use keyharbor_core::policy;
use keyharbor_domain::policy::{Effect, Resource, Rule};
use keyharbor_domain::ports::Store;

#[derive(Subcommand)]
pub enum PolicyCmd {
    /// List rules: agent, resource, target, effect
    List {
        #[arg(long)]
        json: bool,
    },
    /// Allow an agent to use a resource
    Allow(RuleArgs),
    /// Deny an agent the use of a resource
    Deny(RuleArgs),
    /// Remove a rule
    Rm(RuleArgs),
}

#[derive(Args)]
pub struct RuleArgs {
    /// Agent name given to `mcp --agent` or `run --agent`, or * for every agent
    #[arg(long)]
    agent: String,
    /// session, item, domain, create_session or raw_cdp
    #[arg(long)]
    resource: String,
    /// Session id, item name, domain (credentials, payment, personal, developer) or *
    #[arg(long, default_value = "*")]
    target: String,
}

fn resource(value: &str) -> Result<Resource> {
    Resource::parse(value)
        .context("resource must be session, item, domain, create_session or raw_cdp")
}

fn rule(args: &RuleArgs, effect: Effect) -> Result<Rule> {
    Rule::new(&args.agent, resource(&args.resource)?, &args.target, effect)
        .map_err(|e| anyhow!("{e}"))
}

fn list(store: &dyn Store, json: bool) -> Result<()> {
    let rules = policy::list(store)?;
    if json {
        let values: Vec<_> = rules
            .iter()
            .map(|r| {
                serde_json::json!({
                    "agent": r.agent,
                    "resource": r.resource.as_str(),
                    "target": r.target,
                    "effect": r.effect.as_str(),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(values));
        return Ok(());
    }
    for r in &rules {
        println!("{}", line(r));
    }
    Ok(())
}

fn line(r: &Rule) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        r.agent,
        r.resource.as_str(),
        r.target,
        r.effect.as_str()
    )
}

fn set(store: &dyn Store, rule: Rule) -> Result<()> {
    policy::set(store, &TtyConfirm, &rule)?;
    println!("{}", line_effect_first(&rule));
    Ok(())
}

fn line_effect_first(r: &Rule) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        r.effect.as_str(),
        r.agent,
        r.resource.as_str(),
        r.target
    )
}

pub fn run(store: &dyn Store, command: PolicyCmd) -> Result<()> {
    match command {
        PolicyCmd::List { json } => list(store, json)?,
        PolicyCmd::Allow(args) => set(store, rule(&args, Effect::Allow)?)?,
        PolicyCmd::Deny(args) => set(store, rule(&args, Effect::Deny)?)?,
        PolicyCmd::Rm(args) => {
            let removed = policy::remove(
                store,
                &TtyConfirm,
                &args.agent,
                resource(&args.resource)?,
                &args.target,
            )?;
            println!(
                "removed\t{}\t{}\t{}",
                removed.agent,
                removed.resource.as_str(),
                removed.target
            );
        }
    }
    Ok(())
}
