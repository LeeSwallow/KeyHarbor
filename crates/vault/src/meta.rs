//! Item metadata MAC (docs/design/07-cryptography-design.md §7.6.3): detects edits to
//! origin lists, binding flags and other plaintext metadata by anyone without the root key.
//!
//! v2 (phase 3) adds the run permission, the expiry date and the descriptions. Items signed
//! before v2 still verify under v1 as long as those attributes hold their v1-era values (the
//! kind's default run permission, no expiry, no descriptions); any other value needs v2. v3
//! adds tags; a v2 (or v1) MAC still verifies an item without tags. v4 adds the fields marked
//! not secret; a v3 (or older) MAC still verifies an item where every field is secret.

use crate::{VaultError, keys::SecretKey, keys::VaultId, record::push_lp};
use keyharbor_crypto::mac::{mac, verify_mac};
use keyharbor_domain::vault::{Domain, ItemKind, ItemName, Origin};

const META_TAG_V1: &[u8] = b"keyharbor/meta/v1";
const META_TAG_V2: &[u8] = b"keyharbor/meta/v2";
const META_TAG_V3: &[u8] = b"keyharbor/meta/v3";
const META_TAG_V4: &[u8] = b"keyharbor/meta/v4";

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
    /// Tags; order and repeats do not matter.
    pub tags: &'a [String],
    /// Fields that are not secret (shown in listings, not masked); order does not matter.
    pub plain_fields: &'a [String],
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
    if tag != META_TAG_V1 {
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
    if tag == META_TAG_V3 || tag == META_TAG_V4 {
        let mut tags: Vec<&str> = meta.tags.iter().map(String::as_str).collect();
        tags.sort_unstable();
        tags.dedup();
        let count = u16::try_from(tags.len()).expect("more than 65535 tags");
        input.extend_from_slice(&count.to_be_bytes());
        for tag in tags {
            push_lp(&mut input, tag.as_bytes());
        }
    }
    if tag == META_TAG_V4 {
        let mut plain: Vec<&str> = meta.plain_fields.iter().map(String::as_str).collect();
        plain.sort_unstable();
        plain.dedup();
        let count = u16::try_from(plain.len()).expect("more than 65535 fields");
        input.extend_from_slice(&count.to_be_bytes());
        for field in plain {
            push_lp(&mut input, field.as_bytes());
        }
    }
    input
}

/// The v4 MAC; every write signs with it.
pub fn meta_mac(mk: &SecretKey, meta: &ItemMeta) -> [u8; 32] {
    mac(mk, &input(META_TAG_V4, meta))
}

/// Accepts a v4 MAC; older MACs only for an item where every field is secret: v3, v2 when
/// it also has no tags, v1 when besides the v2 attributes hold their v1-era values. An older
/// MAC never vouches for a field marked not secret.
pub fn verify_meta_mac(mk: &SecretKey, meta: &ItemMeta, tag: &[u8]) -> Result<(), VaultError> {
    if verify_mac(mk, &input(META_TAG_V4, meta), tag) {
        return Ok(());
    }
    if !meta.plain_fields.is_empty() {
        return Err(VaultError::Tampered);
    }
    let untagged = meta.tags.is_empty();
    let ok = verify_mac(mk, &input(META_TAG_V3, meta), tag)
        || (untagged && verify_mac(mk, &input(META_TAG_V2, meta), tag))
        || (untagged && meta.v1_compatible() && verify_mac(mk, &input(META_TAG_V1, meta), tag));
    if ok {
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
            tags: &[],
            plain_fields: &[],
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
        let v1 = mac(&mk, &input(META_TAG_V1, &base));
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

    #[test]
    fn tags_are_signed_and_older_macs_only_cover_untagged_items() {
        let f = fixture();
        let mk = SecretKey::from_bytes([3; 32]);
        let base = meta(&f, &f.origins);
        let v2 = mac(&mk, &input(META_TAG_V2, &base));
        assert!(
            verify_meta_mac(&mk, &base, &v2).is_ok(),
            "an item from before v3"
        );
        let tags = ["prod".to_owned(), "team-a".to_owned()];
        let mut tagged = meta(&f, &f.origins);
        tagged.tags = &tags;
        assert!(
            verify_meta_mac(&mk, &tagged, &v2).is_err(),
            "v2 never vouches for tags"
        );
        let v3 = meta_mac(&mk, &tagged);
        assert!(verify_meta_mac(&mk, &tagged, &v3).is_ok());
        assert!(verify_meta_mac(&mk, &base, &v3).is_err());
        let reordered = ["team-a".to_owned(), "prod".to_owned(), "prod".to_owned()];
        let mut same = meta(&f, &f.origins);
        same.tags = &reordered;
        assert!(verify_meta_mac(&mk, &same, &v3).is_ok());
    }

    #[test]
    fn plain_fields_are_signed_and_no_older_mac_vouches_for_them() {
        let f = fixture();
        let mk = SecretKey::from_bytes([3; 32]);
        let base = meta(&f, &f.origins);
        let v3 = mac(&mk, &input(META_TAG_V3, &base));
        assert!(
            verify_meta_mac(&mk, &base, &v3).is_ok(),
            "an item from before v4"
        );
        let plain = ["username".to_owned()];
        let mut marked = meta(&f, &f.origins);
        marked.plain_fields = &plain;
        assert!(
            verify_meta_mac(&mk, &marked, &v3).is_err(),
            "marking a field plain in the DB"
        );
        let v4 = meta_mac(&mk, &marked);
        assert!(verify_meta_mac(&mk, &marked, &v4).is_ok());
        assert!(verify_meta_mac(&mk, &base, &v4).is_err());
        let other = ["password".to_owned()];
        let mut swapped = meta(&f, &f.origins);
        swapped.plain_fields = &other;
        assert!(verify_meta_mac(&mk, &swapped, &v4).is_err());
    }
}
