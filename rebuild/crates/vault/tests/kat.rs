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
