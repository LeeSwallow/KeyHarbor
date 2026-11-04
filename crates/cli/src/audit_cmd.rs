//! `keyharbor audit`: the usage record (never contains values), and `--denials`: why agents
//! stopped, by cause, with how to lift each (J-240).

use crate::compose::App;
use anyhow::Result;
use clap::Args;
use keyharbor_core::audit::{self, StopFilter, StopGroup, StopRow};
use keyharbor_domain::audit::{AuditRecord, Lookback};

#[derive(Args)]
pub struct AuditArgs {
    /// Show only the most recent N events
    #[arg(long, conflicts_with = "denials")]
    limit: Option<usize>,
    /// Why agents stopped: refusals, locks and handoffs still waiting, by cause
    #[arg(long)]
    denials: bool,
    /// With --denials: only this session (its id)
    #[arg(long, requires = "denials")]
    session: Option<String>,
    /// With --denials: only this recent, <N>m, <N>h or <N>d (e.g. 24h)
    #[arg(long, requires = "denials", value_parser = parse_since)]
    since: Option<Lookback>,
    #[arg(long)]
    json: bool,
}

fn parse_since(value: &str) -> Result<Lookback, String> {
    Lookback::parse(value).map_err(|e| e.to_string())
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

pub fn run(app: &App, args: AuditArgs) -> Result<()> {
    if args.denials {
        let filter = StopFilter {
            session: args.session,
            since: args.since,
        };
        let groups = audit::stops(&*app.store()?, app.clock(), &filter)?;
        print_stops(&groups, args.json);
        return Ok(());
    }
    let records = audit::list(&*app.store()?, args.limit)?;
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

fn print_stops(groups: &[StopGroup], as_json: bool) {
    if as_json {
        println!(
            "{}",
            serde_json::Value::Array(groups.iter().map(group_json).collect())
        );
        return;
    }
    if groups.is_empty() {
        println!("no refusals, locks or waiting handoffs");
    }
    for g in groups {
        println!("{}\t{}\tlast {}", g.cause, g.count, g.last_at);
        for r in &g.rows {
            println!("  {}", row_line(r));
        }
        println!("  fix: {}", g.cause.remedy());
    }
}

/// `agent session item origin count last`, `-` for what the record does not name.
fn row_line(r: &StopRow) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        r.agent,
        r.session.as_deref().unwrap_or("-"),
        r.item.as_deref().unwrap_or("-"),
        r.origin.as_deref().unwrap_or("-"),
        r.count,
        r.last_at
    )
}

fn group_json(g: &StopGroup) -> serde_json::Value {
    serde_json::json!({
        "reason": g.cause.as_str(),
        "count": g.count,
        "last_at": g.last_at,
        "fix": g.cause.remedy(),
        "rows": g.rows.iter().map(|r| serde_json::json!({
            "agent": r.agent,
            "session_id": r.session,
            "item_name": r.item,
            "origin": r.origin,
            "count": r.count,
            "last_at": r.last_at,
        })).collect::<Vec<_>>(),
    })
}
