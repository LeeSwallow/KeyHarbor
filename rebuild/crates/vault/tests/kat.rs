//! docs/design/07-cryptography-design.md §7.6.1 v1 format KAT.
//! Changing these values means existing vaults can no longer be read.

use keyharbor_domain::vault::Domain;
use keyharbor_vault::{
    keys::{KeyId, KeySet, SecretKey, VaultId},
    record::{FieldAd, open_field, seal_field_with_nonce},
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const KEK_HEX: &str = "50e57b267060479352f0371205a95d6dfc95509d4ebc436c14a6cb170f826ffc";
const DEVELOPER_RECORD_HEX: &str = "010144444444444444444444444444444444666666666666666666666666666666666666666666666666f7159b83f76058b5257856d2797687e022dab9576615";
const MAC_HEX: &str = "dbbbe3429d5dcf75b555f8d134c8f28de6666f0790bff98a26f1f54fbbe06479";
const FIELD_RECORD_HEX: &str = "010144444444444444444444444444444444666666666666666666666666666666666666666666666666f7159b83f760b5caa17a237a12585b8564417968cbf3";

#[test]
fn hkdf_outputs_are_stable() {
    let keys = KeySet::derive(
        &SecretKey::from_bytes([0x11; 32]),
        &VaultId::from_bytes([0x22; 16]),
    );
    assert_eq!(hex(keys.kek().expose()), KEK_HEX);
    assert_eq!(hex(keys.mac().expose()), MAC_HEX);
}

/// The developer domain (R-40, byte 4) seals like the others; only the AD's domain byte differs.
#[test]
fn developer_field_records_are_bound_to_their_domain() {
    let vault = VaultId::from_bytes([0x22; 16]);
    let item = [0x55; 16];
    let dk = SecretKey::from_bytes([0x33; 32]);
    let ad = |domain| FieldAd {
        vault: &vault,
        domain,
        item: &item,
        field: "value",
    };
    let record = seal_field_with_nonce(
        &dk,
        &KeyId::from_bytes([0x44; 16]),
        &ad(Domain::Developer),
        b"KH-KAT",
        [0x66; 24],
    );
    assert_eq!(hex(&record), DEVELOPER_RECORD_HEX);
    assert_eq!(
        &*open_field(&dk, &ad(Domain::Developer), &record).unwrap(),
        b"KH-KAT"
    );
    assert!(open_field(&dk, &ad(Domain::Credentials), &record).is_err());
}

#[test]
fn field_record_bytes_are_stable() {
    let vault = VaultId::from_bytes([0x22; 16]);
    let item = [0x55; 16];
    let ad = FieldAd {
        vault: &vault,
        domain: Domain::Credentials,
        item: &item,
        field: "password",
    };
    let dk = SecretKey::from_bytes([0x33; 32]);
    let record = seal_field_with_nonce(
        &dk,
        &KeyId::from_bytes([0x44; 16]),
        &ad,
        b"KH-KAT",
        [0x66; 24],
    );
    assert_eq!(hex(&record), FIELD_RECORD_HEX);
    assert_eq!(&*open_field(&dk, &ad, &record).unwrap(), b"KH-KAT");
}

const META_V1_HEX: &str = "7263b493640d4a2ae564062c249db35f664507d37b79a8333a6b5b1016372e58";
const META_V2_HEX: &str = "243dba424b4566e6d8dafec19ea3b767379af742d1c86ab7cc7fdebac551ea71";

/// §7.6.3 metadata MAC: v2 bytes are stable, and an item signed with v1 still verifies.
#[test]
fn meta_macs_are_stable() {
    use keyharbor_domain::vault::{ItemKind, ItemName, Origin};
    use keyharbor_vault::meta::{ItemMeta, meta_mac, verify_meta_mac};
    let keys = KeySet::derive(
        &SecretKey::from_bytes([0x11; 32]),
        &VaultId::from_bytes([0x22; 16]),
    );
    let vault = VaultId::from_bytes([0x22; 16]);
    let name = ItemName::parse("github").unwrap();
    let origins = [Origin::parse("https://github.com").unwrap()];
    let mut meta = ItemMeta {
        vault: &vault,
        item: &[0x55; 16],
        domain: Domain::Credentials,
        kind: ItemKind::Login,
        name: &name,
        origin_binding: true,
        origins: &origins,
        status: "active",
        updated_at: "2026-10-28T00:00:00Z",
        run_allowed: false,
        expires_at: None,
        description: None,
        field_descriptions: &[],
    };
    let v1: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&META_V1_HEX[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    assert!(verify_meta_mac(keys.mac(), &meta, &v1).is_ok());
    let described = [("password".to_owned(), "web".to_owned())];
    meta.run_allowed = true;
    meta.expires_at = Some("2027-01-31");
    meta.description = Some("work");
    meta.field_descriptions = &described;
    assert!(verify_meta_mac(keys.mac(), &meta, &v1).is_err());
    let v2 = meta_mac(keys.mac(), &meta);
    assert_eq!(hex(&v2), META_V2_HEX);
}
