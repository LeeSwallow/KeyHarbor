//! Which browser a session runs in (#120, ADR-0018): Chrome with its own profile, or ego lite
//! through its SDK. The person or agent names a choice; the runtime resolves it to a kind.

use crate::vault::InvalidValue;
use std::{fmt, path::PathBuf};

/// The browsers a session can run in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrowserKind {
    /// Chrome (or a Chromium-like executable) started by the supervisor with its own profile.
    Chrome,
    /// ego lite, driven through its SDK on the person's ego profile.
    EgoLite,
}

impl BrowserKind {
    pub const ALL: [BrowserKind; 2] = [Self::Chrome, Self::EgoLite];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::EgoLite => "ego-lite",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
    /// Who else can drive the browser while KeyHarbor does (03 §3.13).
    pub fn protection(self) -> Protection {
        match self {
            Self::Chrome => Protection::Owned,
            Self::EgoLite => Protection::Shared,
        }
    }
}

impl fmt::Display for BrowserKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How far KeyHarbor's promises hold in a session's browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    /// Only the supervisor drives the browser: agents never read what it typed.
    Owned,
    /// Other programs can drive the same browser (ego lite's SDK): an agent with a shell can
    /// read what KeyHarbor typed (R-47(2)).
    Shared,
}

impl Protection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::Shared => "shared",
        }
    }
}

/// What a person or agent asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserChoice {
    /// ego lite when it answers, otherwise Chrome (R-47(1)).
    Auto,
    Chrome,
    EgoLite,
    /// A Chrome-like executable at this absolute path.
    Path(PathBuf),
}

impl BrowserChoice {
    /// The setting `browser` is never set to anything else by default.
    pub const DEFAULT: &'static str = "auto";

    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        match value {
            "auto" => Ok(Self::Auto),
            "chrome" => Ok(Self::Chrome),
            "ego-lite" => Ok(Self::EgoLite),
            path if path.starts_with('/') => Ok(Self::Path(PathBuf::from(path))),
            _ => Err(InvalidValue(
                "browser must be auto, chrome, ego-lite or an absolute path",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_browser_choice() {
        assert_eq!(BrowserChoice::parse("auto").unwrap(), BrowserChoice::Auto);
        assert_eq!(
            BrowserChoice::parse("chrome").unwrap(),
            BrowserChoice::Chrome
        );
        assert_eq!(
            BrowserChoice::parse("ego-lite").unwrap(),
            BrowserChoice::EgoLite
        );
        assert_eq!(
            BrowserChoice::parse("/Applications/Chromium.app/Contents/MacOS/Chromium").unwrap(),
            BrowserChoice::Path("/Applications/Chromium.app/Contents/MacOS/Chromium".into())
        );
        for bad in ["", "Chrome", "ego", "relative/chrome"] {
            assert!(BrowserChoice::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn only_ego_lite_shares_its_browser() {
        assert_eq!(BrowserKind::Chrome.protection(), Protection::Owned);
        assert_eq!(BrowserKind::EgoLite.protection(), Protection::Shared);
        for kind in BrowserKind::ALL {
            assert_eq!(BrowserKind::parse(kind.as_str()), Some(kind));
        }
    }
}
