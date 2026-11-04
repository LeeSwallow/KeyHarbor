//! What an item, a field or a session is for (R-38).

use super::InvalidValue;

/// What an item, a field or a session is for, so an agent can tell when to use it (R-38).
/// 1–500 characters, no control characters but newlines. Not a secret: it is stored in
/// plaintext (signed in the metadata MAC for items) and shown to agents.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Description(String);

impl Description {
    number_text!(pub MAX_CHARS: usize = 500, RULE = "a description is 1–", " characters");

    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let value = value.trim();
        let count = value.chars().count();
        if count == 0 || count > Self::MAX_CHARS {
            return Err(InvalidValue(Self::RULE));
        }
        if value.chars().any(|c| c.is_control() && c != '\n') {
            return Err(InvalidValue("a description has no control characters"));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::vault::*;
    #[test]
    fn descriptions_are_short_plain_text() {
        assert_eq!(
            Description::parse("  Work GitHub, for CI  ")
                .unwrap()
                .as_str(),
            "Work GitHub, for CI"
        );
        assert!(Description::parse("two\nlines").is_ok());
        assert!(Description::parse(&"가".repeat(500)).is_ok());
        for bad in ["", "   ", "tab\there", "\u{1b}[31m"] {
            assert!(Description::parse(bad).is_err(), "{bad:?}");
        }
        assert!(Description::parse(&"x".repeat(501)).is_err());
        assert!(
            KINDS
                .iter()
                .flat_map(|k| k.fields)
                .all(|f| !f.description.is_empty())
        );
    }
}
