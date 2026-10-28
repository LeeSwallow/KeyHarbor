//! `keyharbor audit`: the usage record (never contains values).

use anyhow::Result;
use clap::Args;
use keyharbor_core::audit;
use keyharbor_domain::{audit::AuditRecord, ports::AuditStore};

#[derive(Args)]
pub struct AuditArgs {
    /// Show only the most recent N events
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    json: bool,
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

fn line(r: &AuditRecord) -> String {
    let e = &r.event;
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        r.seq,
        r.at,
        e.agent,
        e.kind.as_str(),
        e.item_name.as_deref().unwrap_or("-"),
        e.origin.as_deref().unwrap_or("-"),
        e.session_id.as_deref().unwrap_or("-"),
        e.result
    )
}

pub fn run(store: &dyn AuditStore, args: AuditArgs) -> Result<()> {
    let records = audit::list(store, args.limit)?;
    if args.json {
        println!(
            "{}",
            serde_json::Value::Array(records.iter().map(json).collect())
        );
        return Ok(());
    }
    for r in &records {
        println!("{}", line(r));
    }
    Ok(())
}
