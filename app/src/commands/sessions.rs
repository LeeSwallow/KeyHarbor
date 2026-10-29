//! The sessions screen (09 §9.4.3): each session's state and its sites' sign-in states.

use crate::Ctx;
use anyhow::Result;
use keyharbor_core::{session, site};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SiteDto {
    pub origin: String,
    pub state: &'static str,
    pub item: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Serialize)]
pub struct SessionDto {
    pub id: String,
    pub name: String,
    /// `running`, `stopped`, `awaiting_user`, `recovery_required`, …
    pub state: &'static str,
    pub cdp: bool,
    pub description: Option<String>,
    pub sites: Vec<SiteDto>,
}

pub fn sessions(ctx: &Ctx) -> Result<Vec<SessionDto>> {
    let store = ctx.store()?;
    session::list(&*store, &*ctx.runtime())?
        .into_iter()
        .map(|view| {
            let sites = site::list(&*store, view.row.id)?
                .into_iter()
                .map(|r| SiteDto {
                    origin: r.origin.as_str().to_owned(),
                    state: r.state.as_str(),
                    item: r.item.map(|i| i.as_str().to_owned()),
                    checked_at: r.checked_at,
                })
                .collect();
            Ok(SessionDto {
                id: view.row.id.to_string(),
                name: view.row.name.as_str().to_owned(),
                state: view.state.as_str(),
                cdp: view.row.cdp,
                description: view.row.description.map(|d| d.as_str().to_owned()),
                sites,
            })
        })
        .collect()
}
