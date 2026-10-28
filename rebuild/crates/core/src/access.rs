//! The capability to read plaintext vault values.

/// Proof that the caller may see vault plaintext. `keyharbor-core` cannot depend on the guard
/// layer, so the constructor is public but hidden; `scripts/check-layers.sh` allows calls to
/// [`PlaintextAccess::issue`] only in `keyharbor-guard` (after its policy, origin and masking
/// checks), in core's export, and in tests.
pub struct PlaintextAccess(());

impl PlaintextAccess {
    #[doc(hidden)]
    pub fn issue() -> Self {
        Self(())
    }
}

impl std::fmt::Debug for PlaintextAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlaintextAccess")
    }
}
