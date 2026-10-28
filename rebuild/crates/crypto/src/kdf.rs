//! Keys derived from the root key (HKDF-SHA256, salt = vault id). Each use has its own
//! [`KeyPurpose`] and so its own info label (key separation). Domain keys are not derived:
//! they are random and wrapped by the `Kek` key, so the root key can change without
//! re-encrypting fields (07 §7.5).

use crate::SecretKey;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPurpose {
    /// Wraps the domain keys.
    Kek,
    /// Signs item metadata.
    MetaMac,
}

impl KeyPurpose {
    /// The HKDF info label. Changing one makes every existing vault unreadable.
    pub fn label(self) -> &'static [u8] {
        match self {
            Self::Kek => b"keyharbor/kek/v1",
            Self::MetaMac => b"keyharbor/meta-mac/v1",
        }
    }
}

/// The key for `purpose` from the root key and the vault id.
pub fn derive(root: &SecretKey, salt: &[u8], purpose: KeyPurpose) -> SecretKey {
    let hkdf = Hkdf::<Sha256>::new(Some(salt), root.expose());
    let mut out = [0u8; 32];
    hkdf.expand(purpose.label(), &mut out)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    let key = SecretKey::from_bytes(out);
    out.zeroize();
    key
}
