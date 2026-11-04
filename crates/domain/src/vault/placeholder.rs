//! Placeholders: how agents name a value without seeing it. `{{secret:<item>.<field>}}` names a
//! stored field; `{{otp:<item>}}` names the one-time code the item's seed gives at the moment
//! it is typed (#206). Only a page receives a code; `keyharbor run` takes secrets only.

use super::{FieldName, InvalidValue, ItemName};
use std::{fmt, ops::Range};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceholderRef {
    pub item: ItemName,
    pub field: FieldName,
}

/// One placeholder of either namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Placeholder {
    /// A stored field's value.
    Secret(PlaceholderRef),
    /// The item's one-time code at the time it is typed.
    Otp(ItemName),
}

impl Placeholder {
    /// The item the placeholder names.
    pub fn item(&self) -> &ItemName {
        match self {
            Self::Secret(reference) => &reference.item,
            Self::Otp(item) => item,
        }
    }
}

/// Where a placeholder starts and ends: `{{secret:<item>.<field>}}`. Every other crate
/// writes and finds placeholders through these and `Display`, never its own literal.
pub const PLACEHOLDER_OPEN: &str = "{{secret:";
/// Where a one-time code placeholder starts: `{{otp:<item>}}`.
pub const OTP_PLACEHOLDER_OPEN: &str = "{{otp:";
pub const PLACEHOLDER_CLOSE: &str = "}}";

/// The placeholder text for a field of an item, e.g. `{{secret:github.password}}`.
pub fn placeholder(item: &str, field: &str) -> String {
    format!("{PLACEHOLDER_OPEN}{item}.{field}{PLACEHOLDER_CLOSE}")
}

/// The placeholder text for an item's one-time code, e.g. `{{otp:github}}`.
pub fn otp_placeholder(item: &str) -> String {
    format!("{OTP_PLACEHOLDER_OPEN}{item}{PLACEHOLDER_CLOSE}")
}

/// Whether `text` holds anything that starts a placeholder of either namespace.
pub fn has_placeholder(text: &str) -> bool {
    text.contains(PLACEHOLDER_OPEN) || text.contains(OTP_PLACEHOLDER_OPEN)
}

impl fmt::Display for PlaceholderRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&placeholder(self.item.as_str(), self.field.as_str()))
    }
}

impl fmt::Display for Placeholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secret(reference) => reference.fmt(f),
            Self::Otp(item) => f.write_str(&otp_placeholder(item.as_str())),
        }
    }
}

/// The next placeholder start in `text`: its offset and namespace opener.
fn next_open(text: &str) -> Option<(usize, &'static str)> {
    [PLACEHOLDER_OPEN, OTP_PLACEHOLDER_OPEN]
        .into_iter()
        .filter_map(|open| text.find(open).map(|at| (at, open)))
        .min_by_key(|(at, _)| *at)
}

fn parse_one(open: &str, inner: &str) -> Result<Placeholder, InvalidValue> {
    if open == OTP_PLACEHOLDER_OPEN {
        return Ok(Placeholder::Otp(ItemName::parse(inner)?));
    }
    let (item, field) = inner
        .split_once('.')
        .ok_or(InvalidValue("placeholder must be {{secret:item.field}}"))?;
    Ok(Placeholder::Secret(PlaceholderRef {
        item: ItemName::parse(item)?,
        field: FieldName::parse(field)?,
    }))
}

/// Every placeholder in `text` with the byte range it covers, in order. A malformed one fails
/// the whole text.
pub fn placeholder_spans(text: &str) -> Result<Vec<(Range<usize>, Placeholder)>, InvalidValue> {
    let mut spans = Vec::new();
    let mut offset = 0;
    while let Some((start, open)) = next_open(&text[offset..]) {
        let inner_start = offset + start + open.len();
        let end = text[inner_start..]
            .find(PLACEHOLDER_CLOSE)
            .ok_or(InvalidValue("unterminated placeholder"))?;
        let found = parse_one(open, &text[inner_start..inner_start + end])?;
        let after = inner_start + end + PLACEHOLDER_CLOSE.len();
        spans.push((offset + start..after, found));
        offset = after;
    }
    Ok(spans)
}

/// Finds every `{{secret:<item>.<field>}}` and `{{otp:<item>}}`. A malformed reference fails
/// the whole text.
pub fn parse_placeholders(text: &str) -> Result<Vec<Placeholder>, InvalidValue> {
    Ok(placeholder_spans(text)?
        .into_iter()
        .map(|(_, found)| found)
        .collect())
}

/// The `{{secret:…}}` references in `text`, for paths a one-time code never takes
/// (`keyharbor run`, files, the clipboard): an `{{otp:…}}` fails the whole text.
pub fn parse_secret_placeholders(text: &str) -> Result<Vec<PlaceholderRef>, InvalidValue> {
    parse_placeholders(text)?
        .into_iter()
        .map(|found| match found {
            Placeholder::Secret(reference) => Ok(reference),
            Placeholder::Otp(_) => Err(InvalidValue(
                "a one-time code ({{otp:item}}) is typed only into a browser page",
            )),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::vault::*;
    #[test]
    fn placeholders_display_as_they_parse() {
        let text = "u={{secret:github.username}}";
        let refs = parse_secret_placeholders(text).unwrap();
        assert_eq!(refs[0].to_string(), "{{secret:github.username}}");
        assert_eq!(placeholder("github", "username"), refs[0].to_string());
        assert!(has_placeholder(text) && !has_placeholder("{secret:x.y}"));
        let otp = parse_placeholders("code {{otp:github}}").unwrap();
        assert_eq!(otp[0].to_string(), "{{otp:github}}");
        assert_eq!(otp_placeholder("github"), otp[0].to_string());
        assert!(has_placeholder("{{otp:github}}") && !has_placeholder("{otp:github}"));
    }
    #[test]
    fn placeholders_parse_and_reject_malformed() {
        let refs = parse_secret_placeholders(
            "id={{secret:github.username}} pw={{secret:github.password}}",
        )
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
            "{{otp:}}",
            "{{otp:a b}}",
            "{{otp:github",
        ] {
            assert!(parse_placeholders(bad).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn both_namespaces_parse_in_order_with_their_spans() {
        let text = "a{{otp:bank}}b{{secret:bank.password}}c{{otp:work/mail}}";
        let spans = placeholder_spans(text).unwrap();
        let shown: Vec<(&str, String)> = spans
            .iter()
            .map(|(range, found)| (&text[range.clone()], found.item().as_str().to_owned()))
            .collect();
        assert_eq!(
            shown,
            vec![
                ("{{otp:bank}}", "bank".to_owned()),
                ("{{secret:bank.password}}", "bank".to_owned()),
                ("{{otp:work/mail}}", "work/mail".to_owned()),
            ]
        );
        assert!(matches!(spans[0].1, Placeholder::Otp(_)));
        assert!(matches!(spans[1].1, Placeholder::Secret(_)));
    }
    #[test]
    fn secret_only_paths_refuse_a_one_time_code() {
        let error = parse_secret_placeholders("{{secret:a.b}} {{otp:a}}").unwrap_err();
        assert_eq!(
            error.to_string(),
            "a one-time code ({{otp:item}}) is typed only into a browser page"
        );
    }
}
