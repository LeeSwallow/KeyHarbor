//! The real design-system/ folder passes every rule. One test per rule, so a failure names it.

use keyharbor_design_check::model::DesignSet;
use keyharbor_design_check::report;
use keyharbor_design_check::rules::{self, RULES};
use keyharbor_design_check::theme::Theme;

fn design() -> DesignSet {
    DesignSet::load(&keyharbor_design_check::repo_root()).expect("read design-system/")
}

fn assert_rule_holds(name: &str) {
    let rule = rules::rule(name).unwrap_or_else(|| panic!("no rule named {name}"));
    let violations = rule.check(&design());
    assert!(violations.is_empty(), "\n{}", report::render(&violations));
}

#[test]
fn every_rule_has_a_test_here_and_a_unique_name() {
    let names: Vec<&str> = RULES.iter().map(|r| r.name()).collect();
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "duplicate rule names: {names:?}");
    let source = include_str!("rules.rs");
    for name in names {
        assert!(
            source.contains(&format!("assert_rule_holds(\"{name}\")")),
            "tests/rules.rs has no test for {name}"
        );
    }
}

#[test]
fn the_design_set_is_found() {
    let ds = design();
    assert!(ds.sheets.len() > 20, "{} stylesheets", ds.sheets.len());
    // The class and icon rules cover the showcase and the browser extension's popup.
    for page in [
        "design-system/index.html",
        "design-system/showcase/sections/components.js",
        "integrations/browser-extension/src/ui/popup/popup.html",
        "integrations/browser-extension/src/ui/popup/view.ts",
    ] {
        assert!(
            ds.consumers.iter().any(|f| f.path == page),
            "{page} is not checked"
        );
    }
    assert!(
        !ds.consumers.iter().any(|f| f.path.contains(".test.")),
        "tests are not consumers"
    );
    assert!(ds.icons.len() >= 41);
}

#[test]
fn layers_reference_only_what_the_table_allows() {
    assert_rule_holds("layer-references");
}

#[test]
fn literal_colours_live_only_in_primitives() {
    assert_rule_holds("literal-colours");
}

#[test]
fn components_take_sizes_from_tokens() {
    assert_rule_holds("px-literals");
}

#[test]
fn only_token_files_define_tokens() {
    assert_rule_holds("token-definitions");
}

#[test]
fn every_var_used_is_defined() {
    assert_rule_holds("defined-vars");
}

#[test]
fn every_component_file_is_imported_and_used() {
    assert_rule_holds("component-files");
}

#[test]
fn both_dark_blocks_match_the_light_theme() {
    assert_rule_holds("theme-parity");
}

#[test]
fn text_colours_meet_contrast_in_both_themes() {
    // Printed so `cargo test -- --nocapture` shows the numbers.
    for m in rules::measure(&design()) {
        let theme = match m.theme {
            Theme::Light => "light",
            Theme::Dark => "dark ",
        };
        let ratio = m.ratio.map_or("—".to_string(), |r| format!("{r:5.2}"));
        println!(
            "{theme} {ratio} ≥ {:.1}  {} on {}",
            m.pair.min, m.pair.fg, m.pair.bg
        );
    }
    assert_rule_holds("contrast");
}

#[test]
fn icons_exist_and_paint_with_current_colour() {
    assert_rule_holds("icons");
}

#[test]
fn every_kh_class_in_use_is_styled() {
    assert_rule_holds("classes");
}
