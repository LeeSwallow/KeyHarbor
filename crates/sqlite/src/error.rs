//! Translating what Diesel, SQLite and the file system report into the ports' `StoreError`
//! (#200, J-260).
//!
//! Inside the adapter, queries and transactions use `anyhow` (Diesel's transaction wants an
//! error built from its own). A repository that means a typed outcome returns the
//! `StoreError` variant itself; everything else reaches the port as `Backend`, shown and
//! chained exactly as before.

use diesel::result::{DatabaseErrorKind, Error as DieselError};
use keyharbor_domain::ports::StoreError;

/// The port error for an adapter error: the `StoreError` a repository raised, or `Backend`.
pub(crate) fn store_error(error: anyhow::Error) -> StoreError {
    match error.downcast::<StoreError>() {
        Ok(typed) => typed,
        Err(other) => StoreError::backend(other),
    }
}

/// `already` when Diesel reports a unique or primary-key clash; any other error as it is.
pub(crate) fn unique_clash(error: DieselError, already: &'static str) -> anyhow::Error {
    match error {
        DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => {
            StoreError::AlreadyExists(already).into()
        }
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raised_store_error_keeps_its_variant() {
        let raised = anyhow::Error::from(StoreError::NotInitialized);
        assert!(matches!(store_error(raised), StoreError::NotInitialized));
    }

    #[test]
    fn other_errors_become_backend_with_the_same_words_and_chain() {
        let error = anyhow::anyhow!("disk full").context("cannot take the vault rotation lock");
        let translated = store_error(error);
        assert!(matches!(translated, StoreError::Backend(_)));
        assert_eq!(
            translated.to_string(),
            "cannot take the vault rotation lock"
        );
        assert_eq!(
            format!("{:#}", anyhow::Error::from(translated)),
            "cannot take the vault rotation lock: disk full"
        );
    }
}
