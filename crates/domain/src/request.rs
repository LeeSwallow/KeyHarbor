//! An agent asking the user for information only the user has (08 §8.2 ③). The user answers
//! outside the agent's reach (CLI hidden prompt now, the app's secure input window later);
//! the agent learns only the outcome and placeholders.

use crate::{
    RequestId,
    vault::{Description, InvalidValue, ItemKind, ItemName, Origin},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultRequestStatus {
    Pending,
    /// The user stored new values or chose an existing item.
    Fulfilled,
    Declined,
    /// Nobody answered within [`VaultRequest::TTL_SECONDS`].
    Expired,
}

impl VaultRequestStatus {
    pub const ALL: [VaultRequestStatus; 4] = [
        Self::Pending,
        Self::Fulfilled,
        Self::Declined,
        Self::Expired,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Fulfilled => "fulfilled",
            Self::Declined => "declined",
            Self::Expired => "expired",
        }
    }
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or(InvalidValue("invalid request status"))
    }
}

/// One request. Names and the agent's reason only; a value never passes through it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRequest {
    pub id: RequestId,
    pub agent: String,
    pub kind: ItemKind,
    /// The name the agent suggests for a new item.
    pub name: ItemName,
    /// Why the agent asks, shown to the user. Plain text from the agent: not trusted.
    pub reason: Description,
    pub origin: Option<Origin>,
    pub status: VaultRequestStatus,
    /// Unix seconds.
    pub created_at: u64,
    /// The item that answered it, once fulfilled.
    pub item: Option<ItemName>,
}

impl VaultRequest {
    /// A day: long enough for a person to come back to the machine, short enough that an old
    /// prompt is not answered by mistake. Fixed: an agent that still needs it asks again.
    pub const TTL_SECONDS: u64 = 24 * 60 * 60;

    /// The status as of `now` (Unix seconds): a pending request past its TTL is expired.
    pub fn status_at(&self, now: u64) -> VaultRequestStatus {
        match self.status {
            VaultRequestStatus::Pending if now >= self.created_at + Self::TTL_SECONDS => {
                VaultRequestStatus::Expired
            }
            status => status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_round_trip() {
        for status in VaultRequestStatus::ALL {
            assert_eq!(VaultRequestStatus::parse(status.as_str()), Ok(status));
        }
        assert!(VaultRequestStatus::parse("done").is_err());
    }

    #[test]
    fn a_pending_request_expires_after_a_day_and_an_answer_does_not() {
        let mut request = VaultRequest {
            id: RequestId::from_bytes([7; 16]),
            agent: "codex".into(),
            kind: ItemKind::Card,
            name: ItemName::parse("my-visa").unwrap(),
            reason: Description::parse("checkout").unwrap(),
            origin: None,
            status: VaultRequestStatus::Pending,
            created_at: 1_000,
            item: None,
        };
        let day = VaultRequest::TTL_SECONDS;
        assert_eq!(
            request.status_at(1_000 + day - 1),
            VaultRequestStatus::Pending
        );
        assert_eq!(request.status_at(1_000 + day), VaultRequestStatus::Expired);
        request.status = VaultRequestStatus::Declined;
        assert_eq!(request.status_at(1_000 + day), VaultRequestStatus::Declined);
    }
}
