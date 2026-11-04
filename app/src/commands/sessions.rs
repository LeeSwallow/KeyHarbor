//! The sessions screen (09 §9.4.3): each session's state and its sites' sign-in states.

use super::ids::session_id;
use crate::Ctx;
use anyhow::Result;
use keyharbor_core::{session, site};
use keyharbor_domain::{browser_choice::BrowserChoice, policy::LOCAL_AGENT};
use keyharbor_supervisor::bridge::SiteAnswer;
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
    /// A run is under way: its browser badge applies (`DisplayState::is_live`).
    pub live: bool,
    /// Only [복구…] brings it back (`DisplayState::is_recoverable`).
    pub recoverable: bool,
    pub cdp: bool,
    /// `chrome` or `ego-lite` while the session has a run; with its `protection`.
    pub browser: Option<&'static str>,
    pub protection: Option<&'static str>,
    /// Why the last run did not start.
    pub failure: Option<String>,
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
                live: view.state.is_live(),
                recoverable: view.state.is_recoverable(),
                cdp: view.row.cdp,
                browser: view.row.browser.map(|b| b.as_str()),
                protection: view.row.browser.map(|b| b.protection().as_str()),
                failure: view.row.failure.clone(),
                description: view.row.description.map(|d| d.as_str().to_owned()),
                sites,
            })
        })
        .collect()
}

/// Opens the session's browser (09 §9.4.3 [열기]) in `browser`, or the `browser` setting.
pub fn open_session(ctx: &Ctx, id: &str, browser: Option<&str>) -> Result<()> {
    let choice = browser.map(BrowserChoice::parse).transpose()?;
    let resolved = session::resolve_browser(&*ctx.store()?, &*ctx.runtime(), choice.as_ref())?;
    let options = session::OpenOptions {
        url: None,
        cdp: false,
        browser: &resolved,
        headless: false,
        agent: LOCAL_AGENT,
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
    session::stop(
        &*ctx.store()?,
        &*ctx.runtime(),
        ctx.clock(),
        session_id(id)?,
        LOCAL_AGENT,
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

/// How a site handed off from the person's browser ended. The window words it (`words.ts`
/// `handOffWord`); no value comes back.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SiteHandOff {
    HandedOff { cookies: u64 },
    Declined,
    NoPermission,
    NoSession,
    BadSite,
    Timeout,
    Failed,
}

impl SiteHandOff {
    /// The bridge's wire answer in the app's words; the wire type stays here.
    fn from_answer(answer: SiteAnswer) -> Self {
        match answer {
            SiteAnswer::HandedOff { cookies } => Self::HandedOff { cookies },
            SiteAnswer::Declined => Self::Declined,
            SiteAnswer::NoPermission => Self::NoPermission,
            SiteAnswer::NoSession => Self::NoSession,
            SiteAnswer::BadSite => Self::BadSite,
            SiteAnswer::Timeout => Self::Timeout,
            SiteAnswer::Failed => Self::Failed,
        }
    }
}

/// Asks the browser extension to hand the site of `url` to the session `name` (drag to hand
/// off, spec 2026-10-31). The extension asks the person in their browser first, so this waits
/// for them.
pub fn request_site(ctx: &Ctx, name: &str, url: &str) -> Result<SiteHandOff> {
    use keyharbor_supervisor::bridge::{SiteAsk, ask};
    let request = SiteAsk {
        session: name.to_owned(),
        url: url.to_owned(),
    };
    Ok(SiteHandOff::from_answer(ask(ctx.root(), &request)?))
}

/// Removes a stopped session and its profile ([지우기], after the undo time).
pub fn remove_session(ctx: &Ctx, id: &str) -> Result<()> {
    session::remove::remove(
        &*ctx.store()?,
        &*ctx.runtime(),
        ctx.clock(),
        session_id(id)?,
        session::remove::Force::No,
        LOCAL_AGENT,
    )
}

/// Which browsers the settings screen can offer (#123).
#[derive(Serialize)]
pub struct BrowsersDto {
    pub ego_lite: bool,
}

/// Whether ego lite answers now, given as long as `auto` would give it (`ego_probe_seconds`).
pub fn browsers(ctx: &Ctx) -> Result<BrowsersDto> {
    Ok(BrowsersDto {
        ego_lite: session::ego_lite_answers(&*ctx.store()?, &*ctx.runtime())?,
    })
}
