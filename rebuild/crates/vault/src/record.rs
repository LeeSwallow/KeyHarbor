//! The vault's sealed records (docs/design/07-cryptography-design.md §7.6): field values and
//! wrapped domain keys in the `keyharbor-crypto` envelope, bound to what they belong to.

use crate::{
    VaultError,
    keys::{KeyId, SecretKey, VaultId},
};
use keyharbor_crypto::envelope;
use keyharbor_domain::vault::Domain;
use zeroize::Zeroizing;

pub use keyharbor_crypto::envelope::VERSION;
pub const SUITE_XCHACHA20_POLY1305: u8 = 0x01;
const FIELD_TAG: &[u8] = b"keyharbor/field/v1";
const DOMAIN_KEY_TAG: &[u8] = b"keyharbor/dk/v1";

/// Everything a field ciphertext is bound to.
pub struct FieldAd<'a> {
    pub vault: &'a VaultId,
    pub domain: Domain,
    pub item: &'a [u8; 16],
    pub field: &'a str,
}

pub(crate) fn push_lp(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u16::try_from(bytes.len()).expect("length-prefixed value exceeds 65535 bytes");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
}

fn field_ad(ad: &FieldAd, header: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(FIELD_TAG);
    out.push(0);
    out.extend_from_slice(ad.vault.as_bytes());
    out.push(ad.domain.as_byte());
    out.extend_from_slice(ad.item);
    push_lp(&mut out, ad.field.as_bytes());
    out.extend_from_slice(header);
    out
}

fn domain_key_ad(vault: &VaultId, domain: Domain, dk_id: &KeyId, header: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(72);
    out.extend_from_slice(DOMAIN_KEY_TAG);
    out.push(0);
    out.extend_from_slice(vault.as_bytes());
    out.push(domain.as_byte());
    out.extend_from_slice(dk_id.as_bytes());
    out.extend_from_slice(header);
    out
}

pub fn seal_field(dk: &SecretKey, dk_id: &KeyId, ad: &FieldAd, plaintext: &[u8]) -> Vec<u8> {
    envelope::seal(dk, dk_id.as_bytes(), |h| field_ad(ad, h), plaintext)
}

#[doc(hidden)]
pub fn seal_field_with_nonce(
    dk: &SecretKey,
    dk_id: &KeyId,
    ad: &FieldAd,
    plaintext: &[u8],
    nonce: [u8; 24],
) -> Vec<u8> {
    envelope::seal_with_nonce(dk, dk_id.as_bytes(), |h| field_ad(ad, h), plaintext, &nonce)
}

pub fn open_field(
    dk: &SecretKey,
    ad: &FieldAd,
    record: &[u8],
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    envelope::open(dk, record, |h| field_ad(ad, h))
}

/// Reads the key id from a record header without decrypting.
pub fn record_key_id(record: &[u8]) -> Result<KeyId, VaultError> {
    envelope::key_id(record).map(KeyId::from_bytes)
}

pub fn wrap_domain_key(
    kek: &SecretKey,
    kek_id: &KeyId,
    vault: &VaultId,
    domain: Domain,
    dk_id: &KeyId,
    dk: &SecretKey,
) -> Vec<u8> {
    envelope::seal(
        kek,
        kek_id.as_bytes(),
        |h| domain_key_ad(vault, domain, dk_id, h),
        dk.expose(),
    )
}

pub fn unwrap_domain_key(
    kek: &SecretKey,
    vault: &VaultId,
    domain: Domain,
    dk_id: &KeyId,
    record: &[u8],
) -> Result<SecretKey, VaultError> {
    let bytes = envelope::open(kek, record, |h| domain_key_ad(vault, domain, dk_id, h))?;
    let array: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::Format("domain key must be 32 bytes"))?;
    Ok(SecretKey::from_bytes(array))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        VaultError,
        keys::{KeyId, VaultId},
    };
    use keyharbor_domain::vault::Domain;

    fn ad<'a>(v: &'a VaultId, item: &'a [u8; 16], field: &'a str, domain: Domain) -> FieldAd<'a> {
        FieldAd {
            vault: v,
            domain,
            item,
            field,
        }
    }
    #[test]
    fn field_roundtrip() {
        let (v, dk, id) = (
            VaultId::new(),
            keyharbor_crypto::random::key(),
            KeyId::new(),
        );
        let item = [7u8; 16];
        let rec = seal_field(
            &dk,
            &id,
            &ad(&v, &item, "password", Domain::Credentials),
            b"hunter2",
        );
        assert_eq!(rec.len(), 42 + 7 + 16);
        assert_eq!(&rec[..2], &[1, 1]);
        assert_eq!(record_key_id(&rec).unwrap(), id);
        assert_eq!(
            &*open_field(&dk, &ad(&v, &item, "password", Domain::Credentials), &rec).unwrap(),
            b"hunter2"
        );
    }
    #[test]
    fn every_ad_component_is_bound() {
        let (v, dk, id) = (
            VaultId::new(),
            keyharbor_crypto::random::key(),
            KeyId::new(),
        );
        let item = [7u8; 16];
        let rec = seal_field(
            &dk,
            &id,
            &ad(&v, &item, "password", Domain::Credentials),
            b"x",
        );
        let other_v = VaultId::new();
        let other_item = [8u8; 16];
        for wrong in [
            ad(&other_v, &item, "password", Domain::Credentials),
            ad(&v, &other_item, "password", Domain::Credentials),
            ad(&v, &item, "username", Domain::Credentials),
            ad(&v, &item, "password", Domain::Payment),
        ] {
            assert!(matches!(
                open_field(&dk, &wrong, &rec),
                Err(VaultError::Decrypt)
            ));
        }
        let mut header_flipped = rec.clone();
        header_flipped[5] ^= 1; // key_id byte
        assert!(
            open_field(
                &dk,
                &ad(&v, &item, "password", Domain::Credentials),
                &header_flipped
            )
            .is_err()
        );
        assert!(
            open_field(
                &keyharbor_crypto::random::key(),
                &ad(&v, &item, "password", Domain::Credentials),
                &rec
            )
            .is_err()
        );
    }
    #[test]
    fn rejects_unknown_version_suite_and_short_records() {
        let (v, dk, id) = (
            VaultId::new(),
            keyharbor_crypto::random::key(),
            KeyId::new(),
        );
        let item = [0u8; 16];
        let a = ad(&v, &item, "f", Domain::Personal);
        let mut rec = seal_field(&dk, &id, &a, b"x");
        rec[0] = 2;
        assert!(matches!(
            open_field(&dk, &a, &rec),
            Err(VaultError::Format(_))
        ));
        assert!(matches!(
            open_field(&dk, &a, &rec[..41]),
            Err(VaultError::Format(_))
        ));
        let mut suite = seal_field(&dk, &id, &a, b"x");
        suite[1] = 9;
        assert!(matches!(
            open_field(&dk, &a, &suite),
            Err(VaultError::Format(_))
        ));
    }
    #[test]
    fn nonces_differ_between_seals() {
        let (v, dk, id) = (
            VaultId::new(),
            keyharbor_crypto::random::key(),
            KeyId::new(),
        );
        let item = [1u8; 16];
        let a = ad(&v, &item, "f", Domain::Personal);
        assert_ne!(
            seal_field(&dk, &id, &a, b"same")[18..42],
            seal_field(&dk, &id, &a, b"same")[18..42]
        );
    }
    #[test]
    fn domain_key_wrap_roundtrip_and_binding() {
        let (v, kek, kek_id, dk, dk_id) = (
            VaultId::new(),
            keyharbor_crypto::random::key(),
            KeyId::new(),
            keyharbor_crypto::random::key(),
            KeyId::new(),
        );
        let w = wrap_domain_key(&kek, &kek_id, &v, Domain::Payment, &dk_id, &dk);
        assert_eq!(record_key_id(&w).unwrap(), kek_id);
        assert_eq!(
            unwrap_domain_key(&kek, &v, Domain::Payment, &dk_id, &w)
                .unwrap()
                .expose(),
            dk.expose()
        );
        assert!(unwrap_domain_key(&kek, &v, Domain::Personal, &dk_id, &w).is_err());
        assert!(unwrap_domain_key(&kek, &v, Domain::Payment, &KeyId::new(), &w).is_err());
        assert!(
            unwrap_domain_key(
                &keyharbor_crypto::random::key(),
                &v,
                Domain::Payment,
                &dk_id,
                &w
            )
            .is_err()
        );
    }
}
