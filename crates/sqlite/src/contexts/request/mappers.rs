//! Request rows as stored ↔ `VaultRequest`.

use super::models::RequestModel;
use anyhow::{Context, Result, anyhow};
use keyharbor_domain::{
    RequestId,
    request::{VaultRequest, VaultRequestStatus},
    vault::{Description, ItemKind, ItemName, Origin},
};

fn stored<T, E>(value: Result<T, E>, what: &str) -> Result<T> {
    value.map_err(|_| anyhow!("invalid stored request {what}"))
}

pub fn request(model: RequestModel) -> Result<VaultRequest> {
    Ok(VaultRequest {
        id: stored(RequestId::parse(&model.id), "id")?,
        agent: model.agent,
        kind: ItemKind::parse(&model.kind).context("invalid stored request kind")?,
        name: stored(ItemName::parse(&model.name), "name")?,
        reason: stored(Description::parse(&model.reason), "reason")?,
        origin: model
            .origin
            .as_deref()
            .map(|o| stored(Origin::parse(o), "origin"))
            .transpose()?,
        status: stored(VaultRequestStatus::parse(&model.status), "status")?,
        created_at: stored(u64::try_from(model.created_at), "time")?,
        item: model
            .item_name
            .as_deref()
            .map(|i| stored(ItemName::parse(i), "item"))
            .transpose()?,
    })
}

pub fn model(request: &VaultRequest) -> Result<RequestModel> {
    Ok(RequestModel {
        id: request.id.to_string(),
        agent: request.agent.clone(),
        kind: request.kind.as_str().to_owned(),
        name: request.name.as_str().to_owned(),
        reason: request.reason.as_str().to_owned(),
        origin: request.origin.as_ref().map(|o| o.as_str().to_owned()),
        status: request.status.as_str().to_owned(),
        created_at: i64::try_from(request.created_at).context("request time out of range")?,
        item_name: request.item.as_ref().map(|i| i.as_str().to_owned()),
    })
}
