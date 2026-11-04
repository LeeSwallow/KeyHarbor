//! Secret values that masking handles badly (03 §3.7, J-230): too short to be masked at all,
//! or so common ("해당없음", "none") that every copy of the word on a page is masked and an
//! agent typing it is refused. Saving such a value only warns; the warning names the item,
//! the field and the reason, never the value.

use super::{FieldName, ItemKind, ItemName};
use std::fmt;
use zeroize::Zeroizing;

/// Values shorter than this many characters are never masked: they would match almost any
/// text (03 §3.7). The guard's masker reads it from here.
pub const MIN_MASKED_CHARS: usize = 4;

/// Answers that application forms and pages carry everywhere, compared ignoring ASCII case
/// and surrounding spaces. Kept short on purpose: each entry warns on a real save, so only
/// words that show up on nearly every form belong here; add one when a report shows a word
/// blanking out pages (J-230). Entries under [`MIN_MASKED_CHARS`] are never masked and warn
/// as [`CommonValueReason::TooShortToMask`] instead; they stay listed so the list reads whole.
const COMMON_WORDS: &[&str] = &[
    // Korean
    "해당없음",
    "해당 없음",
    "해당사항 없음",
    "해당사항없음",
    "없음",
    "있음",
    "미필",
    "군필",
    "면제",
    "미대상",
    "비대상",
    "예",
    "아니오",
    "아니요",
    "네",
    "남",
    "여",
    "남자",
    "여자",
    "남성",
    "여성",
    "대한민국",
    "한국",
    "내국인",
    "외국인",
    // English
    "none",
    "n/a",
    "na",
    "nil",
    "null",
    "yes",
    "no",
    "true",
    "false",
    "male",
    "female",
    "other",
    "unknown",
    "not applicable",
    "korea",
    "south korea",
];

/// The longest all-digit value counted as too simple: a four-digit number is a year, a
/// count or a code that pages print often (J-230).
const SIMPLE_DIGITS_MAX: usize = 4;

/// Why a secret value is likely to cause trouble once stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommonValueReason {
    /// Under [`MIN_MASKED_CHARS`] characters: never masked, so it can show on pages as typed.
    TooShortToMask,
    /// A common answer: every copy of it on a page is masked, and typing it is refused.
    CommonWord,
    /// One character repeated, or a short number: the same trouble as a common word.
    TooSimple,
}

impl CommonValueReason {
    pub const ALL: [Self; 3] = [Self::TooShortToMask, Self::CommonWord, Self::TooSimple];
    /// Why `value` is a poor secret, or `None` when masking handles it well.
    pub fn of(value: &str) -> Option<Self> {
        if value.chars().count() < MIN_MASKED_CHARS {
            return Some(Self::TooShortToMask);
        }
        let trimmed = value.trim();
        if COMMON_WORDS.iter().any(|w| trimmed.eq_ignore_ascii_case(w)) {
            return Some(Self::CommonWord);
        }
        if is_too_simple(trimmed) {
            return Some(Self::TooSimple);
        }
        None
    }
    /// A stable code for interfaces that word the reason themselves (the app).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TooShortToMask => "too_short_to_mask",
            Self::CommonWord => "common_word",
            Self::TooSimple => "too_simple",
        }
    }
    /// The reason in a sentence, the same for every interface that prints English.
    pub fn explain(self) -> &'static str {
        match self {
            Self::TooShortToMask => {
                "under 4 characters, so it is never masked and can show on pages as typed"
            }
            Self::CommonWord => {
                "a common word, so every copy of it on a page is masked and typing it is refused"
            }
            Self::TooSimple => {
                "too simple, so every copy of it on a page is masked and typing it is refused"
            }
        }
    }
}

fn is_too_simple(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return true;
    };
    chars.clone().all(|c| c == first)
        || (value.chars().all(|c| c.is_ascii_digit()) && value.chars().count() <= SIMPLE_DIGITS_MAX)
}

/// A stored secret value that masking handles badly. Holds no value: safe to print.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommonValueWarning {
    pub item: ItemName,
    pub field: FieldName,
    pub reason: CommonValueReason,
}

impl fmt::Display for CommonValueWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{} is {}; a field marked plain is not masked",
            self.item.as_str(),
            self.field.as_str(),
            self.reason.explain()
        )
    }
}

/// The warnings for `fields` about to be stored in `item`: one per secret field (by the kind
/// registry, and not in `plain`) whose value [`CommonValueReason::of`] flags.
pub fn common_value_warnings(
    item: &ItemName,
    kind: ItemKind,
    fields: &[(FieldName, Zeroizing<String>)],
    plain: &[FieldName],
) -> Vec<CommonValueWarning> {
    fields
        .iter()
        .filter(|(field, _)| kind.is_secret_field(field.as_str()) && !plain.contains(field))
        .filter_map(|(field, value)| {
            CommonValueReason::of(value).map(|reason| CommonValueWarning {
                item: item.clone(),
                field: field.clone(),
                reason,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(value: &str) -> Option<CommonValueReason> {
        CommonValueReason::of(value)
    }

    #[test]
    fn common_answers_are_flagged_ignoring_case_and_spaces() {
        for value in [
            "해당없음",
            "해당 없음",
            "대한민국",
            " None ",
            "N/A ",
            "FALSE",
            "Female",
        ] {
            assert_eq!(
                reason(value),
                Some(CommonValueReason::CommonWord),
                "{value}"
            );
        }
    }

    #[test]
    fn values_under_four_characters_are_flagged_as_unmasked() {
        for value in ["미필", "군필", "면제", "예", "남", "no", "yes", "abc"] {
            assert_eq!(
                reason(value),
                Some(CommonValueReason::TooShortToMask),
                "{value}"
            );
        }
    }

    #[test]
    fn repeated_characters_and_short_numbers_are_too_simple() {
        for value in ["aaaa", "0000", "------", "1234", "2024"] {
            assert_eq!(reason(value), Some(CommonValueReason::TooSimple), "{value}");
        }
    }

    #[test]
    fn rare_values_are_quiet() {
        for value in [
            "sk-live-4f9a2c",
            "12345",
            "correct horse",
            "보훈대상자 2급",
            "abcd",
        ] {
            assert_eq!(reason(value), None, "{value}");
        }
    }

    fn fields(pairs: &[(&str, &str)]) -> Vec<(FieldName, Zeroizing<String>)> {
        pairs
            .iter()
            .map(|(f, v)| {
                (
                    FieldName::parse(f).unwrap(),
                    Zeroizing::new((*v).to_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn only_secret_fields_that_are_not_plain_warn() {
        let item = ItemName::parse("me").unwrap();
        let given = fields(&[
            ("military_service", "해당없음"),
            ("gender", "여자"),
            ("disability", "없음"),
            ("veterans", "국가유공자 유족"),
        ]);
        let plain = [FieldName::parse("disability").unwrap()];
        let warnings = common_value_warnings(&item, ItemKind::Identity, &given, &plain);
        assert_eq!(
            warnings,
            [CommonValueWarning {
                item: item.clone(),
                field: FieldName::parse("military_service").unwrap(),
                reason: CommonValueReason::CommonWord,
            }]
        );
    }

    #[test]
    fn a_warning_names_the_field_and_never_the_value() {
        let item = ItemName::parse("me").unwrap();
        let given = fields(&[("military_service", "해당없음")]);
        let text = common_value_warnings(&item, ItemKind::Identity, &given, &[])[0].to_string();
        assert!(text.contains("me.military_service"), "{text}");
        assert!(text.contains("plain"), "{text}");
        assert!(!text.contains("해당없음"), "{text}");
    }
}
