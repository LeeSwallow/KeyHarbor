//! The window's lists of stored names come from Rust (J-175): `ui/test/vocabulary.json` is what
//! `Resource::ALL` and `AuditKind::ALL` say, and the UI tests check the window against it. A
//! name added in Rust fails here until the file has it, and then fails the UI tests until the
//! window offers it and has a word for it.

use keyharbor_domain::{audit::AuditKind, policy::Resource};
use serde_json::{Value, json};
use std::path::Path;

const FIXTURE: &str = "ui/test/vocabulary.json";

fn expected() -> Value {
    json!({
        "resources": Resource::ALL.map(Resource::as_str),
        "audit_kinds": AuditKind::ALL.map(AuditKind::as_str),
    })
}

#[test]
fn the_window_vocabulary_fixture_matches_rust() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let text = std::fs::read_to_string(&path).expect("app/ui/test/vocabulary.json exists");
    let found: Value = serde_json::from_str(&text).expect("the fixture is JSON");
    assert_eq!(
        found,
        expected(),
        "{FIXTURE} is behind Rust; write this into it:\n{:#}",
        expected()
    );
}
