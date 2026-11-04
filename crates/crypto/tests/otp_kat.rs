//! RFC 6238 Appendix B test vectors (docs/design/07 §7.9): the seed of each hash, as an
//! `otpauth://` URI with 8 digits, gives the published code at each time.

use keyharbor_crypto::otp::OtpSeed;

/// "1234567890" repeated to 20, 32 and 64 bytes, in base32.
const SHA1: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
const SHA256: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA====";
const SHA512: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNA=";

/// (time, SHA-1, SHA-256, SHA-512) from RFC 6238 Appendix B.
const VECTORS: [(u64, &str, &str, &str); 6] = [
    (59, "94287082", "46119246", "90693936"),
    (1111111109, "07081804", "68084774", "25091201"),
    (1111111111, "14050471", "67062674", "99943326"),
    (1234567890, "89005924", "91819424", "93441116"),
    (2000000000, "69279037", "90698825", "38618901"),
    (20000000000, "65353130", "77737706", "47863826"),
];

fn seed(secret: &str, algorithm: &str) -> OtpSeed {
    let secret = secret.replace('=', "%3D");
    OtpSeed::parse(&format!(
        "otpauth://totp/RFC6238?secret={secret}&algorithm={algorithm}&digits=8&period=30"
    ))
    .unwrap()
}

#[test]
fn rfc_6238_appendix_b() {
    let (sha1, sha256, sha512) = (
        seed(SHA1, "SHA1"),
        seed(SHA256, "SHA256"),
        seed(SHA512, "SHA512"),
    );
    for (time, one, two, five) in VECTORS {
        assert_eq!(*sha1.code_at(time), one, "SHA-1 at {time}");
        assert_eq!(*sha256.code_at(time), two, "SHA-256 at {time}");
        assert_eq!(*sha512.code_at(time), five, "SHA-512 at {time}");
    }
}

#[test]
fn a_bare_seed_is_sha1_six_digits_thirty_seconds() {
    let bare = OtpSeed::parse(SHA1).unwrap();
    assert_eq!(bare.period(), 30);
    for (time, one, _, _) in VECTORS {
        // Six digits are the last six of the eight (RFC 4226 §5.3: the same number mod 10^6).
        assert_eq!(*bare.code_at(time), one[2..], "at {time}");
    }
}
