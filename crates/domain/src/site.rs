//! Whether a session is signed in to a site (02 §2.7 `SITE_STATE`). The agent records what it
//! saw; KeyHarbor does not check it, so a state is a note, not proof.

use crate::{
    SessionId,
    vault::{InvalidValue, ItemName, Origin},
};

/// A session's sign-in state at one origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteState {
    LoggedIn,
    Expired,
    /// Also what an origin with no record reads as.
    Unknown,
}

impl SiteState {
    pub const ALL: [SiteState; 3] = [Self::LoggedIn, Self::Expired, Self::Unknown];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LoggedIn => "logged_in",
            Self::Expired => "expired",
            Self::Unknown => "unknown",
        }
    }
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == value)
            .ok_or(InvalidValue("state must be logged_in, expired or unknown"))
    }
}

/// One recorded state: which item signed in, when and by whom (names only, never a value).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteRecord {
    pub session: SessionId,
    pub origin: Origin,
    pub state: SiteState,
    pub item: Option<ItemName>,
    /// RFC 3339 UTC.
    pub checked_at: String,
    pub recorded_by: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_and_others_are_refused() {
        for state in SiteState::ALL {
            assert_eq!(SiteState::parse(state.as_str()), Ok(state));
        }
        assert!(SiteState::parse("signed_in").is_err());
        assert!(SiteState::parse("").is_err());
    }
}
