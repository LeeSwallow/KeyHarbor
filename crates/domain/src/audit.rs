//! Usage records (02 §2.7). An event names who did what to which item where — never a value.

use crate::{
    denial::{DenialReason, StopCause},
    vault::InvalidValue,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AuditKind {
    SecretUsed,
    SecretDenied,
    OriginChangedAfterFill,
    SessionOpened,
    SessionStopped,
    SessionRemoved,
    FileUploaded,
    HandoffRequested,
    HandoffResumed,
    PolicyChanged,
    VaultItemChanged,
    EnvInjected,
    /// The guard stopped all substitution in a session (focus moved during input, repeated
    /// probing with vault values).
    SubstitutionLocked,
    /// An agent's request carried a vault value (guessing it with the masker as an oracle).
    MaskProbe,
    /// Refusals left out of the record because a session exceeded the per-second cap; the
    /// result is how many.
    DeniedSuppressed,
    /// A personal item was typed into an origin it had not been typed into before (R-36).
    FirstUseOrigin,
    /// An agent or the user recorded a session's sign-in state at a site.
    SiteStateChanged,
    /// An agent asked the user for information (08 §8.2 ③); the result is the request id.
    VaultRequested,
    /// The user answered a request: `fulfilled` or `declined`.
    VaultRequestAnswered,
    /// A sign-in the user typed during a handoff: `draft` (stored for confirmation) or
    /// `known` (the vault already had it).
    CredentialCaptured,
    /// A grant was added, revoked, or its cookies cleared after it ended (R-43).
    GrantChanged,
    /// A document outside the session's grants was not loaded.
    NavigationDenied,
    /// The user handed a site's login to the session from their own browser (R-42); the
    /// result is how many cookies came along.
    SiteHandedOff,
    /// The user looked at or copied a value in the admin app; the result is `shown`.
    ValueRevealed,
    /// The person filled a login into a page of their own browser after confirming with
    /// Touch ID (ADR-0017); the result is `filled`.
    ValueAutofilled,
    /// An agent typed an item's one-time code into a page (`{{otp:item}}`, #206). Like every
    /// event it names the item and origin, never the code.
    OtpUsed,
}
impl AuditKind {
    pub const ALL: [AuditKind; 26] = [
        Self::SecretUsed,
        Self::SecretDenied,
        Self::OriginChangedAfterFill,
        Self::SessionOpened,
        Self::SessionStopped,
        Self::SessionRemoved,
        Self::FileUploaded,
        Self::HandoffRequested,
        Self::HandoffResumed,
        Self::PolicyChanged,
        Self::VaultItemChanged,
        Self::EnvInjected,
        Self::SubstitutionLocked,
        Self::MaskProbe,
        Self::DeniedSuppressed,
        Self::FirstUseOrigin,
        Self::SiteStateChanged,
        Self::VaultRequested,
        Self::VaultRequestAnswered,
        Self::CredentialCaptured,
        Self::GrantChanged,
        Self::NavigationDenied,
        Self::SiteHandedOff,
        Self::ValueRevealed,
        Self::ValueAutofilled,
        Self::OtpUsed,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SecretUsed => "secret_used",
            Self::SecretDenied => "secret_denied",
            Self::OriginChangedAfterFill => "origin_changed_after_fill",
            Self::SessionOpened => "session_opened",
            Self::SessionStopped => "session_stopped",
            Self::SessionRemoved => "session_removed",
            Self::FileUploaded => "file_uploaded",
            Self::HandoffRequested => "handoff_requested",
            Self::HandoffResumed => "handoff_resumed",
            Self::PolicyChanged => "policy_changed",
            Self::VaultItemChanged => "vault_item_changed",
            Self::EnvInjected => "env_injected",
            Self::SubstitutionLocked => "substitution_locked",
            Self::MaskProbe => "mask_probe",
            Self::DeniedSuppressed => "secret_denied_suppressed",
            Self::FirstUseOrigin => "first_use_origin",
            Self::SiteStateChanged => "site_state_changed",
            Self::VaultRequested => "vault_requested",
            Self::VaultRequestAnswered => "vault_request_answered",
            Self::CredentialCaptured => "credential_captured",
            Self::GrantChanged => "grant_changed",
            Self::NavigationDenied => "navigation_denied",
            Self::SiteHandedOff => "site_handed_off",
            Self::ValueRevealed => "value_revealed",
            Self::ValueAutofilled => "value_autofilled",
            Self::OtpUsed => "otp_used",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == value)
    }
    /// Whether the record says an agent was refused: a value, a document outside its grants,
    /// or refusals counted together under the per-second cap. The one place the admin app's
    /// notices and its usage record screen read "거부" from (J-200).
    pub fn is_denial(self) -> bool {
        matches!(
            self,
            Self::SecretDenied | Self::DeniedSuppressed | Self::NavigationDenied
        )
    }
}

/// One usage event. `result` is a short outcome or reason code (`ok`, `policy`, `origin`, …).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEvent {
    pub agent: String,
    pub session_id: Option<String>,
    pub kind: AuditKind,
    pub item_name: Option<String>,
    pub origin: Option<String>,
    pub result: String,
}
impl AuditEvent {
    pub fn new(agent: &str, kind: AuditKind, result: &str) -> Self {
        Self {
            agent: agent.to_owned(),
            session_id: None,
            kind,
            item_name: None,
            origin: None,
            result: result.to_owned(),
        }
    }
    pub fn session(mut self, id: impl ToString) -> Self {
        self.session_id = Some(id.to_string());
        self
    }
    pub fn item(mut self, name: &str) -> Self {
        self.item_name = Some(name.to_owned());
        self
    }
    pub fn origin(mut self, origin: Option<&str>) -> Self {
        self.origin = origin.map(str::to_owned);
        self
    }

    /// Why this event stopped an agent, if it did, and how many stops it stands for (a
    /// suppressed record counts every refusal it holds). A handoff request is a stop even if
    /// it was answered later; whether it still waits is for the caller to tell (J-240).
    /// A refusal whose reason this build does not know is not counted.
    pub fn stop(&self) -> Option<(StopCause, u64)> {
        let cause = match self.kind {
            AuditKind::SecretDenied => StopCause::Denied(DenialReason::parse(&self.result)?),
            AuditKind::NavigationDenied => StopCause::OutOfGrant,
            AuditKind::DeniedSuppressed => {
                return Some((StopCause::Suppressed, self.result.parse().unwrap_or(1)));
            }
            AuditKind::SubstitutionLocked => StopCause::SessionLocked,
            AuditKind::HandoffRequested => StopCause::AwaitingHandoff,
            _ => return None,
        };
        Some((cause, 1))
    }
}

/// How far back to look in the usage record: `<N>m`, `<N>h` or `<N>d` (`--since 24h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lookback(u64);

impl Lookback {
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        const BAD: InvalidValue = InvalidValue("a period is <N>m, <N>h or <N>d with N above 0");
        let (number, unit) = value.split_at(value.len().saturating_sub(1));
        let unit: u64 = match unit {
            "m" => 60,
            "h" => 3_600,
            "d" => 86_400,
            _ => return Err(BAD),
        };
        let n: u64 = number.parse().map_err(|_| BAD)?;
        match n.checked_mul(unit) {
            Some(seconds) if seconds > 0 => Ok(Self(seconds)),
            _ => Err(BAD),
        }
    }
    pub fn seconds(self) -> u64 {
        self.0
    }
}

/// The `result` of a [`AuditKind::GrantChanged`] record. Stored, so the text never changes.
pub mod grant_change {
    pub const ADDED: &str = "added";
    pub const REVOKED: &str = "revoked";
    /// The cookies of an ended grant were removed.
    pub const CLEARED: &str = "cleared";
}

/// The `result` of a [`AuditKind::NavigationDenied`] record: no grant covers the document's
/// origin. Stored, so the text never changes.
pub const OUT_OF_GRANT: &str = "out-of-grant";

/// A stored event with its sequence number and UTC time (RFC 3339).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRecord {
    pub seq: i64,
    pub at: String,
    pub event: AuditEvent,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kinds_round_trip() {
        for kind in AuditKind::ALL {
            assert_eq!(AuditKind::parse(kind.as_str()), Some(kind));
        }
        let event = AuditEvent::new("claude", AuditKind::SecretUsed, "ok")
            .item("github")
            .origin(Some("https://github.com"));
        assert_eq!(event.item_name.as_deref(), Some("github"));
        assert_eq!(event.session_id, None);
    }

    #[test]
    fn denials_are_refusals_of_a_value_or_a_document() {
        let denials: Vec<&str> = AuditKind::ALL
            .into_iter()
            .filter(|k| k.is_denial())
            .map(AuditKind::as_str)
            .collect();
        assert_eq!(
            denials,
            [
                "secret_denied",
                "secret_denied_suppressed",
                "navigation_denied"
            ]
        );
    }

    #[test]
    fn stops_are_refusals_locks_and_handoffs() {
        let stop = |kind, result: &str| AuditEvent::new("a", kind, result).stop();
        assert_eq!(
            stop(AuditKind::SecretDenied, "origin"),
            Some((StopCause::Denied(DenialReason::Origin), 1))
        );
        assert_eq!(stop(AuditKind::SecretDenied, "no-such-reason"), None);
        assert_eq!(
            stop(AuditKind::NavigationDenied, OUT_OF_GRANT),
            Some((StopCause::OutOfGrant, 1))
        );
        assert_eq!(
            stop(AuditKind::DeniedSuppressed, "15"),
            Some((StopCause::Suppressed, 15))
        );
        assert_eq!(
            stop(AuditKind::SubstitutionLocked, "focus_moved"),
            Some((StopCause::SessionLocked, 1))
        );
        assert_eq!(
            stop(AuditKind::HandoffRequested, "awaiting_user"),
            Some((StopCause::AwaitingHandoff, 1))
        );
        for kind in [
            AuditKind::SecretUsed,
            AuditKind::MaskProbe,
            AuditKind::HandoffResumed,
        ] {
            assert_eq!(stop(kind, "ok"), None);
        }
    }

    #[test]
    fn lookbacks_are_minutes_hours_or_days() {
        assert_eq!(Lookback::parse("30m").unwrap().seconds(), 1_800);
        assert_eq!(Lookback::parse("24h").unwrap().seconds(), 86_400);
        assert_eq!(Lookback::parse("7d").unwrap().seconds(), 7 * 86_400);
        for bad in [
            "",
            "h",
            "0h",
            "5",
            "1w",
            "-1h",
            "1.5h",
            "99999999999999999999d",
        ] {
            assert!(Lookback::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn stored_results_keep_their_text() {
        assert_eq!(grant_change::ADDED, "added");
        assert_eq!(grant_change::REVOKED, "revoked");
        assert_eq!(grant_change::CLEARED, "cleared");
        assert_eq!(OUT_OF_GRANT, "out-of-grant");
    }
}
