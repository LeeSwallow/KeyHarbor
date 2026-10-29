//! Request tools: ask the user for information only they have and wait for the answer
//! (08 §8.2 ③). The answer's values go from the user to the vault; the agent gets the
//! outcome and placeholders.

use crate::server::{Server, fail, outcome};
use anyhow::Result;
use keyharbor_core::{
    request::{self, Ask},
    session::{ResumeWait, poll},
};
use keyharbor_domain::{
    RequestId,
    request::{VaultRequest, VaultRequestStatus},
    vault::{Description, ItemKind, ItemName, Origin, placeholder},
};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, schemars::JsonSchema)]
struct RequestArgs {
    /// login, secret, env, card, identity or address.
    kind: String,
    /// Name for the new item (1–64 letters, digits, - or _).
    name: String,
    /// Why you need it, shown to the user (at most 500 characters). Never put a secret here.
    reason: String,
    /// Site origin it is for, without a path, e.g. https://shop.example.
    origin: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct WaitArgs {
    /// Request ID returned by vault_request.
    request_id: String,
    /// Maximum wait in seconds, 1 to 300; default 120. The request stays open after a timeout.
    timeout_seconds: Option<u64>,
}

fn ask(args: RequestArgs) -> Result<Ask> {
    let invalid = |e: keyharbor_domain::vault::InvalidValue| anyhow::anyhow!("{e}");
    Ok(Ask {
        kind: ItemKind::parse(&args.kind).ok_or_else(|| anyhow::anyhow!("unknown kind"))?,
        name: ItemName::parse_new(&args.name).map_err(invalid)?,
        reason: Description::parse(&args.reason).map_err(invalid)?,
        origin: args
            .origin
            .as_deref()
            .map(Origin::parse)
            .transpose()
            .map_err(invalid)?,
    })
}

#[tool_router(router = request_tools, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "Ask the user for information only they have (a card, an address, a password you do not have). The user answers outside your reach, with new values or an existing item; you never see the values. Returns a request_id; then call vault_wait."
    )]
    async fn vault_request(&self, Parameters(args): Parameters<RequestArgs>) -> CallToolResult {
        let ask = match ask(args) {
            Ok(ask) => ask,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                let id = request::ask(&**inner.store()?, &*inner.clock, inner.agent(), ask)?;
                Ok(json!({
                    "request_id": id.to_string(),
                    "status": VaultRequestStatus::Pending.as_str(),
                    "note": format!("the user answers with `keyharbor vault fulfill {id}` or declines"),
                }))
            })
            .await,
        )
    }

    #[tool(
        description = "Wait for the user to answer a vault_request, up to timeout_seconds (1–300, default 120). Returns status fulfilled (with the item name and placeholders), declined, expired, or pending if the wait ran out (the request stays open; wait again)."
    )]
    async fn vault_wait(&self, Parameters(args): Parameters<WaitArgs>) -> CallToolResult {
        let Ok(id) = RequestId::parse(&args.request_id) else {
            return fail("invalid request_id");
        };
        let wait = match ResumeWait::seconds(args.timeout_seconds) {
            Ok(wait) => wait,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                // The store is held for one look at a time, not for the whole wait.
                let check = || request::answered(&**inner.store()?, &*inner.clock, id);
                let Some(answer) = poll(wait, check, std::thread::sleep)? else {
                    return Ok(json!({"request_id": id.to_string(), "status": VaultRequestStatus::Pending.as_str()}));
                };
                answer_json(&answer, || (inner.open_vault)()?.list())
            })
            .await,
        )
    }
}

/// The outcome as the agent sees it: the answering item's name and placeholders, no value.
fn answer_json(
    answer: &VaultRequest,
    items: impl FnOnce() -> Result<Vec<keyharbor_core::ItemSummary>>,
) -> Result<Value> {
    let mut out = json!({"request_id": answer.id.to_string(), "status": answer.status.as_str()});
    if let Some(name) = &answer.item {
        let fields = items()?
            .into_iter()
            .find(|item| item.name == name.as_str())
            .map(|item| item.fields)
            .unwrap_or_default();
        out["name"] = json!(name.as_str());
        out["placeholders"] = json!(
            fields
                .iter()
                .map(|f| placeholder(name.as_str(), f))
                .collect::<Vec<_>>()
        );
    }
    Ok(out)
}
