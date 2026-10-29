//! Site rows as stored ↔ `SiteRecord`.

use super::models::SiteModel;
use anyhow::{Context, Result};
use keyharbor_domain::{
    SessionId,
    site::{SiteRecord, SiteState},
    vault::{ItemName, Origin},
};

pub fn record(model: SiteModel) -> Result<SiteRecord> {
    Ok(SiteRecord {
        session: SessionId::parse(&model.session_id).context("invalid stored session id")?,
        origin: Origin::parse(&model.origin)
            .map_err(|_| anyhow::anyhow!("invalid stored origin"))?,
        state: SiteState::parse(&model.state)
            .map_err(|_| anyhow::anyhow!("invalid stored site state"))?,
        item: model
            .item_name
            .as_deref()
            .map(ItemName::parse)
            .transpose()
            .map_err(|_| anyhow::anyhow!("invalid stored item name"))?,
        checked_at: model.checked_at,
        recorded_by: model.recorded_by,
    })
}

pub fn model(record: &SiteRecord) -> SiteModel {
    SiteModel {
        session_id: record.session.to_string(),
        origin: record.origin.as_str().to_owned(),
        state: record.state.as_str().to_owned(),
        item_name: record.item.as_ref().map(|item| item.as_str().to_owned()),
        checked_at: record.checked_at.clone(),
        recorded_by: record.recorded_by.clone(),
    }
}
