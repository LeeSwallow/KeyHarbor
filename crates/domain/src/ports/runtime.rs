//! The session runtime: supervisor processes, their OS locks and control sockets.
//! `keyharbor-supervisor` implements it; the session use cases in `keyharbor-core` drive it.

use crate::{
    Generation, SessionId,
    browser_choice::{BrowserChoice, BrowserKind},
};
use anyhow::Result;
use std::{
    any::Any,
    path::{Path, PathBuf},
    time::Duration,
};

/// Held while it lives (an OS file lock).
pub type RuntimeLock = Box<dyn Any + Send>;

pub struct LaunchOptions<'a> {
    pub browser: &'a ResolvedBrowser,
    /// Already validated by `validate_start_url`.
    pub url: Option<&'a str>,
    pub cdp: bool,
    /// Run the browser without a window (Chrome only).
    pub headless: bool,
}

/// What a browser choice came to on this machine (#120).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBrowser {
    pub kind: BrowserKind,
    /// Chrome's executable, or the `ego-browser` command for ego lite.
    pub executable: PathBuf,
}

/// A supervisor process that was started for one run.
pub trait StartedSupervisor: Send {
    /// `Some(status)` once the process has exited.
    fn exited(&mut self) -> Result<Option<String>>;
    /// Stops watching; the process is reaped in the background when it exits.
    fn detach(self: Box<Self>);
}

pub trait SessionRuntime: Send + Sync {
    /// True if `browser` names a file a supervisor could start.
    fn has_browser(&self, browser: &Path) -> bool;
    /// What `choice` means here. `Auto` is ego lite if its command answers within `probe`,
    /// otherwise Chrome; `EgoLite` fails when ego lite does not answer.
    fn resolve_browser(&self, choice: &BrowserChoice, probe: Duration) -> Result<ResolvedBrowser>;
    /// Fails, before anything is reserved, when this session cannot run here (for example the
    /// data directory is too long for its control socket).
    fn preflight(&self, id: SessionId) -> Result<()>;
    /// True while a supervisor holds the session's lock.
    fn is_alive(&self, id: SessionId) -> bool;
    /// Takes the session's supervisor lock; fails at once if it is held.
    fn lock(&self, id: SessionId) -> Result<RuntimeLock>;
    /// Starts a detached supervisor for the reserved run `generation`.
    fn start(
        &self,
        id: SessionId,
        generation: Generation,
        options: &LaunchOptions<'_>,
    ) -> Result<Box<dyn StartedSupervisor>>;
    /// Asks the running supervisor to close the browser.
    fn close(&self, id: SessionId) -> Result<()>;
    /// Removes a dead supervisor's control socket (best effort).
    fn remove_socket(&self, id: SessionId);
    /// Deletes the session's browser profile, if it has one. Only for a stopped session.
    fn remove_profile(&self, id: SessionId) -> Result<()>;
    /// The raw CDP WebSocket address the browser published, checked for shape.
    fn debug_endpoint(&self, id: SessionId) -> Result<String>;
}
