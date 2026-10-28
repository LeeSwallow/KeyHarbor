//! Session rows as stored ↔ domain types.

use super::models::SessionModel;
use anyhow::{Context, Result};
use keyharbor_domain::{
    Generation, HandoffStatus, RequestId, SessionId, SessionName, State, ports::SessionRow,
};

pub fn session_row(model: SessionModel) -> Result<SessionRow> {
    Ok(SessionRow {
        id: SessionId::parse(&model.id).context("invalid stored session id")?,
        name: SessionName::parse(&model.name).context("invalid stored session name")?,
        state: State::parse(&model.state).context("invalid stored session state")?,
        generation: model
            .generation
            .map(|g| Generation::parse(&g))
            .transpose()
            .context("invalid stored generation")?,
        cdp: model.cdp.unwrap_or(false),
    })
}

pub fn request_id(value: &str) -> Result<RequestId> {
    RequestId::parse(value).context("invalid stored request id")
}

pub fn handoff_status(value: &str) -> Result<HandoffStatus> {
    HandoffStatus::parse(value).context("invalid stored handoff status")
}
