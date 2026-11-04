//! Why a session stopped substituting (03 §3.8, C2 and I1), and the words the usage record
//! keeps for it. The guard decides when to lock; only a person lifts the lock (#190, J-220).

/// Why a session stopped substituting. Its text is the `substitution_locked` result the
/// usage record keeps, so it never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockReason {
    /// Requests carrying a vault value reached the guard's limit (I1).
    MaskProbe,
    /// Page code moved the focus while a value was being typed (C2).
    FocusMoved,
}

impl LockReason {
    pub const ALL: [Self; 2] = [Self::MaskProbe, Self::FocusMoved];

    /// The `substitution_locked` result recorded for this reason.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaskProbe => "mask_probe",
            Self::FocusMoved => "focus_moved",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == value)
    }
}

/// The `substitution_locked` result when a person let the session substitute again. Stored,
/// so the text never changes.
pub const UNLOCKED: &str = "unlocked";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_reasons_keep_their_stored_words() {
        let words: Vec<&str> = LockReason::ALL
            .into_iter()
            .map(LockReason::as_str)
            .collect();
        assert_eq!(words, ["mask_probe", "focus_moved"]);
        for reason in LockReason::ALL {
            assert_eq!(LockReason::parse(reason.as_str()), Some(reason));
        }
        assert_eq!(LockReason::parse(UNLOCKED), None);
    }
}
