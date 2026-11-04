//! Names of items and fields.

use super::InvalidValue;

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ItemName(String);

/// Joins an item's name and a draft number: `github~draft-2` (08 §8.3). `~` is not allowed in
/// a chosen name, so only a draft can carry it.
pub const DRAFT_MARK: &str = "~draft-";

impl ItemName {
    number_text!(
        /// The longest chosen name, in characters.
        pub MAX_CHARS: usize = 64,
        RULE = "item name must be 1–", " letters, digits, '-' or '_', in segments joined by '/'"
    );

    /// A stored or referenced name: a chosen name, or a draft of one.
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        match value.split_once(DRAFT_MARK) {
            Some((base, number)) => {
                Self::parse_new(base)?;
                match number.parse::<u32>() {
                    Ok(n) if n > 0 && number == n.to_string() => Ok(Self(value.to_owned())),
                    _ => Err(InvalidValue("draft number must be a positive integer")),
                }
            }
            None => Self::parse_new(value),
        }
    }
    /// A name a person or an agent chooses for a new item: never a draft's name.
    /// A name may be a path, `myapp/prod/db`, to group items for listing (`--prefix`); the
    /// store gives the segments no other meaning (developer secrets spec §2).
    pub fn parse_new(value: &str) -> Result<Self, InvalidValue> {
        let count = value.chars().count();
        let segment_ok = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        };
        if count == 0 || count > Self::MAX_CHARS || !value.split('/').all(segment_ok) {
            return Err(InvalidValue(Self::RULE));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// The chosen name this draft belongs to (itself if it is not a draft).
    pub fn base(&self) -> ItemName {
        match self.0.split_once(DRAFT_MARK) {
            Some((base, _)) => Self(base.to_owned()),
            None => self.clone(),
        }
    }
    pub fn is_draft(&self) -> bool {
        self.0.contains(DRAFT_MARK)
    }
    /// Draft number `n` of this name's base.
    pub fn draft(&self, n: u32) -> ItemName {
        Self(format!("{}{DRAFT_MARK}{n}", self.base().0))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct FieldName(String);
impl FieldName {
    number_text!(
        /// The longest field name, in bytes (always ASCII).
        pub MAX_BYTES: usize = 64,
        RULE = "field name must be 1–", " ASCII letters, digits or '_'"
    );

    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        if value.is_empty()
            || value.len() > Self::MAX_BYTES
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(InvalidValue(Self::RULE));
        }
        Ok(Self(value.to_owned()))
    }
    /// Environment variable names: uppercase letters, digits, '_', not starting with a digit.
    pub fn parse_env(value: &str) -> Result<Self, InvalidValue> {
        let name = Self::parse(value)?;
        let valid = value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && !value.as_bytes()[0].is_ascii_digit();
        if !valid {
            return Err(InvalidValue(
                "environment variable names use A–Z, 0–9 and '_' and must not start with a digit",
            ));
        }
        Ok(name)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::vault::*;
    #[test]
    fn item_names_reject_placeholder_syntax() {
        let long = "a".repeat(65);
        for bad in [
            "",
            " ",
            "my.github",
            "a}b",
            "a{b",
            "a b",
            "x\n",
            long.as_str(),
        ] {
            assert!(ItemName::parse(bad).is_err(), "{bad:?}");
        }
        let max = "a".repeat(64);
        for good in ["github", "my-visa", "회사_메일", max.as_str()] {
            assert!(ItemName::parse(good).is_ok(), "{good:?}");
        }
    }
    #[test]
    fn names_may_be_paths_of_plain_segments() {
        for good in ["myapp/prod/db", "a/b", "회사/메일"] {
            assert!(ItemName::parse(good).is_ok(), "{good:?}");
        }
        for bad in ["/a", "a/", "a//b", "/", "a/b c"] {
            assert!(ItemName::parse(bad).is_err(), "{bad:?}");
        }
        let draft = ItemName::parse("myapp/db").unwrap().draft(1);
        assert_eq!(ItemName::parse(draft.as_str()), Ok(draft.clone()));
        let refs = parse_placeholders("{{secret:myapp/prod/db.password}}").unwrap();
        assert_eq!(refs[0].item.as_str(), "myapp/prod/db");
    }

    #[test]
    fn drafts_are_named_after_their_item_and_cannot_be_chosen() {
        let github = ItemName::parse("github").unwrap();
        let draft = github.draft(2);
        assert_eq!(draft.as_str(), "github~draft-2");
        assert_eq!(ItemName::parse("github~draft-2"), Ok(draft.clone()));
        assert!(draft.is_draft() && !github.is_draft());
        assert_eq!(draft.base(), github);
        assert_eq!(draft.draft(3).as_str(), "github~draft-3");
        assert!(ItemName::parse_new("github~draft-2").is_err());
        for bad in [
            "github~draft-0",
            "github~draft-",
            "github~draft-02",
            "~draft-1",
            "a~draft-1~draft-2",
            "github~draft-x",
        ] {
            assert!(ItemName::parse(bad).is_err(), "{bad:?}");
        }
        let refs = parse_placeholders("{{secret:github~draft-2.password}}").unwrap();
        assert_eq!(refs[0].item, draft);
    }
}
