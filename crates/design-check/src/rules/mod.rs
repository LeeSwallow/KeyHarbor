//! The rules, one per file, grouped by what they look at: `tokens` (layers and custom
//! properties), `theme` (dark blocks and contrast), `usage` (classes, icons and stylesheets the
//! pages use). Every rule is registered in `RULES` here and nowhere else. A new rule is a file in
//! its group, one `use` in the group's `mod.rs`, and one line below.

use crate::model::DesignSet;
use crate::report::Violation;

mod theme;
mod tokens;
mod usage;

pub use theme::{CONTRAST_PAIRS, ContrastPair, Measured, measure};

/// A check over the whole design system. It reports, it does not fix.
pub trait Rule: Sync {
    /// A short kebab-case name, unique in `RULES`.
    fn name(&self) -> &'static str;
    fn check(&self, ds: &DesignSet) -> Vec<Violation>;
}

pub static RULES: &[&dyn Rule] = &[
    &tokens::LayerReferences,
    &tokens::LiteralColours,
    &tokens::PxLiterals,
    &tokens::TokenDefinitions,
    &tokens::DefinedVars,
    &usage::ComponentFiles,
    &theme::ThemeParity,
    &theme::Contrast,
    &usage::Icons,
    &usage::Classes,
];

/// The rule named `name`.
pub fn rule(name: &str) -> Option<&'static dyn Rule> {
    RULES.iter().copied().find(|r| r.name() == name)
}

/// Runs every rule.
pub fn check_all(ds: &DesignSet) -> Vec<Violation> {
    RULES.iter().flat_map(|r| r.check(ds)).collect()
}
