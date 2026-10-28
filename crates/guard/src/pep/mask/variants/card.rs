//! Card numbers as payment pages show them: groups of four separated by spaces or hyphens,
//! and the bare digits. Masked layouts such as `424242******4242` are left alone.

use super::Variant;
use zeroize::Zeroizing;

pub struct Groups;

/// The digits of a card-number-like value (12–19 digits, optionally spaced or hyphenated).
fn card_digits(value: &str) -> Option<Zeroizing<String>> {
    if !value
        .chars()
        .all(|c| c.is_ascii_digit() || c == ' ' || c == '-')
    {
        return None;
    }
    let digits = Zeroizing::new(
        value
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>(),
    );
    (12..=19).contains(&digits.len()).then_some(digits)
}

fn grouped(digits: &str, separator: &str) -> Zeroizing<String> {
    Zeroizing::new(
        digits
            .as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).expect("ASCII digits"))
            .collect::<Vec<_>>()
            .join(separator),
    )
}

impl Variant for Groups {
    fn expand(&self, value: &str) -> Vec<Zeroizing<String>> {
        let Some(digits) = card_digits(value) else {
            return Vec::new();
        };
        vec![grouped(&digits, " "), grouped(&digits, "-"), digits]
    }
}
