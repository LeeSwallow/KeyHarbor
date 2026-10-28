//! `keyharbor audit` (the usage record) and `keyharbor settings`.

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use keyharbor_domain::audit::AuditRecord;
use keyharbor_sqlite::{AUDIT_ENABLED, SqliteStore};
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
    /// Change a setting: audit_enabled true|false
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

fn check_key(key: &str) -> Result<()> {
    if key != AUDIT_ENABLED {
        bail!("unknown setting; known settings: {AUDIT_ENABLED}");
    }
    Ok(())
}

pub fn settings(root: &Path, command: SettingsCmd) -> Result<()> {
    let store = SqliteStore::open(root)?;
    match command {
        SettingsCmd::Get { key } => {
            check_key(&key)?;
            let value = store.setting_get(&key)?;
            println!("{}", value.as_deref().unwrap_or("true"));
        }
        SettingsCmd::Set { key, value } => {
            check_key(&key)?;
            if !matches!(value.as_str(), "true" | "false") {
                bail!("{AUDIT_ENABLED} must be true or false");
            }
            store.setting_set(&key, &value)?;
            println!("{key}\t{value}");
        }
    }
    Ok(())
}
