//! `keyharbor audit` (the usage record) and `keyharbor settings`.

use crate::adapters::TtyConfirm;
use anyhow::{Result, anyhow, bail};
use clap::{Args, Subcommand};
use keyharbor_core::weaken;
use keyharbor_domain::audit::AuditRecord;
use keyharbor_domain::ports::*;
use keyharbor_sqlite::SqliteStore;
use std::path::Path;

#[derive(Args)]
pub struct AuditArgs {
    /// Show only the most recent N events
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
pub enum SettingsCmd {
    /// Print a setting
    Get { key: String },
    /// Change a setting: audit_enabled true|false, capture_enabled true|false
    Set { key: String, value: String },
}

fn json(record: &AuditRecord) -> serde_json::Value {
    let e = &record.event;
    serde_json::json!({
        "seq": record.seq,
        "at": record.at,
        "agent": e.agent,
        "session_id": e.session_id,
        "kind": e.kind.as_str(),
        "item_name": e.item_name,
        "origin": e.origin,
        "result": e.result,
    })
}

pub fn audit(root: &Path, args: AuditArgs) -> Result<()> {
    let records = SqliteStore::open(root)?.list_audit(args.limit)?;
    if args.json {
        println!(
            "{}",
            serde_json::Value::Array(records.iter().map(json).collect())
        );
        return Ok(());
    }
    for r in &records {
        let e = &r.event;
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.seq,
            r.at,
            e.agent,
            e.kind.as_str(),
            e.item_name.as_deref().unwrap_or("-"),
            e.origin.as_deref().unwrap_or("-"),
            e.session_id.as_deref().unwrap_or("-"),
            e.result
        );
    }
    Ok(())
}

/// Every setting, its default and the values it takes.
const SETTINGS: &[(&str, &str, &[&str])] = &[
    (AUDIT_ENABLED, "true", &["true", "false"]),
    (weaken::CAPTURE_ENABLED, "false", &["true", "false"]),
];

fn setting(key: &str) -> Result<&'static (&'static str, &'static str, &'static [&'static str])> {
    SETTINGS
        .iter()
        .find(|(name, _, _)| *name == key)
        .ok_or_else(|| {
            let known: Vec<&str> = SETTINGS.iter().map(|(name, _, _)| *name).collect();
            anyhow!("unknown setting; known settings: {}", known.join(", "))
        })
}

pub fn settings(root: &Path, command: SettingsCmd) -> Result<()> {
    let store = SqliteStore::open(root)?;
    match command {
        SettingsCmd::Get { key } => {
            let (_, default, _) = setting(&key)?;
            let value = store.setting_get(&key)?;
            println!("{}", value.as_deref().unwrap_or(default));
        }
        SettingsCmd::Set { key, value } => {
            let (_, _, values) = setting(&key)?;
            if !values.contains(&value.as_str()) {
                bail!("{key} must be {}", values.join(" or "));
            }
            // Turning the usage record off or changing capture needs a person (R-35).
            weaken::require(&TtyConfirm, weaken::setting(&key, &value).as_slice())?;
            store.setting_set(&key, &value)?;
            println!("{key}\t{value}");
        }
    }
    Ok(())
}
