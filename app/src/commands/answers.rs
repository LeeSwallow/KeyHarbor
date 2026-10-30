//! What the person answers from the app: an agent's request for information (the secure input
//! form, 09 §9.4.5) and a browser handed to them (resume).

use crate::Ctx;
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_core::{
    request::{self, Answer},
    session,
};
use keyharbor_domain::{RequestId, SessionId, policy::LOCAL_AGENT};
use serde::Serialize;
use std::collections::BTreeMap;
use zeroize::Zeroizing;

#[derive(Debug, Serialize)]
pub struct FormField {
    pub name: &'static str,
    /// Shown as a password input, never echoed.
    pub secret: bool,
}

/// The secure input form for one request: who asks, for what, why, where, and the fields.
#[derive(Debug, Serialize)]
pub struct RequestForm {
    pub id: String,
    pub agent: String,
    pub kind: &'static str,
    pub name: String,
    pub origin: Option<String>,
    pub reason: String,
    pub fields: Vec<FormField>,
    /// Free-form names (env items): the person names each variable.
    pub free_fields: bool,
}

fn request_id(id: &str) -> Result<RequestId> {
    RequestId::parse(id).context("invalid request id")
}

/// Where the secure input window (09 §9.4.5) loads: the form for `id`, or for the oldest
/// request when none is named; `None` when nothing waits. The id goes into the URL only as
/// the parsed id of a request that is still pending.
pub fn request_window_path(ctx: &Ctx, id: Option<&str>) -> Result<Option<String>> {
    let store = ctx.store()?;
    let id = match id {
        Some(id) => {
            let request = request::status(&*store, ctx.clock(), request_id(id)?)?;
            if request.status != keyharbor_domain::request::VaultRequestStatus::Pending {
                bail!("the request is no longer waiting");
            }
            request.id
        }
        None => match request::pending(&*store, ctx.clock())?.first() {
            Some(request) => request.id,
            None => return Ok(None),
        },
    };
    Ok(Some(format!("index.html?request={id}")))
}

pub fn request_form(ctx: &Ctx, id: &str) -> Result<RequestForm> {
    let request = request::status(&*ctx.store()?, ctx.clock(), request_id(id)?)?;
    let spec = request.kind.spec();
    Ok(RequestForm {
        id: request.id.to_string(),
        agent: request.agent,
        kind: request.kind.as_str(),
        name: request.name.as_str().to_owned(),
        origin: request.origin.map(|o| o.as_str().to_owned()),
        reason: request.reason.as_str().to_owned(),
        fields: spec
            .fields
            .iter()
            .map(|f| FormField {
                name: f.name,
                secret: f.secret,
            })
            .collect(),
        free_fields: spec.free_fields,
    })
}

/// Stores the values the person typed as the requested item; returns its name. Empty values
/// are left out; the values never come back.
pub fn fulfill_request(ctx: &Ctx, id: &str, values: BTreeMap<String, String>) -> Result<String> {
    let request = request::status(&*ctx.store()?, ctx.clock(), request_id(id)?)?;
    let mut fields = Vec::new();
    for (name, value) in values {
        let value = Zeroizing::new(value);
        if value.is_empty() {
            continue;
        }
        let field = request.kind.field_name(&name).map_err(|e| anyhow!("{e}"))?;
        fields.push((field, value));
    }
    if fields.is_empty() {
        bail!("nothing was entered");
    }
    let answer = Answer::Values(fields);
    let item = request::fulfill(
        &*ctx.store()?,
        &ctx.vault()?,
        ctx.clock(),
        request.id,
        answer,
    )?;
    Ok(item.as_str().to_owned())
}

/// Answers a request with an item already in the vault.
pub fn fulfill_with_item(ctx: &Ctx, id: &str, item: &str) -> Result<String> {
    let name = keyharbor_domain::vault::ItemName::parse(item).map_err(|e| anyhow!("{e}"))?;
    let answer = Answer::Existing(name);
    let item = request::fulfill(
        &*ctx.store()?,
        &ctx.vault()?,
        ctx.clock(),
        request_id(id)?,
        answer,
    )?;
    Ok(item.as_str().to_owned())
}

/// A browser handed to the person, waiting for them to say they are done.
#[derive(Debug, Serialize)]
pub struct HandoffDto {
    pub session: String,
    pub session_name: String,
    pub request_id: String,
    pub origin: Option<String>,
}

pub fn handoffs(ctx: &Ctx) -> Result<Vec<HandoffDto>> {
    let (store, runtime) = (ctx.store()?, ctx.runtime());
    let mut out = Vec::new();
    for view in session::list(&*store, &*runtime)? {
        if let Ok((request, origin)) = session::pending_handoff(&*store, &*runtime, view.row.id) {
            out.push(HandoffDto {
                session: view.row.id.to_string(),
                session_name: view.row.name.as_str().to_owned(),
                request_id: request.to_string(),
                origin,
            });
        }
    }
    Ok(out)
}

pub fn resume_handoff(ctx: &Ctx, session_id: &str, request: &str) -> Result<()> {
    let id = SessionId::parse(session_id).context("invalid session id")?;
    session::resume(&*ctx.store()?, &*ctx.runtime(), id, request, LOCAL_AGENT)
}
