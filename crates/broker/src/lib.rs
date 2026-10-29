//! The Chrome adapter: starts Chrome with its CDP pipe ([`chrome`], ADR-0004) and implements
//! `BrowserPort`, `PageInput` and `FrameInspector` over CDP ([`CdpBrowser`]).
//!
//! It knows only the domain ports. `CdpBrowser` types exactly the text it is given and returns
//! results unmasked, so only `keyharbor-supervisor` depends on this crate, and it hands every
//! control request to `keyharbor_guard::GuardedBrowser` wrapped around it (check-layers B). It
//! also reports sign-ins the user submits (`CdpLink::capture_logins`, 08 §8.2 ②).

mod browser;
mod capture;
mod cdp;
pub mod chrome;

pub use browser::CdpBrowser;
pub use chrome::DEFAULT_BROWSER;
