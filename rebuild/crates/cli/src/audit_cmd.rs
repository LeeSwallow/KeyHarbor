//! `keyharbor audit` (the usage record) and `keyharbor settings`.

use crate::adapters::TtyConfirm;
use anyhow::{Result, anyhow, bail};
use clap::{Args, Subcommand};
use keyharbor_core::weaken;
use keyharbor_domain::ports::*;
use keyharbor_domain::{audit::AuditRecord, date::Ttl};
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
    /// Change a setting: audit_enabled true|false, capture_enabled true|false,
    /// default_item_ttl <N>d|none
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

/// One setting: its default and how a value is checked.
struct Setting {
    key: &'static str,
    default: &'static str,
    valid: fn(&str) -> Result<()>,
}

fn boolean(value: &str) -> Result<()> {
    if !matches!(value, "true" | "false") {
        bail!("must be true or false");
    }
    Ok(())
}

fn ttl(value: &str) -> Result<()> {
    if value != keyharbor_core::NO_TTL {
        Ttl::parse(value).map_err(|e| anyhow!("{e}, or none"))?;
    }
    Ok(())
}

/// Every setting.
const SETTINGS: &[Setting] = &[
    Setting {
        key: AUDIT_ENABLED,
        default: "true",
        valid: boolean,
    },
    Setting {
        key: weaken::CAPTURE_ENABLED,
        default: "false",
        valid: boolean,
    },
    Setting {
        key: keyharbor_core::DEFAULT_ITEM_TTL,
        default: keyharbor_core::NO_TTL,
        valid: ttl,
    },
];

fn setting(key: &str) -> Result<&'static Setting> {
    SETTINGS.iter().find(|s| s.key == key).ok_or_else(|| {
        let known: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
        anyhow!("unknown setting; known settings: {}", known.join(", "))
    })
}

pub fn settings(root: &Path, command: SettingsCmd) -> Result<()> {
    let store = SqliteStore::open(root)?;
    match command {
        SettingsCmd::Get { key } => {
            let setting = setting(&key)?;
            let value = store.setting_get(&key)?;
            println!("{}", value.as_deref().unwrap_or(setting.default));
        }
        SettingsCmd::Set { key, value } => {
            (setting(&key)?.valid)(&value).map_err(|e| anyhow!("{key} {e}"))?;
            // Turning the usage record off or changing capture needs a person (R-35).
            weaken::require(&TtyConfirm, weaken::setting(&key, &value).as_slice())?;
            store.setting_set(&key, &value)?;
            println!("{key}\t{value}");
        }
    }
    Ok(())
}
