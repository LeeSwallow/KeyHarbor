use keyharbor_crypto::kdf::{KeyPurpose, derive};
use uuid::Uuid;

/// A 256-bit symmetric key, defined with the key-store port in `keyharbor-domain`.
pub use keyharbor_domain::secret::SecretKey;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub struct $name([u8; 16]);
        impl $name {
            pub fn new() -> Self {
                Self(keyharbor_crypto::random::uuid_v4())
            }
            pub fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(bytes)
            }
            pub fn from_slice(bytes: &[u8]) -> Option<Self> {
                bytes.try_into().ok().map(Self)
            }
            pub fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
            pub fn to_hex(&self) -> String {
                self.0.iter().map(|b| format!("{b:02x}")).collect()
            }
            pub fn to_uuid_string(&self) -> String {
                Uuid::from_bytes(self.0).to_string()
            }
            pub fn parse(value: &str) -> Option<Self> {
                Uuid::parse_str(value).ok().map(|id| Self(*id.as_bytes()))
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}
id_type!(KeyId);
id_type!(VaultId);

/// Keys derived from the root key: one for wrapping domain keys, one for metadata MACs.
pub struct KeySet {
    kek: SecretKey,
    mac: SecretKey,
}
impl KeySet {
    pub fn derive(root: &SecretKey, vault: &VaultId) -> Self {
        Self {
            kek: derive(root, vault.as_bytes(), KeyPurpose::Kek),
            mac: derive(root, vault.as_bytes(), KeyPurpose::MetaMac),
        }
    }
    pub fn kek(&self) -> &SecretKey {
        &self.kek
    }
    pub fn mac(&self) -> &SecretKey {
        &self.mac
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyset_derivation_separates_keys_and_depends_on_vault() {
        let root = SecretKey::from_bytes([9; 32]);
        let a = KeySet::derive(&root, &VaultId::from_bytes([1; 16]));
        let b = KeySet::derive(&root, &VaultId::from_bytes([2; 16]));
        assert_ne!(a.kek().expose(), a.mac().expose());
        assert_ne!(a.kek().expose(), b.kek().expose());
        assert_eq!(format!("{:?}", root), "SecretKey([REDACTED])");
    }
    #[test]
    fn generated_ids_and_keys_are_distinct() {
        assert_ne!(KeyId::new(), KeyId::new());
        assert_ne!(
            keyharbor_crypto::random::key().expose(),
            keyharbor_crypto::random::key().expose()
        );
        let v = VaultId::from_bytes([0xab; 16]);
        assert_eq!(VaultId::parse(&v.to_uuid_string()).unwrap(), v);
    }
}
