//! The browser a session's supervisor drives. `keyharbor-broker` implements these traits
//! over CDP; `keyharbor-guard` wraps the implementation in `GuardedBrowser`, which is the
//! only `BrowserPort` that control requests (and therefore MCP and the CLI) can reach.
//!
//! Results are JSON values in the control protocol's shape. A target is `{"ref": N}` (a
//! backend node id from a snapshot) or `{"selector": css}`.

use crate::vault::Origin;
use anyhow::Result;
use serde_json::Value;
use std::future::Future;

/// What `wait` waits for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitFor {
    /// Until an element matches the CSS selector (at most 15 s).
    Selector(String),
    /// A fixed time in milliseconds (at most 15 s).
    Millis(u64),
}

/// The operations an agent asks for. Text given to `fill` and `type_text` is typed as is.
pub trait BrowserPort: Send {
    fn navigate(&mut self, url: &str) -> impl Future<Output = Result<Value>> + Send;
    fn snapshot(&mut self) -> impl Future<Output = Result<Value>> + Send;
    fn screenshot(&mut self) -> impl Future<Output = Result<Value>> + Send;
    fn click(&mut self, target: &Value) -> impl Future<Output = Result<Value>> + Send;
    /// Replaces the target element's value with `text`.
    fn fill(&mut self, target: &Value, text: &str) -> impl Future<Output = Result<Value>> + Send;
    /// Types `text` into the focused element.
    fn type_text(&mut self, text: &str) -> impl Future<Output = Result<Value>> + Send;
    fn press(&mut self, key: &str) -> impl Future<Output = Result<Value>> + Send;
    fn tabs(&mut self) -> impl Future<Output = Result<Value>> + Send;
    fn new_tab(&mut self, url: Option<&str>) -> impl Future<Output = Result<Value>> + Send;
    fn switch_tab(&mut self, id: &str) -> impl Future<Output = Result<Value>> + Send;
    fn wait(&mut self, until: &WaitFor) -> impl Future<Output = Result<Value>> + Send;
    /// Asks the browser to exit; returns at once.
    fn close(&mut self) -> Result<Value>;
}

/// The attached page (tab) the input steps act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page(pub String);

/// The element that has keyboard focus and the frame whose document holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub frame: String,
    pub node: u64,
}

/// The steps `fill` and `type_text` are made of, so an enforcement point can check the focus
/// and frame between them. Not a `BrowserPort`: nothing outside the guard calls them.
pub trait PageInput: Send {
    /// The active tab, attaching to it on first use.
    fn page(&mut self) -> impl Future<Output = Result<Page>> + Send;
    /// Resolves `target` and gives it keyboard focus; returns its backend node id.
    fn focus_element(
        &mut self,
        page: &Page,
        target: &Value,
    ) -> impl Future<Output = Result<u64>> + Send;
    /// Empties the element's value (this runs page code).
    fn clear_element(&mut self, page: &Page, node: u64) -> impl Future<Output = Result<()>> + Send;
    fn insert_text(&mut self, page: &Page, text: &str) -> impl Future<Output = Result<()>> + Send;
}

/// Where keyboard input would land, as the browser (not the page) reports it.
pub trait FrameInspector: Send {
    /// Follows `document.activeElement` down through same-process frames.
    fn focus(&mut self, page: &Page) -> impl Future<Output = Result<Focus>> + Send;
    /// The security origin of `frame`, if it is one an item can be bound to.
    fn frame_origin(
        &mut self,
        page: &Page,
        frame: &str,
    ) -> impl Future<Output = Option<Origin>> + Send;
}
