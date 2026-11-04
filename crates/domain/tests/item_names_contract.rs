//! `ItemName::parse_new` against contracts/item-names.json, the fixture the browser extension's
//! `isFillableItemName` also reads. The extension checks the name early; this rule is the judge.

use keyharbor_domain::vault::ItemName;
use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/item-names.json"
    );
    let text = std::fs::read_to_string(path).expect("contracts/item-names.json is readable");
    serde_json::from_str(&text).expect("contracts/item-names.json is JSON")
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
        let parsed = ItemName::parse_new(name).unwrap_or_else(|_| panic!("{name:?} is valid"));
        assert_eq!(parsed.as_str(), name);
    }
}

#[test]
fn invalid_names_are_refused() {
    for name in strings(&fixture(), "invalid") {
        assert!(ItemName::parse_new(name).is_err(), "{name:?} is invalid");
    }
}
