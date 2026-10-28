//! Usage records (02 §2.7). An event names who did what to which item where — never a value.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AuditKind {
    SecretUsed,
    SecretDenied,
    OriginChangedAfterFill,
    SessionOpened,
    SessionStopped,
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
}
impl AuditKind {
    pub const ALL: [AuditKind; 13] = [
        Self::SecretUsed,
        Self::SecretDenied,
        Self::OriginChangedAfterFill,
        Self::SessionOpened,
        Self::SessionStopped,
        Self::HandoffRequested,
        Self::HandoffResumed,
        Self::PolicyChanged,
        Self::VaultItemChanged,
        Self::EnvInjected,
        Self::SubstitutionLocked,
        Self::MaskProbe,
        Self::DeniedSuppressed,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SecretUsed => "secret_used",
            Self::SecretDenied => "secret_denied",
            Self::OriginChangedAfterFill => "origin_changed_after_fill",
            Self::SessionOpened => "session_opened",
            Self::SessionStopped => "session_stopped",
            Self::HandoffRequested => "handoff_requested",
            Self::HandoffResumed => "handoff_resumed",
            Self::PolicyChanged => "policy_changed",
            Self::VaultItemChanged => "vault_item_changed",
            Self::EnvInjected => "env_injected",
            Self::SubstitutionLocked => "substitution_locked",
            Self::MaskProbe => "mask_probe",
            Self::DeniedSuppressed => "secret_denied_suppressed",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == value)
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
}

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
}
