//! The browser a run starts: Chrome over its pipe, or ego lite through the relay.

use anyhow::Result;
use keyharbor_broker::{
    Gate,
    chrome::{self, ChromeLaunch},
    ego::{self, EgoLaunch},
};

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
