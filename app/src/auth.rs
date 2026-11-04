//! Who may use the app's protected paths (03 §3.10, R-21): the [`Authenticator`] port and its
//! one adapter, [`NoneAuth`]. A UI control, not a key: it does not guard the vault key (03
//! §3.3). The port lives in the app because only the app asks it (J-126).

use std::fmt;

/// What a person is about to do in the app; an authenticator may ask for each separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthScope {
    OpenApp,
    Reveal,
    Edit,
    Export,
    PolicyChange,
}

impl AuthScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenApp => "open_app",
            Self::Reveal => "reveal",
            Self::Edit => "edit",
            Self::Export => "export",
            Self::PolicyChange => "policy_change",
        }
    }
}

/// The person did not pass the challenge for the scope; nothing was done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused(pub AuthScope);

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "authentication refused for {}", self.0.as_str())
    }
}

impl std::error::Error for Refused {}

/// Asks the person at the Mac to prove they may go on. Commands call [`challenge`] before
/// they do anything for a scope, and do nothing when it is refused. It does not remember
/// earlier answers unless an adapter chooses to.
///
/// [`challenge`]: Authenticator::challenge
pub trait Authenticator: Send + Sync {
    /// The method's stable name, which the settings screen shows in words (`none`).
    fn id(&self) -> &'static str;
    /// `Ok` when the person may go on with `scope`.
    fn challenge(&self, scope: AuthScope) -> Result<(), Refused>;
}

/// No authentication: every challenge passes (the default and, for now, the only method, R-21).
pub struct NoneAuth;

impl Authenticator for NoneAuth {
    fn id(&self) -> &'static str {
        "none"
    }
    fn challenge(&self, _scope: AuthScope) -> Result<(), Refused> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_auth_always_passes() {
        use AuthScope::*;
        for scope in [OpenApp, Reveal, Edit, Export, PolicyChange] {
            assert_eq!(NoneAuth.challenge(scope), Ok(()));
        }
        assert_eq!(NoneAuth.id(), "none");
    }
}
