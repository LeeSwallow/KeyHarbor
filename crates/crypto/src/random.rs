//! Randomness from the operating system's generator.

use crate::{SecretKey, aead::Suite};
use keyharbor_domain::password::PasswordRule;
use zeroize::Zeroizing;

/// Fills `out`. Panics if the OS generator fails: nothing safe can be done without it.
pub fn fill(out: &mut [u8]) {
    getrandom::fill(out).expect("OS random number generator failed");
}

/// A new 256-bit key.
pub fn key() -> SecretKey {
    let mut bytes = Zeroizing::new([0u8; 32]);
    fill(bytes.as_mut());
    SecretKey::from_bytes(*bytes)
}

/// A new nonce for `suite`.
pub fn nonce(suite: Suite) -> Vec<u8> {
    let mut nonce = vec![0u8; suite.nonce_len()];
    fill(&mut nonce);
    nonce
}

/// 16 random bytes laid out as a UUID version 4 (RFC 9562): session ids, run generations,
/// request ids and key ids are made from these, here only (ADR-0016).
pub fn uuid_v4() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    fill(&mut bytes);
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .into_bytes()
}

/// A new password following `rule`: characters drawn uniformly from its alphabet, drawn again
/// until every class appears (08 §8.2 ①).
pub fn password(rule: &PasswordRule) -> Zeroizing<String> {
    let alphabet = rule.alphabet();
    loop {
        let password: Zeroizing<String> = Zeroizing::new(
            (0..rule.length())
                .map(|_| alphabet[below(alphabet.len())])
                .collect(),
        );
        if rule.is_met_by(&password) {
            return password;
        }
    }
}

/// A uniform index below `n` (n ≤ 256), rejecting bytes past the last whole multiple of `n`.
fn below(n: usize) -> usize {
    let limit = 256 - 256 % n;
    loop {
        let mut byte = [0u8; 1];
        fill(&mut byte);
        if usize::from(byte[0]) < limit {
            return usize::from(byte[0]) % n;
        }
    }
}

/// A new random id of any type built from UUID v4 bytes, e.g. `random::id::<SessionId>()`.
pub fn id<T: From<[u8; 16]>>() -> T {
    T::from(uuid_v4())
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_are_version_4_and_differ() {
        let (a, b) = (super::uuid_v4(), super::uuid_v4());
        assert_ne!(a, b);
        assert_eq!(a[6] >> 4, 4);
        assert_eq!(a[8] >> 6, 0b10);
    }

    #[test]
    fn passwords_follow_the_rule_and_differ() {
        let rule = keyharbor_domain::password::PasswordRule::new(16, true, "0O").unwrap();
        let (a, b) = (super::password(&rule), super::password(&rule));
        assert_ne!(*a, *b);
        for p in [a, b] {
            assert!(rule.is_met_by(&p), "{}", p.len());
            assert!(!p.contains(['0', 'O']));
        }
    }

    #[test]
    fn indexes_cover_the_whole_range() {
        let mut seen = [false; 7];
        for _ in 0..1000 {
            seen[super::below(7)] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }
}
