//! What a storage port fails with (#200, J-260). Callers decide by variant, never by the text:
//! the text is what a person sees, and it never carries a value.

use std::error::Error;

/// Why a storage port call failed. The adapter translates its technology's errors into this.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The write lost a race (keys rotated, or the vault version moved); nothing was changed.
    /// Open the vault again and retry.
    #[error("{0}")]
    Stale(Staleness),
    /// A record the call needs is gone. The text says which.
    #[error("{0}")]
    NotFound(&'static str),
    /// A record with the same key is already stored. The text says which.
    #[error("{0}")]
    AlreadyExists(&'static str),
    /// There is no vault yet.
    #[error("vault not initialized")]
    NotInitialized,
    /// The record is not in the state the write expects (a session's run or a handoff moved
    /// on). The text says what.
    #[error("{0}")]
    Conflict(&'static str),
    /// Anything else the storage technology reported. Shown and chained as the inner error.
    #[error(transparent)]
    Backend(Box<dyn Error + Send + Sync>),
}

impl StoreError {
    /// Wraps an error the storage technology reported.
    pub fn backend(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self::Backend(error.into())
    }
    /// The write lost a race and may succeed when tried again.
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale(_))
    }
}

/// Which race a [`StoreError::Stale`] write lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Staleness {
    /// The key generation the writer sealed with is no longer current.
    #[error("vault changed (keys were rotated); reopen it and retry")]
    KeysRotated,
    /// The vault version moved since the writer read it (compare-and-swap lost).
    #[error("vault changed during rotation; nothing was changed")]
    VersionMoved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_show_the_words_people_saw_before() {
        let shown = |e: StoreError| e.to_string();
        assert_eq!(
            shown(StoreError::Stale(Staleness::KeysRotated)),
            "vault changed (keys were rotated); reopen it and retry"
        );
        assert_eq!(
            shown(StoreError::Stale(Staleness::VersionMoved)),
            "vault changed during rotation; nothing was changed"
        );
        assert_eq!(shown(StoreError::NotInitialized), "vault not initialized");
        assert_eq!(
            shown(StoreError::AlreadyExists("item already exists")),
            "item already exists"
        );
    }

    #[test]
    fn a_backend_error_is_shown_and_chained_as_the_inner_error() {
        let inner = std::io::Error::other("disk full");
        let error = StoreError::backend(inner);
        assert_eq!(error.to_string(), "disk full");
        assert!(error.source().is_none());
        assert!(!error.is_stale());
    }
}
