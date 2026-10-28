//! `SessionName::parse` against contracts/session-names.json, the fixture the browser extension's
//! `parseSessionName` also reads. The extension sends the name; this rule is the judge.

use keyharbor_domain::SessionName;
use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../contracts/session-names.json"
    );
    let text = std::fs::read_to_string(path).expect("contracts/session-names.json is readable");
    serde_json::from_str(&text).expect("contracts/session-names.json is JSON")
}

fn strings<'a>(fixture: &'a Value, key: &str) -> Vec<&'a str> {
    let list = fixture[key]
        .as_array()
        .unwrap_or_else(|| panic!("`{key}` is a list"));
    assert!(!list.is_empty(), "`{key}` has cases");
    list.iter().map(|v| v.as_str().expect("a string")).collect()
}

#[test]
fn valid_names_parse_unchanged() {
    for name in strings(&fixture(), "valid") {
        let parsed = SessionName::parse(name).unwrap_or_else(|_| panic!("{name:?} is valid"));
        assert_eq!(parsed.as_str(), name);
    }
}

#[test]
fn invalid_names_are_refused() {
    for name in strings(&fixture(), "invalid") {
        assert!(SessionName::parse(name).is_err(), "{name:?} is invalid");
    }
}

#[test]
fn names_are_trimmed_of_white_space_only() {
    let fixture = fixture();
    let pairs = fixture["trims"].as_array().expect("`trims` is a list");
    assert!(!pairs.is_empty(), "`trims` has cases");
    for pair in pairs {
        let (input, trimmed) = (
            pair[0].as_str().expect("input"),
            pair[1].as_str().expect("trimmed"),
        );
        let parsed = SessionName::parse(input).unwrap_or_else(|_| panic!("{input:?} is valid"));
        assert_eq!(parsed.as_str(), trimmed, "{input:?}");
    }
}
