//! Answering agents' requests (08 §8.2 ③) until the app's secure input window exists: list,
//! fulfill with a hidden prompt or stdin JSON (or an existing item), decline. The rules are
//! core's (`keyharbor_core::request`).

use super::input::{FieldInput, collect_fields, item_name};
use crate::compose::App;
use anyhow::{Context, Result};
use clap::Subcommand;
use keyharbor_core::request::{self, Answer};
use keyharbor_domain::{RequestId, request::VaultRequest};

#[derive(Subcommand)]
pub enum RequestCmd {
    /// List requests waiting for an answer
    Requests,
    /// Answer a request: secret fields by hidden prompt or --stdin JSON, or --use an item
    Fulfill {
        id: String,
        #[command(flatten)]
        input: FieldInput,
        /// Answer with an item already in the vault
        #[arg(long = "use", value_name = "ITEM", conflicts_with_all = ["set", "stdin"])]
        existing: Option<String>,
    },
    /// Refuse a request; the agent sees "declined"
    Decline { id: String },
}

/// `id  agent  kind  name  origin|-  reason` (the reason is the agent's text, one line).
fn line(request: &VaultRequest) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        request.id,
        request.agent,
        request.kind.as_str(),
        request.name.as_str(),
        request.origin.as_ref().map_or("-", |o| o.as_str()),
        request.reason.as_str().replace(['\n', '\t'], " ")
    )
}

fn request_id(value: &str) -> Result<RequestId> {
    RequestId::parse(value).context("invalid request id")
}

pub fn run(app: &App, command: RequestCmd) -> Result<()> {
    let store = app.store()?;
    match command {
        RequestCmd::Requests => {
            for pending in request::pending(&*store, app.clock())? {
                println!("{}", line(&pending));
            }
        }
        RequestCmd::Fulfill {
            id,
            input,
            existing,
        } => {
            let id = request_id(&id)?;
            let asked = request::status(&*store, app.clock(), id)?;
            let answer = match existing {
                Some(name) => Answer::Existing(item_name(&name)?),
                None => Answer::Values(collect_fields(asked.kind, &input, true)?),
            };
            let vault = app.vault()?;
            let item = request::fulfill(&*store, &vault, app.clock(), id, answer)?;
            println!("{id}\tfulfilled\t{}", item.as_str());
        }
        RequestCmd::Decline { id } => {
            let id = request_id(&id)?;
            request::decline(&*store, app.clock(), id)?;
            println!("{id}\tdeclined");
        }
    }
    Ok(())
}
