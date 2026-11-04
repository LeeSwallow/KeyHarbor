//! Everything a run checks and assembles before the browser starts: the start page, the
//! private paths ([`prepare`]) and the browser to launch (Chrome over its pipe, or ego lite
//! through the relay). Any failure here is the run's start failure (J-180).

use super::prepare::{Prepared, prepare};
use crate::{argv::SuperviseArgs, ops::DENIED_OUT_OF_GRANT, paths};
use anyhow::{Context, Result, anyhow};
use keyharbor_broker::{
    Gate,
    chrome::{self, ChromeLaunch},
    ego::{self, EgoLaunch},
};
use keyharbor_core::{grant, settings};
use keyharbor_domain::{
    SessionId,
    browser_choice::BrowserKind,
    ports::{Clock, Store},
    settings::EGO_POLL_MS,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// How much of the session id an ego lite space name carries: the UUID's first group, enough
/// to tell two sessions of the same name apart in ego's space list.
const SPACE_ID_CHARS: usize = 8;

/// The browser a run starts: Chrome over its pipe, or ego lite through the relay.
pub(super) enum Launch {
    Chrome(ChromeLaunch),
    Ego(EgoLaunch),
}

impl Launch {
    /// How this browser keeps out-of-grant documents out.
    pub(super) fn gate(&self) -> Gate {
        match self {
            Self::Chrome(_) => Gate::Intercept,
            Self::Ego(_) => Gate::AfterCommit,
        }
    }

    pub(super) async fn spawn(&self) -> Result<chrome::Chrome> {
        match self {
            Self::Chrome(launch) => chrome::spawn(launch),
            Self::Ego(launch) => ego::spawn(launch).await,
        }
    }
}

/// A run ready to start: the browser to launch and the run's control socket and screenshot
/// folder.
pub(super) struct Planned {
    pub launch: Launch,
    pub socket: PathBuf,
    pub shots: PathBuf,
}

/// Checks the start request against the session and assembles its browser launch.
/// `headless` is the composition root's default (`KEYHARBOR_BROWSER_HEADLESS`).
pub(super) fn launch_for(
    data: &Path,
    store: &dyn Store,
    clock: &dyn Clock,
    args: SuperviseArgs,
    headless: bool,
) -> Result<Planned> {
    let SuperviseArgs {
        id,
        browser,
        url,
        cdp,
        kind,
        headless: asked_headless,
        call_timeout,
        ..
    } = args;
    let is_ego = kind == BrowserKind::EgoLite;
    if is_ego && cdp {
        return Err(anyhow!("raw CDP is not available in ego lite sessions"));
    }
    let url = start_url(store, clock, id, url.as_deref())?;
    let Prepared {
        profile,
        socket,
        shots,
    } = prepare(data, &id, cdp, !is_ego)?;
    let launch = if is_ego {
        Launch::Ego(EgoLaunch {
            command: browser,
            socket: paths::ego_socket_path(data, &id),
            log: paths::ego_log_path(data, &id),
            space: ego_space(store, id)?,
            url,
            poll: ego_poll(store)?,
            call_timeout,
        })
    } else {
        Launch::Chrome(ChromeLaunch {
            executable: browser,
            profile,
            url,
            debug_port: cdp,
            headless: headless || asked_headless,
            call_timeout,
        })
    };
    Ok(Planned {
        launch,
        socket,
        shots,
    })
}

/// The validated start page. The page watcher attaches after the browser loads it, so its
/// grant is checked here.
fn start_url(
    store: &dyn Store,
    clock: &dyn Clock,
    id: SessionId,
    url: Option<&str>,
) -> Result<Option<String>> {
    let Some(url) = url
        .map(keyharbor_domain::validate_start_url)
        .transpose()
        .map_err(|error| anyhow!("{error}"))?
    else {
        return Ok(None);
    };
    if !grant::allows(store, clock, id, &url)? {
        return Err(anyhow!(DENIED_OUT_OF_GRANT));
    }
    Ok(Some(url))
}

/// The ego lite space this session runs in: its name and the start of its id.
fn ego_space(store: &dyn Store, id: SessionId) -> Result<String> {
    let row = store.get(id)?.context("session not found")?;
    let id = id.to_string();
    Ok(format!(
        "keyharbor-{}-{}",
        row.name.as_str(),
        &id[..SPACE_ID_CHARS]
    ))
}

/// How often the ego lite relay takes page events (`ego_poll_ms`).
fn ego_poll(store: &dyn Store) -> Result<Duration> {
    let poll = settings::get(store, EGO_POLL_MS)?
        .parse()
        .context("invalid ego_poll_ms")?;
    Ok(Duration::from_millis(poll))
}
