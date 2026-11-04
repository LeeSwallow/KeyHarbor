//! Placeholders: how agents name a value without seeing it.

use super::{FieldName, InvalidValue, ItemName};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceholderRef {
    pub item: ItemName,
    pub field: FieldName,
}

/// Where a placeholder starts and ends: `{{secret:<item>.<field>}}`. Every other crate
/// writes and finds placeholders through these and `Display`, never its own literal.
pub const PLACEHOLDER_OPEN: &str = "{{secret:";
pub const PLACEHOLDER_CLOSE: &str = "}}";

/// The placeholder text for a field of an item, e.g. `{{secret:github.password}}`.
pub fn placeholder(item: &str, field: &str) -> String {
    format!("{PLACEHOLDER_OPEN}{item}.{field}{PLACEHOLDER_CLOSE}")
}

/// Whether `text` holds anything that starts a placeholder.
pub fn has_placeholder(text: &str) -> bool {
    text.contains(PLACEHOLDER_OPEN)
}

impl fmt::Display for PlaceholderRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&placeholder(self.item.as_str(), self.field.as_str()))
    }
}

/// Finds every `{{secret:<item>.<field>}}` reference. A malformed reference fails the whole text.
pub fn parse_placeholders(text: &str) -> Result<Vec<PlaceholderRef>, InvalidValue> {
    let mut refs = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(PLACEHOLDER_OPEN) {
        let after = &rest[start + PLACEHOLDER_OPEN.len()..];
        let end = after
            .find(PLACEHOLDER_CLOSE)
            .ok_or(InvalidValue("unterminated placeholder"))?;
        let (item, field) = after[..end]
            .split_once('.')
            .ok_or(InvalidValue("placeholder must be {{secret:item.field}}"))?;
        refs.push(PlaceholderRef {
            item: ItemName::parse(item)?,
            field: FieldName::parse(field)?,
        });
        rest = &after[end + PLACEHOLDER_CLOSE.len()..];
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use crate::vault::*;
    #[test]
    fn placeholders_display_as_they_parse() {
        let text = "u={{secret:github.username}}";
        let refs = parse_placeholders(text).unwrap();
        assert_eq!(refs[0].to_string(), "{{secret:github.username}}");
        assert_eq!(placeholder("github", "username"), refs[0].to_string());
        assert!(has_placeholder(text) && !has_placeholder("{secret:x.y}"));
    }
    #[test]
    fn placeholders_parse_and_reject_malformed() {
        let refs =
            parse_placeholders("id={{secret:github.username}} pw={{secret:github.password}}")
                .unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[1].item.as_str(), "github");
        assert_eq!(refs[1].field.as_str(), "password");
        assert!(parse_placeholders("plain text").unwrap().is_empty());
        for bad in [
            "{{secret:github}}",
            "{{secret:.x}}",
            "{{secret:a.b",
            "{{secret:a b.c}}",
        ] {
            assert!(parse_placeholders(bad).is_err(), "{bad:?}");
        }
    }
}
