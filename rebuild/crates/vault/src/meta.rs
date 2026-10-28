//! Item metadata MAC (docs/design/07-cryptography-design.md §7.6.3): detects edits to
//! origin lists, binding flags and other plaintext metadata by anyone without the root key.

use crate::{VaultError, keys::SecretKey, keys::VaultId, record::push_lp};
use hmac::{Hmac, KeyInit, Mac};
use keyharbor_domain::vault::{Domain, ItemKind, ItemName, Origin};
use sha2::Sha256;

const META_TAG: &[u8] = b"keyharbor/meta/v1";

pub struct ItemMeta<'a> {
    pub vault: &'a VaultId,
    pub item: &'a [u8; 16],
    pub domain: Domain,
    pub kind: ItemKind,
    pub name: &'a ItemName,
    pub origin_binding: bool,
    pub origins: &'a [Origin],
    pub status: &'a str,
    pub updated_at: &'a str,
}

fn mac_for(mk: &SecretKey, meta: &ItemMeta) -> Hmac<Sha256> {
    let mut input = Vec::with_capacity(160);
    input.extend_from_slice(META_TAG);
    input.push(0);
    input.extend_from_slice(meta.vault.as_bytes());
    input.extend_from_slice(meta.item);
    input.push(meta.domain.as_byte());
    push_lp(&mut input, meta.kind.as_str().as_bytes());
    push_lp(&mut input, meta.name.as_str().as_bytes());
    input.push(u8::from(meta.origin_binding));
    let mut origins: Vec<&str> = meta.origins.iter().map(Origin::as_str).collect();
    origins.sort_unstable();
    origins.dedup();
    let count = u16::try_from(origins.len()).expect("more than 65535 origins");
    input.extend_from_slice(&count.to_be_bytes());
    for origin in origins {
        push_lp(&mut input, origin.as_bytes());
    }
    push_lp(&mut input, meta.status.as_bytes());
    push_lp(&mut input, meta.updated_at.as_bytes());
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(mk.expose())
        .expect("HMAC accepts any key length");
    mac.update(&input);
    mac
}

pub fn meta_mac(mk: &SecretKey, meta: &ItemMeta) -> [u8; 32] {
    mac_for(mk, meta).finalize().into_bytes().into()
}

pub fn verify_meta_mac(mk: &SecretKey, meta: &ItemMeta, mac: &[u8]) -> Result<(), VaultError> {
    mac_for(mk, meta)
        .verify_slice(mac)
        .map_err(|_| VaultError::Tampered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{SecretKey, VaultId};
    use keyharbor_domain::vault::{Domain, ItemKind, ItemName, Origin};

    struct Fixture {
        vault: VaultId,
        item: [u8; 16],
        name: ItemName,
        origins: Vec<Origin>,
    }
    fn fixture() -> Fixture {
        Fixture {
            vault: VaultId::from_bytes([1; 16]),
            item: [2; 16],
            name: ItemName::parse("github").unwrap(),
            origins: vec![
                Origin::parse("https://github.com").unwrap(),
                Origin::parse("https://gist.github.com").unwrap(),
            ],
        }
    }
    fn meta<'a>(f: &'a Fixture, origins: &'a [Origin]) -> ItemMeta<'a> {
        ItemMeta {
            vault: &f.vault,
            item: &f.item,
            domain: Domain::Credentials,
            kind: ItemKind::Login,
            name: &f.name,
            origin_binding: true,
            origins,
            status: "active",
            updated_at: "2026-10-28T00:00:00Z",
        }
    }

    #[test]
    fn mac_detects_origin_and_binding_changes_but_not_order() {
        let f = fixture();
        let mk = SecretKey::from_bytes([3; 32]);
        let mac = meta_mac(&mk, &meta(&f, &f.origins));
        // 1) same input
        assert!(verify_meta_mac(&mk, &meta(&f, &f.origins), &mac).is_ok());
        // 2) order only
        let reversed: Vec<_> = f.origins.iter().rev().cloned().collect();
        assert!(verify_meta_mac(&mk, &meta(&f, &reversed), &mac).is_ok());
        // 3) extra origin
        let mut extra = f.origins.clone();
        extra.push(Origin::parse("https://evil.example").unwrap());
        assert!(matches!(
            verify_meta_mac(&mk, &meta(&f, &extra), &mac),
            Err(crate::VaultError::Tampered)
        ));
        // 4) binding off
        let mut m = meta(&f, &f.origins);
        m.origin_binding = false;
        assert!(verify_meta_mac(&mk, &m, &mac).is_err());
        // 5) name, kind, status, updated_at
        let other_name = ItemName::parse("gitlab").unwrap();
        let mut m = meta(&f, &f.origins);
        m.name = &other_name;
        assert!(verify_meta_mac(&mk, &m, &mac).is_err());
        let mut m = meta(&f, &f.origins);
        m.kind = ItemKind::Secret;
        assert!(verify_meta_mac(&mk, &m, &mac).is_err());
        let mut m = meta(&f, &f.origins);
        m.status = "draft";
        assert!(verify_meta_mac(&mk, &m, &mac).is_err());
        let mut m = meta(&f, &f.origins);
        m.updated_at = "2026-10-29T00:00:00Z";
        assert!(verify_meta_mac(&mk, &m, &mac).is_err());
        // 6) different MK
        assert!(
            verify_meta_mac(&SecretKey::from_bytes([4; 32]), &meta(&f, &f.origins), &mac).is_err()
        );
    }
}
