//! Why the guard refused to use an item. The usage record, the error an agent sees and the
//! CLI all show [`DenialReason::as_str`], defined only here.

/// One reason a use of an item was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DenialReason {
    /// No such item or field.
    NotFound,
    /// The item's metadata failed verification.
    Tampered,
    Discarded,
    /// A policy rule denies the agent.
    Policy,
    /// The frame's origin is not one of the item's origins.
    Origin,
    /// A developer item lists no origin, so it goes into no page (R-36).
    NoOrigin,
    /// A password field may go only into a password input (C1).
    FieldKind,
    /// The focus is not on the target, or not on an element that takes text (C2).
    Focus,
    /// The focused frame could not be located or entered.
    Frame,
    /// Substitution is locked for the session (C2, I1).
    Locked,
    /// A navigation is under way or happened during the checks (I4).
    Navigating,
    /// The vault could not be read.
    Vault,
    /// `keyharbor run` may not inject the item (R-35).
    RunNotAllowed,
    /// The item's expiry date has passed (R-37).
    Expired,
}

impl DenialReason {
    pub const ALL: [DenialReason; 14] = [
        Self::NotFound,
        Self::Tampered,
        Self::Discarded,
        Self::Policy,
        Self::Origin,
        Self::NoOrigin,
        Self::FieldKind,
        Self::Focus,
        Self::Frame,
        Self::Locked,
        Self::Navigating,
        Self::Vault,
        Self::RunNotAllowed,
        Self::Expired,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Tampered => "tampered",
            Self::Discarded => "discarded",
            Self::Policy => "policy",
            Self::Origin => "origin",
            Self::NoOrigin => "no-origin",
            Self::FieldKind => "field-kind",
            Self::Focus => "focus",
            Self::Frame => "frame",
            Self::Locked => "locked",
            Self::Navigating => "navigating",
            Self::Vault => "vault",
            Self::RunNotAllowed => "run-not-allowed",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == value)
    }
}

impl std::fmt::Display for DenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reasons_round_trip() {
        for reason in DenialReason::ALL {
            assert_eq!(DenialReason::parse(reason.as_str()), Some(reason));
        }
    }
}
