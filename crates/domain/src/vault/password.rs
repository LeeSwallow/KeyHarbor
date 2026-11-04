//! What a generated password looks like (08 §8.2 ①). The characters are drawn by
//! `keyharbor_crypto::random::password`; this is only the rule.

use crate::vault::InvalidValue;

/// Character classes a password draws from. Symbols leave out quotes, backslash and spaces
/// so a value survives shells and form fields.
pub const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
pub const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const DIGITS: &str = "0123456789";
pub const SYMBOLS: &str = "!#$%&*+-=?@^_~";

pub const DEFAULT_LENGTH: usize = 20;
/// Below this a password is guessable; above, sites start to cut it off.
pub const LENGTHS: std::ops::RangeInclusive<usize> = 8..=128;

/// `{length, symbols, forbid}`: a password of `length` characters with at least one of each
/// class left after removing `forbid` (symbols only if `symbols`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordRule {
    length: usize,
    classes: Vec<Vec<char>>,
}

impl Default for PasswordRule {
    fn default() -> Self {
        Self::new(DEFAULT_LENGTH, true, "").expect("the default rule is valid")
    }
}

impl PasswordRule {
    pub fn new(length: usize, symbols: bool, forbid: &str) -> Result<Self, InvalidValue> {
        if !LENGTHS.contains(&length) {
            return Err(InvalidValue("password length must be 8 to 128"));
        }
        let mut sets = vec![LOWER, UPPER, DIGITS];
        if symbols {
            sets.push(SYMBOLS);
        }
        let classes: Vec<Vec<char>> = sets
            .into_iter()
            .map(|set| set.chars().filter(|c| !forbid.contains(*c)).collect())
            .filter(|class: &Vec<char>| !class.is_empty())
            .collect();
        if classes.iter().map(Vec::len).sum::<usize>() < 10 {
            return Err(InvalidValue("forbid leaves fewer than 10 characters"));
        }
        Ok(Self { length, classes })
    }
    pub fn length(&self) -> usize {
        self.length
    }
    /// Every allowed character.
    pub fn alphabet(&self) -> Vec<char> {
        self.classes.concat()
    }
    /// Whether `password` has one character of each class (the generator retries until so).
    pub fn is_met_by(&self, password: &str) -> bool {
        password.chars().count() == self.length
            && self
                .classes
                .iter()
                .all(|class| password.chars().any(|c| class.contains(&c)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_outside_8_to_128_are_refused() {
        assert!(PasswordRule::new(7, true, "").is_err());
        assert!(PasswordRule::new(129, true, "").is_err());
        assert_eq!(PasswordRule::default().length(), 20);
    }

    #[test]
    fn forbidden_characters_and_symbols_are_left_out() {
        let rule = PasswordRule::new(12, false, "0Oo1lI").unwrap();
        let alphabet = rule.alphabet();
        assert!(!alphabet.iter().any(|c| "0Oo1lI".contains(*c)));
        assert!(!alphabet.iter().any(|c| SYMBOLS.contains(*c)));
        assert!(PasswordRule::new(12, false, &format!("{LOWER}{UPPER}{DIGITS}")).is_err());
    }

    #[test]
    fn a_password_needs_each_class() {
        let rule = PasswordRule::new(8, false, "").unwrap();
        assert!(rule.is_met_by("aB3aB3aB"));
        assert!(!rule.is_met_by("abcdefgh"));
        assert!(!rule.is_met_by("aB3"));
    }
}
