//! Which items a running program may ask for by name (`keyharbor run --allow`, developer
//! secrets spec §3 channel ④): an exact name, or a name prefix ending in `*`.

use crate::vault::{InvalidValue, ItemName};

/// `myapp/dev/db` (that item), `myapp/dev/*` (every item whose name starts with
/// `myapp/dev/`) or `*` (every item). `*` may only end the pattern.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemPattern {
    Exact(ItemName),
    Prefix(String),
}

impl ItemPattern {
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let Some(prefix) = value.strip_suffix('*') else {
            return ItemName::parse_new(value).map(Self::Exact);
        };
        let name_chars = prefix
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'));
        if !name_chars || prefix.chars().count() > ItemName::MAX_CHARS {
            return Err(InvalidValue(
                "a pattern is an item name, or the start of one followed by '*'",
            ));
        }
        Ok(Self::Prefix(prefix.to_owned()))
    }

    pub fn matches(&self, item: &ItemName) -> bool {
        match self {
            Self::Exact(name) => name == item,
            Self::Prefix(prefix) => item.as_str().starts_with(prefix.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> ItemName {
        ItemName::parse(text).unwrap()
    }

    #[test]
    fn exact_names_and_prefixes_match_what_they_name() {
        let exact = ItemPattern::parse("myapp/dev/db").unwrap();
        assert!(exact.matches(&name("myapp/dev/db")));
        assert!(!exact.matches(&name("myapp/dev/db2")));
        let prefix = ItemPattern::parse("myapp/dev/*").unwrap();
        assert!(prefix.matches(&name("myapp/dev/db")));
        assert!(prefix.matches(&name("myapp/dev/cache/redis")));
        assert!(!prefix.matches(&name("myapp/prod/db")));
        assert!(!prefix.matches(&name("myapp/devx")));
        let all = ItemPattern::parse("*").unwrap();
        assert!(all.matches(&name("anything")));
    }

    #[test]
    fn a_star_only_ends_a_pattern() {
        for bad in ["", "my*app", "*/db", "a b*", "my?app", "**"] {
            assert!(ItemPattern::parse(bad).is_err(), "{bad}");
        }
    }
}
