//! The session runtime: supervisor processes, their OS locks and control sockets.
//! `keyharbor-supervisor` implements it; the session use cases in `keyharbor-core` drive it.

use crate::{Generation, SessionId};
use anyhow::Result;
use std::{any::Any, path::Path};

/// Held while it lives (an OS file lock).
pub type RuntimeLock = Box<dyn Any + Send>;

pub struct LaunchOptions<'a> {
    pub browser: &'a Path,
    /// Already validated by `validate_start_url`.
    pub url: Option<&'a str>,
    pub cdp: bool,
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
    /// The raw CDP WebSocket address the browser published, checked for shape.
    fn debug_endpoint(&self, id: SessionId) -> Result<String>;
}
