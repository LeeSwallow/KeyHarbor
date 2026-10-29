//! Tags on items (developer secrets spec §2): short labels to find items by, like Secrets
//! Manager's. Not secret: stored in plain text, signed in the metadata MAC, shown to agents.

use crate::vault::InvalidValue;

/// At most this many tags per item.
pub const MAX_TAGS: usize = 16;

/// `env:prod`, `team-a`: 1–32 of lowercase ASCII letters, digits, `-`, `_`, `:` and `.`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Tag(String);

impl Tag {
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let ok = (1..=32).contains(&value.len())
            && value.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'-' | b'_' | b':' | b'.')
            });
        if !ok {
            return Err(InvalidValue(
                "a tag is 1–32 lowercase letters, digits, '-', '_', ':' or '.'",
            ));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Sorts and dedups `tags`, refusing more than [`MAX_TAGS`].
pub fn tag_set(mut tags: Vec<Tag>) -> Result<Vec<Tag>, InvalidValue> {
    tags.sort();
    tags.dedup();
    if tags.len() > MAX_TAGS {
        return Err(InvalidValue("an item has at most 16 tags"));
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_short_lowercase_labels() {
        for good in ["prod", "env:prod", "team-a", "v1.2", "a_b"] {
            assert!(Tag::parse(good).is_ok(), "{good}");
        }
        for bad in ["", "Prod", "a b", "a,b", "한글", &"x".repeat(33)] {
            assert!(Tag::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_tag_set_is_sorted_unique_and_bounded() {
        let t = |s: &str| Tag::parse(s).unwrap();
        assert_eq!(
            tag_set(vec![t("b"), t("a"), t("b")]).unwrap(),
            [t("a"), t("b")]
        );
        let many = (0..17).map(|i| t(&format!("t{i}"))).collect();
        assert!(tag_set(many).is_err());
    }
}
