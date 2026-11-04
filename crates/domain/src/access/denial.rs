//! Why the guard refused to use an item. The usage record, the error an agent sees and the
//! CLI all show [`DenialReason::as_str`], defined only here. [`StopCause`] widens it to every
//! reason an agent's run stopped, and each cause's remedy is written here once (J-240).

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
    /// A program inside `keyharbor run` asked for an item the run did not cover (channel ④).
    OutsideRun,
    /// A request to a run's lookup socket did not carry the run's token (channel ④).
    BadToken,
    /// Agents may not type one-time codes: `agent_otp` is off (R-48(3)).
    OtpOff,
    /// The item's one-time codes are turned off (`otp_allowed`, R-48(3)).
    OtpNotAllowed,
}

impl DenialReason {
    pub const ALL: [DenialReason; 18] = [
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
        Self::OutsideRun,
        Self::BadToken,
        Self::OtpOff,
        Self::OtpNotAllowed,
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
            Self::OutsideRun => "outside-run",
            Self::BadToken => "bad-token",
            Self::OtpOff => "otp-off",
            Self::OtpNotAllowed => "otp-not-allowed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == value)
    }

    /// One line telling a person how to lift the refusal. `<item>`, `<origin>`, `<agent>`,
    /// `<session>` stand for the row it is shown under; never a value.
    pub fn remedy(self) -> &'static str {
        match self {
            Self::NotFound => "check the name: keyharbor vault list",
            Self::Tampered => {
                "the item failed verification; restore it from a backup: keyharbor vault import <backup>"
            }
            Self::Discarded => "add the item again: keyharbor vault add <item> --stdin",
            Self::Policy => {
                "keyharbor policy list, then keyharbor policy allow --agent <agent> --resource item --target <item>"
            }
            Self::Origin => "keyharbor vault edit <item> --origin <origin>",
            Self::NoOrigin => {
                "keyharbor vault edit <item> --origin <origin> (a one-time code also needs --origin-binding on)"
            }
            Self::FieldKind => {
                "a password goes only into a password input; point the agent at that field"
            }
            Self::Focus => "the focus left the input; have the agent click the field and try again",
            Self::Frame => {
                "the focused frame could not be entered; try again once the page settles"
            }
            Self::Locked => SESSION_UNLOCK,
            Self::Navigating => "a page was loading; try again once it has loaded",
            Self::Vault => "the vault could not be read; check keyharbor status",
            Self::RunNotAllowed => "keyharbor vault edit <item> --run-allowed on",
            Self::Expired => "keyharbor vault edit <item> --expires-in <N>d (or --no-expiry)",
            Self::OutsideRun => "keyharbor run --allow <pattern> -- …",
            Self::BadToken => {
                "only the program keyharbor run started may ask; check what else used its socket"
            }
            Self::OtpOff => {
                "type the code yourself (handoff), or let agents type codes: keyharbor settings set agent_otp true"
            }
            Self::OtpNotAllowed => "keyharbor vault edit <item> --otp-allowed on",
        }
    }
}

/// Lifting a session's substitution lock. The command arrives with #190 (J-241).
const SESSION_UNLOCK: &str = "keyharbor unlock <session>";

/// Why an agent's run stopped, as the usage record tells it: a refused item, a document
/// outside the session's grants, refusals counted together under the per-second cap, a
/// session whose substitution was locked, or a handoff still waiting for the person (J-240).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StopCause {
    Denied(DenialReason),
    OutOfGrant,
    Suppressed,
    SessionLocked,
    AwaitingHandoff,
}

impl StopCause {
    /// The text shown for the cause; a refused item shows [`DenialReason::as_str`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Denied(reason) => reason.as_str(),
            Self::OutOfGrant => crate::audit::OUT_OF_GRANT,
            Self::Suppressed => "suppressed",
            Self::SessionLocked => "session-locked",
            Self::AwaitingHandoff => "awaiting-handoff",
        }
    }

    /// One line telling a person how to get the run moving again.
    pub fn remedy(self) -> &'static str {
        match self {
            Self::Denied(reason) => reason.remedy(),
            Self::OutOfGrant => "keyharbor grant add <session> <origin>",
            Self::Suppressed => {
                "refusals over the per-second cap were counted together; lift the session's other causes first"
            }
            Self::SessionLocked => SESSION_UNLOCK,
            Self::AwaitingHandoff => {
                "finish in the browser, then keyharbor resume <session> <request id> (keyharbor handoff <session> shows it)"
            }
        }
    }
}

impl std::fmt::Display for StopCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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

    #[test]
    fn every_cause_has_a_remedy_and_its_own_text() {
        let causes: Vec<StopCause> = DenialReason::ALL
            .into_iter()
            .map(StopCause::Denied)
            .chain([
                StopCause::OutOfGrant,
                StopCause::Suppressed,
                StopCause::SessionLocked,
                StopCause::AwaitingHandoff,
            ])
            .collect();
        let texts: std::collections::HashSet<&str> = causes.iter().map(|c| c.as_str()).collect();
        assert_eq!(texts.len(), causes.len());
        assert!(causes.iter().all(|c| !c.remedy().is_empty()));
        assert_eq!(
            StopCause::Denied(DenialReason::Origin).remedy(),
            "keyharbor vault edit <item> --origin <origin>"
        );
        assert_eq!(
            StopCause::OutOfGrant.remedy(),
            "keyharbor grant add <session> <origin>"
        );
        assert_eq!(
            StopCause::SessionLocked.remedy(),
            StopCause::Denied(DenialReason::Locked).remedy()
        );
    }
}
