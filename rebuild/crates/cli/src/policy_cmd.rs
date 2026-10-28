//! `keyharbor policy …`: rules that limit what each agent may use (02 §2.8).
//! With no rules everything is allowed; `*` as the agent applies a rule to every agent.

use crate::confirm::Terminal;
use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use keyharbor_core::weaken;
use keyharbor_domain::ports::*;
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    policy::{Effect, Resource, Rule, normalize_target, parse_agent},
};
use keyharbor_sqlite::SqliteStore;
use std::path::Path;

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

pub fn run(root: &Path, command: PolicyCmd) -> Result<()> {
    let store = SqliteStore::open(root)?;
    match command {
        PolicyCmd::List { json } => {
            let rules = store.policy_rules()?;
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
            } else {
                for r in &rules {
                    println!(
                        "{}\t{}\t{}\t{}",
                        r.agent,
                        r.resource.as_str(),
                        r.target,
                        r.effect.as_str()
                    );
                }
            }
        }
        PolicyCmd::Allow(args) => set(&store, rule(&args, Effect::Allow)?)?,
        PolicyCmd::Deny(args) => set(&store, rule(&args, Effect::Deny)?)?,
        PolicyCmd::Rm(args) => {
            let agent = parse_agent(&args.agent).map_err(|e| anyhow!("{e}"))?;
            let resource = resource(&args.resource)?;
            let target = normalize_target(resource, &args.target).map_err(|e| anyhow!("{e}"))?;
            let existing = store
                .policy_rules()?
                .into_iter()
                .find(|r| r.agent == agent && r.resource == resource && r.target == target)
                .context("rule not found")?;
            // Removing a deny rule widens access (R-35).
            weaken::require(&Terminal, weaken::policy_remove(&existing).as_slice())?;
            if !store.policy_remove(&agent, resource, &target)? {
                bail!("rule not found");
            }
            changed(
                &store,
                &format!("removed {agent} {} {target}", resource.as_str()),
            );
            println!("removed\t{agent}\t{}\t{target}", resource.as_str());
        }
    }
    Ok(())
}

fn set(store: &SqliteStore, rule: Rule) -> Result<()> {
    // Any allow rule widens access (R-35).
    weaken::require(&Terminal, weaken::policy_set(&rule).as_slice())?;
    store.policy_set(&rule)?;
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
    println!(
        "{}\t{}\t{}\t{}",
        rule.effect.as_str(),
        rule.agent,
        rule.resource.as_str(),
        rule.target
    );
    Ok(())
}

/// Records a policy change. Best effort: the change is already saved.
fn changed(store: &SqliteStore, what: &str) {
    let _ = store.record(&AuditEvent::new(
        keyharbor_core::LOCAL_AGENT,
        AuditKind::PolicyChanged,
        what,
    ));
}
