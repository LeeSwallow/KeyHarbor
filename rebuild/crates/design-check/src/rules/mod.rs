//! The rules, one per file, registered in `RULES`. A new rule is a file here plus one line below.

use crate::model::DesignSet;
use crate::report::Violation;

mod classes;
mod component_files;
mod contrast;
mod defined_vars;
mod icons;
mod layer_references;
mod literal_colours;
mod px_literals;
mod theme_parity;
mod token_definitions;

pub use contrast::{CONTRAST_PAIRS, ContrastPair, Measured, measure};

/// A check over the whole design system. It reports, it does not fix.
pub trait Rule: Sync {
    /// A short kebab-case name, unique in `RULES`.
    fn name(&self) -> &'static str;
    fn check(&self, ds: &DesignSet) -> Vec<Violation>;
}

pub static RULES: &[&dyn Rule] = &[
    &layer_references::LayerReferences,
    &literal_colours::LiteralColours,
    &px_literals::PxLiterals,
    &token_definitions::TokenDefinitions,
    &defined_vars::DefinedVars,
    &component_files::ComponentFiles,
    &theme_parity::ThemeParity,
    &contrast::Contrast,
    &icons::Icons,
    &classes::Classes,
];

/// The rule named `name`.
pub fn rule(name: &str) -> Option<&'static dyn Rule> {
    RULES.iter().copied().find(|r| r.name() == name)
}

/// Runs every rule.
pub fn check_all(ds: &DesignSet) -> Vec<Violation> {
    RULES.iter().flat_map(|r| r.check(ds)).collect()
}
