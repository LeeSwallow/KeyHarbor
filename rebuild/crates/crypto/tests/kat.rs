//! docs/design/07-cryptography-design.md §7.5 key derivation KAT (moved here from
//! keyharbor-vault with the derivation; the values are unchanged). Changing them means
//! existing vaults can no longer be read.

use keyharbor_crypto::{
    SecretKey,
    kdf::{KeyPurpose, derive},
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const KEK_HEX: &str = "50e57b267060479352f0371205a95d6dfc95509d4ebc436c14a6cb170f826ffc";
const MAC_HEX: &str = "dbbbe3429d5dcf75b555f8d134c8f28de6666f0790bff98a26f1f54fbbe06479";

#[test]
fn hkdf_outputs_are_stable() {
    let root = SecretKey::from_bytes([0x11; 32]);
    let vault = [0x22; 16];
    assert_eq!(
        hex(derive(&root, &vault, KeyPurpose::Kek).expose()),
        KEK_HEX
    );
    assert_eq!(
        hex(derive(&root, &vault, KeyPurpose::MetaMac).expose()),
        MAC_HEX
    );
}
