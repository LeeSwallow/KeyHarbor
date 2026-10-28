//! Item metadata MAC (docs/design/07-cryptography-design.md §7.6.3): detects edits to
//! origin lists, binding flags and other plaintext metadata by anyone without the root key.
//!
//! v2 (phase 3) adds the run permission, the expiry date and the descriptions. Items signed
//! before v2 still verify under v1 as long as those attributes hold their v1-era values (the
//! kind's default run permission, no expiry, no descriptions); any other value needs v2.

use crate::{VaultError, keys::SecretKey, keys::VaultId, record::push_lp};
use hmac::{Hmac, KeyInit, Mac};
use keyharbor_domain::vault::{Domain, ItemKind, ItemName, Origin};
use sha2::Sha256;

const META_TAG_V1: &[u8] = b"keyharbor/meta/v1";
const META_TAG_V2: &[u8] = b"keyharbor/meta/v2";

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
    /// `keyharbor run` may inject the item's values.
    pub run_allowed: bool,
    /// The last day (UTC, `YYYY-MM-DD`) agents may use the item; `None` never expires.
    pub expires_at: Option<&'a str>,
    pub description: Option<&'a str>,
    /// Per-field descriptions, `(field, text)`; order does not matter.
    pub field_descriptions: &'a [(String, String)],
}

impl ItemMeta<'_> {
    /// The v2 attributes hold the values every item had before v2 existed.
    fn v1_compatible(&self) -> bool {
        self.run_allowed == self.kind.default_run_allowed()
            && self.expires_at.is_none()
            && self.description.is_none()
            && self.field_descriptions.is_empty()
    }
}

fn push_opt(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        None => out.push(0),
        Some(value) => {
            out.push(1);
            push_lp(out, value.as_bytes());
        }
    }
}

fn input(tag: &[u8], meta: &ItemMeta) -> Vec<u8> {
    let mut input = Vec::with_capacity(256);
    input.extend_from_slice(tag);
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
    if tag == META_TAG_V2 {
        input.push(u8::from(meta.run_allowed));
        push_opt(&mut input, meta.expires_at);
        push_opt(&mut input, meta.description);
        let mut fields: Vec<&(String, String)> = meta.field_descriptions.iter().collect();
        fields.sort_unstable();
        fields.dedup();
        let count = u16::try_from(fields.len()).expect("more than 65535 fields");
        input.extend_from_slice(&count.to_be_bytes());
        for (field, text) in fields {
            push_lp(&mut input, field.as_bytes());
            push_lp(&mut input, text.as_bytes());
        }
    }
    input
}

fn mac_for(mk: &SecretKey, tag: &[u8], meta: &ItemMeta) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(mk.expose())
        .expect("HMAC accepts any key length");
    mac.update(&input(tag, meta));
    mac
}

/// The v2 MAC; every write signs with it.
pub fn meta_mac(mk: &SecretKey, meta: &ItemMeta) -> [u8; 32] {
    mac_for(mk, META_TAG_V2, meta)
        .finalize()
        .into_bytes()
        .into()
}

/// Accepts a v2 MAC, or a v1 MAC when the v2 attributes hold their v1-era values.
pub fn verify_meta_mac(mk: &SecretKey, meta: &ItemMeta, mac: &[u8]) -> Result<(), VaultError> {
    if mac_for(mk, META_TAG_V2, meta).verify_slice(mac).is_ok() {
        return Ok(());
    }
    if meta.v1_compatible() && mac_for(mk, META_TAG_V1, meta).verify_slice(mac).is_ok() {
        return Ok(());
    }
    Err(VaultError::Tampered)
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
            run_allowed: false,
            expires_at: None,
            description: None,
            field_descriptions: &[],
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

    #[test]
    fn v2_attributes_are_signed_and_v1_macs_only_cover_v1_values() {
        let f = fixture();
        let mk = SecretKey::from_bytes([3; 32]);
        let base = meta(&f, &f.origins);
        let v1 = mac_for(&mk, META_TAG_V1, &base).finalize().into_bytes();
        assert!(
            verify_meta_mac(&mk, &base, &v1).is_ok(),
            "an item from before v2"
        );
        let described = [("password".to_owned(), "for the web".to_owned())];
        for change in 0..4 {
            let mut m = meta(&f, &f.origins);
            match change {
                0 => m.run_allowed = true,
                1 => m.expires_at = Some("2027-01-31"),
                2 => m.description = Some("work account"),
                _ => m.field_descriptions = &described,
            }
            // A v1 MAC never vouches for a v2 value ...
            assert!(verify_meta_mac(&mk, &m, &v1).is_err());
            // ... and a v2 MAC binds it.
            let v2 = meta_mac(&mk, &m);
            assert!(verify_meta_mac(&mk, &m, &v2).is_ok());
            assert!(verify_meta_mac(&mk, &base, &v2).is_err());
        }
        // Absent and empty differ; field descriptions are bound to their field.
        let mut none = meta(&f, &f.origins);
        none.description = None;
        let mut empty = meta(&f, &f.origins);
        empty.description = Some("");
        assert_ne!(meta_mac(&mk, &none), meta_mac(&mk, &empty));
        let swapped = [("username".to_owned(), "for the web".to_owned())];
        let mut a = meta(&f, &f.origins);
        a.field_descriptions = &described;
        let mut b = meta(&f, &f.origins);
        b.field_descriptions = &swapped;
        assert_ne!(meta_mac(&mk, &a), meta_mac(&mk, &b));
    }
}
