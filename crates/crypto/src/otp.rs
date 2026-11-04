//! Time-based one-time codes (RFC 6238) for a login's `totp_seed` (#206, docs/design/07 §7.4.4).
//! The HMAC, the dynamic truncation and the base32 decoding are totp-rs's; this module only
//! reads the seed's text (base32, or an `otpauth://totp/` URI with its `algorithm`, `digits`
//! and `period`) and asks for the code at a given time. The clock is the caller's.

use std::fmt::{self, Write};
use totp_rs::{Algorithm, Builder, Secret, Totp};
use zeroize::Zeroizing;

/// RFC 6238's defaults, used when a URI leaves them out (and always for a bare base32 seed).
const DEFAULT_DIGITS: u8 = 6;
const DEFAULT_PERIOD: u64 = 30;
/// RFC 4226 §5.3 allows 6 to 8 digits; totp-rs panics at 10 or more.
const DIGITS: std::ops::RangeInclusive<u8> = 6..=8;
/// A period past an hour is no authenticator's; refusing it catches a mistyped URI.
const PERIODS: std::ops::RangeInclusive<u64> = 1..=3600;
/// The shortest seed accepted, in bytes: 80 bits, the 16 base32 characters older services
/// (Google among them) still hand out. RFC 4226 asks for 128; refusing 80 would refuse those
/// accounts, and anything shorter is a typo, not a seed (J-291).
const MIN_SEED_BYTES: usize = 10;

const URI_START: &str = "otpauth://";

/// Text that is not a one-time code seed. It never repeats the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSeed(&'static str);
impl fmt::Display for InvalidSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not a one-time code seed ({}); give the base32 seed or the otpauth://totp/ URI",
            self.0
        )
    }
}
impl std::error::Error for InvalidSeed {}

/// A seed ready to make codes. Its secret is wiped when it is dropped (totp-rs `zeroize`).
pub struct OtpSeed {
    totp: Totp,
}

impl fmt::Debug for OtpSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OtpSeed(..)")
    }
}

impl OtpSeed {
    /// Reads a base32 seed (spaces, dashes, padding and case ignored) or an
    /// `otpauth://totp/<label>?secret=…[&algorithm=SHA1|SHA256|SHA512][&digits=6..8][&period=N]`
    /// URI. Anything else is refused without echoing it.
    pub fn parse(text: &str) -> Result<Self, InvalidSeed> {
        let text = text.trim();
        let prefix = text.get(..URI_START.len());
        if prefix.is_some_and(|p| p.eq_ignore_ascii_case(URI_START)) {
            Self::from_uri(&text[URI_START.len()..])
        } else {
            Self::build(text, Algorithm::SHA1, DEFAULT_DIGITS, DEFAULT_PERIOD)
        }
    }

    /// `rest` is the URI after `otpauth://`.
    fn from_uri(rest: &str) -> Result<Self, InvalidSeed> {
        let (path, query) = rest.split_once('?').ok_or(InvalidSeed("no secret"))?;
        let kind = path.split('/').next().unwrap_or_default();
        if !kind.eq_ignore_ascii_case("totp") {
            return Err(InvalidSeed("only time-based (totp) codes"));
        }
        let (mut secret, mut algorithm, mut digits, mut period) =
            (None, Algorithm::SHA1, DEFAULT_DIGITS, DEFAULT_PERIOD);
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = percent_decoded(value)?;
            match key.to_ascii_lowercase().as_str() {
                "secret" => secret = Some(value),
                "algorithm" => {
                    algorithm = value
                        .parse()
                        .map_err(|_| InvalidSeed("algorithm is SHA1, SHA256 or SHA512"))?;
                }
                "digits" => {
                    digits = value
                        .parse()
                        .map_err(|_| InvalidSeed("digits is 6, 7 or 8"))?;
                }
                "period" => {
                    period = value
                        .parse()
                        .map_err(|_| InvalidSeed("period is whole seconds"))?;
                }
                _ => {}
            }
        }
        let secret = secret.ok_or(InvalidSeed("no secret"))?;
        Self::build(&secret, algorithm, digits, period)
    }

    fn build(
        base32: &str,
        algorithm: Algorithm,
        digits: u8,
        period: u64,
    ) -> Result<Self, InvalidSeed> {
        if !DIGITS.contains(&digits) {
            return Err(InvalidSeed("digits is 6, 7 or 8"));
        }
        if !PERIODS.contains(&period) {
            return Err(InvalidSeed("period is 1 to 3600 seconds"));
        }
        let normalized: Zeroizing<String> = Zeroizing::new(
            base32
                .chars()
                .filter(|c| !matches!(c, ' ' | '-' | '='))
                .map(|c| c.to_ascii_uppercase())
                .collect(),
        );
        let secret =
            Secret::try_from_base32(&*normalized).map_err(|_| InvalidSeed("not base32"))?;
        if secret.as_bytes().len() < MIN_SEED_BYTES {
            return Err(InvalidSeed("too short"));
        }
        // `build` would also refuse seeds under 128 bits; the checks above are ours (J-291).
        let totp = Builder::new()
            .with_algorithm(algorithm)
            .with_digits(digits)
            .with_step_duration(period)
            .with_skew(0)
            .with_secret(secret)
            .build_noncompliant();
        Ok(Self { totp })
    }

    /// Seconds one code lasts.
    pub fn period(&self) -> u64 {
        self.totp.step()
    }

    /// The code for the step that holds `unix_seconds`, zero-padded to the seed's digits.
    pub fn code_at(&self, unix_seconds: u64) -> Zeroizing<String> {
        let token = self.totp.generate(unix_seconds);
        let mut out = Zeroizing::new(String::with_capacity(usize::from(self.totp.digits())));
        write!(out, "{token}").expect("writing to a String does not fail");
        out
    }
}

/// `%XX` escapes decoded; the result must be UTF-8. Kept in a wiped buffer: it may be the
/// secret.
fn percent_decoded(value: &str) -> Result<Zeroizing<String>, InvalidSeed> {
    const BAD: InvalidSeed = InvalidSeed("bad %-escape");
    let bytes = value.as_bytes();
    let mut out = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = value.get(i + 1..i + 3).ok_or(BAD)?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| BAD)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let text = std::str::from_utf8(&out).map_err(|_| BAD)?;
    Ok(Zeroizing::new(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn a_base32_seed_reads_however_it_is_written() {
        let plain = OtpSeed::parse(SEED).unwrap();
        for written in [
            "gezd gnbv gy3t qojq gezd gnbv gy3t qojq",
            "GEZD-GNBV-GY3T-QOJQ-GEZD-GNBV-GY3T-QOJQ",
            "  GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ====\n",
        ] {
            let seed = OtpSeed::parse(written).unwrap();
            assert_eq!(*seed.code_at(59), *plain.code_at(59), "{written:?}");
        }
        assert_eq!(plain.period(), 30);
        assert_eq!(plain.code_at(59).len(), 6);
    }

    #[test]
    fn a_uri_sets_algorithm_digits_and_period() {
        let uri = format!(
            "otpauth://totp/Example:me%40x.com?secret={SEED}&issuer=Example&algorithm=SHA1&digits=8&period=60"
        );
        let seed = OtpSeed::parse(&uri).unwrap();
        assert_eq!(seed.period(), 60);
        assert_eq!(seed.code_at(0).len(), 8);
        let upper = OtpSeed::parse(&format!("OTPAUTH://TOTP/x?SECRET={SEED}")).unwrap();
        assert_eq!(
            *upper.code_at(59),
            *OtpSeed::parse(SEED).unwrap().code_at(59)
        );
    }

    #[test]
    fn what_is_not_a_seed_is_refused_without_echoing_it() {
        for bad in [
            "",
            "not base32 at all!",
            "GEZDGNBV",
            "otpauth://hotp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&counter=1",
            "otpauth://totp/x?issuer=nobody",
            "otpauth://totp/x",
            "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=10",
            "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&period=0",
            "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&algorithm=MD5",
            "otpauth://totp/x?secret=GEZD%ZZ",
        ] {
            let error = OtpSeed::parse(bad).unwrap_err().to_string();
            assert!(error.starts_with("not a one-time code seed"), "{bad:?}");
            if !bad.is_empty() {
                assert!(!error.contains(bad), "{error}");
            }
            assert!(!error.contains("GEZDGNBV"), "{error}");
        }
        assert!(!format!("{:?}", OtpSeed::parse(SEED).unwrap()).contains(SEED));
    }
}
