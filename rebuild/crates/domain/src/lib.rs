use std::{error::Error, fmt};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}
impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionName(String);
impl SessionName {
    pub fn parse(value: &str) -> Result<Self, InvalidName> {
        let value = value.trim();
        if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
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
        f.write_str("session name must be 1–120 bytes and contain no control characters")
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Generation(Uuid);
impl Generation {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for Generation {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for Generation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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
