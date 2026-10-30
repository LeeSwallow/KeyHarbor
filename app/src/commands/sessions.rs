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

fn session_id(id: &str) -> Result<keyharbor_domain::SessionId> {
    use anyhow::Context;
    keyharbor_domain::SessionId::parse(id).context("invalid session id")
}

/// Opens the session's browser (09 §9.4.3 [열기]).
pub fn open_session(ctx: &Ctx, id: &str) -> Result<()> {
    let options = session::OpenOptions {
        url: None,
        cdp: false,
        browser: std::path::Path::new(keyharbor_supervisor::DEFAULT_BROWSER),
        agent: keyharbor_domain::policy::LOCAL_AGENT,
    };
    session::open(
        &*ctx.store()?,
        &*ctx.runtime(),
        ctx.clock(),
        session_id(id)?,
        &options,
    )
}

/// Closes the session's browser ([중지]).
pub fn stop_session(ctx: &Ctx, id: &str) -> Result<()> {
    let agent = keyharbor_domain::policy::LOCAL_AGENT;
    session::stop(
        &*ctx.store()?,
        &*ctx.runtime(),
        ctx.clock(),
        session_id(id)?,
        agent,
    )
    .map(|_| ())
}

/// Marks a session whose supervisor is gone as stopped ([복구…]). Only after the person
/// confirmed that its browser windows are closed, as `recover --confirm-browser-closed`.
pub fn recover_session(ctx: &Ctx, id: &str, browser_closed: bool) -> Result<()> {
    if !browser_closed {
        anyhow::bail!("close the session's browser windows first, then confirm");
    }
    session::recover(&*ctx.store()?, &*ctx.runtime(), session_id(id)?)
}

/// Asks the browser extension to hand the site of `url` to the session `name` (drag to hand
/// off, spec 2026-10-31). The extension asks the person in their browser first, so this waits
/// for them; the text says how it ended. No value comes back.
pub fn request_site(ctx: &Ctx, name: &str, url: &str) -> Result<String> {
    use keyharbor_supervisor::bridge::{SiteAnswer, SiteAsk, ask};
    let answer = ask(
        ctx.root(),
        &SiteAsk {
            session: name.to_owned(),
            url: url.to_owned(),
        },
    )?;
    Ok(match answer {
        SiteAnswer::HandedOff { cookies } => format!("쿠키 {cookies}개를 {name} 세션에 넘겼습니다"),
        SiteAnswer::Declined => "브라우저에서 넘기지 않았습니다".to_owned(),
        SiteAnswer::NoPermission => {
            "확장이 이 사이트에 접근할 수 없습니다. 확장 팝업에서 한 번 넘겨 허용하세요".to_owned()
        }
        SiteAnswer::NoSession => format!("실행 중인 {name} 세션이 없습니다"),
        SiteAnswer::BadSite => "http·https 사이트만 넘길 수 있습니다".to_owned(),
        SiteAnswer::Timeout => "브라우저에서 답이 오지 않았습니다".to_owned(),
        SiteAnswer::Failed => "넘기지 못했습니다".to_owned(),
    })
}
