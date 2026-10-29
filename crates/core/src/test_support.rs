//! Test helpers: a vault handle whose `resolve` issues the plaintext capability itself, so
//! tests read like the use cases they check.

use crate::{PlaintextAccess, Vault};
use anyhow::Result;
use keyharbor_domain::vault::PlaceholderRef;
use zeroize::Zeroizing;

pub struct TestVault(pub Vault);

impl TestVault {
    pub fn resolve(&self, reference: &PlaceholderRef) -> Result<Zeroizing<String>> {
        self.0.resolve(&PlaintextAccess::issue(), reference)
    }
    pub fn mask_values(&self) -> Result<crate::MaskValues> {
        self.0.mask_values(&PlaintextAccess::issue())
    }
}

impl std::ops::Deref for TestVault {
    type Target = Vault;
    fn deref(&self) -> &Vault {
        &self.0
    }
}

impl std::ops::DerefMut for TestVault {
    fn deref_mut(&mut self) -> &mut Vault {
        &mut self.0
    }
}
