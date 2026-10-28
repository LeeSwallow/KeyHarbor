//! Response masking (03 §3.7): every vault value — and the forms it commonly takes in pages
//! and URLs — is replaced by its `{{secret:item.field}}` placeholder before text reaches an
//! agent. One Aho-Corasick scan per string, leftmost-longest.
//!
//! Values shorter than 4 characters are not masked (they would match almost any text). The
//! automaton keeps its own copy of the patterns, which is not zeroized on drop.

use aho_corasick::{AhoCorasick, MatchKind};
use anyhow::Result;
use keyharbor_core::{PlaintextAccess, Vault};
use keyharbor_domain::vault::PlaceholderRef;
use serde_json::Value;
use zeroize::Zeroizing;

pub const MIN_MASKED_CHARS: usize = 4;

pub struct Masker {
    automaton: Option<AhoCorasick>,
    /// The placeholder for each pattern, by pattern index.
    replacements: Vec<String>,
    version: Option<i64>,
}

impl std::fmt::Debug for Masker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Masker")
            .field("patterns", &self.replacements.len())
            .field("version", &self.version)
            .finish()
    }
}

/// encodeURIComponent's unreserved set.
fn component_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b)
}

fn percent(value: &str, keep: impl Fn(u8) -> bool, upper: bool, plus: bool) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for b in value.bytes() {
        if plus && b == b' ' {
            out.push('+');
        } else if keep(b) {
            out.push(b as char);
        } else if upper {
            out.push_str(&format!("%{b:02X}"));
        } else {
            out.push_str(&format!("%{b:02x}"));
        }
    }
    out
}

fn html(value: &str, apostrophe: &str) -> String {
    let mut out = String::with_capacity(value.len() + 16);
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str(apostrophe),
            c => out.push(c),
        }
    }
    out
}

/// The digits of a card-number-like value (12–19 digits, optionally spaced or hyphenated).
fn card_digits(value: &str) -> Option<String> {
    if !value
        .chars()
        .all(|c| c.is_ascii_digit() || c == ' ' || c == '-')
    {
        return None;
    }
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    (12..=19).contains(&digits.len()).then_some(digits)
}

fn grouped(digits: &str, separator: &str) -> String {
    digits
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).expect("ASCII digits"))
        .collect::<Vec<_>>()
        .join(separator)
}

/// The body of a JSON string literal for `value` (`"` → `\"`, `\` → `\\`, control
/// characters → `\n`, `\u001f`, …), as snapshots and other JSON text carry it.
fn json_escaped(value: &str) -> String {
    let quoted = serde_json::to_string(value).expect("strings serialize");
    quoted[1..quoted.len() - 1].to_owned()
}

/// The forms of `value` that are masked: as written, percent-encoded (component, strict and
/// `application/x-www-form-urlencoded`; upper- and lowercase hex, `+` for spaces),
/// HTML-escaped, JSON-escaped, and for card numbers the bare digits and groups of four
/// separated by spaces or hyphens.
pub fn variants(value: &str) -> Vec<Zeroizing<String>> {
    if value.chars().count() < MIN_MASKED_CHARS {
        return Vec::new();
    }
    let strict = |b: u8| b.is_ascii_alphanumeric() || b"-_.~".contains(&b);
    // What browsers send for form fields (WHATWG URL, urlencoded serializer).
    let form = |b: u8| b.is_ascii_alphanumeric() || b"*-._".contains(&b);
    let mut out = vec![
        Zeroizing::new(value.to_owned()),
        Zeroizing::new(percent(value, component_safe, true, false)),
        Zeroizing::new(percent(value, component_safe, false, false)),
        Zeroizing::new(percent(value, strict, true, false)),
        Zeroizing::new(percent(value, component_safe, true, true)),
        Zeroizing::new(percent(value, strict, true, true)),
        Zeroizing::new(percent(value, form, true, true)),
        Zeroizing::new(percent(value, form, false, true)),
        Zeroizing::new(html(value, "&#39;")),
        Zeroizing::new(html(value, "&#x27;")),
        Zeroizing::new(json_escaped(value)),
    ];
    if let Some(digits) = card_digits(value) {
        out.push(Zeroizing::new(grouped(&digits, " ")));
        out.push(Zeroizing::new(grouped(&digits, "-")));
        out.push(Zeroizing::new(digits));
    }
    let mut unique: Vec<Zeroizing<String>> = Vec::with_capacity(out.len());
    for v in out {
        if !unique.contains(&v) {
            unique.push(v);
        }
    }
    unique
}

impl Masker {
    /// A masker that changes nothing.
    pub fn empty() -> Self {
        Self {
            automaton: None,
            replacements: Vec::new(),
            version: None,
        }
    }

    pub fn build(values: impl IntoIterator<Item = (PlaceholderRef, Zeroizing<String>)>) -> Self {
        let mut patterns: Vec<(Zeroizing<String>, String)> = Vec::new();
        for (reference, value) in values {
            let placeholder = format!(
                "{{{{secret:{}.{}}}}}",
                reference.item.as_str(),
                reference.field.as_str()
            );
            for variant in variants(&value) {
                patterns.push((variant, placeholder.clone()));
            }
        }
        if patterns.is_empty() {
            return Self::empty();
        }
        // The first value to produce a pattern names it (the sort is stable).
        patterns.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        patterns.dedup_by(|later, first| later.0 == first.0);
        let (keys, replacements): (Vec<_>, Vec<_>) = patterns.into_iter().unzip();
        let automaton = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(keys.iter().map(|k| k.as_bytes()))
            .expect("patterns build an automaton");
        Self {
            automaton: Some(automaton),
            replacements,
            version: None,
        }
    }

    pub fn with_version(mut self, version: i64) -> Self {
        self.version = Some(version);
        self
    }

    /// The vault version the values were read at.
    pub fn version(&self) -> Option<i64> {
        self.version
    }

    pub fn redact(&self, text: &str) -> String {
        match &self.automaton {
            Some(ac) => ac.replace_all(text, &self.replacements),
            None => text.to_owned(),
        }
    }

    pub fn redact_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        match &self.automaton {
            Some(ac) => ac.replace_all_bytes(bytes, &self.replacements),
            None => bytes.to_vec(),
        }
    }

    /// Masks every string in a JSON value, object keys included.
    pub fn redact_json(&self, value: Value) -> Value {
        if self.automaton.is_none() {
            return value;
        }
        match value {
            Value::String(s) => Value::String(self.redact(&s)),
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|v| self.redact_json(v)).collect())
            }
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| (self.redact(&k), self.redact_json(v)))
                    .collect(),
            ),
            other => other,
        }
    }
}

/// A masker for every field value in the vault, whatever its status or policy (an item an
/// agent may not use must not leak either), tagged with the vault version it was read at.
/// Fails if any value cannot be decrypted, so callers can withhold responses rather than send
/// them unmasked. The value list never leaves this function; only the automaton does (J-37).
pub fn masker(vault: &Vault) -> Result<Masker> {
    let values = vault.mask_values(&PlaintextAccess::issue())?;
    Ok(Masker::build(values.values).with_version(values.version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env, field, init, login, pref};
    use keyharbor_domain::vault::ItemKind;
    use serde_json::json;

    fn masker(values: &[(&str, &str, &str)]) -> Masker {
        Masker::build(
            values
                .iter()
                .map(|(i, f, v)| (pref(i, f), Zeroizing::new((*v).to_owned()))),
        )
    }

    #[test]
    fn values_become_placeholders() {
        let m = masker(&[("github", "password", "hunter22")]);
        assert_eq!(
            m.redact("pw=hunter22; again hunter22."),
            "pw={{secret:github.password}}; again {{secret:github.password}}."
        );
        assert_eq!(m.redact("nothing here"), "nothing here");
        assert_eq!(Masker::empty().redact("hunter22"), "hunter22");
    }

    #[test]
    fn longest_overlapping_value_wins() {
        let m = masker(&[("a", "value", "abcd"), ("b", "value", "abcdefgh")]);
        assert_eq!(m.redact("xabcdefghx"), "x{{secret:b.value}}x");
        assert_eq!(m.redact("xabcdefx"), "x{{secret:a.value}}efx");
        let m = masker(&[("a", "value", "1234-5678"), ("b", "value", "5678-9999")]);
        assert_eq!(m.redact("1234-5678-9999"), "{{secret:a.value}}-9999");
    }

    #[test]
    fn encoded_forms_are_masked() {
        let m = masker(&[("api", "value", "p@ss w&rd'<x>")]);
        for form in [
            "p%40ss%20w%26rd'%3Cx%3E",
            "p%40ss%20w%26rd%27%3Cx%3E",
            "p%40ss+w%26rd'%3Cx%3E",
            "p%40ss%20w%26rd'%3cx%3e",
            "p@ss w&amp;rd&#39;&lt;x&gt;",
            "p@ss w&amp;rd&#x27;&lt;x&gt;",
            "p@ss w&rd'<x>",
        ] {
            assert_eq!(
                m.redact(&format!("?k={form}&")),
                "?k={{secret:api.value}}&",
                "{form}"
            );
        }
    }

    #[test]
    fn json_escaped_and_form_encoded_forms_are_masked() {
        // I2: a value with quotes, backslashes or control characters, as JSON text holds it.
        let m = masker(&[("site", "username", "KH\"q\\z\n\u{1}x")]);
        for form in ["KH\\\"q\\\\z\\n\\u0001x", "KH\"q\\z\n\u{1}x"] {
            assert_eq!(
                m.redact(&format!("value=\"{form}\"")),
                "value=\"{{secret:site.username}}\"",
                "{form}"
            );
        }
        let snapshot = format!(
            "[1] textbox value={}",
            serde_json::to_string("KH\"q\\z\n\u{1}x").unwrap()
        );
        assert_eq!(
            m.redact(&snapshot),
            "[1] textbox value=\"{{secret:site.username}}\""
        );
        // I5: what a browser sends for a form field: `*` kept, `~ ! ' ( )` encoded, `+` for space.
        let m = masker(&[("api", "value", "a~b!c'd(e)f*g h")]);
        for form in ["a%7Eb%21c%27d%28e%29f*g+h", "a%7eb%21c%27d%28e%29f*g+h"] {
            assert_eq!(
                m.redact(&format!("?q={form}&")),
                "?q={{secret:api.value}}&",
                "{form}"
            );
        }
    }

    #[test]
    fn short_values_are_not_masked() {
        let m = masker(&[("visa", "exp_month", "12"), ("visa", "cvc", "123")]);
        assert_eq!(m.redact("12/2030 cvc 123"), "12/2030 cvc 123");
        let m = masker(&[("visa", "cvc", "1234"), ("me", "full_name", "김민수")]);
        assert_eq!(m.redact("cvc 1234"), "cvc {{secret:visa.cvc}}");
        assert_eq!(m.redact("김민수"), "김민수", "three characters");
    }

    #[test]
    fn card_numbers_are_masked_in_common_layouts() {
        let m = masker(&[("visa", "number", "4242 4242 4242 4242")]);
        for form in [
            "4242424242424242",
            "4242 4242 4242 4242",
            "4242-4242-4242-4242",
        ] {
            assert_eq!(m.redact(form), "{{secret:visa.number}}", "{form}");
        }
        // The masked display sites show is left alone.
        assert_eq!(m.redact("424242******4242"), "424242******4242");
        assert_eq!(m.redact("•••• 4242"), "•••• 4242");
    }

    #[test]
    fn json_and_bytes_are_masked() {
        let m = masker(&[("github", "password", "hunter22")]);
        let value = json!({"text": "hunter22", "list": ["a hunter22", 3, null], "hunter22": true});
        assert_eq!(
            m.redact_json(value),
            json!({"text": "{{secret:github.password}}", "list": ["a {{secret:github.password}}", 3, null], "{{secret:github.password}}": true})
        );
        assert_eq!(
            m.redact_bytes(b"x hunter22\xff\n"),
            b"x {{secret:github.password}}\xff\n".to_vec()
        );
        assert!(!format!("{m:?}").contains("hunter22"));
    }

    #[test]
    fn vault_masker_covers_every_item_and_tracks_the_version() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "KH-SYNTH-pw-1")).unwrap();
        let mut card = login("visa", "");
        card.kind = ItemKind::Card;
        card.fields = vec![field("number", "4000056655665556"), field("cvc", "123")];
        v.add(card).unwrap();
        let mut draft = login("draft", "KH-SYNTH-draft");
        draft.status = keyharbor_core::ItemStatus::Discarded;
        v.add(draft).unwrap();
        let m = v.masker().unwrap();
        assert_eq!(m.version(), Some(v.version().unwrap()));
        assert_eq!(
            m.redact("KH-SYNTH-pw-1 me@x.com 4000 0566 5566 5556 KH-SYNTH-draft 123"),
            "{{secret:github.password}} {{secret:draft.username}} {{secret:visa.number}} {{secret:draft.password}} 123"
        );
    }
}
