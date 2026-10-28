use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;

pub const KEK_INFO: &[u8] = b"keyharbor/kek/v1";
pub const META_MAC_INFO: &[u8] = b"keyharbor/meta-mac/v1";

/// A 256-bit symmetric key, defined with the key-store port in `keyharbor-domain`.
pub use keyharbor_domain::secret::SecretKey;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub struct $name([u8; 16]);
        impl $name {
            pub fn new() -> Self {
                Self(*Uuid::new_v4().as_bytes())
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
        let hkdf = Hkdf::<Sha256>::new(Some(vault.as_bytes()), root.expose());
        let expand = |info: &[u8]| {
            let mut out = [0u8; 32];
            hkdf.expand(info, &mut out)
                .expect("32 bytes is a valid HKDF-SHA256 output length");
            let key = SecretKey::from_bytes(out);
            zeroize::Zeroize::zeroize(&mut out);
            key
        };
        Self {
            kek: expand(KEK_INFO),
            mac: expand(META_MAC_INFO),
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
            SecretKey::generate().expose(),
            SecretKey::generate().expose()
        );
        let v = VaultId::from_bytes([0xab; 16]);
        assert_eq!(VaultId::parse(&v.to_uuid_string()).unwrap(), v);
    }
}
