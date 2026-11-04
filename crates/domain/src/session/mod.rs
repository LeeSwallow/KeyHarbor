//! Browser sessions: their ids, names and states, authentication handoffs, which URLs they
//! open, which browser they run in and which files they may upload.

pub mod browser_choice;
pub mod substitution_lock;
pub mod upload;
mod url;

pub use url::{
    NAVIGATION_URL_MAX_BYTES, START_URL_MAX_BYTES, validate_navigation_url, validate_start_url,
};

use std::{error::Error, fmt};

uuid_id!(
    /// Identifies one session.
    SessionId
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionName(String);
impl SessionName {
    number_text!(
        /// The longest session name, in bytes.
        pub MAX_BYTES: usize = 120,
        RULE = "session name must be 1–", " bytes and contain no control characters"
    );

    pub fn parse(value: &str) -> Result<Self, InvalidName> {
        let value = value.trim();
        if value.is_empty() || value.len() > Self::MAX_BYTES || value.chars().any(char::is_control)
        {
            return Err(InvalidName);
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug)]
pub struct InvalidName;
impl fmt::Display for InvalidName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(SessionName::RULE)
    }
}
impl Error for InvalidName {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Stopped,
    Starting,
    Running,
    AwaitingUser,
    Stopping,
    Failed,
}
impl State {
    pub fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Stopped, Self::Starting)
                | (Self::Starting, Self::Running | Self::Failed)
                | (
                    Self::Running,
                    Self::AwaitingUser | Self::Stopping | Self::Stopped | Self::Failed
                )
                | (
                    Self::AwaitingUser,
                    Self::Running | Self::Stopping | Self::Stopped | Self::Failed
                )
                | (Self::Stopping, Self::Stopped | Self::Failed)
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::AwaitingUser => "awaiting_user",
            Self::Stopping => "stopping",
            Self::Failed => "failed",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stopped" => Some(Self::Stopped),
            "starting" => Some(Self::Starting),
            "running" => Some(Self::Running),
            "awaiting_user" => Some(Self::AwaitingUser),
            "stopping" => Some(Self::Stopping),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

uuid_id!(Generation);

uuid_id!(
    /// Identifies one authentication handoff request; the user resumes by quoting it.
    RequestId
);

/// Where a handoff request stands. Only `Resumed` means the user said they are done.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandoffStatus {
    Pending,
    Resumed,
    /// A newer request for the same session replaced it.
    Superseded,
    /// The session stopped, failed or was recovered while it was pending.
    Cancelled,
}
impl HandoffStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resumed => "resumed",
            Self::Superseded => "superseded",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "resumed" => Some(Self::Resumed),
            "superseded" => Some(Self::Superseded),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: SessionId,
    pub name: SessionName,
    pub state: State,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_trimmed_and_reject_controls() {
        assert_eq!(SessionName::parse("  work  ").unwrap().as_str(), "work");
        assert!(SessionName::parse("\t ").is_err());
        assert!(SessionName::parse("a\nb").is_err());
        assert!(SessionName::parse(&"x".repeat(121)).is_err());
    }
    #[test]
    fn state_changes_require_valid_edges() {
        assert!(State::Stopped.permits(State::Starting));
        assert!(State::Running.permits(State::AwaitingUser));
        assert!(!State::Stopped.permits(State::Running));
        assert!(!State::Failed.permits(State::Starting));
    }
}
