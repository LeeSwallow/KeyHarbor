//! Bearer tokens for local channels that only one process should be able to use (the lookup
//! socket of `keyharbor run`, developer secrets spec §3 channel ④).

use crate::{SecretKey, mac, random};
use zeroize::Zeroizing;

/// Random bytes in a token: 256 bits, so guessing one is hopeless.
const TOKEN_BYTES: usize = 32;

/// A random token, shown as lowercase hex. Checking a presented token takes the same time
/// whatever it holds: it is compared as an HMAC under a key made for this token, through
/// [`mac::verify_mac`].
pub struct AccessToken {
    text: Zeroizing<String>,
    key: SecretKey,
    tag: [u8; 32],
}

impl AccessToken {
    pub fn new() -> Self {
        let mut bytes = Zeroizing::new([0u8; TOKEN_BYTES]);
        random::fill(bytes.as_mut());
        let text: Zeroizing<String> =
            Zeroizing::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        let key = random::key();
        let tag = mac::mac(&key, text.as_bytes());
        Self { text, key, tag }
    }

    /// The token, to hand to the one process that may use it.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether `given` is this token, in constant time.
    pub fn matches(&self, given: &str) -> bool {
        mac::verify_mac(&self.key, given.as_bytes(), &self.tag)
    }
}

impl Default for AccessToken {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessToken(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_matches_only_itself() {
        let (a, b) = (AccessToken::new(), AccessToken::new());
        assert_eq!(a.as_str().len(), 2 * TOKEN_BYTES);
        assert!(a.as_str().bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a.as_str(), b.as_str());
        assert!(a.matches(a.as_str()));
        assert!(!a.matches(b.as_str()));
        assert!(!a.matches(""));
        assert!(!a.matches(&a.as_str()[1..]));
        assert!(!format!("{a:?}").contains(a.as_str()));
    }
}
