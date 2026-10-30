//! `receive_site`: a site handed over from the extension to a running session (R-42).

use super::{Refusal, VERSION};
use crate::compose::App;
use keyharbor_core::session as sessions;
use keyharbor_domain::{
    SessionName,
    handoff_site::{self, SiteCookie},
    vault::Origin,
};
use keyharbor_supervisor::control::{
    client,
    wire::{ControlEnvelope, ControlRequest, CookieWire},
};
use serde::Deserialize;
use std::collections::BTreeMap;

/// `receive_site` as the extension sends it (`ReceiveSiteWire` in protocol.ts).
#[derive(Deserialize)]
struct ReceiveSiteWire {
    v: u64,
    r#type: String,
    origin: String,
    cookies: Vec<CookieWire>,
    #[serde(default)]
    local_storage: Option<BTreeMap<String, String>>,
    ttl_seconds: u64,
    session: String,
}

/// Checks the request and hands it to the named session's supervisor; returns the session
/// name and how many cookies it took.
pub(super) fn receive(app: &App, body: &[u8]) -> Result<(String, u64), Refusal> {
    let wire: ReceiveSiteWire = serde_json::from_slice(body).map_err(|_| Refusal::BadRequest)?;
    if wire.v != VERSION || wire.r#type != "receive_site" {
        return Err(Refusal::BadRequest);
    }
    let origin = Origin::parse(&wire.origin).map_err(|_| Refusal::BadRequest)?;
    let name = SessionName::parse(&wire.session).map_err(|_| Refusal::BadRequest)?;
    let cookies: Vec<SiteCookie> = wire.cookies.into_iter().map(Into::into).collect();
    handoff_site::check(&origin, &cookies, wire.ttl_seconds).map_err(|_| Refusal::Refused)?;

    let store = app.store().map_err(|_| Refusal::Failed)?;
    let runtime = app.runtime().map_err(|_| Refusal::Failed)?;
    let session = sessions::list(&*store, &*runtime)
        .map_err(|_| Refusal::Failed)?
        .into_iter()
        .find(|view| view.row.name == name)
        .ok_or(Refusal::NoSession)?;
    if !runtime.is_alive(session.row.id) {
        return Err(Refusal::NotRunning);
    }
    let request = ControlRequest::ReceiveSite {
        origin: origin.as_str().to_owned(),
        cookies: cookies.iter().map(CookieWire::from).collect(),
        local_storage: wire.local_storage,
        ttl_seconds: wire.ttl_seconds,
    };
    let result = client::request(
        app.root(),
        &session.row.id,
        &ControlEnvelope::local(request),
    )
    .map_err(|_| Refusal::Failed)?;
    Ok((
        name.as_str().to_owned(),
        result["cookies"].as_u64().unwrap_or(0),
    ))
}
