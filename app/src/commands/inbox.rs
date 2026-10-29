//! "확인 대기" (09 §9.3): agents' requests for information and drafts to settle.

use crate::Ctx;
use anyhow::{Context, Result, anyhow};
use keyharbor_core::{ItemStatus, request};
use keyharbor_domain::{RequestId, policy::LOCAL_AGENT, vault::ItemName};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RequestDto {
    pub id: String,
    pub agent: String,
    pub kind: &'static str,
    pub name: String,
    pub origin: Option<String>,
    /// The agent's words: shown as text, not trusted.
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct DraftDto {
    pub name: String,
    pub kind: &'static str,
    pub source: &'static str,
    pub origins: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct InboxDto {
    pub requests: Vec<RequestDto>,
    pub drafts: Vec<DraftDto>,
}

pub fn inbox(ctx: &Ctx) -> Result<InboxDto> {
    let store = ctx.store()?;
    let requests = request::pending(&*store, ctx.clock())?
        .into_iter()
        .map(|r| RequestDto {
            id: r.id.to_string(),
            agent: r.agent,
            kind: r.kind.as_str(),
            name: r.name.as_str().to_owned(),
            origin: r.origin.map(|o| o.as_str().to_owned()),
            reason: r.reason.as_str().to_owned(),
        })
        .collect();
    let drafts = if store.vault_get()?.is_some() {
        ctx.vault()?
            .list()?
            .into_iter()
            .filter(|i| i.status == ItemStatus::Draft && !i.tampered)
            .map(|i| DraftDto {
                name: i.name,
                kind: i.kind.as_str(),
                source: i.source.as_str(),
                origins: i.origins,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(InboxDto { requests, drafts })
}

fn request_id(id: &str) -> Result<RequestId> {
    RequestId::parse(id).context("invalid request id")
}
fn item(name: &str) -> Result<ItemName> {
    ItemName::parse(name).map_err(|e| anyhow!("{e}"))
}

pub fn decline_request(ctx: &Ctx, id: &str) -> Result<()> {
    request::decline(&*ctx.store()?, ctx.clock(), request_id(id)?)
}

pub fn confirm_draft(ctx: &Ctx, name: &str) -> Result<String> {
    Ok(ctx
        .vault()?
        .confirm(&item(name)?, LOCAL_AGENT)?
        .as_str()
        .to_owned())
}

pub fn discard_draft(ctx: &Ctx, name: &str) -> Result<()> {
    ctx.vault()?.discard(&item(name)?)
}
