//! Where an item stands — draft, active or discarded — and where it came from (#200).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Draft,
    Active,
    Discarded,
}
impl ItemStatus {
    /// Every status, in the order people are shown them (`vault edit --status`, J-185).
    pub const ALL: [ItemStatus; 3] = [Self::Active, Self::Draft, Self::Discarded];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Discarded => "discarded",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSource {
    User,
    Import,
    Generated,
    Captured,
    Requested,
    Agent,
}
impl ItemSource {
    pub const ALL: [ItemSource; 6] = [
        Self::User,
        Self::Import,
        Self::Generated,
        Self::Captured,
        Self::Requested,
        Self::Agent,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Import => "import",
            Self::Generated => "generated",
            Self::Captured => "captured",
            Self::Requested => "requested",
            Self::Agent => "agent",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_and_source_round_trips_through_its_name() {
        for status in ItemStatus::ALL {
            assert_eq!(ItemStatus::parse(status.as_str()), Some(status));
        }
        for source in ItemSource::ALL {
            assert_eq!(ItemSource::parse(source.as_str()), Some(source));
        }
        assert_eq!(ItemStatus::parse("pending"), None);
    }

    #[test]
    fn statuses_are_listed_active_first() {
        assert_eq!(
            ItemStatus::ALL.map(ItemStatus::as_str),
            ["active", "draft", "discarded"]
        );
    }
}
