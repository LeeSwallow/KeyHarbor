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
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(value)?))
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

/// Identifies one authentication handoff request; the user resumes by quoting it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId(Uuid);
impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}
impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

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

fn is_local(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// The URL a session opens with. It is passed on the browser command line (visible in `ps`),
/// so it may not carry credentials, a query or a fragment, and must be HTTPS or local HTTP.
pub fn validate_start_url(value: &str) -> Result<String, vault::InvalidValue> {
    if value.len() > 2048 {
        return Err(vault::InvalidValue("URL exceeds 2048 bytes"));
    }
    let url = url::Url::parse(value).map_err(|_| vault::InvalidValue("invalid URL"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(vault::InvalidValue(
            "URL must not include user information, query, or fragment",
        ));
    }
    if url.host().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && is_local(&url)))
    {
        return Err(vault::InvalidValue(
            "URL must use HTTPS or HTTP on localhost",
        ));
    }
    Ok(url.into())
}

/// A URL an agent may navigate to: any http(s) URL, or `about:blank`.
pub fn validate_navigation_url(value: &str) -> Result<String, vault::InvalidValue> {
    if value == "about:blank" {
        return Ok(value.to_owned());
    }
    if value.len() > 8192 {
        return Err(vault::InvalidValue("URL exceeds 8192 bytes"));
    }
    let url = url::Url::parse(value).map_err(|_| vault::InvalidValue("invalid URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err(vault::InvalidValue("URL must use HTTP or HTTPS"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(vault::InvalidValue("URL must not include user information"));
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_urls_reject_unsafe_destinations_and_credentials() {
        assert!(validate_start_url("https://example.com/login").is_ok());
        assert!(validate_start_url("http://127.0.0.1:8080/login").is_ok());
        for url in [
            "http://example.com",
            "file:///etc/passwd",
            "https://user:secret@example.com",
            "https://example.com/?token=secret",
            "https://example.com/#secret",
        ] {
            assert!(validate_start_url(url).is_err(), "{url}");
        }
        assert!(validate_navigation_url("http://example.com/?q=1").is_ok());
        assert!(validate_navigation_url("about:blank").is_ok());
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "chrome://settings",
            "https://u:p@example.com",
        ] {
            assert!(validate_navigation_url(url).is_err(), "{url}");
        }
    }
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
pub mod audit;
pub mod policy;
pub mod ports;
pub mod secret;
pub mod vault;
