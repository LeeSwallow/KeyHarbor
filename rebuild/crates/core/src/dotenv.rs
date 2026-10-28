//! `.env` parsing for `vault import-env`. Error messages name lines, never values.

use anyhow::{Result, anyhow, bail};
use keyharbor_domain::vault::FieldName;
use std::collections::BTreeSet;
use zeroize::Zeroizing;

fn double_quoted(rest: &str, line: usize) -> Result<(Zeroizing<String>, &str)> {
    let mut value = Zeroizing::new(String::new());
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((value, &rest[i + 1..])),
            '\\' => match chars.next() {
                Some((_, 'n')) => value.push('\n'),
                Some((_, 't')) => value.push('\t'),
                Some((_, 'r')) => value.push('\r'),
                Some((_, other)) => value.push(other),
                None => break,
            },
            c => value.push(c),
        }
    }
    bail!("line {line}: unterminated double-quoted value")
}

/// Accepts `KEY=VALUE`, an optional `export ` prefix, `#` comment lines, blank lines,
/// double-quoted values with `\n \t \r \" \\` escapes, single-quoted literal values, and
/// trailing ` #comment` on unquoted values. Duplicate keys are rejected.
pub fn parse_dotenv(text: &str) -> Result<Vec<(FieldName, Zeroizing<String>)>> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let body = trimmed
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(trimmed);
        let (key, rest) = body
            .split_once('=')
            .ok_or_else(|| anyhow!("line {line}: expected KEY=VALUE"))?;
        let key = key.trim();
        let name = FieldName::parse_env(key).map_err(|e| anyhow!("line {line}: {e}"))?;
        let rest = rest.trim_start();
        let value = if let Some(quoted) = rest.strip_prefix('"') {
            let (value, tail) = double_quoted(quoted, line)?;
            let tail = tail.trim();
            if !tail.is_empty() && !tail.starts_with('#') {
                bail!("line {line}: unexpected text after closing quote");
            }
            value
        } else if let Some(quoted) = rest.strip_prefix('\'') {
            let end = quoted
                .find('\'')
                .ok_or_else(|| anyhow!("line {line}: unterminated single-quoted value"))?;
            let tail = quoted[end + 1..].trim();
            if !tail.is_empty() && !tail.starts_with('#') {
                bail!("line {line}: unexpected text after closing quote");
            }
            Zeroizing::new(quoted[..end].to_owned())
        } else {
            let unquoted = match rest.find(" #") {
                Some(i) => &rest[..i],
                None => rest,
            };
            Zeroizing::new(unquoted.trim_end().to_owned())
        };
        if !seen.insert(key.to_owned()) {
            bail!("duplicate key {key} on line {line}");
        }
        out.push((name, value));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(text: &str) -> Vec<(String, String)> {
        parse_dotenv(text)
            .unwrap()
            .into_iter()
            .map(|(k, v)| (k.as_str().to_owned(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_common_dotenv_forms() {
        let t = "# c\nexport A=1\nB=\"x\\ny\"\nC='raw $X'\nD=plain # tail\n\nE=\nF=\"say \\\"hi\\\"\"\n";
        assert_eq!(
            pairs(t),
            vec![
                ("A".into(), "1".into()),
                ("B".into(), "x\ny".into()),
                ("C".into(), "raw $X".into()),
                ("D".into(), "plain".into()),
                ("E".into(), "".into()),
                ("F".into(), "say \"hi\"".into()),
            ]
        );
    }
    #[test]
    fn rejects_duplicate_keys() {
        let err = parse_dotenv("A=1\nA=2").unwrap_err().to_string();
        assert!(
            err.contains("duplicate key A") && err.contains("line 2"),
            "{err}"
        );
    }
    #[test]
    fn rejects_invalid_names_and_lines() {
        for bad in ["a-b=1", "1A=1", "lower=1", "NOEQUALS", "A=\"unterminated"] {
            assert!(parse_dotenv(bad).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn errors_never_echo_values() {
        let err = parse_dotenv("A=\"KH-SYNTH-secret").unwrap_err().to_string();
        assert!(!err.contains("KH-SYNTH"), "{err}");
    }
}
